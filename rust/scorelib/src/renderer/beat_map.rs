//! Beat timing and position computation for cross-staff alignment.

use crate::model::*;
use super::constants::*;
use super::jianpu;
use super::lyrics::{LyricEvent, lyric_pair_min_spacing};

/// Width allocated per grace note (px) — roughly 66% of a normal notehead.
pub(super) const GRACE_NOTE_WIDTH: f64 = 8.0;

/// Compute the beat-time offset for each note in a measure,
/// using per-voice time tracking to handle MusicXML backup semantics.
pub(super) fn compute_note_beat_times(notes: &[Note], divisions: i32) -> Vec<f64> {
    use std::collections::HashMap;
    // Use (staff, voice) as the key so that overlapping voice numbers
    // across staves (common in MuseScore exports) are tracked independently.
    type VoiceKey = (i32, i32);
    let mut voice_times: HashMap<VoiceKey, f64> = HashMap::new();
    // Track the beat time of the last non-chord note per voice,
    // so chord notes can share the same beat position.
    let mut voice_last_beat: HashMap<VoiceKey, f64> = HashMap::new();
    let mut beat_times = Vec::with_capacity(notes.len());

    for note in notes {
        let vk: VoiceKey = (note.staff.unwrap_or(1), note.voice.unwrap_or(1));
        let current = voice_times.entry(vk).or_insert(0.0);

        if note.grace {
            beat_times.push(*current);
        } else if note.chord {
            // Chord notes share the same beat as their principal note
            let last = voice_last_beat.get(&vk).copied().unwrap_or(0.0);
            beat_times.push(last);
        } else {
            let beat = *current;
            voice_last_beat.insert(vk, beat);
            beat_times.push(beat);
            let dur = note.duration as f64 / divisions.max(1) as f64;
            *current += dur;
        }
    }

    beat_times
}

/// Minimum pixel gap between consecutive note positions.
const MIN_NOTE_SPACING: f64 = 12.0;

/// Count unique beat-time positions (note starts). Used for jianpu row packing so multiple measures
/// fit per row; note positions inside each measure still use the slot rule (whole=4, half=2, etc.).
pub(super) fn count_unique_beat_slots(all_beat_times: &[Vec<f64>]) -> usize {
    let mut unique: Vec<f64> = Vec::new();
    for beats in all_beat_times {
        for &bt in beats {
            if !unique.iter().any(|&u| (u - bt).abs() < 0.001) {
                unique.push(bt);
            }
        }
    }
    unique.len()
}

/// Sorted unique beat times across all parts (same 0.001 tolerance as beat map). Used when building
/// slot counts for jianpu so note positions follow the same whole=4, half=2, quarter=1 rule.
pub(super) fn unique_beat_times_sorted(all_beat_times: &[Vec<f64>]) -> Vec<f64> {
    let mut unique: Vec<f64> = Vec::new();
    for beats in all_beat_times {
        for &bt in beats {
            if !unique.iter().any(|&u| (u - bt).abs() < 0.001) {
                unique.push(bt);
            }
        }
    }
    unique.sort_by(|a, b| a.total_cmp(b));
    unique
}

/// Slot count at a given beat for one part (first non-grace note at that beat). Returns 0 if none.
fn slot_count_at_beat(notes: &[Note], beat_times: &[f64], divisions: i32, beat: f64) -> f64 {
    let div = divisions.max(1) as f64;
    for (i, &bt) in beat_times.iter().enumerate() {
        if (bt - beat).abs() < 0.001 && !notes.get(i).map_or(true, |n| n.grace) {
            let n = &notes[i];
            return jianpu::slot_count_for_duration(n.duration as f64 / div);
        }
    }
    0.0
}

/// Slot counts for each unique beat (same order as unique_beats). Whole=4, half-dotted=3, half=2, quarter or less=1.
/// When multiple parts have a note at the same beat, takes the max slot count.
pub(super) fn slot_counts_for_unique_beats(
    unique_beats: &[f64],
    all_beat_times: &[Vec<f64>],
    all_notes: &[&[Note]],
    divisions: &[i32],
) -> Vec<f64> {
    unique_beats
        .iter()
        .map(|&bt| {
            all_beat_times
                .iter()
                .zip(all_notes.iter().zip(divisions.iter()))
                .map(|(beats, (notes, &div))| slot_count_at_beat(notes, beats, div, bt))
                .fold(0.0_f64, f64::max)
        })
        .collect()
}

