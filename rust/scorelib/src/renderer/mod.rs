//! Score renderer — converts a parsed Score into SVG output.
//!
//! The renderer computes its own layout from the musical content (pitch,
//! duration, time signature) and produces a self-contained SVG string
//! that can be displayed in any SVG-capable view.

mod constants;
mod glyphs;
mod svg_builder;
mod beat_map;
mod lyrics;
mod slurs;
mod notes;
mod staff;
mod layout;

use crate::model::*;
use constants::*;
use svg_builder::{SvgBuilder, empty_svg};
use beat_map::note_x_positions_from_beat_map;
use lyrics::*;
use slurs::SlurStart;
use notes::render_notes;
use staff::*;
use layout::*;

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
        /// Active octave-shift display offset (e.g. -1 for 8va, +1 for 8vb)
        octave_shift: i32,
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

            PartState { clefs, key, time, divisions, transpose_octave, octave_shift: 0 }
        })
        .collect();

    // Open slurs that carry across systems, keyed by (part_idx, staff_num, slur_number)
    let mut global_open_slurs: std::collections::HashMap<(usize, usize, i32), SlurStart> =
        std::collections::HashMap::new();

    // Render each system
    for system in &layout.systems {
        let system_y = system.y;

        // Pre-update part states from the first measure of this system
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
                    // Update octave-shift state from directions
                    let ps = &mut part_states[pidx];
                    for dir in &measure.directions {
                        if let Some(ref ost) = dir.octave_shift_type {
                            match ost.as_str() {
                                "down" => { ps.octave_shift = -(dir.octave_shift_size / 8); }
                                "up" => { ps.octave_shift = dir.octave_shift_size / 8; }
                                "stop" => { ps.octave_shift = 0; }
                                _ => {}
                            }
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

                render_staff_lines(&mut svg, PAGE_MARGIN_LEFT, system.x_end, staff_y);

                if system.show_clef {
                    if let Some(ref clef) = ps.clefs[staff_num] {
                        render_clef(&mut svg, PAGE_MARGIN_LEFT + 5.0, staff_y, clef);
                    }
                }

                let key_x = PAGE_MARGIN_LEFT + CLEF_SPACE;
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

            // Brace for multi-staff parts
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

            svg.line(
                PAGE_MARGIN_LEFT, top_y, PAGE_MARGIN_LEFT, bottom_y,
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
                    PAGE_MARGIN_LEFT - 10.0, top_staff_y - 8.0, color, measure_num
                ));
            }
        }

        // ── Pre-compute lyrics baseline for this system ──
        let mut system_lowest_y: f64 = system_y + STAFF_HEIGHT;
        for ml_pre in &system.measures {
            for part_info in &system.parts {
                let pidx = part_info.part_idx;
                let ps = &part_states[pidx];
                if ml_pre.measure_idx >= score.parts[pidx].measures.len() {
                    continue;
                }
                let measure = &score.parts[pidx].measures[ml_pre.measure_idx];
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
                if ml_scan.measure_idx < score.parts[pidx].measures.len() {
                    let count = score.parts[pidx].measures[ml_scan.measure_idx].directions.iter()
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
                if ml.measure_idx < score.parts[pidx].measures.len() {
                    score.parts[pidx].measures[ml.measure_idx].notes.iter()
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
            for staff_num in 1..=part_info.num_staves {
                let mut staff_slurs = std::collections::HashMap::new();
                let keys_to_remove: Vec<(usize, usize, i32)> = global_open_slurs.keys()
                    .filter(|&&(p, s, _)| p == pidx && s == staff_num)
                    .cloned()
                    .collect();
                for key in keys_to_remove {
                    if let Some(start) = global_open_slurs.remove(&key) {
                        let staff_y = system_y
                            + part_info.y_offset
                            + (staff_num as f64 - 1.0) * (STAFF_HEIGHT + GRAND_STAFF_GAP);
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
                                ps.octave_shift = -(dir.octave_shift_size / 8);
                            }
                            "up" => {
                                ps.octave_shift = dir.octave_shift_size / 8;
                            }
                            _ => {} // "stop" handled after notes
                        }
                    }
                }

                for staff_num in 1..=part_info.num_staves {
                    let staff_y = system_y
                        + part_info.y_offset
                        + (staff_num as f64 - 1.0) * (STAFF_HEIGHT + GRAND_STAFF_GAP);

                    // ── Inline key/time signature changes ──
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

                    // ── Directions (only on top staff of first part) ──
                    if staff_num == 1 && pidx == parts_staves[0].0 {
                        let mut below_word_idx: usize = 0;
                        let mut above_word_idx: usize = 0;
                        for dir in &measure.directions {
                            if dir.sound_tempo.is_some() || dir.metronome.is_some() {
                                render_tempo_marking(&mut svg, mx + 4.0, staff_y, dir);
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
                    if staff_num == 1 && pidx == parts_staves[0].0 {
                        render_harmonies(&mut svg, measure, mx, mw, staff_y);
                    }

                    // Notes and rests for this staff
                    let staff_filter = if part_info.num_staves > 1 {
                        Some(staff_num as i32)
                    } else {
                        None
                    };

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

                    // Slurs
                    {
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
                    }

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

        // ── End-of-system slur handling ──
        for ((pidx, staff_num), staff_slurs) in &system_open_slurs {
            if !staff_slurs.is_empty() {
                slurs::render_open_slur_continuations(&mut svg, staff_slurs, system.x_end);
                for (&slur_num, start) in staff_slurs {
                    global_open_slurs.insert((*pidx, *staff_num, slur_num), start.clone());
                }
            }
        }
    }

    svg.build()
}

// ═══════════════════════════════════════════════════════════════════════
// Playback map helpers — extract measure/system positions for cursor sync
// ═══════════════════════════════════════════════════════════════════════

/// Compute the visual position of each measure and system in the SVG.
///
/// Returns two vectors:
/// - Measures: `(measure_idx, x, width, system_idx, beat_x_map)` for each measure
/// - Systems: `(y, height)` for each system (line of music)
///
/// The `beat_x_map` is a `Vec<(f64, f64)>` of `(beat_time_in_quarters, svg_x)`
/// pairs for each unique rhythmic onset in the measure, enabling note-level
/// cursor positioning.
pub fn compute_measure_positions(
    score: &Score,
    page_width: Option<f64>,
) -> (Vec<(usize, f64, f64, usize, Vec<(f64, f64)>)>, Vec<(f64, f64)>) {
    let page_width = match page_width {
        Some(w) if w > 0.0 => w,
        _ => DEFAULT_PAGE_WIDTH,
    };

    if score.parts.is_empty() {
        return (Vec::new(), Vec::new());
    }

    let parts_staves: Vec<(usize, usize)> = score
        .parts
        .iter()
        .enumerate()
        .map(|(i, part)| (i, detect_staves(part)))
        .collect();

    let layout = compute_layout(score, &parts_staves, page_width);

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

        system_positions.push((system.y, y_offset));

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
