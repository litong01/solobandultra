//! Top-layer note extraction for multi-voice and chord measures.
//!
//! Used for note selection: extract only the "top layer" — i.e. the top voice
//! (soprano) on a staff and at each beat position the highest note of a chord.
//! For note selection, pass the **staff the user selected** (e.g. `Some(1)` for
//! treble, `Some(2)` for bass, `Some(3)` for a third staff) so only that staff's
//! top layer is returned; use `staff_filter: None` only if you need all staves.
//!
//! **Jianpu preprocessing:** Before rendering jianpu, the score can be simplified
//! so that each measure keeps only the top layer (first voice on the staff, and
//! at each chord only the highest-pitched note). The simplified music is then
//! passed to the normal render path so layout and drawing see a single layer.

use crate::model::{Measure, Note, Part, Pitch};
use std::collections::{HashMap, HashSet};

/// Default voice number for the top layer when only one voice exists (matches MusicXML/jianpu).
const DEFAULT_TOP_VOICE: i32 = 1;

/// Beat-time tolerance when grouping notes at the same position.
const BEAT_EPS: f64 = 0.001;

/// One extracted top-layer note (one per staff per beat position).
#[derive(Debug, Clone, PartialEq)]
pub struct TopLayerNote {
    pub measure_idx: usize,
    pub staff: i32,
    pub beat_time: f64,
    pub pitch_name: String,
    pub midi: i32,
    pub note_index: usize,
}

/// Result of analyzing a measure for multi-voice/chord and top-layer extraction.
#[derive(Debug, Clone)]
pub struct MeasureTopLayer {
    pub measure_idx: usize,
    pub has_multiple_voices: bool,
    pub has_chord: bool,
    pub divisions: i32,
    pub top_notes: Vec<TopLayerNote>,
}

/// Compute beat-time offset for each note using per-(staff, voice) tracking.
/// Matches the logic in renderer/beat_map.rs so grouping aligns with layout.
fn compute_note_beat_times(notes: &[Note], divisions: i32) -> Vec<f64> {
    type VoiceKey = (i32, i32);
    let mut voice_times: HashMap<VoiceKey, f64> = HashMap::new();
    let mut voice_last_beat: HashMap<VoiceKey, f64> = HashMap::new();
    let mut beat_times = Vec::with_capacity(notes.len());
    let div = divisions.max(1) as f64;

    for note in notes {
        let vk: VoiceKey = (note.staff.unwrap_or(1), note.voice.unwrap_or(1));
        let current = voice_times.entry(vk).or_insert(0.0);

        if note.grace {
            beat_times.push(*current);
        } else if note.chord {
            let last = voice_last_beat.get(&vk).copied().unwrap_or(0.0);
            beat_times.push(last);
        } else {
            let beat = *current;
            voice_last_beat.insert(vk, beat);
            beat_times.push(beat);
            let dur = note.duration as f64 / div;
            *current += dur;
        }
    }
    beat_times
}

fn pitch_to_name(pitch: &Pitch) -> String {
    let alter = pitch.alter.unwrap_or(0.0);
    let acc = if alter >= 1.0 {
        "#"
    } else if alter <= -1.0 {
        "b"
    } else {
        ""
    };
    format!("{}{}{}", pitch.step, acc, pitch.octave)
}

/// Detect if the measure has more than one (staff, voice) pair with at least one non-rest note.
pub fn measure_has_multiple_voices(measure: &Measure) -> bool {
    let mut voices: HashSet<(i32, i32)> = HashSet::new();
    for note in &measure.notes {
        if note.rest || note.grace {
            continue;
        }
        let vk = (note.staff.unwrap_or(1), note.voice.unwrap_or(1));
        voices.insert(vk);
    }
    voices.len() > 1
}

/// Detect if the measure contains any chord (note with chord=true).
pub fn measure_has_chord(measure: &Measure) -> bool {
    measure.notes.iter().any(|n| n.chord)
}