/// Build a sorted beat-time → x-position mapping from note beat times across
/// all parts. This is the core of cross-staff/cross-part vertical alignment.
///
/// `total_quarters` is the full duration of the measure in quarter notes
/// (from the time signature). Notes are spaced proportionally to their
/// duration, so a half note gets twice the space of a quarter note.
///
/// When `min_trailing_gap` is `Some(v)`, the space after the last note uses
/// at least `v` (e.g. jianpu uses a smaller value to avoid excess end space).
/// When `max_trailing_fraction` is `Some(f)`, cap the trailing gap at `f` of usable width (jianpu only; staff passes None).
/// When `min_note_spacing_override` is `Some(v)` (jianpu), inter-note gaps are clamped to at least `v` after scaling
/// so short-note runs (e.g. 16ths) don't sit too close to the following note.
/// When `evenly_spread` is true (jianpu), note x positions follow the same slot rule: whole=4, half=2, quarter=1.
/// If `slot_counts` is Some, positions are proportional to cumulative slot count; otherwise to beat time.
pub(super) fn compute_beat_x_map(
    all_beat_times: &[Vec<f64>],
    mx: f64,
    mw: f64,
    left_pad: f64,
    right_pad: f64,
    lyric_events: &[LyricEvent],
    total_quarters: f64,
    min_trailing_gap: Option<f64>,
    max_trailing_fraction: Option<f64>,
    min_note_spacing_override: Option<f64>,
    evenly_spread: bool,
    slot_counts: Option<&[f64]>,
) -> Vec<(f64, f64)> {
    let usable_width = mw - left_pad - right_pad;

    let mut unique_beats: Vec<f64> = Vec::new();
    for beats in all_beat_times {
        for &bt in beats {
            if !unique_beats.iter().any(|&u| (u - bt).abs() < 0.001) {
                unique_beats.push(bt);
            }
        }
    }
    unique_beats.sort_by(|a, b| a.total_cmp(b));

    if unique_beats.is_empty() {
        return vec![];
    }

    let n = unique_beats.len();
    let left_x = mx + left_pad;
    let total_q = total_quarters.max(0.001);

    // Jianpu evenly_spread: place each note at x proportional to cumulative slot count (whole=4, half=2, quarter=1).
    if evenly_spread {
        let positions: Vec<f64> = if let Some(slots) = slot_counts {
            if slots.len() != n {
                // Fallback if lengths don't match
                unique_beats
                    .iter()
                    .map(|&bt| left_x + (bt / total_q) * usable_width)
                    .collect()
            } else {
                let total_slots: f64 = slots.iter().sum();
                if total_slots <= 0.0 {
                    unique_beats
                        .iter()
                        .map(|&bt| left_x + (bt / total_q) * usable_width)
                        .collect()
                } else {
                    let mut cum = 0.0_f64;
                    slots
                        .iter()
                        .map(|&s| {
                            let x = left_x + (cum / total_slots) * usable_width;
                            cum += s;
                            x
                        })
                        .collect()
                }
            }
        } else {
            unique_beats
                .iter()
                .map(|&bt| left_x + (bt / total_q) * usable_width)
                .collect()
        };
        return (0..n)
            .map(|i| (unique_beats[i], positions[i]))
            .collect();
    }

    // Compute gap sizes (n gaps: between consecutive notes + trailing gap to measure end)
    let event_at = |bt: f64| -> Option<&LyricEvent> {
        lyric_events.iter().find(|ev| (ev.beat_time - bt).abs() < 0.001)
    };

    let mut gaps: Vec<f64> = Vec::with_capacity(n);

    let min_gap = min_note_spacing_override.unwrap_or(MIN_NOTE_SPACING);

    for i in 1..n {
        // Proportional distance based on duration fraction
        let prop_dist = ((unique_beats[i] - unique_beats[i - 1]) / total_q) * usable_width;

        // Lyrics minimum spacing (if applicable)
        let lyrics_dist = match (event_at(unique_beats[i - 1]), event_at(unique_beats[i])) {
            (Some(le), Some(re)) => lyric_pair_min_spacing(le, re),
            (Some(le), None) => le.text_width / 2.0,
            (None, Some(re)) => re.text_width / 2.0,
            (None, None) => 0.0,
        };

        gaps.push(prop_dist.max(lyrics_dist).max(min_gap));
    }

    // Trailing gap: space after the last note to the measure's right edge.
    // When max_trailing_fraction is set (jianpu only), cap to avoid huge empty space when the last note is early.
    let last_beat = unique_beats.last().copied().unwrap_or(0.0);
    let trailing_prop = ((total_q - last_beat) / total_q) * usable_width;
    let trailing_min = min_trailing_gap.unwrap_or(MIN_NOTE_SPACING);
    let mut trailing = match max_trailing_fraction {
        Some(f) => trailing_prop.max(trailing_min).min(usable_width * f),
        None => trailing_prop.max(trailing_min),
    };
    gaps.push(trailing);

    // Scale all gaps so they sum to exactly usable_width
    let total_gaps: f64 = gaps.iter().sum();
    let scale = if total_gaps > 0.0 { usable_width / total_gaps } else { 1.0 };

    // Apply scale to gaps
    for g in &mut gaps {
        *g *= scale;
    }

    // For jianpu: preserve minimum spacing between notes so short-note runs (16ths) don't sit on top of the next note.
    if let Some(min_jianpu) = min_note_spacing_override {
        let num_inter = n; // n gaps: n-1 between notes + 1 trailing
        let inter_gaps = &mut gaps[0..num_inter - 1];
        for g in inter_gaps.iter_mut() {
            if *g < min_jianpu {
                *g = min_jianpu;
            }
        }
        let inter_sum: f64 = gaps[0..num_inter - 1].iter().sum();
        let new_total = inter_sum + gaps[num_inter - 1];
        if new_total > usable_width {
            let excess = new_total - usable_width;
            trailing = (gaps[num_inter - 1] - excess).max(trailing_min * 0.5);
            gaps[num_inter - 1] = trailing;
        }
    }

    // Place notes at cumulative gap positions
    let mut result = Vec::with_capacity(n);
    let mut x = mx + left_pad;
    for i in 0..n {
        result.push((unique_beats[i], x));
        x += gaps[i];
    }

    result
}

