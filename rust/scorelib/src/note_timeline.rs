//! Note timeline: per-note absolute timestamps and pitches for the melody part.
//!
//! This is the data source for the real-time feedback feature.  The caller
//! loads this once after the score is parsed (same time as the playback map),
//! then uses it to look up which pitch is expected at any given `musicMs`.
//!
//! Only **voice 1 of part 0** is returned — the melody the user is expected
//! to play.  Rests, grace notes, and chord duplicates are excluded.

use serde::Serialize;

use crate::model::{Pitch, Score};
use crate::timemap::generate_timemap;
use crate::unroller::unroll;
use crate::{parse_bytes, transpose_score};

/// A single melody note with absolute timing and pitch information.
#[derive(Debug, Clone, Serialize)]
pub struct NoteEvent {
    /// Absolute start time in milliseconds from the beginning of the piece.
    pub start_ms: f64,
    /// Absolute end time in milliseconds (exclusive).
    pub end_ms: f64,
    /// MIDI note number (middle C = 60).
    pub midi: i32,
    /// Human-readable pitch name, e.g. "C4", "F#3", "Bb5".
    pub name: String,
}

/// Generate a note timeline from a parsed and (optionally transposed) score.
///
/// Returns a flat, sorted list of `NoteEvent`s for voice 1 of part 0,
/// with absolute timestamps derived from the timemap.
pub fn generate_note_timeline(score: &Score) -> Vec<NoteEvent> {
    let part_idx = 0;
    let part = match score.parts.get(part_idx) {
        Some(p) => p,
        None => return Vec::new(),
    };

    let unrolled = unroll(score, part_idx);
    let tmap = generate_timemap(score, part_idx, &unrolled);

    let mut events: Vec<NoteEvent> = Vec::new();

    for entry in &tmap {
        let measure = match part.measures.get(entry.original_index) {
            Some(m) => m,
            None => continue,
        };

        let divisions = entry.divisions;
        if divisions <= 0 {
            continue;
        }

        let ms_per_division = entry.duration_ms / (entry.effective_quarters * divisions as f64);

        // Walk notes in voice 1, advancing offset only on non-chord notes.
        let mut offset_divisions: f64 = 0.0;

        for note in &measure.notes {
            // Skip rests, grace notes, and non-voice-1 notes.
            if note.rest || note.grace {
                if !note.chord {
                    offset_divisions += note.duration as f64;
                }
                continue;
            }
            // Only melody voice (voice 1 or unspecified).
            let voice = note.voice.unwrap_or(1);
            if voice != 1 {
                if !note.chord {
                    offset_divisions += note.duration as f64;
                }
                continue;
            }

            if let Some(ref pitch) = note.pitch {
                let start_ms = entry.timestamp_ms + offset_divisions * ms_per_division;
                let dur_ms = note.duration as f64 * ms_per_division;
                let end_ms = start_ms + dur_ms;
                let midi = pitch_to_midi(pitch);
                let name = pitch_to_name(pitch);
                events.push(NoteEvent { start_ms, end_ms, midi, name });
            }

            // Advance offset only for non-chord notes.
            if !note.chord {
                offset_divisions += note.duration as f64;
            }
        }
    }

    // Sort by start time (should already be sorted, but ensure it).
    events.sort_by(|a, b| a.start_ms.partial_cmp(&b.start_ms).unwrap_or(std::cmp::Ordering::Equal));
    events
}

/// Serialize a note timeline to a JSON string.
pub fn note_timeline_to_json(events: &[NoteEvent]) -> String {
    serde_json::to_string(events).unwrap_or_else(|_| "[]".to_string())
}

/// Parse MusicXML bytes and return a note timeline JSON string.
pub fn note_timeline_from_bytes(
    data: &[u8],
    extension: Option<&str>,
    transpose: i32,
) -> Result<String, String> {
    let mut score = parse_bytes(data, extension)?;
    transpose_score(&mut score, transpose);
    let events = generate_note_timeline(&score);
    Ok(note_timeline_to_json(&events))
}

// ── Pitch helpers ────────────────────────────────────────────────────────────

/// Convert a `Pitch` to a MIDI note number.
/// Middle C (C4) = 60.
fn pitch_to_midi(pitch: &Pitch) -> i32 {
    let step_semi: i32 = match pitch.step.as_str() {
        "C" => 0,
        "D" => 2,
        "E" => 4,
        "F" => 5,
        "G" => 7,
        "A" => 9,
        "B" => 11,
        _ => 0,
    };
    let alter = pitch.alter.unwrap_or(0.0).round() as i32;
    (pitch.octave + 1) * 12 + step_semi + alter
}

/// Convert a `Pitch` to a human-readable name such as "C4", "F#3", "Bb5".
fn pitch_to_name(pitch: &Pitch) -> String {
    let alter = pitch.alter.unwrap_or(0.0);
    let accidental = if alter >= 1.0 {
        "#"
    } else if alter <= -1.0 {
        "b"
    } else {
        ""
    };
    format!("{}{}{}", pitch.step, accidental, pitch.octave)
}

// ── C FFI export (used by iOS static lib) ────────────────────────────────────

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

/// Generate a note timeline JSON string from MusicXML bytes.
///
/// Returns a JSON array of `{ start_ms, end_ms, midi, name }` objects,
/// one per melody note in play order.  Returns NULL on error.
/// The caller must free the result with `scorelib_free_string`.
///
/// # Safety
/// `data` must point to `len` valid bytes. `extension` may be null.
#[no_mangle]
pub unsafe extern "C" fn scorelib_note_timeline(
    data: *const u8,
    len: usize,
    extension: *const c_char,
    transpose: i32,
) -> *mut c_char {
    if data.is_null() || len == 0 {
        return std::ptr::null_mut();
    }
    let bytes = unsafe { std::slice::from_raw_parts(data, len) };
    let ext = if extension.is_null() {
        None
    } else {
        unsafe { CStr::from_ptr(extension) }.to_str().ok()
    };

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        note_timeline_from_bytes(bytes, ext, transpose)
    }));

    match result {
        Ok(Ok(json)) => CString::new(json).unwrap_or_default().into_raw(),
        _ => std::ptr::null_mut(),
    }
}
