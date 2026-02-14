//! Beat timing and position computation for cross-staff alignment.

use crate::model::*;
use super::constants::*;
use super::lyrics::{LyricEvent, lyric_pair_min_spacing};

/// Width allocated per grace note (px) — roughly 66% of a normal notehead.
pub(super) const GRACE_NOTE_WIDTH: f64 = 8.0;

/// Compute the beat-time offset for each note in a measure,
/// using per-voice time tracking to handle MusicXML backup semantics.
pub(super) fn compute_note_beat_times(notes: &[Note], divisions: i32) -> Vec<f64> {
    use std::collections::HashMap;
    let mut voice_times: HashMap<i32, f64> = HashMap::new();
    // Track the beat time of the last non-chord note per voice,
    // so chord notes can share the same beat position.
    let mut voice_last_beat: HashMap<i32, f64> = HashMap::new();
    let mut beat_times = Vec::with_capacity(notes.len());

    for note in notes {
        let voice = note.voice.unwrap_or(1);
        let current = voice_times.entry(voice).or_insert(0.0);

        if note.grace {
            beat_times.push(*current);
        } else if note.chord {
            // Chord notes share the same beat as their principal note
            let last = voice_last_beat.get(&voice).copied().unwrap_or(0.0);
            beat_times.push(last);
        } else {
            let beat = *current;
            voice_last_beat.insert(voice, beat);
            beat_times.push(beat);
            let dur = note.duration as f64 / divisions.max(1) as f64;
            *current += dur;
        }
    }

    beat_times
}

/// Minimum pixel gap between consecutive note positions.
const MIN_NOTE_SPACING: f64 = 12.0;

/// Build a sorted beat-time → x-position mapping from note beat times across
/// all parts. This is the core of cross-staff/cross-part vertical alignment.
///
/// `total_quarters` is the full duration of the measure in quarter notes
/// (from the time signature). Notes are spaced proportionally to their
/// duration, so a half note gets twice the space of a quarter note.
pub(super) fn compute_beat_x_map(
    all_beat_times: &[Vec<f64>],
    mx: f64,
    mw: f64,
    left_pad: f64,
    right_pad: f64,
    lyric_events: &[LyricEvent],
    total_quarters: f64,
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
    unique_beats.sort_by(|a, b| a.partial_cmp(b).unwrap());

    if unique_beats.is_empty() {
        return vec![];
    }

    let total_q = total_quarters.max(0.001);
    let n = unique_beats.len();

    // Compute gap sizes (n gaps: between consecutive notes + trailing gap to measure end)
    let event_at = |bt: f64| -> Option<&LyricEvent> {
        lyric_events.iter().find(|ev| (ev.beat_time - bt).abs() < 0.001)
    };

    let mut gaps: Vec<f64> = Vec::with_capacity(n);

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

        gaps.push(prop_dist.max(lyrics_dist).max(MIN_NOTE_SPACING));
    }

    // Trailing gap: space after the last note to the measure's right edge
    let last_beat = unique_beats.last().copied().unwrap_or(0.0);
    let trailing_prop = ((total_q - last_beat) / total_q) * usable_width;
    gaps.push(trailing_prop.max(MIN_NOTE_SPACING));

    // Scale all gaps so they sum to exactly usable_width
    let total_gaps: f64 = gaps.iter().sum();
    let scale = if total_gaps > 0.0 { usable_width / total_gaps } else { 1.0 };

    // Place notes at cumulative gap positions
    let mut result = Vec::with_capacity(n);
    let mut x = mx + left_pad;
    for i in 0..n {
        result.push((unique_beats[i], x));
        x += gaps[i] * scale;
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