/// Look up the x position for a given beat time in the beat map.
pub(super) fn lookup_beat_x(beat_x_map: &[(f64, f64)], beat_time: f64) -> f64 {
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
/// Grace notes are offset to the left of their principal note.
pub(super) fn note_x_positions_from_beat_map(
    notes: &[Note],
    divisions: i32,
    beat_x_map: &[(f64, f64)],
) -> Vec<f64> {
    let beat_times = compute_note_beat_times(notes, divisions);

    let mut positions: Vec<f64> = beat_times
        .iter()
        .map(|&bt| lookup_beat_x(beat_x_map, bt))
        .collect();

    let n = notes.len();
    let mut i = 0;
    while i < n {
        if notes[i].grace {
            let grace_start = i;
            while i < n && notes[i].grace {
                i += 1;
            }
            let grace_count = i - grace_start;
            let principal_x = if i < n { positions[i] } else {
                positions[grace_start]
            };
            for (j, gi) in (grace_start..grace_start + grace_count).enumerate() {
                let offset = (grace_count - j) as f64 * GRACE_NOTE_WIDTH;
                positions[gi] = principal_x - offset;
            }
        } else {
            i += 1;
        }
    }

    positions
}

pub(super) fn pitch_to_staff_y(pitch: &Pitch, clef: Option<&Clef>, transpose_octave: i32) -> f64 {
    let step_index = match pitch.step.as_str() {
        "C" => 0, "D" => 1, "E" => 2, "F" => 3,
        "G" => 4, "A" => 5, "B" => 6, _ => 0,
    };

    let display_octave = pitch.octave + transpose_octave;
    let note_position = display_octave * 7 + step_index;

    let (ref_position, ref_y) = match clef.map(|c| c.sign.as_str()) {
        Some("F") => {
            let line = clef.map_or(4, |c| c.line);
            let y = (5 - line) as f64 * STAFF_LINE_SPACING;
            (3 * 7 + 3, y) // F3
        }
        Some("C") => {
            let line = clef.map_or(3, |c| c.line);
            let y = (5 - line) as f64 * STAFF_LINE_SPACING;
            (4 * 7 + 0, y) // C4
        }
        _ => {
            let line = clef.map_or(2, |c| c.line);
            let y = (5 - line) as f64 * STAFF_LINE_SPACING;
            (4 * 7 + 4, y) // G4
        }
    };

    let staff_steps = note_position - ref_position;
    ref_y - staff_steps as f64 * (STAFF_LINE_SPACING / 2.0)
}

pub(super) fn is_filled_note(note_type: Option<&str>) -> bool {
    match note_type {
        Some("whole") | Some("half") => false,
        _ => true,
    }
}
