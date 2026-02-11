//! Compute absolute timestamps and durations for each measure in the
//! unrolled sequence.  This is the bridge between the score model and
//! MIDI event generation — it answers "when does each measure start?"
//! and "how long is it?" in wall-clock time.

use crate::model::Score;
use crate::unroller::UnrolledMeasure;

/// Timing information for one measure in the unrolled sequence.
#[derive(Debug, Clone)]
pub struct TimemapEntry {
    /// Index in the unrolled sequence (0-based)
    pub index: usize,
    /// Index into Part.measures for the original measure data
    pub original_index: usize,
    /// Cumulative start time in milliseconds from the beginning
    pub timestamp_ms: f64,
    /// Duration of this measure in milliseconds
    pub duration_ms: f64,
    /// Active tempo (BPM) at this measure
    pub tempo_bpm: f64,
    /// Time signature: (beats, beat_type)
    pub time_sig: (i32, i32),
    /// MusicXML divisions (divisions per quarter note)
    pub divisions: i32,
}

/// Default tempo if none is specified in the score.
const DEFAULT_TEMPO: f64 = 120.0;
/// Default time signature.
const DEFAULT_TIME_SIG: (i32, i32) = (4, 4);
/// Default divisions per quarter note.
const DEFAULT_DIVISIONS: i32 = 1;

/// State snapshot at a particular original measure position.
/// Pre-computed by walking measures in score order so that jumps
/// (D.S., D.C.) correctly restore the tempo/time-sig/divisions
/// that were in effect at the jump destination.
#[derive(Debug, Clone, Copy)]
struct MeasureState {
    tempo: f64,
    time_sig: (i32, i32),
    divisions: i32,
}

/// Pre-compute the effective state (tempo, time sig, divisions) at each
/// original measure index by walking through the part in score order.
/// This allows the unrolled timemap to look up the correct state even
/// after D.S./D.C. jumps.
fn precompute_measure_states(
    part: &crate::model::Part,
) -> Vec<MeasureState> {
    let mut states = Vec::with_capacity(part.measures.len());
    let mut tempo: f64 = DEFAULT_TEMPO;
    let mut time_sig = DEFAULT_TIME_SIG;
    let mut divisions: i32 = DEFAULT_DIVISIONS;

    for measure in &part.measures {
        // Update state from attributes
        if let Some(ref attrs) = measure.attributes {
            if let Some(d) = attrs.divisions {
                divisions = d;
            }
            if let Some(ref ts) = attrs.time {
                time_sig = (ts.beats, ts.beat_type);
            }
        }

        // Update tempo from directions
        for dir in &measure.directions {
            if let Some(t) = dir.sound_tempo {
                tempo = t;
            } else if let Some(ref metro) = dir.metronome {
                tempo = metro.per_minute as f64;
            }
        }

        states.push(MeasureState {
            tempo,
            time_sig,
            divisions,
        });
    }

    states
}

/// Generate a timemap for an unrolled measure sequence.
///
/// First pre-computes the effective tempo / time-sig / divisions at each
/// original measure by walking in score order.  Then walks the unrolled
/// sequence, looking up each measure's state from the pre-computed table.
/// This ensures that D.S./D.C. jumps correctly restore the tempo that was
/// in effect at the jump destination (e.g. jumping from 90 BPM back to a
/// section that was at 120 BPM).
pub fn generate_timemap(
    score: &Score,
    part_idx: usize,
    unrolled: &[UnrolledMeasure],
) -> Vec<TimemapEntry> {
    let part = match score.parts.get(part_idx) {
        Some(p) => p,
        None => return Vec::new(),
    };

    // Pre-compute the effective state at each original measure in score order.
    let states = precompute_measure_states(part);

    let mut entries = Vec::with_capacity(unrolled.len());
    let mut current_time_ms: f64 = 0.0;

    for (i, um) in unrolled.iter().enumerate() {
        let measure = &part.measures[um.original_index];
        let state = &states[um.original_index];

        let tempo = state.tempo;
        let time_sig = state.time_sig;
        let divisions = state.divisions;

        // ── Compute measure duration ────────────────────────────────
        // quarter_notes = (beats / beat_type) * 4
        let quarter_notes = (time_sig.0 as f64 / time_sig.1 as f64) * 4.0;
        let ms_per_quarter = 60_000.0 / tempo;
        let mut duration_ms = quarter_notes * ms_per_quarter;

        // Handle pickup measures: if this is an implicit measure (anacrusis),
        // compute duration from actual note content instead.
        if measure.implicit {
            let actual_quarters = actual_note_quarters(measure, divisions);
            if actual_quarters > 0.0 && actual_quarters < quarter_notes {
                duration_ms = actual_quarters * ms_per_quarter;
            }
        }

        entries.push(TimemapEntry {
            index: i,
            original_index: um.original_index,
            timestamp_ms: current_time_ms,
            duration_ms,
            tempo_bpm: tempo,
            time_sig,
            divisions,
        });

        current_time_ms += duration_ms;
    }

    entries
}

/// Sum the actual note durations in a measure (in quarter-note units).
/// Used for pickup measures where the nominal duration doesn't match
/// the actual content.
fn actual_note_quarters(measure: &crate::model::Measure, divisions: i32) -> f64 {
    if divisions <= 0 {
        return 0.0;
    }
    let mut total_divisions: i32 = 0;
    for note in &measure.notes {
        // Chord notes share time with the previous note — don't double-count
        if note.chord || note.grace {
            continue;
        }
        total_divisions += note.duration;
    }
    total_divisions as f64 / divisions as f64
}

/// Total duration of the entire timemap in milliseconds.
pub fn total_duration_ms(timemap: &[TimemapEntry]) -> f64 {
    timemap.last().map_or(0.0, |e| e.timestamp_ms + e.duration_ms)
}
