//! Score renderer — converts a parsed Score into SVG output.
//!
//! The renderer computes its own layout from the musical content (pitch,
//! duration, time signature) and produces a self-contained SVG string
//! that can be displayed in any SVG-capable view.

use crate::model::*;

// ═══════════════════════════════════════════════════════════════════════
// Constants (all in SVG user units)
// ═══════════════════════════════════════════════════════════════════════

const PAGE_WIDTH: f64 = 820.0;
const PAGE_MARGIN_LEFT: f64 = 50.0;
const PAGE_MARGIN_RIGHT: f64 = 30.0;
const PAGE_MARGIN_TOP: f64 = 30.0;

const STAFF_LINE_SPACING: f64 = 10.0; // distance between staff lines
const STAFF_HEIGHT: f64 = 40.0; // 5 lines, 4 spaces
const SYSTEM_SPACING: f64 = 90.0; // vertical space between systems
const GRAND_STAFF_GAP: f64 = 60.0; // vertical gap between staves in a grand staff
const PART_GAP: f64 = 80.0; // vertical gap between different parts/instruments
const BRACE_WIDTH: f64 = 10.0; // width of the brace/bracket

const HEADER_HEIGHT: f64 = 70.0; // space for title + composer
const FIRST_SYSTEM_TOP: f64 = PAGE_MARGIN_TOP + HEADER_HEIGHT;

const CLEF_SPACE: f64 = 32.0; // horizontal space for clef at system start
const KEY_SIG_SPACE: f64 = 12.0; // per accidental in key signature
const TIME_SIG_SPACE: f64 = 24.0; // horizontal space for time signature

const NOTEHEAD_RX: f64 = 5.5; // notehead ellipse x-radius
const NOTEHEAD_RY: f64 = 4.0; // notehead ellipse y-radius
const STEM_LENGTH: f64 = 30.0; // stem length
const STEM_WIDTH: f64 = 1.2;
const BEAM_THICKNESS: f64 = 4.0;
const BARLINE_WIDTH: f64 = 1.0;
const STAFF_LINE_WIDTH: f64 = 0.8;
const LEDGER_LINE_WIDTH: f64 = 0.8;
const LEDGER_LINE_EXTEND: f64 = 5.0; // how far ledger lines extend past notehead

const MIN_NOTE_SPACING: f64 = 18.0;
const MIN_MEASURE_WIDTH: f64 = 50.0;
const CHORD_SYMBOL_OFFSET_Y: f64 = -18.0; // above staff

const NOTE_COLOR: &str = "#1a1a1a";
const STAFF_COLOR: &str = "#555555";
const BARLINE_COLOR: &str = "#333333";
const CHORD_COLOR: &str = "#4a4a9a";
const HEADER_COLOR: &str = "#1a1a1a";
const REST_COLOR: &str = "#1a1a1a";

// ═══════════════════════════════════════════════════════════════════════
// Layout structures
// ═══════════════════════════════════════════════════════════════════════

struct ScoreLayout {
    systems: Vec<SystemLayout>,
    total_height: f64,
}

/// Info about one part's staves within a system
struct PartStaffInfo {
    part_idx: usize,
    y_offset: f64,     // vertical offset from system.y to this part's first staff
    num_staves: usize, // staves in this part (1 for most, 2 for piano)
}

struct SystemLayout {
    y: f64, // Y of top-most staff line in the system
    #[allow(dead_code)]
    x_start: f64, // content start (after clef/key/time)
    x_end: f64,
    measures: Vec<MeasureLayout>,
    parts: Vec<PartStaffInfo>,
    show_clef: bool,
    show_time: bool,
    total_staves: usize, // total staves across all parts
}

struct MeasureLayout {
    measure_idx: usize,
    x: f64,
    width: f64,
    beat_x_map: Vec<(f64, f64)>, // (beat_time, x_position)
}

// ═══════════════════════════════════════════════════════════════════════
// Public API
// ═══════════════════════════════════════════════════════════════════════

/// Render a parsed Score into a complete SVG string.
pub fn render_score_to_svg(score: &Score) -> String {
    if score.parts.is_empty() {
        return empty_svg("No parts in score");
    }

    // Determine staves per part
    let parts_staves: Vec<(usize, usize)> = score
        .parts
        .iter()
        .enumerate()
        .map(|(i, part)| (i, detect_staves(part)))
        .collect();

    let layout = compute_layout(score, &parts_staves);

    let mut svg = SvgBuilder::new(PAGE_WIDTH, layout.total_height);

    // Background
    svg.rect(0.0, 0.0, PAGE_WIDTH, layout.total_height, "white", "none", 0.0);

    // Title and composer
    render_header(&mut svg, score);

    // Running attributes per part — (clefs vec indexed 1-based, key, time, divisions, transpose)
    struct PartState {
        clefs: Vec<Option<Clef>>,  // index 0 unused, 1..=num_staves
        key: Option<Key>,
        time: Option<TimeSignature>,
        divisions: i32,
        transpose_octave: i32,
    }

    let mut part_states: Vec<PartState> = parts_staves
        .iter()
        .map(|&(pidx, ns)| {
            let part = &score.parts[pidx];
            let mut clefs: Vec<Option<Clef>> = vec![None; ns + 1];
            let mut key = None;
            let mut time = None;
            let mut divisions = 1;
            let mut transpose_octave = 0;

            // Pre-scan for initial attributes
            for measure in &part.measures {
                if let Some(ref attrs) = measure.attributes {
                    for clef in &attrs.clefs {
                        let idx = clef.number as usize;
                        if idx < clefs.len() {
                            clefs[idx] = Some(clef.clone());
                        }
                    }
                    if attrs.key.is_some() {
                        key = attrs.key.clone();
                    }
                    if attrs.time.is_some() {
                        time = attrs.time.clone();
                    }
                    if let Some(d) = attrs.divisions {
                        divisions = d;
                    }
                    if let Some(ref t) = attrs.transpose {
                        transpose_octave = t.octave_change.unwrap_or(0);
                    }
                    break;
                }
            }

            // Default treble clef for staff 1 if none found
            if clefs[1].is_none() {
                clefs[1] = Some(Clef {
                    number: 1,
                    sign: "G".into(),
                    line: 2,
                    octave_change: None,
                });
            }

            PartState { clefs, key, time, divisions, transpose_octave }
        })
        .collect();

    // Render each system
    for system in &layout.systems {
        let system_y = system.y;

        // ── Staff lines, clefs, key/time signatures per part ──
        for part_info in &system.parts {
            let ps = &part_states[part_info.part_idx];

            for staff_num in 1..=part_info.num_staves {
                let staff_y = system_y
                    + part_info.y_offset
                    + (staff_num as f64 - 1.0) * (STAFF_HEIGHT + GRAND_STAFF_GAP);

                // Staff lines
                render_staff_lines(&mut svg, PAGE_MARGIN_LEFT, system.x_end, staff_y);

                // Clef
                if system.show_clef {
                    if let Some(ref clef) = ps.clefs[staff_num] {
                        render_clef(&mut svg, PAGE_MARGIN_LEFT + 5.0, staff_y, clef);
                    }
                }

                // Key signature
                let key_x = PAGE_MARGIN_LEFT + CLEF_SPACE;
                if let Some(ref key) = ps.key {
                    render_key_signature(
                        &mut svg, key_x, staff_y, key,
                        ps.clefs[staff_num].as_ref(),
                    );
                }

                // Time signature
                if system.show_time {
                    if let Some(ref time) = ps.time {
                        let time_x = key_x + key_sig_width(ps.key.as_ref());
                        render_time_signature(&mut svg, time_x, staff_y, time);
                    }
                }
            }

            // Brace for multi-staff parts (e.g. piano grand staff)
            if part_info.num_staves > 1 {
                let top_y = system_y + part_info.y_offset;
                let bottom_y = top_y
                    + (part_info.num_staves as f64 - 1.0) * (STAFF_HEIGHT + GRAND_STAFF_GAP)
                    + STAFF_HEIGHT;
                render_brace(&mut svg, PAGE_MARGIN_LEFT - 2.0, top_y, bottom_y);
            }
        }

        // Bracket spanning ALL staves in the system (if multiple parts)
        if system.parts.len() > 1 || system.total_staves > 1 {
            let first_part = system.parts.first().unwrap();
            let last_part = system.parts.last().unwrap();
            let top_y = system_y + first_part.y_offset;
            let bottom_y = system_y
                + last_part.y_offset
                + (last_part.num_staves as f64 - 1.0) * (STAFF_HEIGHT + GRAND_STAFF_GAP)
                + STAFF_HEIGHT;

            // Connecting barline at the left edge
            svg.line(
                PAGE_MARGIN_LEFT, top_y, PAGE_MARGIN_LEFT, bottom_y,
                BARLINE_COLOR, BARLINE_WIDTH,
            );
        }

        // ── Render measures ──
        for ml in &system.measures {
            let mx = ml.x;
            let mw = ml.width;

            for part_info in &system.parts {
                let pidx = part_info.part_idx;
                let part = &score.parts[pidx];
                let ps = &mut part_states[pidx];

                if ml.measure_idx >= part.measures.len() {
                    continue;
                }
                let measure = &part.measures[ml.measure_idx];

                // Update running attributes for this part
                if let Some(ref attrs) = measure.attributes {
                    for clef in &attrs.clefs {
                        let idx = clef.number as usize;
                        if idx < ps.clefs.len() {
                            ps.clefs[idx] = Some(clef.clone());
                        }
                    }
                    if let Some(ref k) = attrs.key {
                        ps.key = Some(k.clone());
                    }
                    if let Some(ref t) = attrs.time {
                        ps.time = Some(t.clone());
                    }
                    if let Some(d) = attrs.divisions {
                        ps.divisions = d;
                    }
                    if let Some(ref t) = attrs.transpose {
                        ps.transpose_octave = t.octave_change.unwrap_or(0);
                    }
                }

                for staff_num in 1..=part_info.num_staves {
                    let staff_y = system_y
                        + part_info.y_offset
                        + (staff_num as f64 - 1.0) * (STAFF_HEIGHT + GRAND_STAFF_GAP);

                    // Chord symbols (only on top staff of first part)
                    if staff_num == 1 && pidx == parts_staves[0].0 {
                        render_harmonies(&mut svg, measure, mx, mw, staff_y);
                    }

                    // Notes and rests for this staff
                    let staff_filter = if part_info.num_staves > 1 {
                        Some(staff_num as i32)
                    } else {
                        None
                    };

                    render_notes(
                        &mut svg,
                        measure,
                        staff_y,
                        ps.clefs[staff_num].as_ref(),
                        ps.divisions,
                        ps.transpose_octave,
                        staff_filter,
                        &ml.beat_x_map,
                    );

                    // Barlines (per-staff)
                    if staff_num == 1 {
                        render_barlines(&mut svg, measure, mx, mw, staff_y);
                    }
                }
            }

            // Right barline spanning all staves across all parts
            let first_part = system.parts.first().unwrap();
            let last_part = system.parts.last().unwrap();
            let top_y = system_y + first_part.y_offset;
            let bottom_y = system_y
                + last_part.y_offset
                + (last_part.num_staves as f64 - 1.0) * (STAFF_HEIGHT + GRAND_STAFF_GAP)
                + STAFF_HEIGHT;
            svg.line(mx + mw, top_y, mx + mw, bottom_y, BARLINE_COLOR, BARLINE_WIDTH);
        }
    }

    svg.build()
}

