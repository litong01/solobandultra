//! Score renderer — converts a parsed Score into SVG output.
//!
//! The renderer computes its own layout from the musical content (pitch,
//! duration, time signature) and produces a self-contained SVG string
//! that can be displayed in any SVG-capable view.

use crate::model::*;

// ═══════════════════════════════════════════════════════════════════════
// Constants (all in SVG user units)
// ═══════════════════════════════════════════════════════════════════════

const DEFAULT_PAGE_WIDTH: f64 = 820.0;
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
const KEY_SIG_SHARP_SPACE: f64 = 10.0; // per sharp in key signature (ha=331 * scale + gap)
const KEY_SIG_FLAT_SPACE: f64 = 8.0; // per flat in key signature (ha=257 * scale + gap)
const KEY_SIG_NATURAL_SPACE: f64 = 8.0; // per natural in key cancellation
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

const MIN_MEASURE_WIDTH: f64 = 38.0;
const PER_BEAT_MIN_WIDTH: f64 = 55.0; // minimum width per quarter-note beat for packing
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

#[allow(dead_code)]
struct MeasureLayout {
    measure_idx: usize,
    x: f64,
    width: f64,
    beat_x_map: Vec<(f64, f64)>, // (beat_time, x_position)
    has_key_change: bool,
    has_time_change: bool,
    prev_key_fifths: Option<i32>,  // previous key (for cancellation naturals)
    left_inset: f64,  // extra left padding for inline key/time changes
    right_inset: f64, // extra right padding for repeat barlines
}

// ═══════════════════════════════════════════════════════════════════════
// Public API
// ═══════════════════════════════════════════════════════════════════════

