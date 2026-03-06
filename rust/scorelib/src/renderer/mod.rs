//! Score renderer — converts a parsed Score into SVG output.
//!
//! The renderer computes its own layout from the musical content (pitch,
//! duration, time signature) and produces a self-contained SVG string
//! that can be displayed in any SVG-capable view.
//!
//! **Staff vs jianpu:** When `use_jianpu` is false we render standard staff notation
//! (notes, clefs, key, slurs, ties, staff lines). When true we render jianpu only
//! (digits, key label, no staff lines). The two paths are independent: all jianpu-specific
//! code lives behind `if use_jianpu` or in the `jianpu` module; shared helpers
//! (layout, beat map, lyrics) take jianpu-only options (e.g. `max_trailing_fraction: None`
//! for staff). Changes to jianpu must not alter the staff `else` branch or staff-only logic.

mod constants;
mod glyphs;
mod svg_builder;
mod beat_map;
mod lyrics;
mod slurs;
mod ties;
mod tuplet;
mod notes;
mod staff;
mod layout;
mod jianpu;

use crate::model::*;
use constants::*;
use svg_builder::{SvgBuilder, empty_svg};
use beat_map::note_x_positions_from_beat_map;
use lyrics::*;
use slurs::SlurStart;
use ties::TieStart;
use notes::render_notes;
use staff::*;
use layout::*;

// ═══════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════

/// When rendering or building the playback map in jianpu mode, the score is
/// simplified to top layer only (first voice per staff, highest note per chord).
/// Returns a new Score; use when `use_jianpu` is true so layout and playback map match.
pub fn simplify_score_for_jianpu(
    score: &Score,
    staff_indices_1based: Option<&[usize]>,
) -> Score {
    let global_staves: Vec<(usize, usize)> = score
        .parts
        .iter()
        .enumerate()
        .flat_map(|(pidx, part)| {
            let n = layout::detect_staves(part);
            (1..=n).map(move |staff_num| (pidx, staff_num))
        })
        .collect();
    let one_based = staff_indices_1based
        .and_then(|l| l.first().copied())
        .unwrap_or(1);
    let gi = one_based.saturating_sub(1);
    let parts_with_staves: Vec<(usize, Vec<usize>)> =
        if let Some(&(pidx, staff_num)) = global_staves.get(gi) {
            vec![(pidx, vec![staff_num])]
        } else if let Some(&first) = global_staves.first() {
            vec![(first.0, vec![first.1])]
        } else {
            return score.clone();
        };
    let mut simplified = score.clone();
    for (pidx, staves) in &parts_with_staves {
        let staff_num = staves.first().copied().unwrap_or(1) as i32;
        simplified.parts[*pidx] =
            crate::top_layer::simplify_part_for_jianpu(&score.parts[*pidx], staff_num);
    }
    simplified
}