/// Detect the number of staves in a part by scanning for staves attribute
/// or staff numbers on notes.
fn detect_staves(part: &Part) -> usize {
    let mut max_staff = 1usize;
    for measure in &part.measures {
        if let Some(ref attrs) = measure.attributes {
            if let Some(s) = attrs.staves {
                max_staff = max_staff.max(s as usize);
            }
            for clef in &attrs.clefs {
                max_staff = max_staff.max(clef.number as usize);
            }
        }
        for note in &measure.notes {
            if let Some(s) = note.staff {
                max_staff = max_staff.max(s as usize);
            }
        }
    }
    max_staff
}

// ═══════════════════════════════════════════════════════════════════════
// Layout computation
// ═══════════════════════════════════════════════════════════════════════

fn compute_layout(score: &Score, parts_staves: &[(usize, usize)]) -> ScoreLayout {
    // parts_staves: Vec of (part_idx, num_staves) for each part

    let content_width = PAGE_WIDTH - PAGE_MARGIN_LEFT - PAGE_MARGIN_RIGHT;
    let mut systems: Vec<SystemLayout> = Vec::new();
    let mut current_y = FIRST_SYSTEM_TOP;

    // Use the first part for system grouping (measure count should be same across parts)
    let ref_part = &score.parts[parts_staves[0].0];

    // Group measures into systems based on new-system hints
    let mut system_groups: Vec<Vec<usize>> = Vec::new();
    let mut current_group: Vec<usize> = Vec::new();

    for (i, measure) in ref_part.measures.iter().enumerate() {
        if measure.new_system && !current_group.is_empty() {
            system_groups.push(current_group);
            current_group = Vec::new();
        }
        current_group.push(i);
    }
    if !current_group.is_empty() {
        system_groups.push(current_group);
    }

    // If no system breaks were found, auto-layout with ~4 measures per system
    if system_groups.len() <= 1 && ref_part.measures.len() > 4 {
        system_groups.clear();
        let measures_per_system = 4;
        for chunk in (0..ref_part.measures.len()).collect::<Vec<_>>().chunks(measures_per_system) {
            system_groups.push(chunk.to_vec());
        }
    }

    // Get initial key signature for width calculation (from any part)
    let initial_key = score.parts.iter()
        .flat_map(|p| p.measures.iter())
        .find_map(|m| m.attributes.as_ref().and_then(|a| a.key.as_ref()));

    // Total staves across all parts
    let total_staves: usize = parts_staves.iter().map(|&(_, ns)| ns).sum();

    // Track divisions per part (for beat mapping)
    let mut divisions_per_part: Vec<i32> = vec![1; score.parts.len()];

    for (sys_idx, group) in system_groups.iter().enumerate() {
        let is_first = sys_idx == 0;
        let prefix_width = CLEF_SPACE
            + key_sig_width(initial_key)
            + if is_first { TIME_SIG_SPACE } else { 0.0 };

        let x_start = PAGE_MARGIN_LEFT + prefix_width;
        let x_end = PAGE_MARGIN_LEFT + content_width;
        let available = x_end - x_start;

        // Distribute measure widths proportionally — take max weight across all parts
        let measure_weights: Vec<f64> = group
            .iter()
            .map(|&mi| {
                parts_staves
                    .iter()
                    .map(|&(pidx, _)| {
                        let part = &score.parts[pidx];
                        if mi < part.measures.len() {
                            let m = &part.measures[mi];
                            let note_count = m.notes.len().max(1) as f64;
                            (note_count * MIN_NOTE_SPACING).max(MIN_MEASURE_WIDTH)
                        } else {
                            MIN_MEASURE_WIDTH
                        }
                    })
                    .fold(0.0f64, f64::max)
            })
            .collect();

        let total_weight: f64 = measure_weights.iter().sum();
        let scale = if total_weight > 0.0 {
            available / total_weight
        } else {
            1.0
        };

        let mut measures = Vec::new();
        let mut x = x_start;

        for (j, &mi) in group.iter().enumerate() {
            let w = measure_weights[j] * scale;

            // Build the shared beat-to-x map for this measure across all parts
            let mut all_beat_times: Vec<Vec<f64>> = Vec::new();
            for &(pidx, _) in parts_staves {
                let part = &score.parts[pidx];
                // Update divisions from this measure's attributes
                if mi < part.measures.len() {
                    if let Some(ref attrs) = part.measures[mi].attributes {
                        if let Some(d) = attrs.divisions {
                            divisions_per_part[pidx] = d;
                        }
                    }
                    let beat_times = compute_note_beat_times(
                        &part.measures[mi].notes,
                        divisions_per_part[pidx],
                    );
                    all_beat_times.push(beat_times);
                }
            }

            let beat_x_map = compute_beat_x_map(&all_beat_times, x, w);

            measures.push(MeasureLayout {
                measure_idx: mi,
                x,
                width: w,
                beat_x_map,
            });
            x += w;
        }

        // Compute per-part vertical offsets within this system
        let mut parts_info: Vec<PartStaffInfo> = Vec::new();
        let mut y_offset = 0.0;

        for (i, &(pidx, num_staves)) in parts_staves.iter().enumerate() {
            parts_info.push(PartStaffInfo {
                part_idx: pidx,
                y_offset,
                num_staves,
            });

            // Height of this part's staves
            let part_height = STAFF_HEIGHT
                + (num_staves as f64 - 1.0) * (STAFF_HEIGHT + GRAND_STAFF_GAP);
            y_offset += part_height;

            // Gap between parts (not after the last)
            if i < parts_staves.len() - 1 {
                y_offset += PART_GAP;
            }
        }

        systems.push(SystemLayout {
            y: current_y,
            x_start,
            x_end,
            measures,
            parts: parts_info,
            show_clef: true,
            show_time: is_first,
            total_staves,
        });

        // Total height of this system
        let system_height = y_offset; // already computed above
        current_y += system_height + SYSTEM_SPACING;
    }

    let total_height = current_y + 40.0;

    ScoreLayout {
        systems,
        total_height,
    }
}