/// Render a parsed Score into a complete SVG string.
///
/// `page_width` sets the SVG width in user units. Pass `None` (or 0.0 from FFI)
/// to use the default (820). On phones, pass the screen width in points so the
/// renderer fits fewer measures per system and keeps notes readable.
pub fn render_score_to_svg(score: &Score, page_width: Option<f64>) -> String {
    let page_width = match page_width {
        Some(w) if w > 0.0 => w,
        _ => DEFAULT_PAGE_WIDTH,
    };

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

    let layout = compute_layout(score, &parts_staves, page_width);

    let mut svg = SvgBuilder::new(page_width, layout.total_height);

    // Background
    svg.rect(0.0, 0.0, page_width, layout.total_height, "white", "none", 0.0);

    // Title and composer
    render_header(&mut svg, score, page_width);

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

        // Pre-update part states from the first measure of this system so the
        // system prefix (clef, key, time) reflects changes that occur at the
        // system boundary rather than lagging one system behind.
        if let Some(first_ml) = system.measures.first() {
            for part_info in &system.parts {
                let pidx = part_info.part_idx;
                let part = &score.parts[pidx];
                if first_ml.measure_idx < part.measures.len() {
                    let measure = &part.measures[first_ml.measure_idx];
                    if let Some(ref attrs) = measure.attributes {
                        let ps = &mut part_states[pidx];
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
                }
            }
        }

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

        // ── Measure number at the start of each system line ──
        // Placed before the clef, above the top staff.  Measure 1 is omitted
        // (standard engraving convention — the first measure is self-evident).
        if let Some(first_ml) = system.measures.first() {
            let measure_num = first_ml.measure_idx + 1; // 1-based display
            if measure_num > 1 {
                let first_part = system.parts.first().unwrap();
                let top_staff_y = system_y + first_part.y_offset;
                let color = "#555555";
                svg.elements.push(format!(
                    "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"15\" font-style=\"italic\" fill=\"{}\" text-anchor=\"start\">{}</text>",
                    PAGE_MARGIN_LEFT - 10.0, top_staff_y - 8.0, color, measure_num
                ));
            }
        }

        // ── Pre-compute lyrics baseline for this system ──
        // Find the lowest note/stem y across all measures in this system so
        // lyrics sit below everything.  We compute per part/staff.
        let mut system_lowest_y: f64 = system_y + STAFF_HEIGHT; // at least bottom staff line
        for ml_pre in &system.measures {
            for part_info in &system.parts {
                let pidx = part_info.part_idx;
                let ps = &part_states[pidx];
                if ml_pre.measure_idx >= score.parts[pidx].measures.len() {
                    continue;
                }
                let measure = &score.parts[pidx].measures[ml_pre.measure_idx];
                // Only check the bottom staff of each part (lyrics go below it)
                let bottom_staff_num = part_info.num_staves;
                let staff_y_bottom = system_y
                    + part_info.y_offset
                    + (bottom_staff_num as f64 - 1.0) * (STAFF_HEIGHT + GRAND_STAFF_GAP);
                let staff_filter = if part_info.num_staves > 1 {
                    Some(bottom_staff_num as i32)
                } else {
                    None
                };
                let lowest = measure_lowest_note_y(
                    measure, staff_y_bottom,
                    ps.clefs.get(bottom_staff_num).and_then(|c| c.as_ref()),
                    ps.transpose_octave, staff_filter,
                );
                if lowest > system_lowest_y {
                    system_lowest_y = lowest;
                }
            }
        }
        let lyrics_base_y = (system_lowest_y + LYRICS_PAD_BELOW)
            .max(system_y + LYRICS_MIN_Y_BELOW_STAFF);

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

                    // ── Inline key/time signature changes ──
                    // Render at the start of the measure (after barline) when a
                    // mid-system change occurs (system-start changes are handled
                    // by the system prefix rendering above).
                    // Start inline_x after any left-side barline decorations
                    // (forward repeats extend ~12px from mx).
                    let mut inline_x = mx + 10.0;
                    for barline in &measure.barlines {
                        if barline.location == "left" {
                            let is_repeat = barline.repeat.is_some();
                            let is_heavy = barline.bar_style.as_deref() == Some("heavy-light")
                                || barline.bar_style.as_deref() == Some("light-heavy");
                            if is_repeat || is_heavy {
                                // Forward repeat: heavy bar + thin bar + dots at mx+10 (r=2)
                                inline_x = inline_x.max(mx + 14.0);
                            }
                        }
                    }
                    if ml.has_key_change {
                        // Cancellation naturals for the old key (only when needed)
                        if let Some(prev_fifths) = ml.prev_key_fifths {
                            let new_fifths = ps.key.as_ref().map_or(0, |k| k.fifths);
                            let num_naturals = cancellation_natural_count(prev_fifths, new_fifths) as usize;
                            if num_naturals > 0 {
                                let positions = if prev_fifths > 0 {
                                    sharp_positions(ps.clefs[staff_num].as_ref())
                                } else {
                                    flat_positions(ps.clefs[staff_num].as_ref())
                                };
                                for i in 0..num_naturals.min(positions.len()) {
                                    let ny = staff_y + positions[i] as f64 * 5.0;
                                    render_natural_sign(&mut svg, inline_x, ny);
                                    inline_x += KEY_SIG_NATURAL_SPACE;
                                }
                                inline_x += 2.0; // small gap before new key
                            }
                        }
                        // New key signature
                        if let Some(ref key) = ps.key {
                            render_key_signature(
                                &mut svg, inline_x, staff_y, key,
                                ps.clefs[staff_num].as_ref(),
                            );
                            inline_x += key_sig_width(Some(key)) + 4.0;
                        }
                    }
                    if ml.has_time_change {
                        if let Some(ref time) = ps.time {
                            render_time_signature(&mut svg, inline_x, staff_y, time);
                        }
                    }

                    // ── Tempo / metronome markings (only on top staff of first part) ──
                    if staff_num == 1 && pidx == parts_staves[0].0 {
                        for dir in &measure.directions {
                            if dir.sound_tempo.is_some() || dir.metronome.is_some() {
                                render_tempo_marking(&mut svg, mx + 4.0, staff_y, dir);
                            }
                        }
                    }

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
                        mx, mw,
                    );

                    // Barlines (per-staff)
                    if staff_num == 1 {
                        render_barlines(&mut svg, measure, mx, mw, staff_y);
                    }

                    // Lyrics (render on bottom staff only)
                    if staff_num == part_info.num_staves {
                        let note_xs = note_x_positions_from_beat_map(
                            &measure.notes, ps.divisions, &ml.beat_x_map,
                        );
                        render_lyrics(
                            &mut svg, measure, &note_xs,
                            lyrics_base_y, staff_filter,
                        );
                    }
                }
            }

            // Right barline spanning all staves across all parts.
            // Skip the default thin barline if the measure already has a
            // special right-side barline (light-light, light-heavy, etc.)
            // rendered by render_barlines().
            let has_special_right_barline = system.parts.first().map_or(false, |pi| {
                let pidx = pi.part_idx;
                if ml.measure_idx < score.parts[pidx].measures.len() {
                    score.parts[pidx].measures[ml.measure_idx].barlines.iter().any(|b| {
                        let is_right = b.location == "right" || b.location.is_empty();
                        is_right && b.bar_style.is_some()
                    })
                } else {
                    false
                }
            });

            if !has_special_right_barline {
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

fn compute_layout(score: &Score, parts_staves: &[(usize, usize)], page_width: f64) -> ScoreLayout {
    // parts_staves: Vec of (part_idx, num_staves) for each part

    let content_width = page_width - PAGE_MARGIN_LEFT - PAGE_MARGIN_RIGHT;
    let mut systems: Vec<SystemLayout> = Vec::new();

    // If the score has a composer/arranger AND chord symbols in the first few
    // measures, push the first system down so they don't overlap.
    let has_composer = score.composer.is_some() || score.arranger.is_some();
    let has_early_chords = score.parts.iter().any(|p| {
        p.measures.iter().take(8).any(|m| !m.harmonies.is_empty())
    });
    let first_system_top = if has_composer && has_early_chords {
        FIRST_SYSTEM_TOP + 18.0 // extra line for chord symbols above staff
    } else {
        FIRST_SYSTEM_TOP
    };
    let mut current_y = first_system_top;

    // Use the first part for system grouping (measure count should be same across parts)
    let ref_part = &score.parts[parts_staves[0].0];

    // OSMD-inspired layout: use musical time (beats from time signature) to
    // determine measure widths. Measures with the same time signature get
    // equal width. This is standard in professional engraving.

    // Scan for time signatures to compute beat duration per measure
    // (in quarter-note equivalents: e.g. 2/4 → 2.0, 3/4 → 3.0, 6/8 → 3.0)
    // Also track running key and time per measure for change detection.
    let mut measure_beats: Vec<f64> = Vec::with_capacity(ref_part.measures.len());
    let mut running_keys: Vec<Option<Key>> = Vec::with_capacity(ref_part.measures.len());
    let mut running_times: Vec<Option<TimeSignature>> = Vec::with_capacity(ref_part.measures.len());
    let mut has_key_change: Vec<bool> = Vec::with_capacity(ref_part.measures.len());
    let mut has_time_change: Vec<bool> = Vec::with_capacity(ref_part.measures.len());

    let mut current_beats = 4.0; // default 4/4
    let mut current_key: Option<Key> = None;
    let mut current_time: Option<TimeSignature> = None;

    for measure in &ref_part.measures {
        let mut key_changed = false;
        let mut time_changed = false;

        if let Some(ref attrs) = measure.attributes {
            if let Some(ref ts) = attrs.time {
                let new_beats = ts.beats as f64 * 4.0 / ts.beat_type as f64;
                // Detect time signature change (not just first assignment)
                if current_time.as_ref().map_or(false, |ct| ct.beats != ts.beats || ct.beat_type != ts.beat_type) {
                    time_changed = true;
                }
                current_beats = new_beats;
                current_time = Some(ts.clone());
            }
            if let Some(ref k) = attrs.key {
                if current_key.as_ref().map_or(false, |ck| ck.fifths != k.fifths) {
                    key_changed = true;
                }
                current_key = Some(k.clone());
            }
        }

        running_keys.push(current_key.clone());
        running_times.push(current_time.clone());
        has_key_change.push(key_changed);
        has_time_change.push(time_changed);

        // Implicit (pickup) measures: scale by actual content vs full bar
        if measure.implicit {
            measure_beats.push((current_beats * 0.5).max(1.0));
        } else {
            measure_beats.push(current_beats);
        }
    }

    // Track divisions per part for lyrics-aware width computation.
    // This mirrors the divisions tracking in the system rendering loop.
    let mut lyrics_divs: Vec<i32> = vec![1; score.parts.len()];

    // Compute the minimum packing width for each measure based on beat duration,
    // plus extra space for mid-piece key/time signature changes.
    // The extra space is computed dynamically based on the actual number of
    // accidentals involved (e.g. 6 flats needs much more space than 1 sharp).
    // Additionally, lyrics text widths can push measures wider so that syllables
    // don't overlap.
    let measure_min_widths: Vec<f64> = measure_beats
        .iter()
        .enumerate()
        .map(|(mi, &beats)| {
            // Update divisions for each part from this measure's attributes
            for (pidx, part) in score.parts.iter().enumerate() {
                if mi < part.measures.len() {
                    if let Some(ref attrs) = part.measures[mi].attributes {
                        if let Some(d) = attrs.divisions {
                            lyrics_divs[pidx] = d;
                        }
                    }
                }
            }

            let mut w = (beats * PER_BEAT_MIN_WIDTH).max(MIN_MEASURE_WIDTH);
            if has_key_change[mi] {
                // Cancellation naturals (only when direction changes or count decreases)
                let old_fifths = if mi > 0 {
                    running_keys[mi - 1].as_ref().map_or(0, |k| k.fifths)
                } else { 0 };
                let new_fifths = running_keys[mi].as_ref().map_or(0, |k| k.fifths);
                let num_cancel = cancellation_natural_count(old_fifths, new_fifths);
                if num_cancel > 0 {
                    w += num_cancel as f64 * KEY_SIG_NATURAL_SPACE + 4.0;
                }
                // New key signature
                let new_width = running_keys[mi].as_ref().map_or(0.0, |k| key_sig_width(Some(k))) + 4.0;
                w += new_width;
            }
            if has_time_change[mi] { w += TIME_SIG_SPACE; }

            // Lyrics-based minimum: ensure the measure is wide enough that
            // lyric syllables don't overlap horizontally.
            // The elongation is capped at MAX_LYRICS_ELONGATION_FACTOR × beat
            // width (OSMD-inspired) to prevent absurdly wide measures.
            let lyrics_w = lyrics_min_measure_width(&score.parts, mi, &lyrics_divs, w);
            if lyrics_w > w {
                w = lyrics_w;
            }

            w
        })
        .collect();

    // Pre-compute prefix widths.  Use the current key at each system boundary
    // rather than the initial key, so systems with more accidentals get more space.
    let initial_key = score.parts.iter()
        .flat_map(|p| p.measures.iter())
        .find_map(|m| m.attributes.as_ref().and_then(|a| a.key.as_ref()));
    let first_prefix = CLEF_SPACE + key_sig_width(initial_key) + TIME_SIG_SPACE;
    let available_first = content_width - first_prefix;

    let mut system_groups: Vec<Vec<usize>> = Vec::new();
    let mut current_group: Vec<usize> = Vec::new();
    let mut current_width = 0.0;
    let mut is_first_system = true;

    for (mi, &min_w) in measure_min_widths.iter().enumerate() {
        // Compute prefix for current system start based on the key active at this point
        let key_at_mi = running_keys[mi].as_ref();
        let later_prefix = CLEF_SPACE + key_sig_width(key_at_mi);
        let available_later = content_width - later_prefix;
        let available = if is_first_system { available_first } else { available_later };

        if !current_group.is_empty() && current_width + min_w > available {
            system_groups.push(current_group);
            current_group = Vec::new();
            current_width = 0.0;
            is_first_system = false;
        }
        current_group.push(mi);
        current_width += min_w;
    }
    if !current_group.is_empty() {
        system_groups.push(current_group);
    }

    // Total staves across all parts
    let total_staves: usize = parts_staves.iter().map(|&(_, ns)| ns).sum();

    // Track divisions per part (for beat mapping)
    let mut divisions_per_part: Vec<i32> = vec![1; score.parts.len()];

    for (sys_idx, group) in system_groups.iter().enumerate() {
        let is_first = sys_idx == 0;
        let first_mi = group[0];
        let key_at_start = running_keys[first_mi].as_ref();
        // For the first system always show time; for later systems, show time if
        // the first measure of the system has a time change
        let show_time_sig = is_first || has_time_change[first_mi];
        let prefix_width = CLEF_SPACE
            + key_sig_width(key_at_start)
            + if show_time_sig { TIME_SIG_SPACE } else { 0.0 };

        let x_start = PAGE_MARGIN_LEFT + prefix_width;
        let x_end = PAGE_MARGIN_LEFT + content_width;
        let available = x_end - x_start;

        // Distribute measure widths proportionally by beat duration (from
        // time signature). Measures with the same time signature get equal
        // width — standard in professional music engraving (OSMD approach).
        let measure_weights: Vec<f64> = group
            .iter()
            .map(|&mi| measure_beats[mi])
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

            // Compute previous key fifths for cancellation naturals
            let prev_key_fifths = if has_key_change[mi] && mi > 0 {
                running_keys[mi - 1].as_ref().map(|k| k.fifths)
            } else {
                None
            };

            // For measures that aren't at a system start, include their change flags
            // so the renderer can draw inline key/time signatures.  For the first
            // measure of a system, the system prefix already handles clef/key/time.
            let is_system_start = j == 0;
            let ml_has_key_change = has_key_change[mi] && !is_system_start;
            let ml_has_time_change = has_time_change[mi] && !is_system_start;

            // Compute left inset: space consumed by inline key/time changes
            let mut left_inset = 14.0; // default left padding
            if ml_has_key_change {
                // Space for cancellation naturals (only when needed)
                if let Some(pf) = prev_key_fifths {
                    let new_fifths = running_keys[mi].as_ref().map_or(0, |k| k.fifths);
                    let num_cancel = cancellation_natural_count(pf, new_fifths);
                    if num_cancel > 0 {
                        left_inset += num_cancel as f64 * KEY_SIG_NATURAL_SPACE + 2.0;
                    }
                }
                // Space for new key signature
                if let Some(ref k) = running_keys[mi] {
                    left_inset += key_sig_width(Some(k)) + 4.0;
                }
            }
            if ml_has_time_change {
                left_inset += TIME_SIG_SPACE;
            }

            // Compute right inset: extra space for repeat barlines.
            // A backward repeat extends ~12px left of the right edge (dots at
            // bx-10 with r=2), so we need ≥30 to leave a clear gap from notes.
            let mut right_inset: f64 = 14.0; // default right padding
            if mi < ref_part.measures.len() {
                let measure = &ref_part.measures[mi];
                for barline in &measure.barlines {
                    let is_right = barline.location == "right"
                        || barline.location.is_empty();
                    let is_left = barline.location == "left";
                    let has_repeat = barline.repeat.is_some();
                    let is_heavy = barline.bar_style.as_deref() == Some("light-heavy")
                        || barline.bar_style.as_deref() == Some("heavy-light");

                    // Right-side backward repeats / double bars
                    if is_right && (has_repeat || is_heavy) {
                        right_inset = right_inset.max(30.0);
                    }
                    // Left-side forward repeats push notes right
                    if is_left && (has_repeat || is_heavy) {
                        left_inset = left_inset.max(28.0);
                    }
                }
            }

            // Compute per-beat lyric events for lyrics-aware spacing (OSMD-style)
            let lyric_evts = collect_lyric_events(&score.parts, mi, &divisions_per_part);
            let beat_x_map = compute_beat_x_map(&all_beat_times, x, w, left_inset, right_inset, &lyric_evts);

            measures.push(MeasureLayout {
                measure_idx: mi,
                x,
                width: w,
                beat_x_map,
                has_key_change: ml_has_key_change,
                has_time_change: ml_has_time_change,
                prev_key_fifths,
                left_inset,
                right_inset,
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

        // Check if any measures in this system have lyrics; if so add extra
        // vertical space below the system for lyric text lines.
        let mut max_lyric_verses = 0i32;
        for ml_check in &measures {
            for &(pidx, _) in parts_staves {
                if ml_check.measure_idx < score.parts[pidx].measures.len() {
                    for note in &score.parts[pidx].measures[ml_check.measure_idx].notes {
                        for lyric in &note.lyrics {
                            max_lyric_verses = max_lyric_verses.max(lyric.number);
                        }
                    }
                }
            }
        }
        let lyrics_extra = if max_lyric_verses > 0 {
            LYRICS_MIN_Y_BELOW_STAFF - STAFF_HEIGHT + max_lyric_verses as f64 * LYRICS_LINE_HEIGHT
        } else {
            0.0
        };

        systems.push(SystemLayout {
            y: current_y,
            x_start,
            x_end,
            measures,
            parts: parts_info,
            show_clef: true,
            show_time: show_time_sig,
            total_staves,
        });

        // Total height of this system
        let system_height = y_offset; // already computed above
        current_y += system_height + lyrics_extra + SYSTEM_SPACING;
    }

    let total_height = current_y + 40.0;

    ScoreLayout {
        systems,
        total_height,
    }
}

fn key_sig_width(key: Option<&Key>) -> f64 {
    match key {
        Some(k) if k.fifths > 0 => k.fifths as f64 * KEY_SIG_SHARP_SPACE,
        Some(k) if k.fifths < 0 => k.fifths.unsigned_abs() as f64 * KEY_SIG_FLAT_SPACE,
        _ => 0.0,
    }
}

/// Determine how many cancellation naturals to show when the key changes.
///
/// Cancellation naturals are needed when:
///   - The direction changes (sharps→flats or flats→sharps)
///   - The key moves to natural (C major / A minor)
///   - The number of accidentals decreases in the same direction
///     (e.g. 6 flats → 1 flat: cancel the 5 removed flats)
///
/// NOT needed when:
///   - Same direction and increasing (e.g. 1 flat → 6 flats:
///     the new key already contains all old accidentals)
fn cancellation_natural_count(old_fifths: i32, new_fifths: i32) -> u32 {
    if old_fifths == 0 {
        return 0; // nothing to cancel
    }
    let same_direction = (old_fifths > 0 && new_fifths > 0)
        || (old_fifths < 0 && new_fifths < 0);

    if same_direction {
        let old_abs = old_fifths.unsigned_abs();
        let new_abs = new_fifths.unsigned_abs();
        if new_abs >= old_abs {
            0 // increasing in same direction → no cancellation
        } else {
            old_abs - new_abs // cancel the removed accidentals
        }
    } else {
        // Direction change or to natural: cancel ALL old accidentals
        old_fifths.unsigned_abs()
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Header rendering
// ═══════════════════════════════════════════════════════════════════════

fn render_header(svg: &mut SvgBuilder, score: &Score, page_width: f64) {
    let center_x = page_width / 2.0;

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
        svg.text(page_width - PAGE_MARGIN_RIGHT, PAGE_MARGIN_TOP + 55.0,
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
        // Sharps: F C G D A E B — line positions matching OSMD
        // Treble: [0, 1.5, -0.5, 1, 2.5, 0.5, 2] * STAFF_LINE_SPACING
        let positions_treble: &[f64] = &[0.0, 15.0, -5.0, 10.0, 25.0, 5.0, 20.0];
        let positions_bass: &[f64]   = &[10.0, 25.0, 5.0, 20.0, 35.0, 15.0, 30.0];
        let positions = if is_treble { positions_treble } else { positions_bass };
        for i in 0..key.fifths.min(7) as usize {
            let sx = x + i as f64 * KEY_SIG_SHARP_SPACE;
            let sy = staff_y + positions[i];
            svg.sharp_glyph(sx, sy);
        }
    } else {
        // Flats: B E A D G C F — line positions matching OSMD
        // Treble: [2, 0.5, 2.5, 1, 3, 1.5, 3.5] * STAFF_LINE_SPACING
        let positions_treble: &[f64] = &[20.0, 5.0, 25.0, 10.0, 30.0, 15.0, 35.0];
        let positions_bass: &[f64]   = &[30.0, 15.0, 35.0, 20.0, 40.0, 25.0, 45.0];
        let positions = if is_treble { positions_treble } else { positions_bass };
        for i in 0..key.fifths.unsigned_abs().min(7) as usize {
            let sx = x + i as f64 * KEY_SIG_FLAT_SPACE;
            let sy = staff_y + positions[i];
            svg.flat_glyph(sx, sy);
        }
    }
}

/// Return staff-line positions (in half-space units) for sharp key signatures.
fn sharp_positions(clef: Option<&Clef>) -> Vec<i32> {
    let is_treble = clef.map_or(true, |c| c.sign == "G");
    if is_treble {
        vec![0, 3, -1, 2, 5, 1, 4] // F C G D A E B on treble
    } else {
        vec![2, 5, 1, 4, 7, 3, 6] // bass clef positions
    }
}

/// Return staff-line positions (in half-space units) for flat key signatures.
fn flat_positions(clef: Option<&Clef>) -> Vec<i32> {
    let is_treble = clef.map_or(true, |c| c.sign == "G");
    if is_treble {
        vec![4, 1, 5, 2, 6, 3, 7] // B E A D G C F on treble
    } else {
        vec![6, 3, 7, 4, 8, 5, 9] // bass clef positions
    }
}

/// Render a natural sign at the given position using VexFlow glyph.
fn render_natural_sign(svg: &mut SvgBuilder, x: f64, y: f64) {
    svg.natural_glyph(x, y);
}

/// Render a tempo/metronome marking above the staff.
/// Draws the note symbol as an SVG notehead+stem rather than Unicode (which
/// most fonts lack), followed by " = BPM" as text.
fn render_tempo_marking(svg: &mut SvgBuilder, x: f64, staff_y: f64, dir: &Direction) {
    let ty = staff_y - 16.0; // baseline for the "= 120" text

    let (bpm, beat_unit, dotted) = if let Some(ref metro) = dir.metronome {
        (metro.per_minute as f64, metro.beat_unit.as_str(), metro.dotted)
    } else if let Some(tempo) = dir.sound_tempo {
        (tempo, "quarter", false)
    } else {
        return;
    };

    // Draw the beat-unit note symbol
    #[allow(unused_assignments)]
    let mut note_end_x = x;
    match beat_unit {
        "quarter" => {
            // Filled notehead
            svg.elements.push(format!(
                "<ellipse cx=\"{:.1}\" cy=\"{:.1}\" rx=\"3.8\" ry=\"2.8\" fill=\"{}\" stroke=\"none\" transform=\"rotate(-15,{:.1},{:.1})\"/>",
                x + 3.8, ty, NOTE_COLOR, x + 3.8, ty
            ));
            // Stem
            svg.line(x + 7.0, ty, x + 7.0, ty - 16.0, NOTE_COLOR, 1.0);
            note_end_x = x + 10.0;
        }
        "half" => {
            // Open notehead
            svg.elements.push(format!(
                "<ellipse cx=\"{:.1}\" cy=\"{:.1}\" rx=\"3.8\" ry=\"2.8\" fill=\"none\" stroke=\"{}\" stroke-width=\"1.2\" transform=\"rotate(-15,{:.1},{:.1})\"/>",
                x + 3.8, ty, NOTE_COLOR, x + 3.8, ty
            ));
            svg.line(x + 7.0, ty, x + 7.0, ty - 16.0, NOTE_COLOR, 1.0);
            note_end_x = x + 10.0;
        }
        "eighth" => {
            // Filled notehead + stem + flag
            svg.elements.push(format!(
                "<ellipse cx=\"{:.1}\" cy=\"{:.1}\" rx=\"3.8\" ry=\"2.8\" fill=\"{}\" stroke=\"none\" transform=\"rotate(-15,{:.1},{:.1})\"/>",
                x + 3.8, ty, NOTE_COLOR, x + 3.8, ty
            ));
            svg.line(x + 7.0, ty, x + 7.0, ty - 16.0, NOTE_COLOR, 1.0);
            // Small flag
            svg.elements.push(format!(
                "<path d=\"M{:.1},{:.1} c 0.7,1.4 2.8,3.5 5,7 c 1.4,2.1 1.4,4.2 -0.7,5.6\" fill=\"none\" stroke=\"{}\" stroke-width=\"1.0\"/>",
                x + 7.0, ty - 16.0, NOTE_COLOR
            ));
            note_end_x = x + 14.0;
        }
        "whole" => {
            svg.elements.push(format!(
                "<ellipse cx=\"{:.1}\" cy=\"{:.1}\" rx=\"4.5\" ry=\"3.0\" fill=\"none\" stroke=\"{}\" stroke-width=\"1.5\" transform=\"rotate(-15,{:.1},{:.1})\"/>",
                x + 4.5, ty, NOTE_COLOR, x + 4.5, ty
            ));
            note_end_x = x + 11.0;
        }
        _ => {
            // Fallback: quarter note
            svg.elements.push(format!(
                "<ellipse cx=\"{:.1}\" cy=\"{:.1}\" rx=\"3.8\" ry=\"2.8\" fill=\"{}\" stroke=\"none\" transform=\"rotate(-15,{:.1},{:.1})\"/>",
                x + 3.8, ty, NOTE_COLOR, x + 3.8, ty
            ));
            svg.line(x + 7.0, ty, x + 7.0, ty - 16.0, NOTE_COLOR, 1.0);
            note_end_x = x + 10.0;
        }
    }

    // Dot (for dotted beat units)
    if dotted {
        svg.elements.push(format!(
            "<circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"1.2\" fill=\"{}\"/>",
            note_end_x, ty - 1.0, NOTE_COLOR
        ));
        note_end_x += 3.0;
    }

    // " = 120" text
    let text = format!(" = {}", bpm as i32);
    svg.text(note_end_x, ty + 4.0, &text, 12.0, "bold", NOTE_COLOR, "start");
}

// ═══════════════════════════════════════════════════════════════════════
// Time signature rendering
// ═══════════════════════════════════════════════════════════════════════

fn render_time_signature(svg: &mut SvgBuilder, x: f64, staff_y: f64, time: &TimeSignature) {
    // OSMD approach: render numerator/denominator using VexFlow font glyphs.
    // Numerator rendered at staff line 2 (middle line), denominator at line 4 (bottom).
    // Each digit glyph spans ~2 staff spaces, filling the top/bottom half of the staff.
    // Multi-digit numbers are centered horizontally relative to each other.

    let s = TIMESIG_GLYPH_SCALE;
    let top_y = staff_y + 2.0 * STAFF_LINE_SPACING; // line 2 = middle line
    let bot_y = staff_y + 4.0 * STAFF_LINE_SPACING; // line 4 = bottom line

    let top_width = timesig_number_width(time.beats);
    let bot_width = timesig_number_width(time.beat_type);
    let max_width = top_width.max(bot_width);

    // Render numerator (top), centered
    let top_start_x = x + (max_width - top_width) / 2.0;
    render_timesig_number(svg, top_start_x, top_y, time.beats, s);

    // Render denominator (bottom), centered
    let bot_start_x = x + (max_width - bot_width) / 2.0;
    render_timesig_number(svg, bot_start_x, bot_y, time.beat_type, s);
}

/// Render a multi-digit number using VexFlow time signature glyphs.
fn render_timesig_number(svg: &mut SvgBuilder, mut x: f64, y: f64, n: i32, scale: f64) {
    let digits: Vec<u32> = if n == 0 {
        vec![0]
    } else {
        let mut d = Vec::new();
        let mut v = n.unsigned_abs();
        while v > 0 {
            d.push(v % 10);
            v /= 10;
        }
        d.reverse();
        d
    };

    for &d in &digits {
        let outline = timesig_digit_glyph(d);
        let path = vexflow_outline_to_svg(outline, scale, x, y);
        svg.path(&path, NOTE_COLOR, "none", 0.0);
        x += TIMESIG_DIGIT_HA[d as usize] * scale;
    }
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
    measure_x: f64,
    measure_w: f64,
) {
    if measure.notes.is_empty() {
        return;
    }

    // Position notes horizontally using the shared beat map
    let note_positions = note_x_positions_from_beat_map(&measure.notes, divisions, beat_x_map);

    // Pre-compute the centre of this measure for whole-measure rests
    let measure_center_x = measure_x + measure_w / 2.0;

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
            // Centre whole-measure rests (and rests with no type) in the measure
            let rest_x = if note.measure_rest || note.note_type.is_none() {
                measure_center_x
            } else {
                nx
            };
            render_rest(svg, rest_x, staff_y, note.note_type.as_deref(), note.measure_rest);
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

            // Accidental — placed snug against the notehead (1–2 units gap)
            if let Some(ref acc) = note.accidental {
                render_accidental(svg, nx - NOTEHEAD_RX - 4.0, note_y, acc);
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

                    // Determine flag count for stem extension
                    let flag_count = note.note_type.as_deref().map_or(0, |nt| match nt {
                        "eighth" => 1,
                        "16th" => 2,
                        "32nd" => 3,
                        "64th" => 4,
                        _ => 0,
                    });

                    // Extend stem for notes with multiple flags so the
                    // VexFlow glyph doesn't encroach on the notehead.
                    let stem_extra = match flag_count {
                        2 => 4.0,   // 16th notes
                        3 => 9.0,   // 32nd notes
                        4 => 13.0,  // 64th notes
                        _ => 0.0,
                    };
                    let stem_len = STEM_LENGTH + stem_extra;

                    let (sx, sy1, sy2) = if stem_up {
                        (nx + NOTEHEAD_RX - 1.0, note_y, note_y - stem_len)
                    } else {
                        (nx - NOTEHEAD_RX + 1.0, note_y, note_y + stem_len)
                    };
                    svg.line(sx, sy1, sx, sy2, NOTE_COLOR, STEM_WIDTH);

                    // Flags for unbeamed eighth notes and shorter
                    if flag_count > 0 {
                        render_flags(svg, sx, sy2, flag_count, stem_up);
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

const LYRICS_COLOR: &str = "#333333";
const LYRICS_FONT_SIZE: f64 = 13.0;
const LYRICS_PAD_BELOW: f64 = 16.0;  // padding below the lowest note/stem
const LYRICS_LINE_HEIGHT: f64 = 16.0; // spacing between lyric verse lines
const LYRICS_MIN_Y_BELOW_STAFF: f64 = 54.0; // minimum offset from staff_y (below bottom line)

// ── OSMD-inspired lyrics spacing constants ──────────────────────────────
// These mirror the approach of OpenSheetMusicDisplay (see EngravingRules):
// 1. We estimate text widths and compare to available spacing.
// 2. Measures are elongated (up to a cap) so lyrics don't overlap.
// 3. The last lyric in a measure can bleed past the barline.
// 4. Multi-syllable words get extra spacing for the connecting dash.

/// Average character width relative to font size (sans-serif approximation).
const LYRICS_CHAR_WIDTH_FACTOR: f64 = 0.55;
/// Minimum horizontal gap between adjacent lyric texts (in px).
const LYRICS_MIN_GAP: f64 = 6.0;
/// Extra gap for connecting dash between syllables of the same word (px).
const LYRICS_DASH_EXTRA_GAP: f64 = 8.0;
/// How many pixels of the last lyric's width can overlap into the next measure
/// (past the barline). OSMD uses ~3.4 em; our equivalent in px at 13px font.
const LYRICS_OVERLAP_INTO_NEXT_MEASURE: f64 = 20.0;
/// Maximum factor by which lyrics can elongate a measure beyond its beat-based
/// minimum width. Prevents absurdly wide measures. (OSMD default: 2.5)
const MAX_LYRICS_ELONGATION_FACTOR: f64 = 2.5;

// ── Lyrics text-width helpers ───────────────────────────────────────────

/// Estimate the rendered width of a text string in pixels for a given font size.
fn estimate_text_width(text: &str, font_size: f64) -> f64 {
    text.len() as f64 * font_size * LYRICS_CHAR_WIDTH_FACTOR
}

/// Metadata about a lyric event at a specific beat: its text width, whether
/// it is part of a multi-syllable word (needs dash), and whether it is the
/// last lyric beat in the measure (can overlap into next measure).
#[derive(Clone, Debug)]
struct LyricEvent {
    beat_time: f64,
    text_width: f64,
    /// True if this syllable is "begin" or "middle" (dash follows).
    has_dash: bool,
    /// True if this is the last lyric-bearing beat in the measure.
    is_last: bool,
}

/// Collect per-beat lyric events for a single measure across all parts.
/// Returns sorted, de-duplicated events (one per unique beat time, keeping
/// the maximum width and most restrictive dash flag).
fn collect_lyric_events(parts: &[Part], mi: usize, divisions_map: &[i32]) -> Vec<LyricEvent> {
    let mut events: Vec<LyricEvent> = Vec::new();

    for (pidx, part) in parts.iter().enumerate() {
        if mi >= part.measures.len() { continue; }
        let measure = &part.measures[mi];
        let divisions = divisions_map[pidx].max(1);
        let beat_times = compute_note_beat_times(&measure.notes, divisions);

        for (i, note) in measure.notes.iter().enumerate() {
            if note.lyrics.is_empty() { continue; }
            let w = note.lyrics.iter()
                .map(|l| estimate_text_width(&l.text, LYRICS_FONT_SIZE))
                .fold(0.0f64, f64::max);
            if w <= 0.0 { continue; }
            // Check if any lyric on this note has a dash after it
            let has_dash = note.lyrics.iter().any(|l| {
                matches!(l.syllabic.as_deref(), Some("begin") | Some("middle"))
            });
            events.push(LyricEvent {
                beat_time: beat_times[i],
                text_width: w,
                has_dash,
                is_last: false,
            });
        }
    }

    if events.is_empty() { return vec![]; }

    // Sort by beat time and merge duplicates
    events.sort_by(|a, b| a.beat_time.partial_cmp(&b.beat_time).unwrap());
    let mut merged: Vec<LyricEvent> = Vec::new();
    for ev in &events {
        if let Some(last) = merged.last_mut() {
            if (last.beat_time - ev.beat_time).abs() < 0.001 {
                last.text_width = last.text_width.max(ev.text_width);
                last.has_dash = last.has_dash || ev.has_dash;
                continue;
            }
        }
        merged.push(ev.clone());
    }

    // Mark the last event
    if let Some(last) = merged.last_mut() {
        last.is_last = true;
    }

    merged
}

/// Compute the minimum spacing required between two consecutive lyric events.
/// Accounts for text widths, dashes, and whether the right event is the last
/// in the measure (allowed to bleed past the barline).
fn lyric_pair_min_spacing(left: &LyricEvent, right: &LyricEvent) -> f64 {
    let gap = if left.has_dash {
        LYRICS_MIN_GAP + LYRICS_DASH_EXTRA_GAP
    } else {
        LYRICS_MIN_GAP
    };
    let right_half = if right.is_last {
        // Last lyric can overlap into next measure, so we need less space
        (right.text_width / 2.0 - LYRICS_OVERLAP_INTO_NEXT_MEASURE).max(0.0)
    } else {
        right.text_width / 2.0
    };
    left.text_width / 2.0 + gap + right_half
}

/// Compute the minimum measure width needed to accommodate lyrics without
/// overlapping, using OSMD-inspired logic. The result is capped at
/// `MAX_LYRICS_ELONGATION_FACTOR` times the beat-based minimum width.
fn lyrics_min_measure_width(
    parts: &[Part],
    mi: usize,
    divisions_map: &[i32],
    beat_based_width: f64,
) -> f64 {
    let events = collect_lyric_events(parts, mi, divisions_map);
    if events.is_empty() { return 0.0; }

    // Total width needed for lyrics
    let mut total = 0.0;
    for i in 0..events.len() {
        if i == 0 {
            total += events[i].text_width / 2.0; // left half of first lyric
        }
        if i > 0 {
            total += lyric_pair_min_spacing(&events[i - 1], &events[i]);
        }
    }
    // Right half of last lyric (with overlap allowance)
    if let Some(last) = events.last() {
        let right_half = if last.is_last {
            (last.text_width / 2.0 - LYRICS_OVERLAP_INTO_NEXT_MEASURE).max(0.0)
        } else {
            last.text_width / 2.0
        };
        total += right_half;
    }
    total += 28.0; // left + right inset padding

    // Cap at MAX_LYRICS_ELONGATION_FACTOR × beat-based width
    let cap = beat_based_width * MAX_LYRICS_ELONGATION_FACTOR;
    total.min(cap)
}

/// Compute the lowest y-coordinate of rendered notes/stems in a measure.
/// This accounts for noteheads, downward stems, and ledger-line regions.
fn measure_lowest_note_y(
    measure: &Measure,
    staff_y: f64,
    clef: Option<&Clef>,
    transpose_octave: i32,
    staff_filter: Option<i32>,
) -> f64 {
    let mut lowest = staff_y + STAFF_HEIGHT; // at least the bottom staff line
    for note in &measure.notes {
        if let Some(sf) = staff_filter {
            if note.staff.unwrap_or(1) != sf { continue; }
        }
        if let Some(ref pitch) = note.pitch {
            let note_y = staff_y + pitch_to_staff_y(pitch, clef, transpose_octave);
            // Account for the notehead itself
            let bottom = note_y + NOTEHEAD_RY;
            if bottom > lowest { lowest = bottom; }
            // Account for downward stems
            if note.stem.as_deref() == Some("down") {
                let stem_bottom = note_y + STEM_LENGTH;
                if stem_bottom > lowest { lowest = stem_bottom; }
            }
        }
    }
    lowest
}

fn render_lyrics(
    svg: &mut SvgBuilder,
    measure: &Measure,
    note_positions: &[f64],
    lyrics_base_y: f64,
    staff_filter: Option<i32>,
) {
    for (i, note) in measure.notes.iter().enumerate() {
        if let Some(sf) = staff_filter {
            if note.staff.unwrap_or(1) != sf { continue; }
        }
        if i >= note_positions.len() { break; }
        let nx = note_positions[i];

        for lyric in &note.lyrics {
            let verse_offset = (lyric.number - 1) as f64 * LYRICS_LINE_HEIGHT;
            let ly = lyrics_base_y + verse_offset;

            // Add hyphen suffix for syllables that continue ("begin" or "middle")
            let display_text = match lyric.syllabic.as_deref() {
                Some("begin") | Some("middle") => format!("{} -", lyric.text),
                _ => lyric.text.clone(),
            };

            svg.text(
                nx, ly,
                &display_text,
                LYRICS_FONT_SIZE,
                "normal",
                LYRICS_COLOR,
                "middle",
            );
        }
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
///
/// `lyric_events` provides per-beat lyric metadata (text width, dash, last
/// flag) from `collect_lyric_events`. When present, the algorithm enforces
/// minimum spacing between consecutive beats so lyric syllables don't overlap,
/// using OSMD-inspired logic: multi-syllable dashes get extra gap, and the
/// last lyric is allowed to bleed past the barline.
fn compute_beat_x_map(
    all_beat_times: &[Vec<f64>],
    mx: f64,
    mw: f64,
    left_pad: f64,
    right_pad: f64,
    lyric_events: &[LyricEvent],
) -> Vec<(f64, f64)> {
    let usable_width = mw - left_pad - right_pad;

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

    // If no lyrics, use pure proportional spacing
    if lyric_events.is_empty() {
        return unique_beats
            .iter()
            .map(|&bt| {
                let x = mx + left_pad + (bt / max_beat) * usable_width;
                (bt, x)
            })
            .collect();
    }

    // Helper: look up lyric event for a beat time
    let event_at = |bt: f64| -> Option<&LyricEvent> {
        lyric_events.iter().find(|ev| (ev.beat_time - bt).abs() < 0.001)
    };

    // Compute per-interval minimum distances based on lyrics.
    // For each pair of consecutive beats, the minimum distance is the
    // maximum of:
    //   (a) the proportional (beat-based) distance, and
    //   (b) the lyrics-based distance: half_left + gap + half_right,
    //       with extra gap for dashes and overlap allowance for the last.
    let n = unique_beats.len();
    let mut min_dists: Vec<f64> = Vec::with_capacity(n.saturating_sub(1));
    let mut total_min = 0.0f64;

    for i in 1..n {
        let prop_dist = ((unique_beats[i] - unique_beats[i - 1]) / max_beat) * usable_width;

        let left_ev = event_at(unique_beats[i - 1]);
        let right_ev = event_at(unique_beats[i]);

        let lyrics_dist = match (left_ev, right_ev) {
            (Some(le), Some(re)) => lyric_pair_min_spacing(le, re),
            (Some(le), None) => le.text_width / 2.0,
            (None, Some(re)) => re.text_width / 2.0,
            (None, None) => 0.0,
        };

        let min_dist = prop_dist.max(lyrics_dist);
        min_dists.push(min_dist);
        total_min += min_dist;
    }

    // Scale so the total fits within usable_width.
    // If total_min <= usable_width, we have room and scale up each interval
    // proportionally so the total equals usable_width.
    // If total_min > usable_width, scale down (shouldn't happen when
    // measure_min_widths already accounts for lyrics, but handle gracefully).
    let scale = if total_min > 0.0 { usable_width / total_min } else { 1.0 };

    let mut result = Vec::with_capacity(n);
    let mut x = mx + left_pad;
    result.push((unique_beats[0], x));

    for i in 0..min_dists.len() {
        x += min_dists[i] * scale;
        result.push((unique_beats[i + 1], x));
    }

    result
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

fn render_rest(svg: &mut SvgBuilder, x: f64, staff_y: f64, note_type: Option<&str>, measure_rest: bool) {
    // Rests are positioned on the staff using standard engraving positions:
    //   whole / measure — hangs from line 4 (staff_y + 10)
    //   half            — sits on line 3   (staff_y + 20)
    //   others          — centred on the staff
    //
    // A full-measure rest (MusicXML: <rest measure="yes"/>) is ALWAYS rendered
    // as a whole-note rest rectangle, regardless of the time signature.  This is
    // standard music engraving convention.

    // If this is a measure rest, or if note_type is missing (common for
    // full-measure rests in MusicXML), render as whole-note rest.
    if measure_rest || note_type.is_none() {
        // Whole-note rest: rectangle hanging below staff line 4
        svg.rect(x - 7.0, staff_y + 10.0, 14.0, 5.0, REST_COLOR, "none", 0.0);
        return;
    }

    match note_type.unwrap() {
        "whole" => {
            // Rectangle hanging below line 4
            svg.rect(x - 7.0, staff_y + 10.0, 14.0, 5.0, REST_COLOR, "none", 0.0);
        }
        "half" => {
            // Rectangle sitting on top of line 3
            svg.rect(x - 7.0, staff_y + 15.0, 14.0, 5.0, REST_COLOR, "none", 0.0);
        }
        "quarter" => {
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
        "eighth" => {
            // Authentic eighth rest from VexFlow Gonville font glyph va5.
            let path = vf_outline_to_svg(VF_EIGHTH_REST, VF_GLYPH_SCALE);
            let gx = x - 5.0;
            let gy = staff_y + 20.0;
            svg.elements.push(format!(
                r#"<path d="{}" fill="{}" transform="translate({:.1},{:.1})"/>"#,
                path, REST_COLOR, gx, gy
            ));
        }
        "16th" => {
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
            // Fallback: whole-note rest (safest default for unknown types)
            svg.rect(x - 7.0, staff_y + 10.0, 14.0, 5.0, REST_COLOR, "none", 0.0);
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Accidental rendering
// ═══════════════════════════════════════════════════════════════════════

fn render_accidental(svg: &mut SvgBuilder, x: f64, y: f64, accidental: &str) {
    match accidental {
        "sharp" => svg.sharp_glyph(x - 4.5, y),
        "flat" => svg.flat_glyph(x - 3.5, y),
        "natural" => svg.natural_glyph(x - 3.5, y),
        "double-sharp" => svg.double_sharp_glyph(x - 5.0, y),
        "flat-flat" => svg.double_flat_glyph(x - 6.5, y),
        _ => {}
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Flag rendering — VexFlow font glyphs (from OSMD's vexflow_font.js)
// ═══════════════════════════════════════════════════════════════════════
// Each glyph is a single combined outline.  Format: VexFlow outline
// commands — m (moveTo), l (lineTo), b (bezier), q (quadratic).
//
// IMPORTANT: VexFlow's 'b' command parameter order is:
//   b endX endY cp1X cp1Y cp2X cp2Y
// which maps to SVG:  C cp1X,cp1Y  cp2X,cp2Y  endX,endY
// (The end point is listed FIRST in the outline, then the two control
// points.  They are re-ordered for the standard bezierCurveTo call.)

/// Scale: VexFlow font units → our SVG pixels.
/// Calibrated so an 8th flag is ≈21 px tall, ≈7 px wide.
const FLAG_GLYPH_SCALE: f64 = 0.021;

// ── 8th note flags ──────────────────────────────────────────────────
const FLAG_8TH_UP: &str = "m -24 -161 l -24 -5 l -20 -5 b 0 -24 -9 -5 -2 -12 b 171 -315 21 -124 84 -233 b 317 -660 268 -406 317 -531 b 187 -1014 317 -782 274 -909 b 161 -1034 172 -1034 171 -1034 b 141 -1013 149 -1034 141 -1025 b 152 -991 141 -1004 142 -1002 b 266 -682 228 -899 266 -788 b 174 -430 266 -588 236 -498 b -23 -317 136 -388 66 -348 b -24 -161 -23 -316 -24 -285";
const FLAG_8TH_DOWN: &str = "m 230 1031 b 238 1032 232 1032 235 1032 b 259 1014 245 1032 251 1027 b 367 662 330 906 367 782 b 364 602 367 641 367 621 b 232 317 352 488 304 384 b 57 120 155 245 103 187 b -1 18 31 84 6 40 b -19 4 -4 11 -12 4 l -21 4 l -21 159 l -21 315 l -16 315 b 96 335 10 315 62 324 b 315 695 227 380 315 527 b 313 738 315 709 314 724 b 224 991 304 825 273 916 b 216 1013 219 999 216 1007 b 230 1031 216 1021 220 1028";

// ── 16th note flags ─────────────────────────────────────────────────
const FLAG_16TH_UP: &str = "m -24 -147 l -24 -5 l -20 -5 b -1 -19 -12 -5 -4 -11 b 58 -123 6 -43 31 -86 b 196 -278 93 -173 134 -219 b 317 -570 274 -356 317 -460 b 294 -713 317 -617 308 -666 l 289 -724 l 294 -735 b 317 -873 308 -780 317 -827 b 235 -1132 317 -963 288 -1054 b 209 -1165 228 -1140 224 -1146 b 189 -1177 204 -1172 196 -1177 b 171 -1164 182 -1177 175 -1172 b 168 -1154 170 -1161 168 -1159 b 181 -1132 168 -1149 172 -1142 b 269 -891 238 -1064 269 -975 b 269 -881 269 -886 269 -884 b 262 -814 269 -857 265 -827 b 258 -800 261 -811 259 -806 b 142 -628 240 -731 198 -667 b -8 -589 112 -606 47 -589 b -20 -589 -13 -589 -19 -589 l -24 -589 l -24 -449 l -24 -308 l -20 -308 b -1 -322 -12 -308 -4 -313 b 58 -424 6 -345 31 -388 b 194 -580 93 -476 136 -523 b 259 -660 221 -606 245 -635 b 261 -663 259 -662 261 -663 b 264 -656 262 -663 262 -660 b 269 -587 268 -632 269 -610 b 264 -521 269 -566 268 -544 b 262 -512 264 -517 262 -513 b 258 -498 261 -509 259 -503 b 142 -326 240 -428 198 -365 b -8 -287 112 -303 47 -288 b -20 -287 -13 -287 -19 -287 l -24 -287 l -24 -147";
const FLAG_16TH_DOWN: &str = "m 302 1031 b 308 1032 304 1032 307 1032 b 330 1016 318 1032 325 1027 b 362 867 351 970 362 920 b 340 738 362 824 353 780 l 336 727 l 340 717 b 362 591 355 677 362 634 b 257 323 362 496 325 401 b 204 272 243 306 227 290 b 20 56 129 206 66 133 b -1 18 12 44 0 22 b -19 4 -4 9 -12 4 l -21 4 l -21 140 l -21 276 l -12 277 b 167 333 61 288 127 309 b 319 598 262 388 319 491 b 311 664 319 620 317 642 l 310 673 l 304 664 b 204 548 279 620 250 587 b 20 333 129 483 66 409 b -1 292 12 320 0 298 b -19 280 -4 285 -12 280 l -21 280 l -21 416 l -21 552 l -12 553 b 167 609 61 564 127 585 b 319 874 264 666 319 770 b 294 992 319 914 311 954 b 288 1011 288 1004 288 1007 b 302 1031 288 1021 294 1028";

// ── 32nd note flags ─────────────────────────────────────────────────
const FLAG_32ND_UP: &str = "m -24 -145 l -24 -5 l -20 -5 b 1 -26 -10 -5 -6 -9 b 175 -241 31 -86 96 -166 b 314 -548 259 -323 304 -420 b 315 -589 315 -555 315 -571 b 314 -630 315 -606 315 -623 b 298 -730 311 -664 306 -699 l 295 -742 l 296 -748 b 314 -850 304 -778 311 -813 b 315 -892 315 -857 315 -874 b 314 -932 315 -909 315 -925 b 298 -1032 311 -967 306 -1002 l 295 -1045 l 296 -1050 b 314 -1153 304 -1081 311 -1115 b 315 -1193 315 -1160 315 -1177 b 314 -1235 315 -1211 315 -1228 b 217 -1526 306 -1338 270 -1444 b 201 -1533 213 -1532 208 -1533 b 182 -1522 193 -1533 185 -1529 b 179 -1514 181 -1518 179 -1517 b 189 -1489 179 -1508 182 -1501 b 266 -1217 240 -1403 266 -1308 b 262 -1156 266 -1196 265 -1177 b 110 -907 247 -1043 190 -950 b 0 -889 87 -895 50 -889 l -1 -889 l -24 -889 l -24 -749 l -24 -610 l -20 -610 b 1 -631 -10 -610 -6 -614 b 175 -846 31 -691 96 -771 b 259 -956 213 -884 236 -914 b 265 -966 262 -961 264 -966 b 265 -966 265 -966 265 -966 b 265 -953 265 -964 265 -959 b 266 -920 266 -943 266 -932 b 262 -853 266 -898 265 -873 b 110 -605 247 -741 190 -648 b 0 -587 87 -592 50 -587 l -1 -587 l -24 -587 l -24 -448 l -24 -308 l -20 -308 b 1 -328 -10 -308 -6 -312 b 175 -544 31 -388 96 -469 b 259 -655 213 -581 236 -612 b 265 -663 262 -659 264 -663 b 265 -663 265 -663 265 -663 b 265 -650 265 -663 265 -657 b 266 -617 266 -641 266 -630 b 262 -551 266 -595 265 -570 b 110 -303 247 -438 190 -345 b 0 -284 87 -290 50 -284 l -1 -284 l -24 -284 l -24 -145";
const FLAG_32ND_DOWN: &str = "m 276 1378 b 284 1379 279 1379 281 1379 b 306 1360 292 1379 298 1374 b 352 1247 326 1326 343 1286 b 366 1139 362 1213 366 1175 b 347 1009 366 1093 359 1049 l 344 1002 l 347 992 b 352 971 348 986 351 977 b 366 863 362 936 366 899 b 347 732 366 818 359 773 l 344 725 l 347 716 b 352 695 348 710 351 700 b 366 588 362 659 366 623 b 223 262 366 464 314 345 b 189 233 212 252 212 252 b 35 76 126 183 73 129 b -1 16 20 56 2 27 b -19 4 -4 9 -12 4 l -21 4 l -21 137 l -21 270 l -17 270 b 186 344 59 281 134 308 b 319 606 270 399 319 499 b 317 650 319 620 319 635 l 315 659 l 314 655 b 223 537 288 607 258 570 b 189 509 212 528 212 528 b 35 352 126 459 73 405 b -1 292 20 333 2 303 b -19 280 -4 285 -12 280 l -21 280 l -21 413 l -21 546 l -17 546 b 186 620 59 557 134 584 b 319 882 270 675 319 775 b 317 925 319 896 319 911 l 315 935 l 314 931 b 223 813 288 884 258 846 b 189 785 212 805 212 805 b 35 628 126 735 73 681 b -1 569 20 609 2 580 b -19 556 -4 562 -12 556 l -21 556 l -21 689 l -21 823 l -17 823 b 202 907 68 835 152 867 b 319 1157 280 968 319 1061 b 270 1338 319 1218 303 1281 b 262 1358 264 1349 262 1353 b 262 1364 262 1360 262 1363 b 276 1378 265 1371 269 1376";

// ── 64th note flags ─────────────────────────────────────────────────
const FLAG_64TH_UP: &str = "m -24 -145 l -24 -5 l -20 -5 b 0 -23 -9 -5 -2 -12 b 27 -87 4 -38 14 -66 b 138 -220 53 -136 88 -177 b 235 -328 179 -255 208 -288 b 314 -592 287 -409 314 -501 b 292 -732 314 -639 307 -687 l 289 -742 l 294 -756 b 314 -896 307 -802 314 -849 b 292 -1035 314 -943 307 -991 l 289 -1045 l 294 -1057 b 314 -1197 307 -1104 314 -1152 b 292 -1338 314 -1246 307 -1292 l 289 -1347 l 294 -1360 b 314 -1500 307 -1407 314 -1454 b 273 -1689 314 -1565 300 -1628 b 250 -1712 265 -1710 261 -1712 b 228 -1691 236 -1712 228 -1704 l 228 -1685 l 234 -1675 b 270 -1507 258 -1621 270 -1564 b 98 -1193 270 -1381 209 -1261 b 40 -1174 76 -1179 58 -1174 b -10 -1189 24 -1174 8 -1178 b -20 -1192 -14 -1192 -16 -1192 l -24 -1192 l -24 -1052 l -24 -913 l -20 -913 b 0 -931 -9 -913 -2 -920 b 27 -995 4 -946 14 -974 b 138 -1128 53 -1043 88 -1085 b 257 -1275 190 -1172 228 -1220 b 262 -1283 259 -1279 262 -1283 l 262 -1283 b 269 -1249 264 -1282 268 -1260 b 270 -1206 270 -1233 270 -1220 b 98 -891 270 -1075 206 -957 b 40 -871 76 -877 58 -871 b -10 -886 24 -871 8 -875 b -20 -889 -14 -889 -16 -889 l -24 -889 l -24 -749 l -24 -610 l -20 -610 b 0 -628 -9 -610 -2 -617 b 27 -692 4 -644 14 -671 b 138 -825 53 -741 88 -782 b 257 -973 190 -870 228 -917 b 262 -981 259 -977 262 -981 l 262 -981 b 269 -946 264 -979 268 -957 b 270 -903 270 -931 270 -917 b 98 -588 270 -774 206 -655 b 40 -569 76 -574 58 -569 b -10 -584 24 -569 8 -574 b -20 -587 -14 -587 -16 -587 l -24 -587 l -24 -448 l -24 -308 l -20 -308 b 0 -326 -9 -308 -2 -315 b 27 -390 4 -341 14 -369 b 138 -523 53 -438 88 -480 b 257 -670 190 -567 228 -614 b 262 -678 259 -674 262 -678 b 262 -678 262 -678 262 -678 b 269 -644 264 -677 268 -656 b 270 -601 270 -628 270 -614 b 98 -285 270 -471 206 -352 b 40 -266 76 -273 58 -266 b -10 -281 24 -266 8 -272 b -20 -284 -14 -284 -16 -284 l -24 -284 l -24 -145";
const FLAG_64TH_DOWN: &str = "m 259 1553 b 265 1553 261 1553 264 1553 b 288 1540 272 1553 277 1550 b 367 1351 340 1493 367 1424 b 336 1221 367 1308 357 1263 l 332 1211 l 333 1208 b 367 1077 356 1170 367 1124 b 336 945 367 1032 357 986 l 332 935 l 333 932 b 367 800 356 893 367 848 b 336 669 367 756 357 710 l 332 659 l 333 656 b 367 523 356 617 367 571 b 345 412 367 485 360 446 b 231 273 322 356 284 310 b -1 19 121 195 27 93 b -17 4 -4 11 -10 5 l -21 4 l -21 134 l -21 265 l -17 265 b 133 291 20 265 96 278 b 318 537 245 328 318 433 b 307 603 318 559 315 582 b 303 614 304 612 304 614 b 298 609 302 614 300 613 b 231 549 281 589 258 567 b -1 295 121 471 27 369 b -17 280 -4 287 -10 281 l -21 280 l -21 410 l -21 541 l -17 541 b 133 567 20 541 96 555 b 318 813 245 605 318 709 b 307 880 318 835 315 859 b 303 891 304 888 304 891 b 298 885 302 891 300 888 b 231 825 281 866 258 843 b -1 571 121 748 27 645 b -17 556 -4 563 -10 557 l -21 556 l -21 687 l -21 817 l -17 817 b 133 843 20 817 96 830 b 318 1089 245 881 318 985 b 307 1156 318 1111 315 1134 b 303 1167 304 1164 304 1167 b 298 1161 302 1167 300 1164 b 231 1102 281 1140 258 1120 b -1 848 121 1024 27 921 b -17 832 -4 839 -10 834 l -21 832 l -21 963 l -21 1093 l -17 1093 b 114 1113 12 1093 78 1103 b 313 1314 215 1142 289 1218 b 318 1364 317 1331 318 1347 b 255 1511 318 1422 295 1478 b 243 1532 247 1519 243 1525 b 259 1553 243 1540 250 1550";

// ═══════════════════════════════════════════════════════════════════════
// Time signature digit glyphs — VexFlow font (from OSMD's vexflow_font.js)
// ═══════════════════════════════════════════════════════════════════════
// Scale: VexFlow uses point=40, resolution=1000 → 40*72/(1000*100) = 0.0288
// Numerator rendered at staff line 2 (middle line), denominator at line 4 (bottom).

const TIMESIG_GLYPH_SCALE: f64 = 0.0288;

// Horizontal advance widths (ha) for each digit at scale 0.0288:
// 0: ha=525→15.1, 1: ha=351→10.1, 2: ha=468→13.5, 3: ha=418→12.0, 4: ha=478→13.8
// 5: ha=418→12.0, 6: ha=485→14.0, 7: ha=451→13.0, 8: ha=499→14.4, 9: ha=485→14.0
const TIMESIG_DIGIT_HA: [f64; 10] = [525.0, 351.0, 468.0, 418.0, 478.0, 418.0, 485.0, 451.0, 499.0, 485.0];

const TIMESIG_DIGIT_0: &str = "m 236 648 b 246 648 238 648 242 648 b 288 646 261 648 283 648 b 472 513 364 634 428 587 b 514 347 502 464 514 413 b 462 163 514 272 499 217 b 257 44 409 83 333 44 b 50 163 181 44 103 83 b 0 347 14 217 0 272 b 40 513 0 413 12 464 b 236 648 87 591 155 638 m 277 614 b 253 616 273 616 261 616 b 242 616 247 616 243 616 b 170 499 193 609 181 589 b 159 348 163 446 159 398 b 166 222 159 308 161 266 b 201 91 174 138 183 106 b 257 76 215 81 235 76 b 311 91 277 76 299 81 b 347 222 330 106 338 138 b 353 348 352 266 353 308 b 344 499 353 398 351 446 b 277 614 333 587 322 606 m 257 -1 l 258 -1 l 255 -1 l 257 -1 m 257 673 l 258 673 l 255 673 l 257 673";
const TIMESIG_DIGIT_1: &str = "m 126 637 l 129 638 l 198 638 l 266 638 l 269 635 b 274 631 272 634 273 632 l 277 627 l 277 395 b 279 156 277 230 277 161 b 329 88 281 123 295 106 b 344 69 341 81 344 79 b 337 55 344 62 343 59 l 333 54 l 197 54 l 61 54 l 58 55 b 50 69 53 59 50 62 b 65 88 50 79 53 81 b 80 97 72 91 74 93 b 117 156 103 113 112 129 b 117 345 117 161 117 222 l 117 528 l 100 503 l 38 406 b 14 383 24 384 23 383 b -1 398 5 383 -1 390 b 4 415 -1 403 1 409 b 16 437 5 416 10 426 l 72 539 l 100 596 b 121 632 119 631 119 631 b 126 637 122 634 125 635 m 171 -1 l 172 -1 l 170 -1 l 171 -1 m 171 673 l 172 673 l 170 673 l 171 673";
const TIMESIG_DIGIT_2: &str = "m 197 648 b 216 648 201 648 208 648 b 258 646 232 648 253 648 b 419 546 333 637 393 599 b 432 489 428 528 432 509 b 356 342 432 440 405 384 b 235 278 322 313 288 295 b 69 170 166 256 107 217 b 69 169 69 170 69 169 b 69 169 69 169 69 169 b 74 173 69 169 72 170 b 209 222 112 204 163 222 b 310 195 247 222 274 215 b 371 179 332 184 352 179 b 396 181 379 179 387 179 b 428 202 409 184 423 194 b 442 212 431 209 436 212 b 458 197 450 212 458 206 b 441 148 458 190 449 165 b 299 44 409 84 353 44 b 288 45 295 44 292 44 b 250 61 274 45 268 49 b 122 99 212 86 164 99 b 73 91 104 99 88 97 b 28 63 53 84 34 72 b 14 54 25 56 20 54 b 1 62 9 54 4 56 l -1 65 l -1 79 b 0 99 -1 91 0 95 b 2 113 1 102 2 108 b 164 309 20 197 81 272 b 285 470 232 341 277 398 b 287 487 287 476 287 481 b 171 595 287 551 239 595 b 155 595 166 595 160 595 b 142 592 145 594 142 594 b 145 589 142 592 142 591 b 179 527 168 576 179 551 b 132 455 179 496 163 467 b 104 451 122 452 112 451 b 27 530 62 451 27 487 b 29 555 27 538 27 546 b 197 648 44 601 115 639 m 228 -1 l 230 -1 l 227 -1 l 228 -1 m 228 673 l 230 673 l 227 673 l 228 673";
const TIMESIG_DIGIT_3: &str = "m 174 648 b 191 648 176 648 183 648 b 225 648 204 648 220 648 b 402 523 317 638 389 588 b 404 503 404 517 404 510 b 402 484 404 495 404 488 b 264 373 389 437 334 394 b 257 370 259 371 257 371 b 257 370 257 370 257 370 b 264 369 258 370 261 369 b 409 202 359 334 409 267 b 318 72 409 152 381 104 b 200 43 281 52 240 43 b 23 113 134 43 69 68 b 0 169 6 129 0 149 b 77 249 0 210 29 249 l 77 249 b 152 174 125 249 152 212 b 103 102 152 145 137 116 b 103 102 103 102 103 102 b 147 94 103 101 132 95 b 153 94 149 94 151 94 b 265 206 219 94 265 141 b 264 226 265 213 265 219 b 147 355 253 299 204 353 b 126 371 133 356 126 362 b 147 388 126 383 132 388 b 254 474 196 391 238 424 b 259 502 258 484 259 494 b 182 592 259 544 228 582 b 156 595 175 595 166 595 b 115 592 142 595 129 594 l 111 591 l 115 588 b 152 524 141 574 152 549 b 92 449 152 491 130 458 b 76 448 87 448 81 448 b -1 530 32 448 -1 488 b 20 581 -1 548 5 566 b 174 648 55 619 108 641 m 204 -1 l 205 -1 l 202 -1 l 204 -1 m 204 673 l 205 673 l 202 673 l 204 673";
const TIMESIG_DIGIT_4: &str = "m 174 637 b 232 638 175 638 189 638 b 277 638 245 638 259 638 l 378 638 l 381 635 b 389 623 386 632 389 627 b 382 609 389 617 386 613 b 366 589 381 606 372 598 l 313 528 l 245 451 l 209 410 l 155 348 l 84 267 b 59 240 72 252 59 240 b 59 240 59 240 59 240 b 151 238 59 238 68 238 l 242 238 l 242 303 b 243 371 242 369 242 370 b 289 426 245 374 254 385 l 303 441 l 317 456 l 338 483 l 360 506 l 371 520 b 386 527 375 526 381 527 b 400 519 392 527 397 524 b 401 440 401 516 401 514 b 401 377 401 423 401 402 l 401 238 l 426 238 b 453 237 449 238 450 238 b 465 217 461 234 465 226 b 460 202 465 212 464 206 b 426 197 454 197 453 197 l 401 197 l 401 180 b 451 88 402 129 412 109 b 468 69 465 81 468 79 b 461 55 468 62 466 59 l 458 54 l 321 54 l 185 54 l 182 55 b 175 69 176 59 175 62 b 191 88 175 79 176 81 b 240 180 230 109 240 129 l 240 197 l 125 197 b 73 195 104 195 87 195 b 8 197 10 195 9 197 b 0 212 2 199 0 205 b 0 212 0 212 0 212 b 20 242 0 219 0 219 b 163 610 104 344 163 492 b 174 637 163 628 166 634 m 234 -1 l 235 -1 l 232 -1 l 234 -1 m 234 673 l 235 673 l 232 673 l 234 673";
const TIMESIG_DIGIT_5: &str = "m 47 637 b 53 638 49 638 50 638 b 69 634 55 638 61 637 b 210 610 114 619 161 610 b 363 634 259 610 311 619 b 382 638 372 637 378 638 b 392 634 386 638 389 637 b 397 623 396 630 397 627 b 393 610 397 620 396 616 b 298 505 368 552 338 520 b 212 494 277 498 246 494 b 65 517 163 494 106 502 b 61 517 62 517 61 517 b 61 517 61 517 61 517 b 51 408 61 517 51 412 b 51 408 51 408 51 408 b 51 408 51 408 51 408 b 61 412 53 408 55 409 b 125 434 80 421 103 430 b 185 441 145 440 166 441 b 409 244 310 441 409 353 b 401 191 409 227 406 209 b 197 43 375 105 287 43 b 159 47 183 43 171 44 b 23 123 112 56 61 86 b 0 180 6 140 0 159 b 76 260 0 220 31 260 b 92 259 81 260 87 259 b 152 183 132 251 152 216 b 100 112 152 152 134 122 b 95 111 98 112 95 111 b 95 111 95 111 95 111 b 129 98 95 109 119 101 b 148 97 136 97 141 97 b 264 235 206 97 261 158 b 265 248 265 240 265 244 b 210 398 265 312 243 373 b 179 408 201 406 194 408 b 174 408 178 408 176 408 b 53 369 130 408 88 394 b 34 359 39 359 38 359 b 17 374 24 359 17 365 b 39 628 17 384 38 625 b 47 637 40 631 43 635 m 204 -1 l 205 -1 l 202 -1 l 204 -1 m 204 673 l 205 673 l 202 673 l 204 673";
const TIMESIG_DIGIT_6: &str = "m 255 648 b 274 648 259 648 266 648 b 314 646 288 648 307 648 b 450 555 374 637 438 594 b 454 530 453 546 454 538 b 375 451 454 485 416 451 b 328 467 359 451 343 455 b 300 526 310 483 300 503 b 352 598 300 557 319 589 b 356 599 355 598 356 599 b 352 602 356 599 355 601 b 288 616 330 612 308 616 b 210 584 257 616 230 605 b 164 433 189 559 174 508 b 160 374 163 415 160 381 b 160 374 160 374 160 374 b 160 374 160 374 160 374 b 168 377 160 374 164 376 b 258 395 200 390 228 395 b 366 367 294 395 328 387 b 475 223 436 333 475 283 b 472 197 475 215 473 206 b 349 65 462 141 419 95 b 259 43 317 51 288 43 b 167 69 230 43 200 52 b 4 290 80 113 20 195 b 0 349 1 309 0 328 b 20 467 0 391 6 433 b 255 648 58 563 155 637 m 269 363 b 257 363 265 363 261 363 b 210 345 236 363 220 356 b 186 226 196 324 186 272 b 187 198 186 216 186 206 b 213 95 191 151 202 112 b 257 76 221 83 238 76 b 270 77 261 76 266 76 b 321 156 299 81 310 99 b 329 229 326 183 329 206 b 321 301 329 252 326 274 b 269 363 311 342 298 359 m 236 -1 l 238 -1 l 235 -1 l 236 -1 m 236 673 l 238 673 l 235 673 l 236 673";
const TIMESIG_DIGIT_7: &str = "m 147 648 b 166 649 153 649 160 649 b 313 598 217 649 273 630 b 340 587 323 588 328 587 l 341 587 b 412 628 367 587 390 601 b 427 638 416 635 421 638 b 439 632 431 638 435 637 b 442 623 441 630 442 628 b 430 569 442 616 439 603 b 352 369 408 492 377 410 b 300 259 325 324 313 298 b 273 84 283 205 273 140 b 265 55 273 65 272 59 l 261 54 l 181 54 l 99 54 l 96 55 b 91 61 95 56 92 59 l 89 63 l 89 77 b 147 263 89 133 111 202 b 261 401 176 313 212 355 b 378 541 315 449 349 489 l 382 548 l 375 544 b 240 495 333 512 285 495 b 129 535 198 495 160 509 b 84 560 108 552 95 560 b 76 559 81 560 78 560 b 31 487 59 555 43 530 b 14 470 27 473 24 470 b 1 477 8 470 4 471 l 0 480 l 0 553 l 0 627 l 1 630 b 16 638 4 635 9 638 b 23 635 17 638 20 637 b 49 626 36 626 39 626 b 96 638 59 626 80 630 b 104 639 99 638 102 639 b 117 644 107 641 112 642 b 147 648 125 645 137 648 m 220 -1 l 221 -1 l 219 -1 l 220 -1 m 220 673 l 221 673 l 219 673 l 220 673";
const TIMESIG_DIGIT_8: &str = "m 217 648 b 245 649 225 648 235 649 b 453 516 343 649 430 595 b 458 478 455 503 458 491 b 412 370 458 440 441 398 b 411 369 412 369 411 369 b 415 365 411 367 412 367 b 488 231 462 331 488 281 b 472 165 488 208 483 186 b 243 43 434 86 338 43 b 63 104 178 43 112 62 b 0 233 20 140 0 186 b 73 365 0 283 24 331 l 77 369 l 72 374 b 29 476 42 406 29 441 b 217 648 29 557 103 635 m 258 605 b 242 606 253 605 247 606 b 157 552 198 606 157 580 b 160 541 157 548 159 544 b 319 413 176 503 242 452 l 337 403 l 338 406 b 359 476 352 428 359 452 b 258 605 359 537 318 595 m 138 326 b 130 330 134 328 130 330 b 130 330 130 330 130 330 b 107 305 127 330 112 313 b 84 231 91 281 84 256 b 243 86 84 156 151 86 b 249 87 245 86 246 87 b 347 156 303 88 347 120 b 344 172 347 162 345 167 b 156 319 325 227 257 281 b 138 326 151 322 144 324 m 243 -1 l 245 -1 l 242 -1 l 243 -1 m 243 673 l 245 673 l 242 673 l 243 673";
const TIMESIG_DIGIT_9: &str = "m 191 646 b 212 649 198 648 205 649 b 255 644 227 649 243 646 b 458 448 348 616 428 539 b 475 342 469 415 475 378 b 460 244 475 308 469 274 b 193 44 421 124 303 44 b 91 69 157 44 122 51 b 19 161 43 97 19 126 b 21 181 19 167 20 174 b 98 241 32 220 65 241 b 170 186 129 241 160 223 b 172 166 171 179 172 173 b 121 94 172 134 152 102 b 117 93 118 94 117 93 b 121 90 117 93 118 91 b 185 76 142 80 164 76 b 270 119 220 76 251 91 b 308 259 287 145 300 194 b 313 317 310 277 313 310 b 313 317 313 317 313 317 b 313 317 313 317 313 317 b 304 315 313 317 308 316 b 216 295 273 302 245 295 b 145 308 193 295 170 299 b 19 398 88 327 42 360 b 0 469 5 420 0 444 b 24 551 0 496 8 526 b 191 646 54 596 125 637 m 227 614 b 215 616 224 616 220 616 b 202 614 210 616 206 616 b 152 535 174 610 163 592 b 144 463 147 509 144 485 b 152 391 144 440 147 417 b 216 328 163 344 179 328 b 280 391 253 328 269 344 b 288 463 285 417 288 440 b 280 535 288 485 285 509 b 227 614 269 594 258 610 m 236 -1 l 238 -1 l 235 -1 l 236 -1 m 236 673 l 238 673 l 235 673 l 236 673";

// Common time (C) and cut time (C|)
const TIMESIG_COMMON: &str = "m 294 322 b 318 323 299 322 308 323 b 360 320 334 323 352 322 b 526 217 430 310 490 273 b 543 166 537 202 543 184 b 447 70 543 117 503 70 b 445 70 447 70 446 70 b 359 159 394 72 359 113 b 368 201 359 173 362 187 b 442 245 382 229 412 245 b 455 244 446 245 451 245 b 460 244 458 244 460 244 b 460 244 460 244 460 244 b 454 248 460 244 458 245 b 325 291 417 276 372 291 b 285 287 313 291 299 290 b 144 -2 183 269 144 190 b 281 -290 144 -208 179 -280 b 304 -291 289 -291 298 -291 b 524 -105 412 -291 506 -212 b 541 -84 526 -88 530 -84 b 556 -101 551 -84 556 -90 b 549 -138 556 -111 553 -122 b 334 -322 521 -237 435 -310 b 302 -324 323 -323 313 -324 b 13 -101 172 -324 54 -234 b -1 -1 4 -68 -1 -34 b 294 322 -1 161 121 303";
const TIMESIG_CUT: &str = "m 289 545 b 298 546 292 545 295 546 b 318 533 306 546 315 541 b 319 428 319 530 319 528 l 319 327 l 334 327 b 526 223 412 326 485 285 b 543 172 537 206 543 190 b 447 76 543 122 503 76 b 445 76 446 76 446 76 b 359 165 394 77 359 119 b 368 205 359 179 362 192 b 441 251 382 233 412 251 b 455 249 446 251 451 251 b 460 248 458 249 460 248 b 460 248 460 248 460 248 b 454 254 460 249 458 251 b 334 295 419 280 378 294 l 319 295 l 319 4 l 319 -287 l 321 -285 b 328 -285 322 -285 325 -285 b 524 -99 424 -277 507 -198 b 541 -79 526 -84 530 -79 b 556 -97 551 -79 556 -84 b 548 -133 556 -105 553 -117 b 334 -317 521 -233 434 -306 b 322 -319 329 -317 323 -317 l 319 -319 l 319 -424 b 319 -471 319 -444 319 -459 b 313 -541 319 -544 318 -535 b 298 -548 308 -545 303 -548 b 279 -534 289 -548 281 -542 b 277 -424 277 -531 277 -530 l 277 -317 l 273 -317 b 13 -95 153 -305 51 -217 b 0 2 4 -62 0 -29 b 182 295 0 126 66 238 b 274 324 210 309 249 320 l 277 324 l 277 427 b 279 533 277 528 277 530 b 289 545 281 538 285 542 m 277 2 b 277 291 277 161 277 291 b 268 288 277 291 273 290 b 144 1 179 265 144 184 b 276 -284 144 -199 175 -267 l 277 -285 l 277 2";

/// Look up the VexFlow glyph outline for a time signature digit (0-9).
fn timesig_digit_glyph(d: u32) -> &'static str {
    match d {
        0 => TIMESIG_DIGIT_0,
        1 => TIMESIG_DIGIT_1,
        2 => TIMESIG_DIGIT_2,
        3 => TIMESIG_DIGIT_3,
        4 => TIMESIG_DIGIT_4,
        5 => TIMESIG_DIGIT_5,
        6 => TIMESIG_DIGIT_6,
        7 => TIMESIG_DIGIT_7,
        8 => TIMESIG_DIGIT_8,
        9 => TIMESIG_DIGIT_9,
        _ => TIMESIG_DIGIT_0,
    }
}

/// Compute the width (in pixels) of a multi-digit number at TIMESIG_GLYPH_SCALE.
fn timesig_number_width(n: i32) -> f64 {
    let s = TIMESIG_GLYPH_SCALE;
    let digits = if n == 0 { vec![0u32] } else {
        let mut d = Vec::new();
        let mut v = n.unsigned_abs();
        while v > 0 {
            d.push(v % 10);
            v /= 10;
        }
        d.reverse();
        d
    };
    digits.iter().map(|&d| TIMESIG_DIGIT_HA[d as usize] * s).sum()
}

// ═══════════════════════════════════════════════════════════════════════
// Accidental glyphs — VexFlow font (from OSMD's vexflow_font.js)
// ═══════════════════════════════════════════════════════════════════════
// Scale: VexFlow uses point=38, resolution=1000 → 38*72/(1000*100) = 0.02736
// At this scale with 10px staff line spacing, glyphs match OSMD's rendering.

const ACCIDENTAL_GLYPH_SCALE: f64 = 0.02736;

// Sharp (v18) — two vertical + two slanted horizontal bars
const SHARP_GLYPH: &str = "m 217 535 b 225 537 220 537 221 537 b 245 524 235 537 242 533 l 246 521 l 247 390 l 247 258 l 273 265 b 306 270 288 269 299 270 b 322 259 315 270 319 267 b 323 208 323 256 323 233 b 322 158 323 184 323 159 b 288 140 318 148 315 147 b 247 130 254 131 247 130 b 247 65 247 130 247 104 b 247 20 247 51 247 36 l 247 -88 l 273 -81 b 306 -76 289 -77 299 -76 b 318 -81 311 -76 315 -77 b 323 -123 323 -87 323 -86 l 323 -138 l 323 -154 b 318 -195 323 -191 323 -190 b 269 -210 314 -199 315 -199 b 249 -216 259 -213 250 -216 l 247 -216 l 247 -349 l 246 -483 l 245 -487 b 225 -499 242 -495 234 -499 b 206 -487 219 -499 210 -495 l 205 -483 l 205 -355 l 205 -227 l 204 -227 l 181 -233 l 138 -244 b 117 -249 127 -247 117 -249 b 115 -385 115 -249 115 -256 l 115 -523 l 114 -526 b 95 -538 110 -534 102 -538 b 74 -526 87 -538 78 -534 l 73 -523 l 73 -391 b 72 -260 73 -269 73 -260 b 72 -260 72 -260 72 -260 b 19 -273 61 -263 23 -273 b 0 -260 10 -273 4 -267 b 0 -209 0 -256 0 -256 l 0 -162 l 1 -158 b 61 -134 5 -148 5 -148 l 73 -131 l 73 -22 b 72 86 73 79 73 86 b 72 86 72 86 72 86 b 19 74 61 83 23 74 b 0 86 10 74 4 79 b 0 137 0 90 0 90 l 0 184 l 1 188 b 61 212 5 198 5 198 l 73 215 l 73 348 l 73 481 l 74 485 b 95 498 78 492 87 498 b 103 495 98 498 100 496 b 114 485 107 494 111 489 l 115 481 l 115 353 l 115 226 l 121 226 b 159 235 123 227 141 231 l 198 247 l 205 248 l 205 384 l 205 521 l 206 524 b 217 535 209 528 212 533 m 205 9 b 205 119 205 70 205 119 l 205 119 b 182 113 204 119 194 116 l 138 102 b 117 97 127 99 117 97 b 115 -12 115 97 115 91 l 115 -122 l 121 -120 b 159 -111 123 -119 141 -115 l 198 -101 l 205 -98 l 205 9";

// Flat (v44) — vertical stem + curved bump
const FLAT_GLYPH: &str = "m -8 631 b -1 632 -6 632 -4 632 b 19 620 8 632 16 628 b 20 383 20 616 20 616 l 20 148 l 21 151 b 137 199 59 183 99 199 b 182 191 152 199 167 197 b 251 84 227 176 251 134 b 228 0 251 58 243 29 b 100 -142 206 -40 178 -72 l 23 -215 b 0 -229 9 -229 6 -229 b -20 -216 -9 -229 -17 -224 l -21 -212 l -21 201 l -21 616 l -20 620 b -8 631 -17 624 -13 630 m 110 131 b 96 133 106 133 100 133 b 89 133 93 133 91 133 b 24 87 63 129 40 113 l 20 80 l 20 -37 l 20 -156 l 23 -152 b 144 81 96 -72 144 20 l 144 83 b 110 131 144 113 134 126";

// Natural (v4e) — offset vertical bars + horizontal bars
const NATURAL_GLYPH: &str = "m 10 460 b 20 462 13 462 14 462 b 39 449 28 462 35 458 l 40 446 l 40 326 b 40 205 40 259 40 205 b 127 227 40 205 80 215 b 220 249 196 244 213 249 b 227 247 224 249 225 248 b 238 237 231 245 235 241 l 239 233 l 239 -106 l 239 -448 l 238 -451 b 219 -463 234 -459 225 -463 b 198 -451 210 -463 202 -459 l 197 -448 l 197 -324 b 197 -201 197 -248 197 -201 b 110 -223 196 -201 157 -210 b 17 -245 42 -240 24 -245 b 10 -242 13 -245 13 -244 b 0 -233 6 -241 2 -237 l 0 -230 l 0 108 l 0 446 l 0 449 b 10 460 2 453 6 458 m 197 22 b 197 70 197 41 197 58 b 196 116 197 113 197 116 l 196 116 b 118 97 196 116 160 106 l 40 77 l 40 -18 b 40 -112 40 -69 40 -112 l 119 -93 l 197 -73 l 197 22";

// Double sharp (v7f) — X-shaped cross
const DOUBLE_SHARP_GLYPH: &str = "m 0 124 l 0 187 l 61 187 l 122 187 l 122 138 l 122 91 l 153 61 l 183 30 l 213 61 l 243 91 l 243 138 l 243 187 l 306 187 l 367 187 l 367 124 l 367 61 l 321 61 l 274 61 l 243 30 l 213 0 l 243 -31 l 274 -62 l 321 -62 l 367 -62 l 367 -124 l 367 -188 l 306 -188 l 243 -188 l 243 -140 l 243 -93 l 213 -62 l 183 -31 l 153 -62 l 122 -93 l 122 -140 l 122 -188 l 61 -188 l 0 -188 l 0 -124 l 0 -62 l 46 -62 l 92 -62 l 123 -31 l 153 0 l 123 30 l 92 61 l 46 61 l 0 61 l 0 124";

// Double flat (v26) — two adjacent flat symbols
const DOUBLE_FLAT_GLYPH: &str = "m -8 631 b -1 632 -6 632 -4 632 b 19 620 8 632 16 628 b 20 383 20 616 20 616 l 20 148 l 21 151 b 140 199 59 183 102 199 b 206 179 164 199 187 192 l 210 176 l 210 396 l 210 617 l 212 621 b 231 632 216 628 223 632 b 250 620 239 632 247 628 b 251 383 251 616 251 616 l 251 148 l 254 151 b 370 199 291 183 332 199 b 415 191 385 199 400 197 b 483 84 458 176 483 134 b 461 0 483 58 476 29 b 332 -142 439 -40 411 -72 l 255 -215 b 231 -229 240 -229 239 -229 b 216 -223 224 -229 220 -227 b 210 -158 210 -217 210 -223 b 210 -120 210 -148 210 -136 l 210 -29 l 205 -34 b 100 -142 182 -65 159 -88 l 23 -215 b -1 -229 9 -229 6 -229 b -20 -216 -9 -229 -17 -224 l -21 -212 l -21 201 l -21 616 l -20 620 b -8 631 -17 624 -13 630 m 110 131 b 96 133 106 133 100 133 b 89 133 93 133 91 133 b 24 87 63 129 40 113 l 20 80 l 20 -37 l 20 -156 l 23 -152 b 144 81 96 -72 144 20 l 144 83 b 110 131 144 113 134 126 m 341 131 b 328 133 337 133 332 133 b 322 133 326 133 323 133 b 257 87 296 129 273 113 l 251 80 l 251 -37 l 251 -156 l 255 -152 b 375 81 328 -72 375 20 l 375 83 b 341 131 375 113 367 126";

/// Convert a VexFlow glyph outline string to an SVG path `d` attribute.
///
/// VexFlow outline token format:
///   m x y                          → SVG M (moveTo)
///   l x y                          → SVG L (lineTo)
///   b endX endY cp1X cp1Y cp2X cp2Y → SVG C cp1X,cp1Y cp2X,cp2Y endX,endY
///   q endX endY cpX cpY            → SVG Q cpX,cpY endX,endY
///
/// The y-axis is inverted (font y goes up; SVG y goes down).
fn vexflow_outline_to_svg(outline: &str, scale: f64, ox: f64, oy: f64) -> String {
    let tokens: Vec<&str> = outline.split_whitespace().collect();
    let mut path = String::with_capacity(outline.len());
    let mut i = 0;

    while i < tokens.len() {
        match tokens[i] {
            "m" if i + 2 < tokens.len() => {
                let x: f64 = tokens[i + 1].parse().unwrap_or(0.0);
                let y: f64 = tokens[i + 2].parse().unwrap_or(0.0);
                path.push_str(&format!("M{:.1} {:.1}", ox + x * scale, oy - y * scale));
                i += 3;
            }
            "l" if i + 2 < tokens.len() => {
                let x: f64 = tokens[i + 1].parse().unwrap_or(0.0);
                let y: f64 = tokens[i + 2].parse().unwrap_or(0.0);
                path.push_str(&format!("L{:.1} {:.1}", ox + x * scale, oy - y * scale));
                i += 3;
            }
            "b" if i + 6 < tokens.len() => {
                // VexFlow order: endX endY cp1X cp1Y cp2X cp2Y
                let ex: f64  = tokens[i + 1].parse().unwrap_or(0.0);
                let ey: f64  = tokens[i + 2].parse().unwrap_or(0.0);
                let c1x: f64 = tokens[i + 3].parse().unwrap_or(0.0);
                let c1y: f64 = tokens[i + 4].parse().unwrap_or(0.0);
                let c2x: f64 = tokens[i + 5].parse().unwrap_or(0.0);
                let c2y: f64 = tokens[i + 6].parse().unwrap_or(0.0);
                // SVG order: C cp1X,cp1Y cp2X,cp2Y endX,endY
                path.push_str(&format!(
                    "C{:.1} {:.1} {:.1} {:.1} {:.1} {:.1}",
                    ox + c1x * scale, oy - c1y * scale,
                    ox + c2x * scale, oy - c2y * scale,
                    ox + ex * scale,  oy - ey * scale,
                ));
                i += 7;
            }
            "q" if i + 4 < tokens.len() => {
                // VexFlow order: endX endY cpX cpY
                let ex: f64 = tokens[i + 1].parse().unwrap_or(0.0);
                let ey: f64 = tokens[i + 2].parse().unwrap_or(0.0);
                let cx: f64 = tokens[i + 3].parse().unwrap_or(0.0);
                let cy: f64 = tokens[i + 4].parse().unwrap_or(0.0);
                // SVG order: Q cpX,cpY endX,endY
                path.push_str(&format!(
                    "Q{:.1} {:.1} {:.1} {:.1}",
                    ox + cx * scale, oy - cy * scale,
                    ox + ex * scale, oy - ey * scale,
                ));
                i += 5;
            }
            _ => { i += 1; }
        }
    }

    path.push('Z');
    path
}

/// Render note flags using VexFlow font glyph outlines.
///
/// The glyph is rendered with both fill AND a thin matching stroke so
/// that sub-pixel-thin connection areas remain visible at our scale.
fn render_flags(svg: &mut SvgBuilder, stem_x: f64, stem_end_y: f64, count: usize, stem_up: bool) {
    let outline = match (count, stem_up) {
        (1, true)  => FLAG_8TH_UP,
        (1, false) => FLAG_8TH_DOWN,
        (2, true)  => FLAG_16TH_UP,
        (2, false) => FLAG_16TH_DOWN,
        (3, true)  => FLAG_32ND_UP,
        (3, false) => FLAG_32ND_DOWN,
        (4, true)  => FLAG_64TH_UP,
        (4, false) => FLAG_64TH_DOWN,
        _ => return,
    };

    let s = FLAG_GLYPH_SCALE;
    let path = vexflow_outline_to_svg(outline, s, stem_x, stem_end_y);
    // Fill + thin stroke ensures narrow connecting regions stay visible.
    svg.path(&path, NOTE_COLOR, NOTE_COLOR, 0.3);
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
            "half-diminished" => "m7b5",
            other => other,
        };

        let alter_str = match harmony.root.alter {
            Some(a) if a > 0.0 => "#",
            Some(a) if a < 0.0 => "b",
            _ => "",
        };

        let label = format!("{}{}{}", harmony.root.step, alter_str, kind_str);
        svg.chord_text(x, y, &label, 12.0, CHORD_COLOR);
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

    fn text_with_spacing(&mut self, x: f64, y: f64, content: &str, size: f64, weight: &str, fill: &str, anchor: &str, letter_spacing: f64) {
        let escaped = content
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        self.elements.push(format!(
            r#"<text x="{:.1}" y="{:.1}" font-size="{:.0}" font-weight="{}" fill="{}" text-anchor="{}" letter-spacing="{:.1}">{}</text>"#,
            x, y, size, weight, fill, anchor, letter_spacing, escaped
        ));
    }

    /// Render chord symbols matching OSMD style: Times New Roman, normal weight, no letter-spacing
    fn chord_text(&mut self, x: f64, y: f64, content: &str, size: f64, fill: &str) {
        let escaped = content
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        self.elements.push(format!(
            r#"<text x="{:.1}" y="{:.1}" font-family="Times New Roman, serif" font-size="{:.0}" font-weight="normal" fill="{}" text-anchor="start">{}</text>"#,
            x, y, size, fill, escaped
        ));
    }

    fn path(&mut self, d: &str, fill: &str, stroke: &str, stroke_width: f64) {
        self.elements.push(format!(
            r#"<path d="{}" fill="{}" stroke="{}" stroke-width="{:.1}" stroke-linecap="round"/>"#,
            d, fill, stroke, stroke_width
        ));
    }

    fn notehead(&mut self, cx: f64, cy: f64, filled: bool, _is_whole: bool) {
        // All noteheads share the same oval shape and size.
        // The only difference: filled (quarter, eighth, etc.) vs unfilled (half, whole).
        let rx = NOTEHEAD_RX;
        let ry = NOTEHEAD_RY;
        if filled {
            self.elements.push(format!(
                r#"<ellipse cx="{:.1}" cy="{:.1}" rx="{:.1}" ry="{:.1}" fill="{}" stroke="none" stroke-width="0" transform="rotate(-15,{:.1},{:.1})"/>"#,
                cx, cy, rx, ry, NOTE_COLOR, cx, cy
            ));
        } else {
            // Unfilled: draw the same oval outline with a thicker inward stroke
            // so the outer boundary matches the filled noteheads exactly.
            let sw = 2.0;
            self.elements.push(format!(
                r#"<ellipse cx="{:.1}" cy="{:.1}" rx="{:.1}" ry="{:.1}" fill="none" stroke="{}" stroke-width="{:.1}" transform="rotate(-15,{:.1},{:.1})"/>"#,
                cx, cy, rx - sw / 2.0, ry - sw / 2.0, NOTE_COLOR, sw, cx, cy
            ));
        }
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

    /// Render a sharp accidental glyph using VexFlow font outline.
    fn sharp_glyph(&mut self, x: f64, y: f64) {
        let s = ACCIDENTAL_GLYPH_SCALE;
        let path = vexflow_outline_to_svg(SHARP_GLYPH, s, x, y);
        self.elements.push(format!(
            r#"<path d="{}" fill="{}" stroke="none"/>"#,
            path, NOTE_COLOR
        ));
    }

    /// Render a flat accidental glyph using VexFlow font outline.
    fn flat_glyph(&mut self, x: f64, y: f64) {
        let s = ACCIDENTAL_GLYPH_SCALE;
        let path = vexflow_outline_to_svg(FLAT_GLYPH, s, x, y);
        self.elements.push(format!(
            r#"<path d="{}" fill="{}" stroke="none"/>"#,
            path, NOTE_COLOR
        ));
    }

    /// Render a natural accidental glyph using VexFlow font outline.
    fn natural_glyph(&mut self, x: f64, y: f64) {
        let s = ACCIDENTAL_GLYPH_SCALE;
        let path = vexflow_outline_to_svg(NATURAL_GLYPH, s, x, y);
        self.elements.push(format!(
            r#"<path d="{}" fill="{}" stroke="none"/>"#,
            path, NOTE_COLOR
        ));
    }

    /// Render a double-sharp accidental glyph using VexFlow font outline.
    fn double_sharp_glyph(&mut self, x: f64, y: f64) {
        let s = ACCIDENTAL_GLYPH_SCALE;
        let path = vexflow_outline_to_svg(DOUBLE_SHARP_GLYPH, s, x, y);
        self.elements.push(format!(
            r#"<path d="{}" fill="{}" stroke="none"/>"#,
            path, NOTE_COLOR
        ));
    }

    /// Render a double-flat accidental glyph using VexFlow font outline.
    fn double_flat_glyph(&mut self, x: f64, y: f64) {
        let s = ACCIDENTAL_GLYPH_SCALE;
        let path = vexflow_outline_to_svg(DOUBLE_FLAT_GLYPH, s, x, y);
        self.elements.push(format!(
            r#"<path d="{}" fill="{}" stroke="none"/>"#,
            path, NOTE_COLOR
        ));
    }
}
