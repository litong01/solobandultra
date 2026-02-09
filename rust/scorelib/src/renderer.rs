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

struct SystemLayout {
    y: f64, // Y of top staff line
    #[allow(dead_code)]
    x_start: f64, // content start (after clef/key/time)
    x_end: f64,
    measures: Vec<MeasureLayout>,
    show_clef: bool,
    show_time: bool,
}

struct MeasureLayout {
    measure_idx: usize,
    x: f64,
    width: f64,
}

// ═══════════════════════════════════════════════════════════════════════
// Public API
// ═══════════════════════════════════════════════════════════════════════

/// Render a parsed Score into a complete SVG string.
pub fn render_score_to_svg(score: &Score) -> String {
    if score.parts.is_empty() {
        return empty_svg("No parts in score");
    }

    let part = &score.parts[0]; // Render first part for now
    let layout = compute_layout(score, part);

    let mut svg = SvgBuilder::new(PAGE_WIDTH, layout.total_height);

    // Background
    svg.rect(0.0, 0.0, PAGE_WIDTH, layout.total_height, "white", "none", 0.0);

    // Title and composer
    render_header(&mut svg, score);

    // Get running attributes (clef, key, time) that carry across measures
    let mut current_clef: Option<&Clef> = None;
    let mut current_key: Option<&Key> = None;
    let mut current_time: Option<&TimeSignature> = None;
    let mut current_divisions: i32 = 1;
    let mut transpose_octave: i32 = 0;

    // Pre-scan for initial attributes
    for measure in &part.measures {
        if let Some(ref attrs) = measure.attributes {
            if attrs.clef.is_some() {
                current_clef = attrs.clef.as_ref();
            }
            if attrs.key.is_some() {
                current_key = attrs.key.as_ref();
            }
            if attrs.time.is_some() {
                current_time = attrs.time.as_ref();
            }
            if let Some(d) = attrs.divisions {
                current_divisions = d;
            }
            if let Some(ref t) = attrs.transpose {
                transpose_octave = t.octave_change.unwrap_or(0);
            }
            break; // only need first measure's attributes for initialization
        }
    }

    // Render each system
    for system in &layout.systems {
        let staff_y = system.y;

        // Staff lines
        render_staff_lines(&mut svg, PAGE_MARGIN_LEFT, system.x_end, staff_y);

        // Clef
        if system.show_clef {
            if let Some(clef) = current_clef {
                render_clef(&mut svg, PAGE_MARGIN_LEFT + 5.0, staff_y, clef);
            }
        }

        // Key signature (at start of each system)
        let key_x = PAGE_MARGIN_LEFT + CLEF_SPACE;
        if let Some(key) = current_key {
            render_key_signature(&mut svg, key_x, staff_y, key, current_clef);
        }

        // Time signature (only on first system or when it changes)
        if system.show_time {
            if let Some(time) = current_time {
                let time_x = key_x + key_sig_width(current_key);
                render_time_signature(&mut svg, time_x, staff_y, time);
            }
        }

        // Render measures
        for ml in &system.measures {
            let measure = &part.measures[ml.measure_idx];
            let mx = ml.x;
            let mw = ml.width;

            // Update running attributes
            if let Some(ref attrs) = measure.attributes {
                if let Some(ref c) = attrs.clef {
                    current_clef = Some(c);
                }
                if let Some(ref k) = attrs.key {
                    current_key = Some(k);
                }
                if let Some(ref t) = attrs.time {
                    current_time = Some(t);
                }
                if let Some(d) = attrs.divisions {
                    current_divisions = d;
                }
                if let Some(ref t) = attrs.transpose {
                    transpose_octave = t.octave_change.unwrap_or(0);
                }
            }

            // Chord symbols
            render_harmonies(&mut svg, measure, mx, mw, staff_y);

            // Notes and rests
            render_notes(
                &mut svg,
                measure,
                mx,
                mw,
                staff_y,
                current_clef,
                current_divisions,
                transpose_octave,
            );

            // Barlines
            render_barlines(&mut svg, measure, mx, mw, staff_y);

            // Right barline (regular)
            svg.line(
                mx + mw, staff_y,
                mx + mw, staff_y + STAFF_HEIGHT,
                BARLINE_COLOR, BARLINE_WIDTH,
            );
        }
    }

    svg.build()
}

// ═══════════════════════════════════════════════════════════════════════
// Layout computation
// ═══════════════════════════════════════════════════════════════════════