fn key_sig_width(key: Option<&Key>) -> f64 {
    match key {
        Some(k) => k.fifths.unsigned_abs() as f64 * KEY_SIG_SPACE,
        None => 0.0,
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Header rendering
// ═══════════════════════════════════════════════════════════════════════

fn render_header(svg: &mut SvgBuilder, score: &Score) {
    let center_x = PAGE_WIDTH / 2.0;

    // Title
    if let Some(ref title) = score.title {
        svg.text(center_x, PAGE_MARGIN_TOP + 22.0, title, 22.0, "bold",
                 HEADER_COLOR, "middle");
    }

    // Subtitle
    if let Some(ref subtitle) = score.subtitle {
        svg.text(center_x, PAGE_MARGIN_TOP + 40.0, subtitle, 14.0, "normal",
                 HEADER_COLOR, "middle");
    }

    // Composer (right-aligned)
    if let Some(ref composer) = score.composer {
        let label = if let Some(ref arranger) = score.arranger {
            format!("{}\nArr. {}", composer, arranger)
        } else {
            composer.clone()
        };
        svg.text(PAGE_WIDTH - PAGE_MARGIN_RIGHT, PAGE_MARGIN_TOP + 55.0,
                 &label, 11.0, "normal", HEADER_COLOR, "end");
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Staff rendering
// ═══════════════════════════════════════════════════════════════════════

fn render_brace(svg: &mut SvgBuilder, x: f64, top_y: f64, bottom_y: f64) {
    // Draw a grand-staff brace (curly bracket) using cubic Bézier curves.
    let mid_y = (top_y + bottom_y) / 2.0;
    let h = bottom_y - top_y;
    let w = BRACE_WIDTH;

    // Right half of the brace (two mirrored C-curves meeting at the midpoint)
    let path = format!(
        "M{:.1},{:.1} C{:.1},{:.1} {:.1},{:.1} {:.1},{:.1} \
         C{:.1},{:.1} {:.1},{:.1} {:.1},{:.1}",
        // Top point
        x, top_y,
        // Control points → midpoint (tip points left)
        x, top_y + h * 0.28,
        x - w, mid_y - h * 0.08,
        x - w, mid_y,
        // Midpoint → bottom
        x - w, mid_y + h * 0.08,
        x, bottom_y - h * 0.28,
        x, bottom_y,
    );
    svg.path(&path, "none", NOTE_COLOR, 2.5);
}

fn render_staff_lines(svg: &mut SvgBuilder, x1: f64, x2: f64, staff_y: f64) {
    for i in 0..5 {
        let y = staff_y + i as f64 * STAFF_LINE_SPACING;
        svg.line(x1, y, x2, y, STAFF_COLOR, STAFF_LINE_WIDTH);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Clef rendering
// ═══════════════════════════════════════════════════════════════════════

fn render_clef(svg: &mut SvgBuilder, x: f64, staff_y: f64, clef: &Clef) {
    match clef.sign.as_str() {
        "G" => {
            // Treble clef — draw a stylized G clef symbol
            let cy = staff_y + 30.0; // line 2 from bottom
            svg.treble_clef(x + 10.0, cy);
            // If octave change, show "8" below
            if clef.octave_change == Some(-1) {
                svg.text(x + 10.0, staff_y + STAFF_HEIGHT + 16.0,
                         "8", 9.0, "normal", STAFF_COLOR, "middle");
            }
        }
        "F" => {
            // Bass clef
            let cy = staff_y + 10.0; // line 4 from bottom
            svg.bass_clef(x + 10.0, cy);
        }
        "C" => {
            // Alto/tenor clef
            let line_y = staff_y + (5 - clef.line) as f64 * STAFF_LINE_SPACING;
            svg.alto_clef(x + 10.0, line_y);
        }
        _ => {}
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Key signature rendering
// ═══════════════════════════════════════════════════════════════════════

fn render_key_signature(
    svg: &mut SvgBuilder, x: f64, staff_y: f64,
    key: &Key, clef: Option<&Clef>,
) {
    if key.fifths == 0 {
        return;
    }

    let is_treble = clef.map_or(true, |c| c.sign == "G");

    if key.fifths > 0 {
        // Sharps: F C G D A E B
        let sharp_positions_treble: &[f64] = &[0.0, 15.0, -5.0, 10.0, 25.0, 5.0, 20.0];
        let positions = if is_treble { sharp_positions_treble } else {
            &[10.0, 25.0, 5.0, 20.0, 35.0, 15.0, 30.0] // bass clef
        };
        for i in 0..key.fifths.min(7) as usize {
            let sx = x + i as f64 * KEY_SIG_SPACE;
            let sy = staff_y + positions[i];
            svg.sharp_sign(sx, sy);
        }
    } else {
        // Flats: B E A D G C F
        let flat_positions_treble: &[f64] = &[20.0, 5.0, 25.0, 10.0, 30.0, 15.0, 35.0];
        let positions = if is_treble { flat_positions_treble } else {
            &[30.0, 15.0, 35.0, 20.0, 40.0, 25.0, 45.0] // bass clef
        };
        for i in 0..key.fifths.unsigned_abs().min(7) as usize {
            let sx = x + i as f64 * KEY_SIG_SPACE;
            let sy = staff_y + positions[i];
            svg.flat_sign(sx, sy);
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Time signature rendering
// ═══════════════════════════════════════════════════════════════════════

fn render_time_signature(svg: &mut SvgBuilder, x: f64, staff_y: f64, time: &TimeSignature) {
    let cx = x + 8.0;
    svg.text(cx, staff_y + 15.0, &time.beats.to_string(),
             16.0, "bold", NOTE_COLOR, "middle");
    svg.text(cx, staff_y + 35.0, &time.beat_type.to_string(),
             16.0, "bold", NOTE_COLOR, "middle");
}

// ═══════════════════════════════════════════════════════════════════════
// Note rendering
// ═══════════════════════════════════════════════════════════════════════

fn render_notes(
    svg: &mut SvgBuilder,
    measure: &Measure,
    staff_y: f64,
    clef: Option<&Clef>,
    divisions: i32,
    transpose_octave: i32,
    staff_filter: Option<i32>,
    beat_x_map: &[(f64, f64)],
) {
    if measure.notes.is_empty() {
        return;
    }

    // Position notes horizontally using the shared beat map
    let note_positions = note_x_positions_from_beat_map(&measure.notes, divisions, beat_x_map);

    // Collect beam groups for eighth notes and shorter
    let beam_groups = find_beam_groups(measure, staff_filter);

    for (i, note) in measure.notes.iter().enumerate() {
        // Filter by staff when rendering a multi-staff part
        if let Some(sf) = staff_filter {
            let note_staff = note.staff.unwrap_or(1);
            if note_staff != sf { continue; }
        }

        let nx = note_positions[i];

        if note.rest {
            render_rest(svg, nx, staff_y, note.note_type.as_deref());
            continue;
        }

        if let Some(ref pitch) = note.pitch {
            let note_y = staff_y + pitch_to_staff_y(pitch, clef, transpose_octave);

            // Ledger lines
            render_ledger_lines(svg, nx, note_y, staff_y);

            // Notehead
            let filled = is_filled_note(note.note_type.as_deref());
            let is_whole = note.note_type.as_deref() == Some("whole");
            svg.notehead(nx, note_y, filled, is_whole);

            // Dot
            if note.dot {
                svg.circle(nx + NOTEHEAD_RX + 4.0, note_y - 1.5, 1.8, NOTE_COLOR);
            }

            // Accidental
            if let Some(ref acc) = note.accidental {
                render_accidental(svg, nx - NOTEHEAD_RX - 8.0, note_y, acc);
            }

            // Stem (not for whole notes)
            if !is_whole {
                // Skip stem drawing for beamed notes — render_beam_group handles those.
                let in_beam = note.beams.iter().any(|b|
                    b.beam_type == "begin" || b.beam_type == "continue" || b.beam_type == "end");

                if !in_beam {
                    let stem_up = match note.stem.as_deref() {
                        Some("up") => true,
                        Some("down") => false,
                        _ => note_y >= staff_y + 20.0, // auto: on or below middle → up
                    };

                    let (sx, sy1, sy2) = if stem_up {
                        (nx + NOTEHEAD_RX - 1.0, note_y, note_y - STEM_LENGTH)
                    } else {
                        (nx - NOTEHEAD_RX + 1.0, note_y, note_y + STEM_LENGTH)
                    };
                    svg.line(sx, sy1, sx, sy2, NOTE_COLOR, STEM_WIDTH);

                    // Flags for unbeamed eighth notes and shorter
                    if let Some(ref nt) = note.note_type {
                        let flag_count = match nt.as_str() {
                            "eighth" => 1,
                            "16th" => 2,
                            "32nd" => 3,
                            _ => 0,
                        };
                        if flag_count > 0 {
                            render_flags(svg, sx, sy2, flag_count, stem_up);
                        }
                    }
                }
            }
        }
    }

    // Render beams
    for group in &beam_groups {
        render_beam_group(svg, measure, &note_positions, staff_y, clef, transpose_octave, group);
    }
}

/// Compute the beat-time offset for each note in a measure,
/// using per-voice time tracking to handle MusicXML backup semantics.
/// Notes in different voices (e.g. voice 1 on staff 1, voice 5 on staff 2)
/// each get their own time cursor, so backup elements are implicitly handled.
fn compute_note_beat_times(notes: &[Note], divisions: i32) -> Vec<f64> {
    use std::collections::HashMap;
    let mut voice_times: HashMap<i32, f64> = HashMap::new();
    let mut beat_times = Vec::with_capacity(notes.len());

    for note in notes {
        let voice = note.voice.unwrap_or(1);
        let current = voice_times.entry(voice).or_insert(0.0);

        if note.chord {
            // Chord notes share the same time as the previous note in this voice
            beat_times.push(*current);
        } else {
            beat_times.push(*current);
            let dur = note.duration as f64 / divisions.max(1) as f64;
            *current += dur;
        }
    }

    beat_times
}

/// Build a sorted beat-time → x-position mapping from note beat times across
/// all parts. This is the core of cross-staff/cross-part vertical alignment:
/// notes at the same beat position get the same x coordinate regardless of
/// which part or staff they belong to.
fn compute_beat_x_map(
    all_beat_times: &[Vec<f64>],
    mx: f64,
    mw: f64,
) -> Vec<(f64, f64)> {
    let padding = 8.0;
    let usable_width = mw - padding * 2.0;

    // Collect all unique beat times across all parts
    let mut unique_beats: Vec<f64> = Vec::new();
    for beats in all_beat_times {
        for &bt in beats {
            if !unique_beats.iter().any(|&u| (u - bt).abs() < 0.001) {
                unique_beats.push(bt);
            }
        }
    }
    unique_beats.sort_by(|a, b| a.partial_cmp(b).unwrap());

    if unique_beats.is_empty() {
        return vec![];
    }

    let max_beat = unique_beats.last().copied().unwrap_or(1.0).max(0.001);

    // Map to x positions proportionally within the measure
    unique_beats
        .iter()
        .map(|&bt| {
            let x = mx + padding + (bt / max_beat) * usable_width;
            (bt, x)
        })
        .collect()
}

/// Look up the x position for a given beat time in the beat map.
fn lookup_beat_x(beat_x_map: &[(f64, f64)], beat_time: f64) -> f64 {
    let mut best_x = beat_x_map.first().map_or(0.0, |b| b.1);
    let mut best_dist = f64::MAX;
    for &(bt, x) in beat_x_map {
        let dist = (bt - beat_time).abs();
        if dist < best_dist {
            best_dist = dist;
            best_x = x;
        }
    }
    best_x
}

/// Build a Vec<f64> of x positions for each note in a measure, using the beat map.
fn note_x_positions_from_beat_map(
    notes: &[Note],
    divisions: i32,
    beat_x_map: &[(f64, f64)],
) -> Vec<f64> {
    let beat_times = compute_note_beat_times(notes, divisions);
    beat_times
        .iter()
        .map(|&bt| lookup_beat_x(beat_x_map, bt))
        .collect()
}

fn pitch_to_staff_y(pitch: &Pitch, clef: Option<&Clef>, transpose_octave: i32) -> f64 {
    let step_index = match pitch.step.as_str() {
        "C" => 0, "D" => 1, "E" => 2, "F" => 3,
        "G" => 4, "A" => 5, "B" => 6, _ => 0,
    };

    // Apply transposition for display (written pitch = sounding + transpose)
    let display_octave = pitch.octave + transpose_octave;
    let note_position = display_octave * 7 + step_index;

    // Reference position depends on clef
    let (ref_position, ref_y) = match clef.map(|c| c.sign.as_str()) {
        Some("F") => {
            // F clef on line 4: F3 at line 4 from bottom (y=10 from top)
            let line = clef.map_or(4, |c| c.line);
            let y = (5 - line) as f64 * STAFF_LINE_SPACING;
            (3 * 7 + 3, y) // F3
        }
        Some("C") => {
            // C clef: middle C on the specified line
            let line = clef.map_or(3, |c| c.line);
            let y = (5 - line) as f64 * STAFF_LINE_SPACING;
            (4 * 7 + 0, y) // C4
        }
        _ => {
            // G clef on line 2: G4 at line 2 from bottom (y=30 from top)
            let line = clef.map_or(2, |c| c.line);
            let y = (5 - line) as f64 * STAFF_LINE_SPACING;
            (4 * 7 + 4, y) // G4
        }
    };

    let staff_steps = note_position - ref_position;
    ref_y - staff_steps as f64 * (STAFF_LINE_SPACING / 2.0)
}

fn is_filled_note(note_type: Option<&str>) -> bool {
    match note_type {
        Some("whole") | Some("half") => false,
        _ => true,
    }
}

fn render_ledger_lines(svg: &mut SvgBuilder, x: f64, note_y: f64, staff_y: f64) {
    let top = staff_y;
    let bottom = staff_y + STAFF_HEIGHT;

    // Ledger lines above staff
    if note_y < top {
        let mut y = top - STAFF_LINE_SPACING;
        while y >= note_y - 1.0 {
            svg.line(
                x - NOTEHEAD_RX - LEDGER_LINE_EXTEND,
                y,
                x + NOTEHEAD_RX + LEDGER_LINE_EXTEND,
                y,
                STAFF_COLOR, LEDGER_LINE_WIDTH,
            );
            y -= STAFF_LINE_SPACING;
        }
    }

    // Ledger lines below staff
    if note_y > bottom {
        let mut y = bottom + STAFF_LINE_SPACING;
        while y <= note_y + 1.0 {
            svg.line(
                x - NOTEHEAD_RX - LEDGER_LINE_EXTEND,
                y,
                x + NOTEHEAD_RX + LEDGER_LINE_EXTEND,
                y,
                STAFF_COLOR, LEDGER_LINE_WIDTH,
            );
            y += STAFF_LINE_SPACING;
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Rest rendering
// ═══════════════════════════════════════════════════════════════════════

fn render_rest(svg: &mut SvgBuilder, x: f64, staff_y: f64, note_type: Option<&str>) {
    // Rests are positioned on the staff using standard engraving positions:
    //   whole  — hangs from line 4 (staff_y + 10)
    //   half   — sits on line 3   (staff_y + 20)
    //   others — centred on the staff

    match note_type {
        Some("whole") => {
            // Rectangle hanging below line 4
            svg.rect(x - 7.0, staff_y + 10.0, 14.0, 5.0, REST_COLOR, "none", 0.0);
        }
        Some("half") => {
            // Rectangle sitting on top of line 3
            svg.rect(x - 7.0, staff_y + 15.0, 14.0, 5.0, REST_COLOR, "none", 0.0);
        }
        Some("quarter") => {
            // Authentic quarter rest from VexFlow Gonville font glyph v7c.
            // Glyph spans ~1014 font-units centred on origin; at VF_GLYPH_SCALE
            // ≈ 28.5 SVG units ≈ 2.85 staff-spaces.  Centred on the middle line.
            let path = vf_outline_to_svg(VF_QUARTER_REST, VF_GLYPH_SCALE);
            let gx = x - 4.0;           // centre horizontally
            let gy = staff_y + 20.0;     // origin at middle line
            svg.elements.push(format!(
                r#"<path d="{}" fill="{}" transform="translate({:.1},{:.1})"/>"#,
                path, REST_COLOR, gx, gy
            ));
        }
        Some("eighth") => {
            // Authentic eighth rest from VexFlow Gonville font glyph va5.
            let path = vf_outline_to_svg(VF_EIGHTH_REST, VF_GLYPH_SCALE);
            let gx = x - 5.0;
            let gy = staff_y + 20.0;
            svg.elements.push(format!(
                r#"<path d="{}" fill="{}" transform="translate({:.1},{:.1})"/>"#,
                path, REST_COLOR, gx, gy
            ));
        }
        Some("16th") => {
            // Authentic 16th rest from VexFlow Gonville font glyph v3c.
            let path = vf_outline_to_svg(VF_16TH_REST, VF_GLYPH_SCALE);
            let gx = x - 6.0;
            let gy = staff_y + 20.0;
            svg.elements.push(format!(
                r#"<path d="{}" fill="{}" transform="translate({:.1},{:.1})"/>"#,
                path, REST_COLOR, gx, gy
            ));
        }
        _ => {
            // Fallback: quarter rest glyph
            let path = vf_outline_to_svg(VF_QUARTER_REST, VF_GLYPH_SCALE);
            let gx = x - 4.0;
            let gy = staff_y + 20.0;
            svg.elements.push(format!(
                r#"<path d="{}" fill="{}" transform="translate({:.1},{:.1})"/>"#,
                path, REST_COLOR, gx, gy
            ));
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Accidental rendering
// ═══════════════════════════════════════════════════════════════════════

fn render_accidental(svg: &mut SvgBuilder, x: f64, y: f64, accidental: &str) {
    let symbol = match accidental {
        "sharp" => "♯",
        "flat" => "♭",
        "natural" => "♮",
        "double-sharp" => "𝄪",
        "flat-flat" => "𝄫",
        _ => return,
    };
    svg.text(x, y + 4.0, symbol, 14.0, "normal", NOTE_COLOR, "middle");
}

// ═══════════════════════════════════════════════════════════════════════
// Flag rendering
// ═══════════════════════════════════════════════════════════════════════

fn render_flags(svg: &mut SvgBuilder, stem_x: f64, stem_end_y: f64, count: usize, stem_up: bool) {
    // Flags are drawn as filled curved shapes (like a small banner).
    for i in 0..count {
        let gap = i as f64 * 8.0;

        if stem_up {
            // Flag hangs to the right and curves downward from the stem tip.
            let y0 = stem_end_y + gap;
            let path = format!(
                "M{:.1},{:.1} c0,0 1,2 8,6 c4,3 4,8 2,14 \
                 c-1,-4 -2,-7 -5,-9 c-3,-2 -5,-3 -5,-5 Z",
                stem_x, y0
            );
            svg.path(&path, NOTE_COLOR, "none", 0.0);
        } else {
            // Flag hangs to the right and curves upward from the stem tip.
            let y0 = stem_end_y - gap;
            let path = format!(
                "M{:.1},{:.1} c0,0 1,-2 8,-6 c4,-3 4,-8 2,-14 \
                 c-1,4 -2,7 -5,9 c-3,2 -5,3 -5,5 Z",
                stem_x, y0
            );
            svg.path(&path, NOTE_COLOR, "none", 0.0);
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Beam rendering
// ═══════════════════════════════════════════════════════════════════════

fn find_beam_groups(measure: &Measure, staff_filter: Option<i32>) -> Vec<Vec<usize>> {
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut current_group: Vec<usize> = Vec::new();

    for (i, note) in measure.notes.iter().enumerate() {
        if note.chord || note.rest {
            continue;
        }
        // Skip notes not on the filtered staff
        if let Some(sf) = staff_filter {
            if note.staff.unwrap_or(1) != sf { continue; }
        }
        let has_beam_begin = note.beams.iter().any(|b| b.number == 1 && b.beam_type == "begin");
        let has_beam_cont = note.beams.iter().any(|b| b.number == 1 && b.beam_type == "continue");
        let has_beam_end = note.beams.iter().any(|b| b.number == 1 && b.beam_type == "end");

        if has_beam_begin {
            current_group = vec![i];
        } else if has_beam_cont {
            current_group.push(i);
        } else if has_beam_end {
            current_group.push(i);
            if current_group.len() >= 2 {
                groups.push(current_group.clone());
            }
            current_group.clear();
        }
    }

    groups
}

fn render_beam_group(
    svg: &mut SvgBuilder,
    measure: &Measure,
    note_positions: &[f64],
    staff_y: f64,
    clef: Option<&Clef>,
    transpose_octave: i32,
    group: &[usize],
) {
    if group.len() < 2 {
        return;
    }

    // Collect note-head positions for the group
    struct BeamNote { x: f64, note_y: f64, stem_x: f64 }
    let mut notes: Vec<BeamNote> = Vec::new();

    for &idx in group {
        let note = &measure.notes[idx];
        let nx = note_positions[idx];
        if let Some(ref pitch) = note.pitch {
            let note_y = staff_y + pitch_to_staff_y(pitch, clef, transpose_octave);
            notes.push(BeamNote { x: nx, note_y, stem_x: 0.0 });
        }
    }
    if notes.len() < 2 { return; }

    // Determine stem direction from the average pitch position.
    // If average note_y > middle line → stem up (notes are low); else stem down.
    let avg_y: f64 = notes.iter().map(|n| n.note_y).sum::<f64>() / notes.len() as f64;
    let middle_line = staff_y + 20.0;

    // Use explicit XML stem direction from first note if available, else auto
    let first_note = &measure.notes[group[0]];
    let stem_up = match first_note.stem.as_deref() {
        Some("up") => true,
        Some("down") => false,
        _ => avg_y >= middle_line, // auto: notes below middle → stems up
    };

    // Set stem_x for each note
    for n in &mut notes {
        n.stem_x = if stem_up { n.x + NOTEHEAD_RX - 1.0 } else { n.x - NOTEHEAD_RX + 1.0 };
    }

    // Calculate beam line from first and last note stem ends.
    // Start with a default stem length, then optionally shorten.
    let first_stem_end = if stem_up { notes.first().unwrap().note_y - STEM_LENGTH }
                         else { notes.first().unwrap().note_y + STEM_LENGTH };
    let last_stem_end  = if stem_up { notes.last().unwrap().note_y - STEM_LENGTH }
                         else { notes.last().unwrap().note_y + STEM_LENGTH };

    let first_x = notes.first().unwrap().stem_x;
    let last_x  = notes.last().unwrap().stem_x;
    let beam_dx = last_x - first_x;

    // Beam line equation: beam_y(x) = first_stem_end + slope * (x - first_x)
    let slope = if beam_dx.abs() > 0.1 {
        ((last_stem_end - first_stem_end) / beam_dx).clamp(-0.5, 0.5) // limit slope
    } else { 0.0 };
    let beam_y = |sx: f64| first_stem_end + slope * (sx - first_x);

    // Ensure every stem is at least MIN_STEM (18 units) long.
    // If any stem would be too short, shift the entire beam line.
    let min_stem = 18.0;
    let mut beam_shift = 0.0_f64;
    for n in &notes {
        let by = beam_y(n.stem_x) + beam_shift;
        let stem_len = (n.note_y - by).abs();
        if stem_len < min_stem {
            let needed = min_stem - stem_len;
            if stem_up { beam_shift -= needed; } else { beam_shift += needed; }
        }
    }

    let beam_y_adj = |sx: f64| beam_y(sx) + beam_shift;

    // Draw stems: each stem goes from the notehead to the beam line
    for n in &notes {
        let by = beam_y_adj(n.stem_x);
        svg.line(n.stem_x, n.note_y, n.stem_x, by, NOTE_COLOR, STEM_WIDTH);
    }

    // Draw primary beam as filled polygon
    let by_first = beam_y_adj(first_x);
    let by_last  = beam_y_adj(last_x);
    svg.beam_line(first_x, by_first, last_x, by_last, BEAM_THICKNESS);

    // Secondary beams (16th notes — beam number 2)
    let has_16th = group.iter().any(|&idx| {
        measure.notes[idx].beams.iter().any(|b| b.number == 2)
    });
    if has_16th && notes.len() >= 2 {
        let offset = if stem_up { BEAM_THICKNESS + 3.0 } else { -(BEAM_THICKNESS + 3.0) };
        svg.beam_line(
            first_x, by_first + offset,
            last_x, by_last + offset,
            BEAM_THICKNESS,
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Harmony (chord symbol) rendering
// ═══════════════════════════════════════════════════════════════════════

fn render_harmonies(
    svg: &mut SvgBuilder, measure: &Measure,
    mx: f64, mw: f64, staff_y: f64,
) {
    if measure.harmonies.is_empty() {
        return;
    }

    let spacing = mw / (measure.harmonies.len() as f64 + 1.0);

    for (i, harmony) in measure.harmonies.iter().enumerate() {
        let x = mx + spacing * (i as f64 + 0.5);
        let y = staff_y + CHORD_SYMBOL_OFFSET_Y;

        let kind_str = match harmony.kind.as_str() {
            "major" => "",
            "minor" => "m",
            "dominant" => "7",
            "dominant-seventh" => "7",
            "major-seventh" => "maj7",
            "minor-seventh" => "m7",
            "diminished" => "dim",
            "augmented" => "aug",
            "half-diminished" => "m7♭5",
            other => other,
        };

        let alter_str = match harmony.root.alter {
            Some(a) if a > 0.0 => "♯",
            Some(a) if a < 0.0 => "♭",
            _ => "",
        };

        let label = format!("{}{}{}", harmony.root.step, alter_str, kind_str);
        svg.text(x, y, &label, 12.0, "bold", CHORD_COLOR, "start");
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Barline rendering
// ═══════════════════════════════════════════════════════════════════════

fn render_barlines(
    svg: &mut SvgBuilder, measure: &Measure,
    mx: f64, mw: f64, staff_y: f64,
) {
    for barline in &measure.barlines {
        let bx = match barline.location.as_str() {
            "left" => mx,
            "right" => mx + mw,
            _ => mx + mw,
        };

        match barline.bar_style.as_deref() {
            Some("heavy-light") => {
                // Forward repeat
                svg.line(bx, staff_y, bx, staff_y + STAFF_HEIGHT,
                         BARLINE_COLOR, 3.0);
                svg.line(bx + 5.0, staff_y, bx + 5.0, staff_y + STAFF_HEIGHT,
                         BARLINE_COLOR, BARLINE_WIDTH);
                // Dots
                svg.circle(bx + 10.0, staff_y + 15.0, 2.0, BARLINE_COLOR);
                svg.circle(bx + 10.0, staff_y + 25.0, 2.0, BARLINE_COLOR);
            }
            Some("light-heavy") => {
                // Backward repeat
                svg.circle(bx - 10.0, staff_y + 15.0, 2.0, BARLINE_COLOR);
                svg.circle(bx - 10.0, staff_y + 25.0, 2.0, BARLINE_COLOR);
                svg.line(bx - 5.0, staff_y, bx - 5.0, staff_y + STAFF_HEIGHT,
                         BARLINE_COLOR, BARLINE_WIDTH);
                svg.line(bx, staff_y, bx, staff_y + STAFF_HEIGHT,
                         BARLINE_COLOR, 3.0);
            }
            Some("light-light") => {
                svg.line(bx - 2.0, staff_y, bx - 2.0, staff_y + STAFF_HEIGHT,
                         BARLINE_COLOR, BARLINE_WIDTH);
                svg.line(bx + 2.0, staff_y, bx + 2.0, staff_y + STAFF_HEIGHT,
                         BARLINE_COLOR, BARLINE_WIDTH);
            }
            _ => {
                // Check if there's a repeat sign without specific bar style
                if let Some(ref repeat) = barline.repeat {
                    match repeat.direction.as_str() {
                        "forward" => {
                            svg.line(bx, staff_y, bx, staff_y + STAFF_HEIGHT,
                                     BARLINE_COLOR, 3.0);
                            svg.line(bx + 5.0, staff_y, bx + 5.0, staff_y + STAFF_HEIGHT,
                                     BARLINE_COLOR, BARLINE_WIDTH);
                            svg.circle(bx + 10.0, staff_y + 15.0, 2.0, BARLINE_COLOR);
                            svg.circle(bx + 10.0, staff_y + 25.0, 2.0, BARLINE_COLOR);
                        }
                        "backward" => {
                            svg.circle(bx - 10.0, staff_y + 15.0, 2.0, BARLINE_COLOR);
                            svg.circle(bx - 10.0, staff_y + 25.0, 2.0, BARLINE_COLOR);
                            svg.line(bx - 5.0, staff_y, bx - 5.0, staff_y + STAFF_HEIGHT,
                                     BARLINE_COLOR, BARLINE_WIDTH);
                            svg.line(bx, staff_y, bx, staff_y + STAFF_HEIGHT,
                                     BARLINE_COLOR, 3.0);
                        }
                        _ => {}
                    }
                }
            }
        }

        // Ending brackets (volta: 1st/2nd ending)
        if let Some(ref ending) = barline.ending {
            if ending.ending_type == "start" {
                let text = ending.text.as_deref()
                    .unwrap_or(&ending.number);
                svg.text(bx + 5.0, staff_y - 10.0,
                         &format!("{}.", text), 10.0, "normal", BARLINE_COLOR, "start");
                svg.line(bx, staff_y - 5.0, bx, staff_y - 15.0,
                         BARLINE_COLOR, BARLINE_WIDTH);
                svg.line(bx, staff_y - 15.0, bx + mw, staff_y - 15.0,
                         BARLINE_COLOR, BARLINE_WIDTH);
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Empty SVG fallback
// ═══════════════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════════════
// VexFlow font glyph support
// ═══════════════════════════════════════════════════════════════════════

/// Scale factor for VexFlow font glyphs: point * 72 / (resolution * 100).
/// Default notation font scale = 39, resolution = 1000.
const VF_GLYPH_SCALE: f64 = 39.0 * 72.0 / (1000.0 * 100.0); // 0.02808

// Glyph outlines from VexFlow's vexflow_font.js (Gonville font).
// Format: m x y | l x y | b endX endY cp1X cp1Y cp2X cp2Y
// Y-up coordinate system (negated for SVG).

const VF_QUARTER_REST: &str = "m 49 505 b 53 506 50 505 51 506 b 70 496 58 506 62 503 b 81 485 73 492 78 488 l 96 473 l 111 459 l 122 449 l 134 438 l 182 396 l 255 330 b 292 291 292 298 292 298 l 292 290 l 292 284 l 283 270 b 209 36 234 197 209 113 b 288 -170 209 -44 235 -119 b 299 -184 295 -179 299 -181 b 300 -191 300 -187 300 -188 b 285 -206 300 -199 294 -206 b 280 -206 283 -206 281 -206 b 247 -201 270 -202 259 -201 b 176 -222 223 -201 197 -208 b 114 -340 136 -249 114 -292 b 172 -471 114 -384 134 -433 b 185 -492 182 -481 185 -487 b 181 -502 185 -496 183 -499 b 171 -508 176 -505 174 -508 b 152 -498 166 -508 160 -503 b 0 -284 65 -428 12 -352 b 0 -260 0 -278 0 -270 b 1 -238 0 -252 0 -242 b 148 -140 16 -177 73 -140 b 209 -148 167 -140 189 -142 b 215 -149 212 -148 215 -149 b 215 -149 215 -149 215 -149 l 215 -149 b 201 -136 215 -148 209 -142 l 157 -97 l 96 -41 b 17 34 21 24 17 29 b 17 37 17 36 17 36 b 17 38 17 37 17 38 b 25 56 17 44 17 44 b 110 298 81 131 110 219 b 46 474 110 390 86 438 b 42 483 43 480 42 481 b 42 487 42 484 42 487 b 49 505 42 494 44 499";

const VF_EIGHTH_REST: &str = "m 88 302 b 103 303 93 302 98 303 b 202 224 149 303 191 270 b 205 199 204 216 205 208 b 178 129 205 173 196 147 l 175 126 l 182 127 b 307 249 236 142 284 190 b 313 259 308 254 311 258 b 329 267 317 265 323 267 b 349 247 340 267 349 259 b 201 -263 349 242 204 -258 b 182 -273 197 -270 190 -273 b 163 -260 174 -273 166 -269 b 161 -256 161 -259 161 -258 b 217 -59 161 -248 170 -220 b 272 129 247 43 272 127 b 272 129 272 129 272 129 b 264 122 272 129 268 126 b 140 80 227 94 183 80 b 36 115 102 80 65 91 b 0 194 10 136 0 165 b 88 302 0 244 32 292";

const VF_16TH_REST: &str = "m 189 302 b 204 303 193 302 198 303 b 303 224 250 303 292 270 b 306 199 304 216 306 208 b 279 129 306 173 296 147 l 276 126 l 281 127 b 408 249 337 142 385 190 b 412 259 409 254 412 258 b 430 267 417 265 423 267 b 450 247 441 267 450 259 b 200 -605 450 242 204 -599 b 182 -616 197 -612 190 -616 b 163 -602 174 -616 166 -610 b 161 -598 161 -601 161 -601 b 217 -402 161 -589 170 -562 b 272 -213 247 -298 272 -213 b 272 -213 272 -213 272 -213 b 264 -219 272 -213 268 -216 b 140 -262 227 -247 182 -262 b 36 -226 102 -262 65 -249 b 0 -145 12 -206 0 -176 b 17 -84 0 -124 5 -104 b 103 -38 38 -54 70 -38 b 191 -91 137 -38 172 -56 b 205 -141 201 -106 205 -124 b 178 -212 205 -167 196 -194 l 175 -215 l 182 -213 b 307 -93 236 -198 284 -151 b 372 129 308 -88 372 127 b 372 129 372 129 372 129 b 364 122 372 129 368 126 b 240 80 328 94 283 80 b 137 115 202 80 166 91 b 99 194 111 136 99 165 b 189 302 99 244 133 292";

/// Convert a VexFlow font outline string to a standard SVG path string.
/// VexFlow format: m x y | l x y | b endX endY cp1X cp1Y cp2X cp2Y
/// Y coords are negated (VexFlow font Y-up → SVG Y-down).
fn vf_outline_to_svg(outline: &str, scale: f64) -> String {
    let mut result = String::with_capacity(outline.len());
    let parts: Vec<&str> = outline.split_whitespace().collect();
    let mut i = 0;
    while i < parts.len() {
        match parts[i] {
            "m" if i + 2 < parts.len() => {
                let x: f64 = parts[i+1].parse().unwrap_or(0.0) * scale;
                let y: f64 = parts[i+2].parse().unwrap_or(0.0) * -scale;
                result.push_str(&format!("M{:.2},{:.2}", x, y));
                i += 3;
            }
            "l" if i + 2 < parts.len() => {
                let x: f64 = parts[i+1].parse().unwrap_or(0.0) * scale;
                let y: f64 = parts[i+2].parse().unwrap_or(0.0) * -scale;
                result.push_str(&format!("L{:.2},{:.2}", x, y));
                i += 3;
            }
            "b" if i + 6 < parts.len() => {
                // VexFlow: b endX endY cp1X cp1Y cp2X cp2Y
                // SVG:     C cp1X -cp1Y cp2X -cp2Y endX -endY
                let ex: f64 = parts[i+1].parse().unwrap_or(0.0) * scale;
                let ey: f64 = parts[i+2].parse().unwrap_or(0.0) * -scale;
                let c1x: f64 = parts[i+3].parse().unwrap_or(0.0) * scale;
                let c1y: f64 = parts[i+4].parse().unwrap_or(0.0) * -scale;
                let c2x: f64 = parts[i+5].parse().unwrap_or(0.0) * scale;
                let c2y: f64 = parts[i+6].parse().unwrap_or(0.0) * -scale;
                result.push_str(&format!("C{:.2},{:.2},{:.2},{:.2},{:.2},{:.2}",
                    c1x, c1y, c2x, c2y, ex, ey));
                i += 7;
            }
            _ => { i += 1; }
        }
    }
    result.push('Z');
    result
}

fn empty_svg(message: &str) -> String {
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 400 100\">\
         <text x=\"200\" y=\"50\" text-anchor=\"middle\" font-size=\"14\" fill=\"gray\">{}</text>\
         </svg>",
        message
    )
}

// ═══════════════════════════════════════════════════════════════════════
// SVG Builder
// ═══════════════════════════════════════════════════════════════════════

struct SvgBuilder {
    elements: Vec<String>,
    width: f64,
    height: f64,
}

impl SvgBuilder {
    fn new(width: f64, height: f64) -> Self {
        Self {
            elements: Vec::new(),
            width,
            height,
        }
    }

    fn build(self) -> String {
        let mut svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {} {}" width="{}" height="{}" style="font-family: 'Georgia', 'Times New Roman', serif;">"#,
            self.width, self.height, self.width, self.height
        );
        svg.push('\n');
        for el in &self.elements {
            svg.push_str("  ");
            svg.push_str(el);
            svg.push('\n');
        }
        svg.push_str("</svg>\n");
        svg
    }

    fn line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, color: &str, width: f64) {
        self.elements.push(format!(
            r#"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="{}" stroke-width="{:.1}" stroke-linecap="round"/>"#,
            x1, y1, x2, y2, color, width
        ));
    }

    fn rect(&mut self, x: f64, y: f64, w: f64, h: f64, fill: &str, stroke: &str, stroke_width: f64) {
        if stroke_width > 0.0 {
            self.elements.push(format!(
                r#"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" fill="{}" stroke="{}" stroke-width="{:.1}"/>"#,
                x, y, w, h, fill, stroke, stroke_width
            ));
        } else {
            self.elements.push(format!(
                r#"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" fill="{}"/>"#,
                x, y, w, h, fill
            ));
        }
    }

    fn circle(&mut self, cx: f64, cy: f64, r: f64, fill: &str) {
        self.elements.push(format!(
            r#"<circle cx="{:.1}" cy="{:.1}" r="{:.1}" fill="{}"/>"#,
            cx, cy, r, fill
        ));
    }

    fn text(&mut self, x: f64, y: f64, content: &str, size: f64, weight: &str, fill: &str, anchor: &str) {
        let escaped = content
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        self.elements.push(format!(
            r#"<text x="{:.1}" y="{:.1}" font-size="{:.0}" font-weight="{}" fill="{}" text-anchor="{}">{}</text>"#,
            x, y, size, weight, fill, anchor, escaped
        ));
    }

    fn path(&mut self, d: &str, fill: &str, stroke: &str, stroke_width: f64) {
        self.elements.push(format!(
            r#"<path d="{}" fill="{}" stroke="{}" stroke-width="{:.1}" stroke-linecap="round"/>"#,
            d, fill, stroke, stroke_width
        ));
    }

    fn notehead(&mut self, cx: f64, cy: f64, filled: bool, is_whole: bool) {
        let rx = if is_whole { NOTEHEAD_RX + 1.5 } else { NOTEHEAD_RX };
        let ry = NOTEHEAD_RY;
        let fill = if filled { NOTE_COLOR } else { "none" };
        let stroke = if filled { "none" } else { NOTE_COLOR };
        let sw = if filled { 0.0 } else { 1.5 };
        self.elements.push(format!(
            r#"<ellipse cx="{:.1}" cy="{:.1}" rx="{:.1}" ry="{:.1}" fill="{}" stroke="{}" stroke-width="{:.1}" transform="rotate(-15,{:.1},{:.1})"/>"#,
            cx, cy, rx, ry, fill, stroke, sw, cx, cy
        ));
    }

    fn beam_line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, thickness: f64) {
        let half = thickness / 2.0;
        // Calculate perpendicular offset for beam thickness
        let dx = x2 - x1;
        let dy = y2 - y1;
        let len = (dx * dx + dy * dy).sqrt().max(0.1);
        let nx = -dy / len * half;
        let ny = dx / len * half;

        let path = format!(
            "M{:.1},{:.1} L{:.1},{:.1} L{:.1},{:.1} L{:.1},{:.1} Z",
            x1 + nx, y1 + ny,
            x2 + nx, y2 + ny,
            x2 - nx, y2 - ny,
            x1 - nx, y1 - ny,
        );
        self.elements.push(format!(
            r#"<path d="{}" fill="{}"/>"#,
            path, NOTE_COLOR
        ));
    }

    fn treble_clef(&mut self, x: f64, y: f64) {
        // Professional treble clef (G clef) using path data from
        // treble-clef-svgrepo-com.svg (two filled outlines).
        //
        // Original viewBox: 0 0 276.164 276.164
        // G line in original coords ≈ y=148; center-x ≈ 138.
        // Scaled by 0.27 and translated so the G line lands at (x, y).

        let scale = 0.243;             // 10% smaller than 0.27
        let tx = x - 138.0 * scale;  // ≈ x - 33.5
        let ty = y - 148.0 * scale - 4.0;  // shifted up 4px

        // Upper body — S-curve, hook, and vertical stroke outline
        let p1 = "M156.716,61.478c-4.111,6.276-8.881,11.511-14.212,15.609\
l-8.728,6.962c-13.339,11.855-22.937,21.433-28.542,28.464\
c-10.209,12.788-15.806,25.779-16.65,38.611c-0.942,14.473,3.187,28.21,12.275,40.84\
c9.636,13.458,21.8,20.754,36.164,21.69c3.291,0.218,6.897,0.182,9.896-0.015\
l-1.121-10.104c-2.09,0.192-4.306,0.223-6.628,0.068\
c-9.437-0.617-17.864-4.511-25.064-11.573c-7.524-7.333-10.895-15.415-10.287-24.7\
c1.149-17.59,12.562-35.004,33.925-51.792l9.543-7.599\
c8.394-7.174,15.192-16.191,20.216-26.825c4.971-10.556,7.886-21.983,8.673-33.96\
c0.466-7.037-0.513-15.775-2.874-25.965c-3.241-13.839-7.854-20.765-14.136-21.179\
c-2.232-0.138-4.676,0.986-7.658,3.617c-7.252,6.548-12.523,14.481-15.683,23.542\
c-2.438,6.926-4.057,16.189-4.805,27.529c-0.313,4.72,0.313,13.438,1.805,23.962\
l8.844-8.192c-0.028-1.183,0.005-2.413,0.096-3.703\
c0.466-7.221,2.289-15.062,5.394-23.293c3.956-10.296,7.689-13.409,10.133-14.204\
c0.668-0.218,1.32-0.298,2.015-0.254c3.185,0.212,6.358,1.559,5.815,9.979\
C164.664,46.132,161.831,53.693,156.716,61.478z";

        // Lower body — loop around G line, vertical stroke, and bottom ornament
        let p2 = "M164.55,209.161c5.728-2.568,10.621-6.478,14.576-11.651\
c5.055-6.561,7.897-14.316,8.467-23.047c0.72-10.719-1.854-20.438-7.617-28.895\
c-6.322-9.264-14.98-14.317-25.745-15.026c-1.232-0.081-2.543-0.075-3.895,0.025\
l-2.304-17.191l-9.668,7.112l1.483,12.194\
c-5.789,2.393-10.827,6.17-15.017,11.255c-4.823,5.924-7.508,12.443-7.964,19.382\
c-0.466,7.208,1.142,13.81,4.782,19.583c1.895,3.081,4.507,5.82,7.498,8.058\
c4.906,3.65,10.563,3.376,11.459,1.393c0.906-1.983-2.455-5.095-5.09-9.248\
c-1.502-2.351-2.242-5.173-2.242-8.497c0-7.053,4.256-13.116,10.317-15.799\
l5.673,44.211l1.325,10.258c0.864,4.873,1.719,9.725,2.537,14.52\
c1,6.488,1.352,12.112,1.041,16.715c-0.419,6.375-2.408,11.584-5.919,15.493\
c-2.234,2.485-4.844,4.055-7.795,4.925c3.961-3.962,6.414-9.43,6.414-15.478\
c0-12.075-9.792-21.872-21.87-21.872c-3.353,0-6.491,0.812-9.329,2.159\
c-0.36,0.155-0.699,0.388-1.054,0.574c-0.779,0.425-1.559,0.85-2.286,1.362\
c-0.249,0.187-0.487,0.403-0.732,0.605c-4.888,3.816-8.091,9.616-8.375,16.229\
c0,0.01-0.011,0.021-0.011,0.031c0,0.005,0,0.01,0,0.016\
c-0.013,0.311-0.09,0.59-0.09,0.896c0,0.259,0.067,0.492,0.078,0.74\
c-0.011,7.084,2.933,13.179,8.839,18.118c5.584,4.666,12.277,7.28,19.892,7.777\
c4.327,0.28,8.505-0.217,12.407-1.485c3.189-1.041,6.275-2.62,9.149-4.687\
c6.96-5.022,10.75-11.584,11.272-19.532c0.399-6.063,0.094-13.235-0.937-21.411\
l-2.838-18.429l-7.156-52.899c7.984,1.532,14.027,8.543,14.027,16.968\
c0,5.986-1.937,15.431-5.551,20.376L164.55,209.161z";

        self.elements.push(format!(
            r#"<g transform="translate({:.2},{:.2}) scale({})"><path d="{}" fill="{}"/><path d="{}" fill="{}"/></g>"#,
            tx, ty, scale, p1, NOTE_COLOR, p2, NOTE_COLOR
        ));
    }

    fn bass_clef(&mut self, x: f64, y: f64) {
        // Professional bass clef (F clef) using path data from
        // bass-clef-svgrepo-com.svg (three filled shapes).
        //
        // Original viewBox: 0 0 512 512
        // F line sits at y_orig ≈ 169 (midpoint between the two dot centres).
        // Dot centre spacing ≈ 167 units → maps to 1 staff space (10 units).
        // Scale 0.06 keeps the dots at the correct staff-space interval.

        let scale = 0.06;
        let tx = x - 176.0 * scale - 2.0;   // body start ≈ x, shifted left a bit
        let ty = y - 169.0 * scale;          // F line lands at y

        // Main body curve
        let p1 = "M176.014,0l-2.823,0.01\
C89.091,1.164,20.78,63.557,15.904,118.564\
c-3.125,35.072,4.693,63.941,22.568,83.494\
c16.307,17.803,39.765,26.836,69.727,26.836\
c31.095,0,61.603-29.77,61.603-60.106\
c0-30.803-25.076-55.869-55.888-55.869\
c-16.569,0-27.575,7.323-34.858,12.179\
c-2.853,1.892-5.796,3.854-7.121,3.854\
c-0.446,0-1.477-1.184-2.458-5.635\
c-3.399-15.335,1.902-33.644,14.212-48.98\
c10.399-12.978,34.858-34.726,81.876-34.726\
c65.67,0,101.833,52.894,101.833,148.952\
c0,192.852-165.703,271.845-216.483,291.459\
c-10.398,4.016-13.778,12.716-12.492,19.553\
C39.828,507.002,45.947,512,53.686,512\
c2.448,0,5.037-0.496,7.657-1.477l5.807-2.165\
C262.916,435.82,362.19,326.247,362.19,182.648\
C362.19,57.164,265.688,0,176.014,0z";

        // Upper dot (4th space, above F line)
        let p2 = "M455.486,126.84\
c22.771,0,41.282-18.522,41.282-41.292\
c0-22.76-18.512-41.271-41.282-41.271\
c-22.759,0-41.281,18.511-41.281,41.271\
C414.205,108.318,432.726,126.84,455.486,126.84z";

        // Lower dot (3rd space, below F line)
        let p3 = "M455.486,211.365\
c-22.759,0-41.281,18.522-41.281,41.282\
c0,22.77,18.522,41.281,41.281,41.281\
c22.771,0,41.282-18.511,41.282-41.281\
C496.768,229.887,478.256,211.365,455.486,211.365z";

        self.elements.push(format!(
            r#"<g transform="translate({:.2},{:.2}) scale({})"><path d="{}" fill="{}"/><path d="{}" fill="{}"/><path d="{}" fill="{}"/></g>"#,
            tx, ty, scale, p1, NOTE_COLOR, p2, NOTE_COLOR, p3, NOTE_COLOR
        ));
    }

    fn alto_clef(&mut self, _x: f64, y: f64) {
        // Simplified C clef - two vertical bars and a bracket
        let x = _x;
        self.rect(x - 2.0, y - 20.0, 3.0, 80.0, NOTE_COLOR, "none", 0.0);
        self.rect(x + 4.0, y - 20.0, 1.5, 80.0, NOTE_COLOR, "none", 0.0);
    }

    fn sharp_sign(&mut self, x: f64, y: f64) {
        // ♯ drawn with two vertical and two horizontal lines
        svg_sharp(&mut self.elements, x, y);
    }

    fn flat_sign(&mut self, x: f64, y: f64) {
        svg_flat(&mut self.elements, x, y);
    }
}

fn svg_sharp(elements: &mut Vec<String>, x: f64, y: f64) {
    // Two vertical lines
    elements.push(format!(
        r#"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="{}" stroke-width="1.2"/>"#,
        x - 1.5, y - 6.0, x - 1.5, y + 6.0, NOTE_COLOR
    ));
    elements.push(format!(
        r#"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="{}" stroke-width="1.2"/>"#,
        x + 1.5, y - 6.0, x + 1.5, y + 6.0, NOTE_COLOR
    ));
    // Two slanted horizontal lines
    elements.push(format!(
        r#"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="{}" stroke-width="1.8"/>"#,
        x - 4.0, y - 1.5, x + 4.0, y - 3.5, NOTE_COLOR
    ));
    elements.push(format!(
        r#"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="{}" stroke-width="1.8"/>"#,
        x - 4.0, y + 3.5, x + 4.0, y + 1.5, NOTE_COLOR
    ));
}

fn svg_flat(elements: &mut Vec<String>, x: f64, y: f64) {
    // Vertical line + curved bump
    elements.push(format!(
        r#"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="{}" stroke-width="1.2"/>"#,
        x, y - 10.0, x, y + 3.0, NOTE_COLOR
    ));
    elements.push(format!(
        r#"<path d="M{:.1},{:.1} c4,-3 8,-2 6,2 c-2,4 -6,4 -6,1" fill="none" stroke="{}" stroke-width="1.2"/>"#,
        x, y - 1.0, NOTE_COLOR
    ));
}
