//! Layout computation — determines how measures are grouped into systems
//! and how they are sized and positioned.

use crate::model::*;
use super::constants::*;
use super::jianpu;
use super::lyrics::*;
use super::beat_map::*;
use super::staff::is_jump_text;

// ═══════════════════════════════════════════════════════════════════════
// Layout structures
// ═══════════════════════════════════════════════════════════════════════

pub(super) struct ScoreLayout {
    pub(super) systems: Vec<SystemLayout>,
    pub(super) total_height: f64,
}

/// Info about one part's staves within a system.
/// When `staff_nums` is non-empty, only those staves (1-based) are drawn; otherwise 1..=num_staves.
pub(super) struct PartStaffInfo {
    pub(super) part_idx: usize,
    pub(super) y_offset: f64,
    pub(super) num_staves: usize,
    /// 1-based staff numbers to draw (e.g. [2] for bass clef only). Empty means draw all 1..=num_staves.
    pub(super) staff_nums: Vec<usize>,
}

pub(super) struct SystemLayout {
    pub(super) y: f64,
    #[allow(dead_code)]
    pub(super) x_start: f64,
    pub(super) x_end: f64,
    pub(super) measures: Vec<MeasureLayout>,
    pub(super) parts: Vec<PartStaffInfo>,
    pub(super) show_clef: bool,
    pub(super) show_time: bool,
    pub(super) total_staves: usize,
}

#[allow(dead_code)]
pub(super) struct MeasureLayout {
    pub(super) measure_idx: usize,
    pub(super) x: f64,
    pub(super) width: f64,
    pub(super) beat_x_map: Vec<(f64, f64)>,
    pub(super) has_key_change: bool,
    pub(super) has_time_change: bool,
    pub(super) prev_key_fifths: Option<i32>,
    pub(super) left_inset: f64,
    pub(super) right_inset: f64,
}

// ═══════════════════════════════════════════════════════════════════════
// Helper functions
// ═══════════════════════════════════════════════════════════════════════

/// Detect the number of staves in a part.
pub(super) fn detect_staves(part: &Part) -> usize {
    // Cap at 8 staves to prevent OOM from malformed input (e.g. negative
    // values wrapping to huge usize, or absurdly large stave counts).
    const MAX_STAVES: usize = 8;
    let mut max_staff = 1usize;
    for measure in &part.measures {
        if let Some(ref attrs) = measure.attributes {
            if let Some(s) = attrs.staves {
                if s > 0 {
                    max_staff = max_staff.max((s as usize).min(MAX_STAVES));
                }
            }
            for clef in &attrs.clefs {
                if clef.number > 0 {
                    max_staff = max_staff.max((clef.number as usize).min(MAX_STAVES));
                }
            }
        }
        for note in &measure.notes {
            if let Some(s) = note.staff {
                if s > 0 {
                    max_staff = max_staff.max((s as usize).min(MAX_STAVES));
                }
            }
        }
    }
    max_staff
}

pub(super) fn key_sig_width(key: Option<&Key>) -> f64 {
    match key {
        Some(k) if k.fifths > 0 => k.fifths as f64 * KEY_SIG_SHARP_SPACE,
        Some(k) if k.fifths < 0 => k.fifths.unsigned_abs() as f64 * KEY_SIG_FLAT_SPACE,
        _ => 0.0,
    }
}