fn compute_layout(_score: &Score, part: &Part) -> ScoreLayout {
    let content_width = PAGE_WIDTH - PAGE_MARGIN_LEFT - PAGE_MARGIN_RIGHT;
    let mut systems: Vec<SystemLayout> = Vec::new();
    let mut current_y = FIRST_SYSTEM_TOP;

    // Group measures into systems based on new-system hints
    let mut system_groups: Vec<Vec<usize>> = Vec::new();
    let mut current_group: Vec<usize> = Vec::new();

    for (i, measure) in part.measures.iter().enumerate() {
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
    if system_groups.len() <= 1 && part.measures.len() > 4 {
        system_groups.clear();
        let measures_per_system = 4;
        for chunk in (0..part.measures.len()).collect::<Vec<_>>().chunks(measures_per_system) {
            system_groups.push(chunk.to_vec());
        }
    }

    // Get initial key signature for width calculation
    let initial_key = part.measures.iter()
        .find_map(|m| m.attributes.as_ref().and_then(|a| a.key.as_ref()));

    for (sys_idx, group) in system_groups.iter().enumerate() {
        let is_first = sys_idx == 0;
        let prefix_width = CLEF_SPACE
            + key_sig_width(initial_key)
            + if is_first { TIME_SIG_SPACE } else { 0.0 };

        let x_start = PAGE_MARGIN_LEFT + prefix_width;
        let x_end = PAGE_MARGIN_LEFT + content_width;
        let available = x_end - x_start;

        // Distribute measure widths proportionally
        let measure_weights: Vec<f64> = group
            .iter()
            .map(|&i| {
                let m = &part.measures[i];
                let note_count = m.notes.len().max(1) as f64;
                (note_count * MIN_NOTE_SPACING).max(MIN_MEASURE_WIDTH)
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
            measures.push(MeasureLayout {
                measure_idx: mi,
                x,
                width: w,
            });
            x += w;
        }

        systems.push(SystemLayout {
            y: current_y,
            x_start,
            x_end,
            measures,
            show_clef: true,
            show_time: is_first,
        });

        current_y += STAFF_HEIGHT + SYSTEM_SPACING;
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
    mx: f64, mw: f64, staff_y: f64,
    clef: Option<&Clef>,
    divisions: i32,
    transpose_octave: i32,
) {
    if measure.notes.is_empty() {
        return;
    }

    // Position notes horizontally based on their order and duration
    let note_positions = compute_note_x_positions(measure, mx, mw, divisions);

    // Collect beam groups for eighth notes and shorter
    let beam_groups = find_beam_groups(measure);

    for (i, note) in measure.notes.iter().enumerate() {
        let nx = note_positions[i];

        if note.rest {
            let rest_y = staff_y + 15.0; // center of staff
            render_rest(svg, nx, rest_y, note.note_type.as_deref());
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
                let stem_up = note.stem.as_deref() != Some("down");
                let stem_up = if note.stem.is_none() {
                    note_y > staff_y + 20.0 // above middle line → stem up
                } else {
                    stem_up
                };

                let (sx, sy1, sy2) = if stem_up {
                    (nx + NOTEHEAD_RX - 1.0, note_y, note_y - STEM_LENGTH)
                } else {
                    (nx - NOTEHEAD_RX + 1.0, note_y, note_y + STEM_LENGTH)
                };
                svg.line(sx, sy1, sx, sy2, NOTE_COLOR, STEM_WIDTH);

                // Flag for unbeamed eighth notes and shorter
                let in_beam = note.beams.iter().any(|b| b.beam_type == "begin" || b.beam_type == "continue" || b.beam_type == "end");
                if !in_beam {
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

fn compute_note_x_positions(
    measure: &Measure, mx: f64, mw: f64, divisions: i32,
) -> Vec<f64> {
    let notes = &measure.notes;
    if notes.is_empty() {
        return Vec::new();
    }

    let padding = 8.0;
    let usable_width = mw - padding * 2.0;

    // Compute time offset for each note
    let mut time_offsets: Vec<f64> = Vec::new();
    let mut current_time = 0.0;

    for note in notes {
        if note.chord {
            // Chord notes share the same position as the previous note
            time_offsets.push(current_time);
        } else {
            time_offsets.push(current_time);
            let dur = note.duration as f64 / divisions.max(1) as f64;
            current_time += dur;
        }
    }

    let total_time = current_time.max(0.001);

    // Map time offsets to x positions
    notes.iter().enumerate().map(|(i, _)| {
        mx + padding + (time_offsets[i] / total_time) * usable_width
    }).collect()
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

fn render_rest(svg: &mut SvgBuilder, x: f64, y: f64, note_type: Option<&str>) {
    match note_type {
        Some("whole") => {
            // Hanging rectangle below line 4
            svg.rect(x - 6.0, y - 4.0, 12.0, 5.0, REST_COLOR, "none", 0.0);
        }
        Some("half") => {
            // Sitting rectangle on line 3
            svg.rect(x - 6.0, y + 1.0, 12.0, 5.0, REST_COLOR, "none", 0.0);
        }
        Some("quarter") => {
            // Simplified quarter rest (zigzag)
            let path = format!(
                "M{},{} l3,-6 l-6,6 l3,3 l6,-6 l-3,-3 l-3,6",
                x - 2.0, y - 5.0
            );
            svg.path(&path, REST_COLOR, "none", 1.5);
        }
        Some("eighth") => {
            // Simplified eighth rest
            svg.circle(x + 2.0, y - 3.0, 2.0, REST_COLOR);
            svg.line(x + 2.0, y - 3.0, x - 2.0, y + 8.0, REST_COLOR, 1.5);
        }
        Some("16th") => {
            svg.circle(x + 2.0, y - 3.0, 2.0, REST_COLOR);
            svg.circle(x + 3.0, y + 3.0, 2.0, REST_COLOR);
            svg.line(x + 2.0, y - 3.0, x - 3.0, y + 12.0, REST_COLOR, 1.5);
        }
        _ => {
            // Default: quarter rest symbol
            svg.text(x, y + 5.0, "𝄾", 16.0, "normal", REST_COLOR, "middle");
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
    for i in 0..count {
        let offset = i as f64 * 8.0;
        let (y_start, curve_dir) = if stem_up {
            (stem_end_y + offset, 1.0)
        } else {
            (stem_end_y - offset, -1.0)
        };

        let path = format!(
            "M{},{} q8,{} 4,{}",
            stem_x, y_start,
            6.0 * curve_dir, 14.0 * curve_dir
        );
        svg.path(&path, "none", NOTE_COLOR, 1.5);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Beam rendering
// ═══════════════════════════════════════════════════════════════════════

fn find_beam_groups(measure: &Measure) -> Vec<Vec<usize>> {
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut current_group: Vec<usize> = Vec::new();

    for (i, note) in measure.notes.iter().enumerate() {
        if note.chord || note.rest {
            continue;
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

    // Determine stem direction from first note
    let first_note = &measure.notes[group[0]];
    let stem_up = first_note.stem.as_deref() != Some("down");

    // Calculate beam endpoints
    let mut beam_points: Vec<(f64, f64)> = Vec::new();

    for &idx in group {
        let note = &measure.notes[idx];
        let nx = note_positions[idx];
        if let Some(ref pitch) = note.pitch {
            let note_y = staff_y + pitch_to_staff_y(pitch, clef, transpose_octave);
            let stem_x = if stem_up { nx + NOTEHEAD_RX - 1.0 } else { nx - NOTEHEAD_RX + 1.0 };
            let stem_end = if stem_up { note_y - STEM_LENGTH } else { note_y + STEM_LENGTH };
            beam_points.push((stem_x, stem_end));
        }
    }

    if beam_points.len() < 2 {
        return;
    }

    // Draw beam as a thick line between first and last stem ends
    let first = beam_points.first().unwrap();
    let last = beam_points.last().unwrap();

    // Primary beam
    let beam_y_offset = if stem_up { 0.0 } else { 0.0 };
    svg.beam_line(
        first.0, first.1 + beam_y_offset,
        last.0, last.1 + beam_y_offset,
        BEAM_THICKNESS,
    );

    // Check for secondary beams (16th notes)
    let has_secondary = group.iter().any(|&idx| {
        measure.notes[idx].beams.iter().any(|b| b.number == 2)
    });

    if has_secondary && beam_points.len() >= 2 {
        let offset = if stem_up { BEAM_THICKNESS + 3.0 } else { -(BEAM_THICKNESS + 3.0) };
        svg.beam_line(
            first.0, first.1 + offset,
            last.0, last.1 + offset,
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
        // Simplified treble clef using SVG paths
        let path = format!(
            "M{},{} \
             c-2,-8 4,-16 4,-24 \
             c0,-8 -6,-12 -8,-8 \
             c-2,4 4,8 6,16 \
             c2,8 0,20 -4,28 \
             c-4,8 -2,16 4,16 \
             c6,0 6,-8 2,-12",
            x, y
        );
        self.elements.push(format!(
            r#"<path d="{}" fill="none" stroke="{}" stroke-width="2" stroke-linecap="round"/>"#,
            path, NOTE_COLOR
        ));
        // Dot at the curl
        self.circle(x + 1.0, y + 14.0, 2.5, NOTE_COLOR);
    }

    fn bass_clef(&mut self, x: f64, y: f64) {
        // Simplified bass clef
        self.circle(x, y, 4.0, NOTE_COLOR);
        let path = format!(
            "M{},{} c8,2 12,8 12,16 c0,8 -4,14 -10,16",
            x + 4.0, y - 2.0
        );
        self.elements.push(format!(
            r#"<path d="{}" fill="none" stroke="{}" stroke-width="2" stroke-linecap="round"/>"#,
            path, NOTE_COLOR
        ));
        self.circle(x + 16.0, y + 4.0, 2.0, NOTE_COLOR);
        self.circle(x + 16.0, y + 14.0, 2.0, NOTE_COLOR);
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
