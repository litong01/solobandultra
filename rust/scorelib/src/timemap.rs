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
    /// Effective quarter-note count used for this measure's duration.
    /// For normal measures this equals `(beats / beat_type) * 4`.
    /// For pickup / implicit measures it reflects the actual note content.
    pub effective_quarters: f64,
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

        // Update tempo from directions.
        // <sound tempo="X"/> is always quarter-notes-per-minute (MusicXML spec) — use directly.
        // <metronome> marks the beat unit explicitly (half, quarter, eighth, etc.) and must be
        // converted to quarter-notes-per-minute so all tempo math stays in a single unit.
        for dir in &measure.directions {
            if let Some(t) = dir.sound_tempo {
                tempo = t;
            } else if let Some(ref metro) = dir.metronome {
                tempo = metro.per_minute as f64 * quarters_per_beat(&metro.beat_unit, metro.dotted);
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
        // Safety: skip entries with out-of-range original_index to prevent
        // panics (especially across FFI boundaries).
        if um.original_index >= part.measures.len() || um.original_index >= states.len() {
            eprintln!(
                "[scorelib] WARNING: unrolled entry {} has original_index {} \
                 but part only has {} measures — skipping",
                i, um.original_index, part.measures.len()
            );
            continue;
        }
        let measure = &part.measures[um.original_index];
        let state = &states[um.original_index];

        let tempo = state.tempo;
        let time_sig = state.time_sig;
        let divisions = state.divisions;

        // ── Compute measure duration ────────────────────────────────
        // quarter_notes = (beats / beat_type) * 4
        // Guard against malformed input: beat_type=0 → Infinity, tempo=0 → Infinity.
        let safe_beat_type = if time_sig.1 > 0 { time_sig.1 } else { 4 };
        let safe_tempo = if tempo > 0.0 { tempo } else { DEFAULT_TEMPO };
        let nominal_quarters = (time_sig.0 as f64 / safe_beat_type as f64) * 4.0;
        let ms_per_quarter = 60_000.0 / safe_tempo;

        // Handle pickup measures: if this is an implicit measure (anacrusis),
        // compute duration from actual note content instead.
        let effective_quarters = if measure.implicit {
            let actual_quarters = actual_note_quarters(measure, divisions);
            if actual_quarters > 0.0 && actual_quarters < nominal_quarters {
                actual_quarters
            } else {
                nominal_quarters
            }
        } else {
            nominal_quarters
        };

        let duration_ms = effective_quarters * ms_per_quarter;

        entries.push(TimemapEntry {
            index: i,
            original_index: um.original_index,
            timestamp_ms: current_time_ms,
            duration_ms,
            tempo_bpm: tempo,
            time_sig,
            divisions,
            effective_quarters,
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

/// Convert a metronome beat unit to its quarter-note equivalent multiplier.
///
/// MusicXML metronome marks specify "X note-type = Y beats per minute".
/// All internal tempo calculations use quarter-notes-per-minute, so we
/// multiply `per_minute` by this factor to normalise.
///
/// Examples:
/// - `half = 60`          → 60 × 2.0 = 120 quarter/min
/// - `dotted quarter = 80` → 80 × 1.5 = 120 quarter/min
/// - `eighth = 240`        → 240 × 0.5 = 120 quarter/min
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quarters_per_beat_standard_units() {
        assert_eq!(quarters_per_beat("whole",   false), 4.0);
        assert_eq!(quarters_per_beat("half",    false), 2.0);
        assert_eq!(quarters_per_beat("quarter", false), 1.0);
        assert_eq!(quarters_per_beat("eighth",  false), 0.5);
        assert_eq!(quarters_per_beat("16th",    false), 0.25);
        assert_eq!(quarters_per_beat("32nd",    false), 0.125);
        println!("✓ quarters_per_beat: all standard units correct");
    }

    #[test]
    fn quarters_per_beat_dotted_units() {
        assert_eq!(quarters_per_beat("half",    true), 3.0);   // dotted half = 3 quarters
        assert_eq!(quarters_per_beat("quarter", true), 1.5);   // dotted quarter = 1.5 quarters
        assert_eq!(quarters_per_beat("eighth",  true), 0.75);  // dotted eighth = 0.75 quarters
        println!("✓ quarters_per_beat: dotted units correct");
    }

    #[test]
    fn half_note_60_bpm_yields_120_quarter_bpm() {
        // "half = 60" is a common slow-swing tempo — equals 120 quarter/min
        let bpm = 60.0 * quarters_per_beat("half", false);
        assert!((bpm - 120.0).abs() < 0.001,
            "half=60 should yield 120 quarter/min, got {}", bpm);
        println!("✓ half=60 → 120 quarter/min");
    }

    #[test]
    fn dotted_quarter_80_bpm_yields_120_quarter_bpm() {
        // "dotted quarter = 80" → 80 × 1.5 = 120 quarter/min
        let bpm = 80.0 * quarters_per_beat("quarter", true);
        assert!((bpm - 120.0).abs() < 0.001,
            "dotted quarter=80 should yield 120 quarter/min, got {}", bpm);
        println!("✓ dotted quarter=80 → 120 quarter/min");
    }

    #[test]
    fn eighth_240_bpm_yields_120_quarter_bpm() {
        // "eighth = 240" → 240 × 0.5 = 120 quarter/min
        let bpm = 240.0 * quarters_per_beat("eighth", false);
        assert!((bpm - 120.0).abs() < 0.001,
            "eighth=240 should yield 120 quarter/min, got {}", bpm);
        println!("✓ eighth=240 → 120 quarter/min");
    }

    #[test]
    fn unknown_beat_unit_defaults_to_quarter() {
        // Unknown unit should default to quarter (multiplier 1.0)
        assert_eq!(quarters_per_beat("unknown", false), 1.0);
        println!("✓ unknown beat unit defaults to quarter");
    }
}

fn quarters_per_beat(beat_unit: &str, dotted: bool) -> f64 {
    let base: f64 = match beat_unit {
        "whole"   => 4.0,
        "half"    => 2.0,
        "quarter" => 1.0,
        "eighth"  => 0.5,
        "16th"    => 0.25,
        "32nd"    => 0.125,
        _         => 1.0, // unknown unit — treat as quarter
    };
    if dotted { base * 1.5 } else { base }
}