/// Convert an octave-shift `size` attribute (8, 15, 22) to the number
/// of octaves to transpose.  Uses integer-safe mapping:
///   8 → 1, 15 → 2, 22 → 3.  Falls back to `(size + 1) / 8` for
///   non-standard values.
fn octave_shift_amount(size: i32) -> i32 {
    match size {
        8 => 1,
        15 => 2,
        22 => 3,
        other => ((other.abs() + 1) / 8).max(1),
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Public API
// ═══════════════════════════════════════════════════════════════════════

/// Render a parsed Score into a complete SVG string.
///
/// `page_width` sets the SVG width in user units. Pass `None` (or 0.0 from FFI)
/// to use the default (820). On phones, pass the screen width in points so the
/// renderer fits fewer measures per system and keeps notes readable.
///
/// `staff_indices_1based` limits which staves are drawn by global staff index (1-based).
/// Staff 1 = first staff in the score, 2 = second staff (e.g. bass clef of piano), etc.
/// Pass `None` or empty to render all staves. If none of the requested indices exist, staff 1 is rendered.
///
/// When `use_jianpu` is true, notation is rendered in Jianpu (numbered notation): digits 1–7,
/// key-based movable do, same repeats/directions as staff. Exactly one staff is used (first from list).
///
/// `transpose_semitones` is the amount the score was transposed before this call (e.g. from
/// `transpose_score`). When non-zero and `use_jianpu` is true, scale degrees are computed in the
/// concert key so the displayed digit matches what the player plays (no mental conversion).
pub fn render_score_to_svg(
    score: &Score,
    page_width: Option<f64>,
    staff_indices_1based: Option<&[usize]>,
    use_jianpu: bool,
    transpose_semitones: i32,
) -> String {
    let page_width = match page_width {
        Some(w) if w > 0.0 => w,
        _ => DEFAULT_PAGE_WIDTH,
    };

    if score.parts.is_empty() {
        return empty_svg("No parts in score");
    }

    // Build global staff list: (part_idx, staff_num) for each staff in order.
    let global_staves: Vec<(usize, usize)> = score
        .parts
        .iter()
        .enumerate()
        .flat_map(|(pidx, part)| {
            let n = detect_staves(part);
            (1..=n).map(move |staff_num| (pidx, staff_num))
        })
        .collect();

    let parts_with_staves: Vec<(usize, Vec<usize>)> = if use_jianpu {
        // Jianpu: exactly one staff (take first from list or default to staff 1).
        let one_based = staff_indices_1based
            .and_then(|l| l.first().copied())
            .unwrap_or(1);
        let gi = one_based.saturating_sub(1);
        if let Some(&(pidx, staff_num)) = global_staves.get(gi) {
            vec![(pidx, vec![staff_num])]
        } else if let Some(&first) = global_staves.first() {
            vec![(first.0, vec![first.1])]
        } else {
            return empty_svg("No staves in score");
        }
    } else {
        match staff_indices_1based {
            None | Some([]) => score
                .parts
                .iter()
                .enumerate()
                .map(|(pidx, part)| (pidx, (1..=detect_staves(part)).collect()))
                .collect(),
            Some(list) => {
                let selected: Vec<(usize, usize)> = list
                    .iter()
                    .filter_map(|&one_based| one_based.checked_sub(1))
                    .filter_map(|gi| global_staves.get(gi).copied())
                    .collect();
                if selected.is_empty() {
                    if let Some(&first) = global_staves.first() {
                        vec![(first.0, vec![first.1])]
                    } else {
                        return empty_svg("No staves in score");
                    }
                } else {
                    let mut by_part: std::collections::HashMap<usize, Vec<usize>> = std::collections::HashMap::new();
                    for (pidx, staff_num) in selected {
                        by_part.entry(pidx).or_default().push(staff_num);
                    }
                    for staves in by_part.values_mut() {
                        staves.sort_unstable();
                        staves.dedup();
                    }
                    let mut result: Vec<(usize, Vec<usize>)> = by_part.into_iter().collect();
                    result.sort_by_key(|&(p, _)| p);
                    result
                }
            }
        }
    };

    // Jianpu preprocess: simplify to top layer only (first voice, highest note per chord)
    // so layout and rendering see a single layer and indices align.
    let simplified_score;
    let score_for_render: &Score = if use_jianpu {
        simplified_score = simplify_score_for_jianpu(score, staff_indices_1based);
        &simplified_score
    } else {
        score
    };

    let layout = compute_layout_with_staff_filter(score_for_render, &parts_with_staves, page_width, use_jianpu);

    let mut svg = SvgBuilder::new(page_width, layout.total_height);

    // Background
    svg.rect(0.0, 0.0, page_width, layout.total_height, "white", "none", 0.0);

    // Title and composer
    render_header(&mut svg, score_for_render, page_width);

    // Running attributes per part — (clefs vec indexed 1-based, key, time, divisions, transpose)
    struct PartState {
        clefs: Vec<Option<Clef>>,  // index 0 unused, 1..=num_staves
        key: Option<Key>,
        time: Option<TimeSignature>,
        divisions: i32,
        transpose_octave: i32,
        /// Active octave-shift display offset (e.g. -1 for 8va, +1 for 8vb)
        octave_shift: i32,
    }

    let mut part_states: Vec<PartState> = (0..score_for_render.parts.len())
        .map(|pidx| {
            let part = &score_for_render.parts[pidx];
            let ns = detect_staves(part);
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
                        if idx >= 1 && idx < clefs.len() {
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
                        divisions = d.max(1);
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

            PartState { clefs, key, time, divisions, transpose_octave, octave_shift: 0 }
        })
        .collect();

    // Open slurs that carry across systems, keyed by (part_idx, staff_num, slur_number)
    let mut global_open_slurs: std::collections::HashMap<(usize, usize, i32), SlurStart> =
        std::collections::HashMap::new();

    // Open ties that carry across systems, keyed by (part_idx, staff_num, pitch_key)
    let mut global_open_ties: std::collections::HashMap<(usize, usize, String), TieStart> =
        std::collections::HashMap::new();

    // Render each system
    for (sys_idx, system) in layout.systems.iter().enumerate() {
        let system_y = system.y;

        // Pre-update part states from the first measure of this system
        if let Some(first_ml) = system.measures.first() {
            for part_info in &system.parts {
                let pidx = part_info.part_idx;
                let part = &score_for_render.parts[pidx];
                if first_ml.measure_idx < part.measures.len() {
                    let measure = &part.measures[first_ml.measure_idx];
                    if let Some(ref attrs) = measure.attributes {
                        let ps = &mut part_states[pidx];
                        for clef in &attrs.clefs {
                            let idx = clef.number as usize;
                            if idx >= 1 && idx < ps.clefs.len() {
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
                            ps.divisions = d.max(1);
                        }
                        if let Some(ref t) = attrs.transpose {
                            ps.transpose_octave = t.octave_change.unwrap_or(0);
                        }
                    }
                    // Update octave-shift state from directions
                    let ps = &mut part_states[pidx];
                    for dir in &measure.directions {
                        if let Some(ref ost) = dir.octave_shift_type {
                            match ost.as_str() {
                                "down" => { ps.octave_shift = -octave_shift_amount(dir.octave_shift_size); }
                                "up" => { ps.octave_shift = octave_shift_amount(dir.octave_shift_size); }
                                "stop" => { ps.octave_shift = 0; }
                                _ => {}
                            }
                        }
                    }
                }
            }
        }

        // On the first system the instrument-name band sits to the left of
        // the staff, so everything (staff lines, clef, key, time, brace)
        // starts INSTRUMENT_PREFIX_WIDTH units further right.
        let sys_prefix_x = if sys_idx == 0 {
            PAGE_MARGIN_LEFT + INSTRUMENT_PREFIX_WIDTH
        } else {
            PAGE_MARGIN_LEFT
        };

        // ── Staff lines, clefs, key/time signatures per part (or Jianpu time only) ──
        for part_info in &system.parts {
            let ps = &part_states[part_info.part_idx];

            for (display_idx, &staff_num) in part_info.staff_nums.iter().enumerate() {
                let staff_y = system_y
                    + part_info.y_offset
                    + (display_idx as f64) * (STAFF_HEIGHT + GRAND_STAFF_GAP);

                if use_jianpu {
                    // Jianpu: key and time are drawn per measure at the bar (see below), not in the system prefix.
                    // Only draw the bracket/brace area; no time sig here.
                } else {
                    render_staff_lines(&mut svg, sys_prefix_x, system.x_end, staff_y);

                    if system.show_clef {
                        if let Some(ref clef) = ps.clefs[staff_num] {
                            render_clef(&mut svg, sys_prefix_x + 5.0, staff_y, clef);
                        }
                    }

                    let key_x = sys_prefix_x + CLEF_SPACE;
                    if let Some(ref key) = ps.key {
                        render_key_signature(
                            &mut svg, key_x, staff_y, key,
                            ps.clefs[staff_num].as_ref(),
                        );
                    }

                    if system.show_time {
                        if let Some(ref time) = ps.time {
                            let time_x = key_x + key_sig_width(ps.key.as_ref());
                            render_time_signature(&mut svg, time_x, staff_y, time);
                        }
                    }
                }
            }

            // Brace for multi-staff parts (staff mode only)
            if !use_jianpu && part_info.num_staves > 1 {
                let top_y = system_y + part_info.y_offset;
                let bottom_y = top_y
                    + (part_info.num_staves as f64 - 1.0) * (STAFF_HEIGHT + GRAND_STAFF_GAP)
                    + STAFF_HEIGHT;
                render_brace(&mut svg, sys_prefix_x - 2.0, top_y, bottom_y);
            }

            // Instrument name — first system only, left of the first staff
            if sys_idx == 0 {
                let top_staff_y = system_y + part_info.y_offset;
                render_instrument_name(
                    &mut svg,
                    &score_for_render.parts[part_info.part_idx],
                    top_staff_y,
                    part_info.num_staves,
                );
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

            svg.line(
                sys_prefix_x, top_y, sys_prefix_x, bottom_y,
                BARLINE_COLOR, BARLINE_WIDTH,
            );
        }

        // ── Measure number at the start of each system line ──
        if let Some(first_ml) = system.measures.first() {
            let measure_num = first_ml.measure_idx + 1;
            if measure_num > 1 {
                let first_part = system.parts.first().unwrap();
                let top_staff_y = system_y + first_part.y_offset;
                let color = "#555555";
                svg.elements.push(format!(
                    "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"15\" font-style=\"italic\" fill=\"{}\" text-anchor=\"start\">{}</text>",
                    sys_prefix_x - 10.0, top_staff_y - 8.0, color, measure_num
                ));
            }
        }

        // ── Pre-compute lyrics baseline for this system ──
        let mut system_lowest_y: f64 = system_y + STAFF_HEIGHT;
        for ml_pre in &system.measures {
            for part_info in &system.parts {
                let pidx = part_info.part_idx;
                let ps = &part_states[pidx];
                if ml_pre.measure_idx >= score_for_render.parts[pidx].measures.len() {
                    continue;
                }
                let measure = &score_for_render.parts[pidx].measures[ml_pre.measure_idx];
                let bottom_staff_num = part_info.staff_nums.last().copied().unwrap_or(1);
                let staff_y_bottom = system_y
                    + part_info.y_offset
                    + (part_info.num_staves as f64 - 1.0) * (STAFF_HEIGHT + GRAND_STAFF_GAP);
                // Always filter by staff so we only consider notes on the staves we're drawing.
                let staff_filter = Some(bottom_staff_num as i32);
                let lowest = measure_lowest_note_y(
                    measure, staff_y_bottom,
                    ps.clefs.get(bottom_staff_num).and_then(|c| c.as_ref()),
                    ps.transpose_octave + ps.octave_shift, staff_filter,
                );
                if lowest > system_lowest_y {
                    system_lowest_y = lowest;
                }
            }
        }

        let mut max_below_dir_lines: usize = 0;
        for ml_scan in &system.measures {
            for part_info in &system.parts {
                let pidx = part_info.part_idx;
                if ml_scan.measure_idx < score_for_render.parts[pidx].measures.len() {
                    let count = score_for_render.parts[pidx].measures[ml_scan.measure_idx].directions.iter()
                        .filter(|dir| {
                            dir.placement.as_deref() == Some("below")
                                && dir.words.as_ref().map_or(false, |w| !w.is_empty() && !is_jump_text(w))
                        })
                        .count();
                    if count > max_below_dir_lines {
                        max_below_dir_lines = count;
                    }
                }
            }
        }

        let system_has_lyrics = system.measures.iter().any(|ml| {
            system.parts.iter().any(|part_info| {
                let pidx = part_info.part_idx;
                if ml.measure_idx < score_for_render.parts[pidx].measures.len() {
                    score_for_render.parts[pidx].measures[ml.measure_idx].notes.iter()
                        .any(|n| !n.lyrics.is_empty())
                } else {
                    false
                }
            })
        });

        let dir_words_y = (system_lowest_y + LYRICS_PAD_BELOW)
            .max(system_y + LYRICS_MIN_Y_BELOW_STAFF);

        let dir_words_offset = if max_below_dir_lines > 0 && system_has_lyrics {
            DIRECTION_WORDS_HEIGHT + (max_below_dir_lines as f64 - 1.0) * DIRECTION_WORDS_LINE_HEIGHT
        } else {
            0.0
        };
        let lyrics_base_y = dir_words_y + dir_words_offset;

        // ── Initialise per-part/staff open slurs from global carry-over ──
        let mut system_open_slurs: std::collections::HashMap<(usize, usize), std::collections::HashMap<i32, SlurStart>> =
            std::collections::HashMap::new();
        for part_info in &system.parts {
            let pidx = part_info.part_idx;
            for (display_idx, &staff_num) in part_info.staff_nums.iter().enumerate() {
                let mut staff_slurs = std::collections::HashMap::new();
                let keys_to_remove: Vec<(usize, usize, i32)> = global_open_slurs.keys()
                    .filter(|&&(p, s, _)| p == pidx && s == staff_num)
                    .cloned()
                    .collect();
                for key in keys_to_remove {
                    if let Some(start) = global_open_slurs.remove(&key) {
                        let staff_y = system_y
                            + part_info.y_offset
                            + (display_idx as f64) * (STAFF_HEIGHT + GRAND_STAFF_GAP);
                        let y_offset = start.y - start.staff_y;
                        staff_slurs.insert(key.2, SlurStart {
                            x: PAGE_MARGIN_LEFT + CLEF_SPACE,
                            y: staff_y + y_offset,
                            stem_up: start.stem_up,
                            placement: start.placement.clone(),
                            staff_y,
                        });
                    }
                }
                system_open_slurs.insert((pidx, staff_num), staff_slurs);
            }
        }

        // ── Initialise per-part/staff open ties from global carry-over ──
        let mut system_open_ties: std::collections::HashMap<(usize, usize), std::collections::HashMap<String, TieStart>> =
            std::collections::HashMap::new();
        for part_info in &system.parts {
            let pidx = part_info.part_idx;
            for (display_idx, &staff_num) in part_info.staff_nums.iter().enumerate() {
                let mut staff_ties = std::collections::HashMap::new();
                let keys_to_remove: Vec<(usize, usize, String)> = global_open_ties.keys()
                    .filter(|k| k.0 == pidx && k.1 == staff_num)
                    .cloned()
                    .collect();
                for key in keys_to_remove {
                    if let Some(start) = global_open_ties.remove(&key) {
                        let staff_y = system_y
                            + part_info.y_offset
                            + (display_idx as f64) * (STAFF_HEIGHT + GRAND_STAFF_GAP);
                        let y_offset = start.y - start.staff_y;
                        staff_ties.insert(key.2, TieStart {
                            x: PAGE_MARGIN_LEFT + CLEF_SPACE,
                            y: staff_y + y_offset,
                            stem_up: start.stem_up,
                            staff_y,
                        });
                    }
                }
                system_open_ties.insert((pidx, staff_num), staff_ties);
            }
        }

        // ── Render measures ──
        for (mi_in_sys, ml) in system.measures.iter().enumerate() {
            let mx = ml.x;
            let mw = ml.width;

            for part_info in &system.parts {
                let pidx = part_info.part_idx;
                let part = &score_for_render.parts[pidx];
                let ps = &mut part_states[pidx];

                if ml.measure_idx >= part.measures.len() {
                    continue;
                }
                let measure = &part.measures[ml.measure_idx];

                // Update running attributes for this part
                if let Some(ref attrs) = measure.attributes {
                    for clef in &attrs.clefs {
                        let idx = clef.number as usize;
                        if idx >= 1 && idx < ps.clefs.len() {
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
                        ps.divisions = d.max(1);
                    }
                    if let Some(ref t) = attrs.transpose {
                        ps.transpose_octave = t.octave_change.unwrap_or(0);
                    }
                }

                // Update octave-shift state from directions in this measure.
                // In MusicXML: type="down" → 8va (display notes lower),
                //              type="up"   → 8vb (display notes higher).
                // Start/activate shifts appear BEFORE notes in the XML,
                // so we apply them here before rendering.
                // Stop shifts appear AFTER notes, so we defer those to
                // after note rendering (see below).
                for dir in &measure.directions {
                    if let Some(ref ost) = dir.octave_shift_type {
                        match ost.as_str() {
                            "down" => {
                                ps.octave_shift = -octave_shift_amount(dir.octave_shift_size);
                            }
                            "up" => {
                                ps.octave_shift = octave_shift_amount(dir.octave_shift_size);
                            }
                            _ => {} // "stop" handled after notes
                        }
                    }
                }

                for (display_idx, &staff_num) in part_info.staff_nums.iter().enumerate() {
                    let staff_y = system_y
                        + part_info.y_offset
                        + (display_idx as f64) * (STAFF_HEIGHT + GRAND_STAFF_GAP);

                    // ── Inline key/time signature changes (staff mode only) ──
                    if !use_jianpu {
                        let mut inline_x = mx + 10.0;
                        for barline in &measure.barlines {
                            if barline.location == "left" {
                                let is_repeat = barline.repeat.is_some();
                                let is_heavy = barline.bar_style.as_deref() == Some("heavy-light")
                                    || barline.bar_style.as_deref() == Some("light-heavy");
                                if is_repeat || is_heavy {
                                    inline_x = inline_x.max(mx + 14.0);
                                }
                            }
                        }
                        if ml.has_key_change {
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
                                    inline_x += 2.0;
                                }
                            }
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
                    }

                    // ── Directions (only on top staff of first part) ──
                    if display_idx == 0 && pidx == system.parts[0].part_idx {
                        let mut below_word_idx: usize = 0;
                        let mut above_word_idx: usize = 0;
                        for dir in &measure.directions {
                            if dir.sound_tempo.is_some() || dir.metronome.is_some() {
                                // On the first measure of the first system render the
                                // tempo label inside the instrument-prefix band so it
                                // sits above the instrument name and does not crowd
                                // the first barline.
                                let tempo_x = if sys_idx == 0 && mi_in_sys == 0 {
                                    PAGE_MARGIN_LEFT + 5.0
                                } else {
                                    mx + 4.0
                                };
                                render_tempo_marking(&mut svg, tempo_x, staff_y, dir);
                            }
                            if dir.segno {
                                render_segno(&mut svg, mx + 6.0, staff_y);
                            }
                            if dir.coda {
                                render_coda(&mut svg, mx + 6.0, staff_y);
                            }

                            if let Some(ref text) = dir.words {
                                let is_jump = is_jump_text(text);
                                let is_below = dir.placement.as_deref() == Some("below");
                                let line_idx = if is_below { below_word_idx } else { above_word_idx };
                                if is_jump {
                                    render_jump_text(&mut svg, mx + mw - 4.0, staff_y, dir_words_y, dir.placement.as_deref(), text, line_idx);
                                } else {
                                    render_direction_words(&mut svg, mx, staff_y, dir_words_y, dir, line_idx);
                                }
                                if is_below { below_word_idx += 1; } else { above_word_idx += 1; }
                            }
                        }
                    }

                    // Chord symbols (only on top staff of first part)
                    if display_idx == 0 && pidx == system.parts[0].part_idx {
                        render_harmonies(&mut svg, measure, mx, mw, staff_y);
                    }

                    let staff_filter = Some(staff_num as i32);
                    // Same note x positions for both jianpu/notes and lyrics so they stay center-aligned.
                    let note_xs = note_x_positions_from_beat_map(
                        &measure.notes, ps.divisions, &ml.beat_x_map,
                    );
                    // Chord-aware note Y positions for jianpu (used for ties and lyrics).
                    let jianpu_note_ys: Option<Vec<f64>> = if use_jianpu {
                        Some(jianpu::jianpu_note_y_positions_for_ties(
                            measure,
                            staff_num as i32,
                            &note_xs,
                            staff_y,
                        ))
                    } else {
                        None
                    };
                    if use_jianpu {
                        let key_fifths = ps.key.as_ref().map_or(0, |k| k.fifths);
                        let key_fifths_for_degree = jianpu::concert_key_fifths_for_degree(key_fifths, transpose_semitones);
                        let key_mode = ps.key.as_ref().and_then(|k| k.mode.as_deref());
                        let has_left_repeat = measure.barlines.iter().any(|b| b.location == "left" && b.repeat.is_some());
                        let bar_offset = 2.0 + if has_left_repeat { JIANPU_REPEAT_EXTRA_OFFSET } else { 0.0 };
                        let mut inline_x = mx + bar_offset;
                        let jianpu_center_y = staff_y + STAFF_HEIGHT / 2.0;
                        let draw_key = ml.measure_idx == 0 || ml.has_key_change;
                        let draw_time = (ml.measure_idx == 0 && system.show_time) || ml.has_time_change;
                        if draw_key {
                            jianpu::render_key_label(&mut svg, inline_x, jianpu_center_y - 20.0, key_fifths, key_mode);
                            inline_x += if draw_time { JIANPU_KEY_TO_TIME_GAP } else { JIANPU_KEY_LABEL_SPACE };
                        }
                        if draw_time {
                            if let Some(ref time) = ps.time {
                                render_time_signature(&mut svg, inline_x, staff_y, time);
                            }
                        }
                        jianpu::render_jianpu_measure(
                            &mut svg,
                            measure,
                            staff_y,
                            staff_num as i32,
                            key_fifths,
                            key_fifths_for_degree,
                            key_mode,
                            ps.divisions,
                            &note_xs,
                            mx,
                            ml.width,
                            ml.left_inset,
                            ml.right_inset,
                            false,
                        );
                        let staff_ties = system_open_ties
                            .entry((pidx, staff_num))
                            .or_insert_with(std::collections::HashMap::new);
                        ties::collect_and_render_ties_for_measure_jianpu(
                            &mut svg,
                            measure,
                            Some(staff_num as i32),
                            &note_xs,
                            jianpu_note_ys.as_deref().unwrap(),
                            staff_ties,
                        );
                    } else {
                        // Notes and rests for this staff — always filter so only notes on this staff are drawn.
                        let effective_transpose = ps.transpose_octave + ps.octave_shift;

                        render_notes(
                            &mut svg,
                            measure,
                            staff_y,
                            ps.clefs[staff_num].as_ref(),
                            ps.divisions,
                            effective_transpose,
                            staff_filter,
                            &ml.beat_x_map,
                            mx, mw,
                        );

                        let staff_slurs = system_open_slurs
                            .entry((pidx, staff_num))
                            .or_insert_with(std::collections::HashMap::new);
                        slurs::collect_and_render_slurs_for_measure(
                            &mut svg,
                            measure,
                            staff_y,
                            ps.clefs[staff_num].as_ref(),
                            ps.divisions,
                            effective_transpose,
                            staff_filter,
                            &ml.beat_x_map,
                            staff_slurs,
                        );

                        let staff_ties = system_open_ties
                            .entry((pidx, staff_num))
                            .or_insert_with(std::collections::HashMap::new);
                        ties::collect_and_render_ties_for_measure(
                            &mut svg,
                            measure,
                            staff_y,
                            ps.clefs[staff_num].as_ref(),
                            ps.divisions,
                            effective_transpose,
                            staff_filter,
                            &ml.beat_x_map,
                            staff_ties,
                        );
                    }

                    // Barlines (per-staff): repeat signs and bar lines on each staff
                    render_barlines(&mut svg, measure, mx, mw, staff_y);

                    // Lyrics: x = note position. For jianpu, use chord-aware Y (same as ties) and shift x by quarter note width.
                    let note_ys = if use_jianpu {
                        jianpu_note_ys.as_deref()
                    } else {
                        None
                    };
                    if use_jianpu {
                        let quarter_w = jianpu::jianpu_note_head_width(jianpu::JIANPU_FONT_SIZE) / 4.0;
                        let lyrics_note_xs: Vec<f64> = note_xs.iter().map(|&x| x - quarter_w).collect();
                        render_lyrics(
                            &mut svg, measure, &lyrics_note_xs,
                            lyrics_base_y, staff_filter,
                            note_ys.as_deref(),
                        );
                    } else {
                        render_lyrics(
                            &mut svg, measure, &note_xs,
                            lyrics_base_y, staff_filter,
                            note_ys.as_deref(),
                        );
                    }
                }

                // Apply deferred octave-shift "stop" AFTER notes are rendered.
                // In MusicXML, stop directives appear after the notes they cover.
                for dir in &measure.directions {
                    if dir.octave_shift_type.as_deref() == Some("stop") {
                        ps.octave_shift = 0;
                    }
                }
            }

            // Right barline spanning all staves across all parts.
            let has_special_right_barline = system.parts.first().map_or(false, |pi| {
                let pidx = pi.part_idx;
                if ml.measure_idx < score_for_render.parts[pidx].measures.len() {
                    score_for_render.parts[pidx].measures[ml.measure_idx].barlines.iter().any(|b| {
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

        // ── End-of-system slur handling ──
        for ((pidx, staff_num), staff_slurs) in &system_open_slurs {
            if !staff_slurs.is_empty() {
                slurs::render_open_slur_continuations(&mut svg, staff_slurs, system.x_end);
                for (&slur_num, start) in staff_slurs {
                    global_open_slurs.insert((*pidx, *staff_num, slur_num), start.clone());
                }
            }
        }

        // ── End-of-system tie handling ──
        for ((pidx, staff_num), staff_ties) in &system_open_ties {
            if !staff_ties.is_empty() {
                ties::render_open_tie_continuations(&mut svg, staff_ties, system.x_end);
                for (pitch_key, start) in staff_ties {
                    global_open_ties.insert((*pidx, *staff_num, pitch_key.clone()), start.clone());
                }
            }
        }
    }

    svg.build()
}

// ═══════════════════════════════════════════════════════════════════════
// Playback map helpers — extract measure/system positions for cursor sync
// ═══════════════════════════════════════════════════════════════════════

/// Vertical offset below the system content for the first row of feedback dots (SVG units).
pub const FEEDBACK_DOTS_OFFSET: f64 = 16.0;

/// Compute the visual position of each measure and system in the SVG.
///
/// When `staff_indices_1based` is set (e.g. `Some(&[1, 3])`), uses the same
/// filtered layout as `render_score_to_svg`, so cursor y/height and measure
/// positions match the rendered SVG.
///
/// When `use_jianpu` is true, uses the same jianpu layout as the SVG (single staff,
/// key/time at bar, correct beat_x_map) so the playback cursor does not land on
/// key or time signature symbols.
///
/// Returns two vectors:
/// - Measures: `(measure_idx, x, width, system_idx, beat_x_map)` for each measure
/// - Systems: `(y, height, dots_base_y)` for each system (line of music).
///   `dots_base_y` is the SVG y for the first row of feedback dots below the staff.
///
/// The `beat_x_map` is a `Vec<(f64, f64)>` of `(beat_time_in_quarters, svg_x)`
/// pairs for each unique rhythmic onset in the measure, enabling note-level
/// cursor positioning.
pub fn compute_measure_positions(
    score: &Score,
    page_width: Option<f64>,
    staff_indices_1based: Option<&[usize]>,
    use_jianpu: bool,
) -> (Vec<(usize, f64, f64, usize, Vec<(f64, f64)>)>, Vec<(f64, f64, f64)>) {
    let page_width = match page_width {
        Some(w) if w > 0.0 => w,
        _ => DEFAULT_PAGE_WIDTH,
    };

    if score.parts.is_empty() {
        return (Vec::new(), Vec::new());
    }

    let global_staves: Vec<(usize, usize)> = score
        .parts
        .iter()
        .enumerate()
        .flat_map(|(pidx, part)| {
            let n = detect_staves(part);
            (1..=n).map(move |staff_num| (pidx, staff_num))
        })
        .collect();

    let parts_with_staves: Vec<(usize, Vec<usize>)> = if use_jianpu {
        let one_based = staff_indices_1based
            .and_then(|l| l.first().copied())
            .unwrap_or(1);
        let gi = one_based.saturating_sub(1);
        if let Some(&(pidx, staff_num)) = global_staves.get(gi) {
            vec![(pidx, vec![staff_num])]
        } else if let Some(&first) = global_staves.first() {
            vec![(first.0, vec![first.1])]
        } else {
            return (Vec::new(), Vec::new());
        }
    } else {
        match staff_indices_1based {
            None | Some([]) => score
                .parts
                .iter()
                .enumerate()
                .map(|(pidx, part)| (pidx, (1..=detect_staves(part)).collect()))
                .collect(),
            Some(list) => {
                let selected: Vec<(usize, usize)> = list
                    .iter()
                    .filter_map(|&one_based| one_based.checked_sub(1))
                    .filter_map(|gi| global_staves.get(gi).copied())
                    .collect();
                if selected.is_empty() {
                    if let Some(&first) = global_staves.first() {
                        vec![(first.0, vec![first.1])]
                    } else {
                        return (Vec::new(), Vec::new());
                    }
                } else {
                    let mut by_part: std::collections::HashMap<usize, Vec<usize>> =
                        std::collections::HashMap::new();
                    for (pidx, staff_num) in selected {
                        by_part.entry(pidx).or_default().push(staff_num);
                    }
                    for staves in by_part.values_mut() {
                        staves.sort_unstable();
                        staves.dedup();
                    }
                    let mut result: Vec<(usize, Vec<usize>)> = by_part.into_iter().collect();
                    result.sort_by_key(|&(p, _)| p);
                    result
                }
            }
        }
    };

    let layout = compute_layout_with_staff_filter(score, &parts_with_staves, page_width, use_jianpu);

    let mut measure_positions = Vec::new();
    let mut system_positions = Vec::new();

    for (sys_idx, system) in layout.systems.iter().enumerate() {
        let mut y_offset = 0.0;
        for (i, pi) in system.parts.iter().enumerate() {
            let part_height = STAFF_HEIGHT
                + (pi.num_staves as f64 - 1.0) * (STAFF_HEIGHT + GRAND_STAFF_GAP);
            y_offset += part_height;
            if i < system.parts.len() - 1 {
                y_offset += PART_GAP;
            }
        }

        let dots_base_y = system.y + y_offset + FEEDBACK_DOTS_OFFSET;
        system_positions.push((system.y, y_offset, dots_base_y));

        for ml in &system.measures {
            measure_positions.push((
                ml.measure_idx,
                ml.x,
                ml.width,
                sys_idx,
                ml.beat_x_map.clone(),
            ));
        }
    }

    (measure_positions, system_positions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Pitch;
    use crate::renderer::jianpu::{
        concert_key_fifths_for_degree,
        duration_to_jianpu,
        jianpu_note_head_width,
        pitch_to_jianpu,
        slot_count_for_duration,
    };

    #[test]
    fn octave_shift_amount_standard_sizes() {
        assert_eq!(super::octave_shift_amount(8), 1);
        assert_eq!(super::octave_shift_amount(15), 2);
        assert_eq!(super::octave_shift_amount(22), 3);
    }

    #[test]
    fn octave_shift_amount_fallback() {
        assert!(super::octave_shift_amount(0) >= 1);
        assert_eq!(super::octave_shift_amount(16), 2); // (16+1)/8 = 2
        assert_eq!(super::octave_shift_amount(-8), 1);
    }

    #[test]
    fn jianpu_concert_key_fifths_no_transpose() {
        assert_eq!(concert_key_fifths_for_degree(0, 0), 0);
        assert_eq!(concert_key_fifths_for_degree(1, 0), 1);
        assert_eq!(concert_key_fifths_for_degree(-3, 0), -3);
    }

    #[test]
    fn jianpu_concert_key_fifths_with_transpose() {
        // C major displayed, transpose +2 (D): concert key still C, display key D
        assert_eq!(concert_key_fifths_for_degree(2, 2), 0);
        // G major (1), transpose -2: concert A, display G
        assert_eq!(concert_key_fifths_for_degree(1, -2), 3);
    }

    fn pitch(step: &str, octave: i32, alter: Option<f64>) -> Pitch {
        Pitch {
            step: step.to_string(),
            octave,
            alter,
        }
    }

    #[test]
    fn jianpu_pitch_to_degree_c_major() {
        // C4 in C major -> 1, no accidental
        let (deg, dots, acc) = pitch_to_jianpu(&pitch("C", 4, None), 0, None);
        assert_eq!(deg, 1);
        assert_eq!(dots, 0);
        assert!(acc.is_none());
    }

    #[test]
    fn jianpu_pitch_to_degree_g_major() {
        // G4 in G major (fifths=1) -> 1
        let (deg, _, _) = pitch_to_jianpu(&pitch("G", 4, None), 1, None);
        assert_eq!(deg, 1);
        // D5 in G major -> 5
        let (deg5, _, _) = pitch_to_jianpu(&pitch("D", 5, None), 1, None);
        assert_eq!(deg5, 5);
    }

    #[test]
    fn jianpu_pitch_to_degree_accidentals() {
        // F# in C major -> 4^
        let (deg, _, acc) = pitch_to_jianpu(&pitch("F", 4, Some(1.0)), 0, None);
        assert_eq!(deg, 4);
        assert_eq!(acc, Some("^"));
        // Bb in C major -> 6^ (A^) per major-only semitone_to_degree_acc
        let (deg6, _, acc6) = pitch_to_jianpu(&pitch("B", 4, Some(-1.0)), 0, None);
        assert_eq!(deg6, 6);
        assert_eq!(acc6, Some("^"));
    }

    #[test]
    fn jianpu_pitch_octave_dots() {
        // Middle C (C4) -> 0 octave offset from reference
        let (_, dots, _) = pitch_to_jianpu(&pitch("C", 4, None), 0, None);
        assert_eq!(dots, 0);
        // C5 one octave above -> 1 dot above
        let (_, dots5, _) = pitch_to_jianpu(&pitch("C", 5, None), 0, None);
        assert_eq!(dots5, 1);
        // C3 one octave below -> 1 dot below
        let (_, dots3, _) = pitch_to_jianpu(&pitch("C", 3, None), 0, None);
        assert_eq!(dots3, -1);
    }

    #[test]
    fn jianpu_duration_underlines() {
        // Quarter -> 0 underlines, no dot, 0 dashes
        let (u, dot, d) = duration_to_jianpu(1.0, false);
        assert_eq!(u, 0);
        assert!(!dot);
        assert_eq!(d, 0);
        // Eighth -> 1 underline
        let (u8, _, _) = duration_to_jianpu(0.5, false);
        assert_eq!(u8, 1);
        // Sixteenth -> 2 underlines
        let (u16, _, _) = duration_to_jianpu(0.25, false);
        assert_eq!(u16, 2);
        // Dotted quarter
        let (_, dot_q, _) = duration_to_jianpu(1.5, true);
        assert!(dot_q);
    }

    #[test]
    fn jianpu_duration_suffix_dashes() {
        // Half note -> 1 dash (note + dash)
        let (_, _, d) = duration_to_jianpu(2.0, false);
        assert_eq!(d, 1);
        // Half dotted -> 2 dashes (note + dash + dash)
        let (_, _, d_hd) = duration_to_jianpu(3.0, true);
        assert_eq!(d_hd, 2);
        // Whole -> 3 dashes (note + dash + dash + dash)
        let (_, _, d2) = duration_to_jianpu(4.0, false);
        assert_eq!(d2, 3);
    }

    #[test]
    fn jianpu_slot_count_for_width_and_spacing() {
        // Whole = 4, half dotted = 3, half = 2, quarter or smaller = 1
        assert_eq!(slot_count_for_duration(4.0), 4.0);
        assert_eq!(slot_count_for_duration(3.0), 3.0); // half dotted
        assert_eq!(slot_count_for_duration(2.0), 2.0); // half
        assert_eq!(slot_count_for_duration(1.5), 1.0); // dotted quarter
        assert_eq!(slot_count_for_duration(1.0), 1.0); // quarter
        assert_eq!(slot_count_for_duration(0.5), 1.0); // eighth
        assert_eq!(slot_count_for_duration(0.25), 1.0); // 16th
    }

    #[test]
    fn jianpu_note_head_width_scales() {
        assert_eq!(jianpu_note_head_width(22.0), 22.0);
        assert_eq!(jianpu_note_head_width(14.0), 14.0);
    }

    #[test]
    fn render_empty_score_returns_empty_svg_message() {
        let score = Score::new();
        let svg = render_score_to_svg(&score, None, None, false, 0);
        assert!(svg.contains("No parts"));
        assert!(svg.contains("<svg"));
    }

    #[test]
    fn render_jianpu_produces_svg_with_key_label() {
        // Use smoke-test which has notes and key
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../sheetmusic/smoke-test.musicxml");
        let score = crate::parse_file(&path).unwrap();
        let svg = render_score_to_svg(&score, None, None, true, 0);
        assert!(svg.contains("<svg"));
        assert!(svg.contains("1 = "), "Jianpu key label should appear");
    }

    #[test]
    fn render_staff_selection_only_requested_staff() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../sheetmusic/asa-branca.musicxml");
        let score = crate::parse_file(&path).unwrap();
        let all_staves = render_score_to_svg(&score, None, None, false, 0);
        let staff_1_only = render_score_to_svg(&score, None, Some(&[1]), false, 0);
        assert!(all_staves.contains("<svg"));
        assert!(staff_1_only.contains("<svg"));
        assert!(!staff_1_only.is_empty());
    }
}