/// Extract top-layer notes from one measure: per staff, the top voice (min voice
/// number on that staff), and at each beat position only the highest pitch
/// (soprano + top note of chord).
///
/// If `staff_filter` is `Some(n)`, only notes on staff `n` are returned (e.g.
/// `Some(1)` for first staff, `Some(2)` for second; pass the user-selected staff).
/// If `None`, all staves are included.
pub fn extract_top_layer_from_measure(
    measure: &Measure,
    measure_idx: usize,
    staff_filter: Option<i32>,
) -> MeasureTopLayer {
    let divisions = measure
        .attributes
        .as_ref()
        .and_then(|a| a.divisions)
        .unwrap_or(1)
        .max(1);

    let beat_times = compute_note_beat_times(&measure.notes, divisions);

    let has_multiple_voices = measure_has_multiple_voices(measure);
    let has_chord = measure_has_chord(measure);

    // Per-staff "top" voice = minimum voice number on that staff (soprano / top layer).
    // MuseScore often uses voice 1 on treble and voice 5 on bass; we want both.
    let mut staff_top_voice: HashMap<i32, i32> = HashMap::new();
    for note in &measure.notes {
        if note.rest || note.grace {
            continue;
        }
        let staff = note.staff.unwrap_or(1);
        let voice = note.voice.unwrap_or(DEFAULT_TOP_VOICE);
        staff_top_voice
            .entry(staff)
            .and_modify(|v| *v = (*v).min(voice))
            .or_insert(voice);
    }

    // Group note indices by (staff, beat_time) — same logic as jianpu grouping by position.
    type GroupKey = (i32, i64); // (staff, beat_scaled) to avoid float key
    let mut groups: HashMap<GroupKey, Vec<usize>> = HashMap::new();
    for (i, &beat) in beat_times.iter().enumerate() {
        if i >= measure.notes.len() {
            break;
        }
        let note = &measure.notes[i];
        if note.grace {
            continue;
        }
        let staff = note.staff.unwrap_or(1);
        let beat_scaled = (beat / BEAT_EPS).round() as i64;
        groups
            .entry((staff, beat_scaled))
            .or_default()
            .push(i);
    }

    let mut top_notes: Vec<TopLayerNote> = Vec::new();
    for ((staff, _), indices) in groups {
        if staff_filter.is_some_and(|s| s != staff) {
            continue;
        }
        let top_voice = *staff_top_voice.get(&staff).unwrap_or(&DEFAULT_TOP_VOICE);
        // Only notes in this staff's top voice at this (staff, beat)
        let voice1: Vec<usize> = indices
            .into_iter()
            .filter(|&k| measure.notes[k].voice.unwrap_or(DEFAULT_TOP_VOICE) == top_voice)
            .collect();
        if voice1.is_empty() {
            continue;
        }
        // Rest-only group: skip (we don't emit rest as "top layer note" for this test)
        let pitched: Vec<(usize, i32)> = voice1
            .iter()
            .filter_map(|&k| {
                let n = &measure.notes[k];
                n.pitch.as_ref().map(|p| (k, p.to_midi()))
            })
            .collect();
        if pitched.is_empty() {
            continue;
        }
        // Highest pitch at this position (soprano / top of chord)
        let (note_index, midi) = pitched.into_iter().max_by_key(|(_, m)| *m).unwrap();
        let pitch = measure.notes[note_index].pitch.as_ref().unwrap();
        let beat_time = beat_times[note_index];
        top_notes.push(TopLayerNote {
            measure_idx,
            staff,
            beat_time,
            pitch_name: pitch_to_name(pitch),
            midi,
            note_index,
        });
    }
    top_notes.sort_by(|a, b| {
        (a.staff, a.beat_time)
            .partial_cmp(&(b.staff, b.beat_time))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    MeasureTopLayer {
        measure_idx,
        has_multiple_voices,
        has_chord,
        divisions,
        top_notes,
    }
}

/// Extract top-layer notes from all measures of a part.
///
/// If `staff_filter` is `Some(n)`, only top-layer notes on staff `n` are returned.
/// Use the staff the user selected (e.g. 1 = treble, 2 = bass). If `None`, all staves.
pub fn extract_top_layer_from_part(
    part: &crate::model::Part,
    staff_filter: Option<i32>,
) -> Vec<MeasureTopLayer> {
    part.measures
        .iter()
        .enumerate()
        .map(|(i, m)| extract_top_layer_from_measure(m, i, staff_filter))
        .collect()
}

// ─── Jianpu preprocess: remove non–top-layer notes ────────────────────────────

/// Returns the set of note indices to keep for jianpu: one note per (staff, beat)
/// (top voice, highest pitch or single rest), plus all top-voice grace notes on that staff.
fn measure_keep_indices(measure: &Measure, staff_filter: i32) -> HashSet<usize> {
    let divisions = measure
        .attributes
        .as_ref()
        .and_then(|a| a.divisions)
        .unwrap_or(1)
        .max(1);
    let beat_times = compute_note_beat_times(&measure.notes, divisions);

    let mut staff_top_voice: HashMap<i32, i32> = HashMap::new();
    for note in &measure.notes {
        if note.rest || note.grace {
            continue;
        }
        let staff = note.staff.unwrap_or(1);
        let voice = note.voice.unwrap_or(DEFAULT_TOP_VOICE);
        staff_top_voice
            .entry(staff)
            .and_modify(|v| *v = (*v).min(voice))
            .or_insert(voice);
    }
    let top_voice = *staff_top_voice.get(&staff_filter).unwrap_or(&DEFAULT_TOP_VOICE);

    type GroupKey = (i32, i64);
    let mut groups: HashMap<GroupKey, Vec<usize>> = HashMap::new();
    for (i, &beat) in beat_times.iter().enumerate() {
        if i >= measure.notes.len() {
            break;
        }
        let note = &measure.notes[i];
        if note.grace {
            continue;
        }
        let staff = note.staff.unwrap_or(1);
        if staff != staff_filter {
            continue;
        }
        let beat_scaled = (beat / BEAT_EPS).round() as i64;
        groups
            .entry((staff, beat_scaled))
            .or_default()
            .push(i);
    }

    let mut keep = HashSet::new();
    for (_, indices) in groups {
        let voice1: Vec<usize> = indices
            .into_iter()
            .filter(|&k| measure.notes[k].voice.unwrap_or(DEFAULT_TOP_VOICE) == top_voice)
            .collect();
        if voice1.is_empty() {
            continue;
        }
        let pitched: Vec<(usize, i32)> = voice1
            .iter()
            .filter_map(|&k| {
                let n = &measure.notes[k];
                n.pitch.as_ref().map(|p| (k, p.to_midi()))
            })
            .collect();
        if pitched.is_empty() {
            keep.insert(voice1[0]);
        } else {
            let (idx, _) = pitched.into_iter().max_by_key(|(_, m)| *m).unwrap();
            keep.insert(idx);
        }
    }
    for (i, note) in measure.notes.iter().enumerate() {
        if note.grace
            && note.staff.unwrap_or(1) == staff_filter
            && note.voice.unwrap_or(DEFAULT_TOP_VOICE) == top_voice
        {
            keep.insert(i);
        }
    }
    keep
}

/// Simplify one measure for jianpu: keep only top-layer notes (first voice on the
/// given staff, and at each chord only the highest-pitched note). Returns a new
/// `Vec<Note>` in display order; kept notes are cloned with `chord: false`.
pub fn simplify_measure_for_jianpu(measure: &Measure, staff_filter: i32) -> Vec<Note> {
    let keep = measure_keep_indices(measure, staff_filter);
    let mut out = Vec::new();
    let mut pending_graces: Vec<Note> = Vec::new();
    for (i, note) in measure.notes.iter().enumerate() {
        if !keep.contains(&i) {
            if !note.grace {
                pending_graces.clear();
            }
            continue;
        }
        if note.grace {
            pending_graces.push(note.clone());
            continue;
        }
        for g in pending_graces.drain(..) {
            out.push(g);
        }
        let mut n = note.clone();
        n.chord = false;
        out.push(n);
    }
    out
}

/// Simplify a part for jianpu: replace each measure's notes with the top layer
/// only (first voice on the given staff, one note per chord = highest). Returns a
/// new `Part`; the original is unchanged.
pub fn simplify_part_for_jianpu(part: &Part, staff_filter: i32) -> Part {
    let mut part = part.clone();
    for measure in &mut part.measures {
        measure.notes = simplify_measure_for_jianpu(measure, staff_filter);
    }
    part
}