pub(super) fn cancellation_natural_count(old_fifths: i32, new_fifths: i32) -> u32 {
    if old_fifths == 0 {
        return 0;
    }
    let same_direction = (old_fifths > 0 && new_fifths > 0)
        || (old_fifths < 0 && new_fifths < 0);

    if same_direction {
        let old_abs = old_fifths.unsigned_abs();
        let new_abs = new_fifths.unsigned_abs();
        if new_abs >= old_abs {
            0
        } else {
            old_abs - new_abs
        }
    } else {
        old_fifths.unsigned_abs()
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Main layout computation
// ═══════════════════════════════════════════════════════════════════════

pub(super) fn compute_layout_with_staff_filter(
    score: &Score,
    parts_with_staves: &[(usize, Vec<usize>)],
    page_width: f64,
    use_jianpu: bool,
) -> ScoreLayout {
    let content_width = page_width - PAGE_MARGIN_LEFT - PAGE_MARGIN_RIGHT;
    let mut systems: Vec<SystemLayout> = Vec::new();

    let has_composer = score.composer.is_some() || score.arranger.is_some();
    let has_early_chords = score.parts.iter().any(|p| {
        p.measures.iter().take(8).any(|m| !m.harmonies.is_empty())
    });
    let first_system_top = if has_composer && has_early_chords {
        FIRST_SYSTEM_TOP + 18.0
    } else {
        FIRST_SYSTEM_TOP
    };
    let mut current_y = first_system_top;

    let ref_part = &score.parts[parts_with_staves[0].0];

    let mut measure_beats: Vec<f64> = Vec::with_capacity(ref_part.measures.len());
    let mut running_keys: Vec<Option<Key>> = Vec::with_capacity(ref_part.measures.len());
    let mut running_times: Vec<Option<TimeSignature>> = Vec::with_capacity(ref_part.measures.len());
    let mut has_key_change: Vec<bool> = Vec::with_capacity(ref_part.measures.len());
    let mut has_time_change: Vec<bool> = Vec::with_capacity(ref_part.measures.len());

    let mut current_beats = 4.0;
    let mut current_key: Option<Key> = None;
    let mut current_time: Option<TimeSignature> = None;

    for measure in &ref_part.measures {
        let mut key_changed = false;
        let mut time_changed = false;

        if let Some(ref attrs) = measure.attributes {
            if let Some(ref ts) = attrs.time {
                let safe_beat_type = if ts.beat_type > 0 { ts.beat_type } else { 4 };
                let new_beats = ts.beats as f64 * 4.0 / safe_beat_type as f64;
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

        if measure.implicit {
            measure_beats.push((current_beats * 0.5).max(1.0));
        } else {
            measure_beats.push(current_beats);
        }
    }

    let mut lyrics_divs: Vec<i32> = vec![1; score.parts.len()];

    // Staff: min width with key/time sig extras. Used for staff packing only.
    let measure_min_widths_staff: Vec<f64> = measure_beats
        .iter()
        .enumerate()
        .map(|(mi, &beats)| {
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
                let old_fifths = if mi > 0 {
                    running_keys[mi - 1].as_ref().map_or(0, |k| k.fifths)
                } else { 0 };
                let new_fifths = running_keys[mi].as_ref().map_or(0, |k| k.fifths);
                let num_cancel = cancellation_natural_count(old_fifths, new_fifths);
                if num_cancel > 0 {
                    w += num_cancel as f64 * KEY_SIG_NATURAL_SPACE + 4.0;
                }
                let new_width = running_keys[mi].as_ref().map_or(0.0, |k| key_sig_width(Some(k))) + 4.0;
                w += new_width;
            }
            if has_time_change[mi] { w += TIME_SIG_SPACE; }

            let lyrics_w = lyrics_min_measure_width(&score.parts, mi, &lyrics_divs, w);
            if lyrics_w > w {
                w = lyrics_w;
            }

            w
        })
        .collect();

    // Jianpu: min width (no key/time extras; key/time drawn inside measure). Used as floor for jianpu estimates and packing.
    let measure_min_widths_jianpu: Vec<f64> = measure_beats
        .iter()
        .enumerate()
        .map(|(mi, &beats)| {
            let mut w = (beats * PER_BEAT_MIN_WIDTH).max(JIANPU_MIN_MEASURE_WIDTH);
            let lyrics_w = lyrics_min_measure_width(&score.parts, mi, &lyrics_divs, w);
            if lyrics_w > w {
                w = lyrics_w;
            }
            w
        })
        .collect();

    // Jianpu: content-based estimated width per measure (notes + lyrics + insets) for row packing.
    let ref_pidx = parts_with_staves[0].0;
    let ref_part = &score.parts[ref_pidx];
    let measure_estimated_widths: Vec<f64> = if use_jianpu {
        let num_measures = ref_part.measures.len();
        (0..num_measures)
            .map(|mi| {
                let mut divs = vec![1; score.parts.len()];
                let mut all_beat_times: Vec<Vec<f64>> = Vec::new();
                for (pidx, _) in parts_with_staves {
                    let part = &score.parts[*pidx];
                    if mi < part.measures.len() {
                        if let Some(ref attrs) = part.measures[mi].attributes {
                            if let Some(d) = attrs.divisions {
                                divs[*pidx] = d;
                            }
                        }
                        let beat_times = compute_note_beat_times(
                            &part.measures[mi].notes,
                            divs[*pidx],
                        );
                        all_beat_times.push(beat_times);
                    }
                }
                let n_slots = count_unique_beat_slots(&all_beat_times);
                let div = divs[ref_pidx].max(1) as f64;
                let max_suffix_w: f64 = if mi < ref_part.measures.len() {
                    ref_part.measures[mi]
                        .notes
                        .iter()
                        .map(|n| {
                            let q = n.duration as f64 / div;
                            jianpu::jianpu_duration_suffix_width(q, n.dot)
                        })
                        .fold(0.0_f64, f64::max)
                } else {
                    0.0
                };
                let mut left_base = if mi == 0 { 40.0 } else { 36.0 };
                let mut right_base: f64 = 14.0;
                if mi < ref_part.measures.len() {
                    for barline in &ref_part.measures[mi].barlines {
                        let is_left = barline.location == "left";
                        let is_right = barline.location == "right" || barline.location.is_empty();
                        let has_repeat = barline.repeat.is_some();
                        if is_left && has_repeat {
                            left_base += JIANPU_REPEAT_EXTRA_OFFSET;
                        }
                        if is_right && has_repeat {
                            right_base = right_base.max(30.0);
                        }
                    }
                }
                let content_w = left_base + right_base + (n_slots as f64 * JIANPU_ESTIMATE_PER_SLOT) + max_suffix_w;
                let with_lyrics = lyrics_min_measure_width(&score.parts, mi, &lyrics_divs, content_w);
                content_w.max(with_lyrics).max(measure_min_widths_jianpu[mi])
            })
            .collect()
    } else {
        Vec::new()
    };

    let initial_key = score.parts.iter()
        .flat_map(|p| p.measures.iter())
        .find_map(|m| m.attributes.as_ref().and_then(|a| a.key.as_ref()));
    // On the first system we reserve INSTRUMENT_PREFIX_WIDTH before the clef
    // for the instrument name and tempo label. For jianpu, key and time are drawn
    // inside the measure at the bar, so no extra prefix.
    let first_prefix = if use_jianpu {
        INSTRUMENT_PREFIX_WIDTH
    } else {
        INSTRUMENT_PREFIX_WIDTH + CLEF_SPACE + key_sig_width(initial_key) + TIME_SIG_SPACE
    };
    let available_first = content_width - first_prefix;

    let mut system_groups: Vec<Vec<usize>> = Vec::new();
    let mut current_group: Vec<usize> = Vec::new();
    let mut current_width = 0.0;
    let mut is_first_system = true;

    for mi in 0..measure_beats.len() {
        let min_w = if use_jianpu {
            measure_min_widths_jianpu[mi]
        } else {
            measure_min_widths_staff[mi]
        };
        let key_at_mi = running_keys[mi].as_ref();
        let later_prefix = if use_jianpu {
            0.0
        } else {
            CLEF_SPACE + key_sig_width(key_at_mi)
        };
        let available_later = content_width - later_prefix;
        let available = if is_first_system { available_first } else { available_later };
        let width_for_packing = if use_jianpu && mi < measure_estimated_widths.len() {
            measure_estimated_widths[mi]
        } else {
            min_w
        };

        if !current_group.is_empty() && current_width + width_for_packing > available {
            system_groups.push(current_group);
            current_group = Vec::new();
            current_width = 0.0;
            is_first_system = false;
        }
        current_group.push(mi);
        current_width += width_for_packing;
    }
    if !current_group.is_empty() {
        system_groups.push(current_group);
    }

    let total_staves: usize = parts_with_staves.iter().map(|(_, sn)| sn.len()).sum();

    let mut divisions_per_part: Vec<i32> = vec![1; score.parts.len()];

    for (sys_idx, group) in system_groups.iter().enumerate() {
        let is_first = sys_idx == 0;
        let first_mi = group[0];
        let key_at_start = running_keys[first_mi].as_ref();
        let show_time_sig = is_first || has_time_change[first_mi];
        let prefix_width = if use_jianpu {
            0.0
        } else {
            CLEF_SPACE
                + key_sig_width(key_at_start)
                + if show_time_sig { TIME_SIG_SPACE } else { 0.0 }
        };

        let extra_prefix = if is_first { INSTRUMENT_PREFIX_WIDTH } else { 0.0 };
        let x_start = PAGE_MARGIN_LEFT + extra_prefix + prefix_width;
        let x_end = PAGE_MARGIN_LEFT + content_width;
        let available = x_end - x_start;

        let (measure_weights, scale) = if use_jianpu && !measure_estimated_widths.is_empty() {
            let weights: Vec<f64> = group
                .iter()
                .map(|&mi| measure_estimated_widths.get(mi).copied().unwrap_or(measure_beats[mi]))
                .collect();
            let total: f64 = weights.iter().sum();
            let s = if total > 0.0 { available / total } else { 1.0 };
            (weights, s)
        } else {
            let weights: Vec<f64> = group.iter().map(|&mi| measure_beats[mi]).collect();
            let total: f64 = weights.iter().sum();
            let s = if total > 0.0 { available / total } else { 1.0 };
            (weights, s)
        };

        let mut measures = Vec::new();
        let mut x = x_start;

        for (j, &mi) in group.iter().enumerate() {
            let w = measure_weights[j] * scale;

            let mut all_beat_times: Vec<Vec<f64>> = Vec::new();
            for (pidx, _) in parts_with_staves {
                let part = &score.parts[*pidx];
                if mi < part.measures.len() {
                    if let Some(ref attrs) = part.measures[mi].attributes {
                        if let Some(d) = attrs.divisions {
                            divisions_per_part[*pidx] = d;
                        }
                    }
                    let beat_times = compute_note_beat_times(
                        &part.measures[mi].notes,
                        divisions_per_part[*pidx],
                    );
                    all_beat_times.push(beat_times);
                }
            }

            let prev_key_fifths = if has_key_change[mi] && mi > 0 {
                running_keys[mi - 1].as_ref().map(|k| k.fifths)
            } else {
                None
            };

            let is_system_start = j == 0;
            let ml_has_key_change = has_key_change[mi] && !is_system_start;
            let ml_has_time_change = has_time_change[mi] && !is_system_start;

            let mut left_inset = 14.0;
            if !use_jianpu {
                if ml_has_key_change {
                    if let Some(pf) = prev_key_fifths {
                        let new_fifths = running_keys[mi].as_ref().map_or(0, |k| k.fifths);
                        let num_cancel = cancellation_natural_count(pf, new_fifths);
                        if num_cancel > 0 {
                            left_inset += num_cancel as f64 * KEY_SIG_NATURAL_SPACE + 2.0;
                        }
                    }
                    if let Some(ref k) = running_keys[mi] {
                        left_inset += key_sig_width(Some(k)) + 4.0;
                    }
                }
                if ml_has_time_change {
                    left_inset += TIME_SIG_SPACE;
                }
            }

            let mut right_inset: f64 = 14.0;
            if mi < ref_part.measures.len() {
                let measure = &ref_part.measures[mi];
                for barline in &measure.barlines {
                    let is_right = barline.location == "right"
                        || barline.location.is_empty();
                    let is_left = barline.location == "left";
                    let has_repeat = barline.repeat.is_some();
                    let is_heavy = barline.bar_style.as_deref() == Some("light-heavy")
                        || barline.bar_style.as_deref() == Some("heavy-light");

                    if is_right && (has_repeat || is_heavy) {
                        right_inset = right_inset.max(30.0);
                    }
                    if is_left && (has_repeat || is_heavy) {
                        left_inset = left_inset.max(28.0);
                    }
                }
            }
            // Jianpu: base inset + space for key label and/or time sig when drawn (match actual drawn width so notes aren’t pushed right).
            if use_jianpu {
                left_inset = left_inset.max(36.0);
                let key_drawn = mi == 0 || ml_has_key_change;
                let time_drawn = ml_has_time_change || (j == 0 && show_time_sig);
                let has_left_repeat = mi < ref_part.measures.len()
                    && ref_part.measures[mi].barlines.iter().any(|b| b.location == "left" && b.repeat.is_some());
                if key_drawn && time_drawn {
                    // Use actual drawn prefix width (bar offset + key-to-time gap + time sig) + gap so first note sits close to time sig.
                    let bar_offset = 2.0 + if has_left_repeat { JIANPU_REPEAT_EXTRA_OFFSET } else { 0.0 };
                    let prefix_w = bar_offset + JIANPU_KEY_TO_TIME_GAP + TIME_SIG_SPACE;
                    left_inset = left_inset.max(prefix_w + JIANPU_TIME_TO_NOTE_GAP);
                } else {
                    if key_drawn {
                        left_inset += JIANPU_KEY_LABEL_SPACE;
                    }
                    if time_drawn {
                        left_inset += TIME_SIG_SPACE + JIANPU_TIME_TO_NOTE_GAP;
                    }
                    if has_left_repeat {
                        left_inset += JIANPU_REPEAT_EXTRA_OFFSET;
                    }
                }
            }

            let lyric_evts = collect_lyric_events(&score.parts, mi, &divisions_per_part);
            let nominal_quarters = running_times[mi]
                .as_ref()
                .map(|ts| {
                    let safe_bt = if ts.beat_type > 0 { ts.beat_type } else { 4 };
                    ts.beats as f64 * 4.0 / safe_bt as f64
                })
                .unwrap_or(4.0);
            // For pickup/implicit measures, use the actual note content duration
            // so notes are spaced the same as in full measures.
            let total_quarters = if mi < ref_part.measures.len() && ref_part.measures[mi].implicit {
                let div = divisions_per_part[parts_with_staves[0].0].max(1);
                let mut total_divs = 0i32;
                for note in &ref_part.measures[mi].notes {
                    if !note.chord && !note.grace {
                        total_divs += note.duration;
                    }
                }
                let actual = total_divs as f64 / div as f64;
                if actual > 0.0 && actual < nominal_quarters {
                    actual
                } else {
                    nominal_quarters
                }
            } else {
                nominal_quarters
            };
            let min_trailing = if use_jianpu { Some(4.0) } else { None };
            let max_trailing_frac = if use_jianpu { Some(0.28) } else { None };
            let min_note_spacing_jianpu = if use_jianpu { Some(18.0) } else { None };
            let jianpu_slot_counts = if use_jianpu && !all_beat_times.is_empty() {
                let unique_beats = unique_beat_times_sorted(&all_beat_times);
                let part_notes: Vec<&[Note]> = parts_with_staves
                    .iter()
                    .filter(|(pidx, _)| mi < score.parts[*pidx].measures.len())
                    .map(|(pidx, _)| score.parts[*pidx].measures[mi].notes.as_slice())
                    .collect();
                let part_divs: Vec<i32> = parts_with_staves
                    .iter()
                    .filter(|(pidx, _)| mi < score.parts[*pidx].measures.len())
                    .map(|(pidx, _)| divisions_per_part[*pidx])
                    .collect();
                Some(slot_counts_for_unique_beats(
                    &unique_beats,
                    &all_beat_times,
                    &part_notes,
                    &part_divs,
                ))
            } else {
                None
            };
            let beat_x_map = compute_beat_x_map(
                &all_beat_times,
                x,
                w,
                left_inset,
                right_inset,
                &lyric_evts,
                total_quarters,
                min_trailing,
                max_trailing_frac,
                min_note_spacing_jianpu,
                use_jianpu, // evenly spread notes within the measure so rows fill the screen and look professional
                jianpu_slot_counts.as_deref(),
            );

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

        let mut parts_info: Vec<PartStaffInfo> = Vec::new();
        let mut y_offset = 0.0;

        for (i, (pidx, staff_nums)) in parts_with_staves.iter().enumerate() {
            let num_staves = staff_nums.len();
            parts_info.push(PartStaffInfo {
                part_idx: *pidx,
                y_offset,
                num_staves,
                staff_nums: staff_nums.clone(),
            });

            let part_height = STAFF_HEIGHT
                + (num_staves as f64 - 1.0) * (STAFF_HEIGHT + GRAND_STAFF_GAP);
            y_offset += part_height;

            if i < parts_with_staves.len() - 1 {
                y_offset += PART_GAP;
            }
        }

        let mut max_lyric_verses = 0i32;
        let mut has_cjk_lyrics = false;
        for ml_check in &measures {
            for (pidx, _) in parts_with_staves {
                if ml_check.measure_idx < score.parts[*pidx].measures.len() {
                    for note in &score.parts[*pidx].measures[ml_check.measure_idx].notes {
                        for lyric in &note.lyrics {
                            max_lyric_verses = max_lyric_verses.max(lyric.number);
                            if has_cjk(&lyric.text) {
                                has_cjk_lyrics = true;
                            }
                        }
                    }
                }
            }
        }

        let mut max_below_dir_lines: usize = 0;
        for ml_check2 in &measures {
            for (pidx, _) in parts_with_staves {
                if ml_check2.measure_idx < score.parts[*pidx].measures.len() {
                    let count = score.parts[*pidx].measures[ml_check2.measure_idx].directions.iter()
                        .filter(|dir| {
                            dir.placement.as_deref() == Some("below")
                                && dir.words.as_ref().map_or(false, |w: &String| !w.is_empty() && !is_jump_text(w))
                        })
                        .count();
                    if count > max_below_dir_lines {
                        max_below_dir_lines = count;
                    }
                }
            }
        }

        let dir_words_extra = if max_below_dir_lines > 0 && max_lyric_verses > 0 {
            DIRECTION_WORDS_HEIGHT + (max_below_dir_lines as f64 - 1.0) * DIRECTION_WORDS_LINE_HEIGHT
        } else {
            0.0
        };

        let lyrics_line_height = if has_cjk_lyrics {
            LYRICS_LINE_HEIGHT_CJK
        } else {
            LYRICS_LINE_HEIGHT
        };
        let lyrics_extra = if max_lyric_verses > 0 {
            LYRICS_MIN_Y_BELOW_STAFF - STAFF_HEIGHT + max_lyric_verses as f64 * lyrics_line_height
                + dir_words_extra
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

        let system_height = y_offset;
        current_y += system_height + lyrics_extra + SYSTEM_SPACING;
    }

    let total_height = current_y + 40.0;

    ScoreLayout {
        systems,
        total_height,
    }
}
