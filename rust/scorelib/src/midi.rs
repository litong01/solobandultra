//! MIDI file generation from a parsed and unrolled score.
//!
//! Produces a Standard MIDI File (SMF) Type 1 as raw bytes.
//! Track 0 is the tempo map; Track 1 is the melody.
//! Additional tracks (piano, bass, strings, drums, metronome) are
//! added by the accompaniment module.

use crate::accompaniment;
use crate::model::Score;
use crate::timemap::TimemapEntry;
use crate::unroller::UnrolledMeasure;

// ═══════════════════════════════════════════════════════════════════════
// Public API
// ═══════════════════════════════════════════════════════════════════════

/// Energy level for accompaniment velocity scaling.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Energy {
    Soft,
    Medium,
    Strong,
}

impl Default for Energy {
    fn default() -> Self {
        Energy::Medium
    }
}

/// Options controlling which MIDI tracks to generate.
#[derive(Debug, Clone)]
pub struct MidiOptions {
    pub include_melody: bool,
    pub include_piano: bool,
    pub include_bass: bool,
    pub include_strings: bool,
    pub include_drums: bool,
    pub include_metronome: bool,
    pub melody_channel: u8,
    pub energy: Energy,
}

impl Default for MidiOptions {
    fn default() -> Self {
        Self {
            include_melody: true,
            include_piano: false,
            include_bass: false,
            include_strings: false,
            include_drums: false,
            include_metronome: true,
            melody_channel: 0,
            energy: Energy::Medium,
        }
    }
}

/// A single MIDI event (note on/off, program change, etc.)
#[derive(Debug, Clone)]
pub struct MidiEvent {
    /// Absolute time in ticks from the start of the track
    pub tick: u32,
    /// Raw MIDI message bytes (status + data)
    pub bytes: Vec<u8>,
}

/// Ticks per quarter note in our MIDI output.
pub const TICKS_PER_QUARTER: u16 = 480;

/// Generate a complete Standard MIDI File (SMF Type 1).
pub fn generate_midi(
    score: &Score,
    part_idx: usize,
    unrolled: &[UnrolledMeasure],
    timemap: &[TimemapEntry],
    options: &MidiOptions,
) -> Vec<u8> {
    let part = match score.parts.get(part_idx) {
        Some(p) => p,
        None => return Vec::new(),
    };

    let mut tracks: Vec<Vec<u8>> = Vec::new();

    // ── Track 0: Tempo map ──────────────────────────────────────────
    tracks.push(build_tempo_track(timemap));

    // ── Track 1: Melody ─────────────────────────────────────────────
    if options.include_melody {
        let melody_events = extract_melody(part, unrolled, timemap, options.melody_channel);
        let mut track_events = Vec::new();
        // Program change (use part's MIDI program if available)
        let program = part.midi_program.unwrap_or(0).max(0).min(127) as u8;
        track_events.push(MidiEvent {
            tick: 0,
            bytes: vec![0xC0 | options.melody_channel, program],
        });
        track_events.extend(melody_events);
        tracks.push(encode_track(&track_events, "Melody"));
    }

    // ── Accompaniment tracks ────────────────────────────────────────
    let chords = accompaniment::analyze_chords(part, unrolled, timemap);

    if options.include_metronome {
        let events = accompaniment::generate_metronome(timemap);
        tracks.push(encode_track(&events, "Metronome"));
    }
    if options.include_piano {
        let events = accompaniment::generate_piano(&chords, options.energy);
        let mut te = vec![MidiEvent {
            tick: 0,
            bytes: vec![0xC1, 0], // Channel 1, Acoustic Grand Piano
        }];
        te.extend(events);
        tracks.push(encode_track(&te, "Piano"));
    }
    if options.include_bass {
        let events = accompaniment::generate_bass(&chords, options.energy);
        let mut te = vec![MidiEvent {
            tick: 0,
            bytes: vec![0xC2, 32], // Channel 2, Acoustic Bass
        }];
        te.extend(events);
        tracks.push(encode_track(&te, "Bass"));
    }
    if options.include_strings {
        let events = accompaniment::generate_strings(&chords, options.energy);
        let mut te = vec![MidiEvent {
            tick: 0,
            bytes: vec![0xC3, 48], // Channel 3, String Ensemble 1
        }];
        te.extend(events);
        tracks.push(encode_track(&te, "Strings"));
    }
    if options.include_drums {
        let events = accompaniment::generate_drums(&chords, options.energy);
        tracks.push(encode_track(&events, "Drums"));
    }

    // ── Assemble SMF ────────────────────────────────────────────────
    build_smf(&tracks)
}

// ═══════════════════════════════════════════════════════════════════════
// Melody extraction
// ═══════════════════════════════════════════════════════════════════════

/// Extract melody note events from the first part.
fn extract_melody(
    part: &crate::model::Part,
    unrolled: &[UnrolledMeasure],
    timemap: &[TimemapEntry],
    channel: u8,
) -> Vec<MidiEvent> {
    let mut events: Vec<MidiEvent> = Vec::new();

    for (i, um) in unrolled.iter().enumerate() {
        let measure = &part.measures[um.original_index];
        let entry = &timemap[i];
        let divisions = entry.divisions.max(1) as f64;
        let quarter_notes_in_measure =
            (entry.time_sig.0 as f64 / entry.time_sig.1 as f64) * 4.0;

        let mut position_in_divisions: f64 = 0.0;

        for note in &measure.notes {
            if note.grace {
                continue;
            }
            // Chord notes share the same onset as the previous note
            if note.chord {
                // Don't advance position — use the previous position
                if !note.rest {
                    if let Some(ref pitch) = note.pitch {
                        let midi_note = pitch.to_midi().max(0).min(127) as u8;
                        let note_time_ms = entry.timestamp_ms
                            + (position_in_divisions / divisions / quarter_notes_in_measure)
                                * entry.duration_ms;
                        let note_dur_ms = (note.duration as f64 / divisions
                            / quarter_notes_in_measure)
                            * entry.duration_ms;
                        let on_tick = ms_to_ticks(note_time_ms, timemap);
                        let off_tick = ms_to_ticks(note_time_ms + note_dur_ms, timemap);
                        // Skip tied continuation notes
                        if !note.tie_stop || note.tie_start {
                            events.push(MidiEvent {
                                tick: on_tick,
                                bytes: vec![0x90 | channel, midi_note, 80],
                            });
                        }
                        if !note.tie_start {
                            events.push(MidiEvent {
                                tick: off_tick,
                                bytes: vec![0x80 | channel, midi_note, 0],
                            });
                        }
                    }
                }
                continue;
            }

            if note.rest {
                position_in_divisions += note.duration as f64;
                continue;
            }

            if let Some(ref pitch) = note.pitch {
                let midi_note = pitch.to_midi().max(0).min(127) as u8;
                let note_time_ms = entry.timestamp_ms
                    + (position_in_divisions / divisions / quarter_notes_in_measure)
                        * entry.duration_ms;
                let note_dur_ms =
                    (note.duration as f64 / divisions / quarter_notes_in_measure)
                        * entry.duration_ms;

                let on_tick = ms_to_ticks(note_time_ms, timemap);
                let off_tick = ms_to_ticks(note_time_ms + note_dur_ms, timemap);

                // For tied notes: only emit note-on for the first note in
                // a tie chain (not tie_stop-only), and only emit note-off
                // for the last (not tie_start).
                if !note.tie_stop || note.tie_start {
                    // This is either the start of a tie or a standalone note
                    events.push(MidiEvent {
                        tick: on_tick,
                        bytes: vec![0x90 | channel, midi_note, 80],
                    });
                }
                if !note.tie_start {
                    // This is either the end of a tie or a standalone note
                    events.push(MidiEvent {
                        tick: off_tick,
                        bytes: vec![0x80 | channel, midi_note, 0],
                    });
                }
            }

            position_in_divisions += note.duration as f64;
        }
    }

    events.sort_by_key(|e| e.tick);
    events
}

// ═══════════════════════════════════════════════════════════════════════
// SMF byte encoding
// ═══════════════════════════════════════════════════════════════════════

/// Build the complete Standard MIDI File bytes.
fn build_smf(tracks: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();

    // MThd header
    out.extend_from_slice(b"MThd");
    out.extend_from_slice(&6u32.to_be_bytes()); // header length
    out.extend_from_slice(&1u16.to_be_bytes()); // format type 1
    out.extend_from_slice(&(tracks.len() as u16).to_be_bytes());
    out.extend_from_slice(&TICKS_PER_QUARTER.to_be_bytes());

    // Track chunks
    for track_data in tracks {
        out.extend_from_slice(b"MTrk");
        out.extend_from_slice(&(track_data.len() as u32).to_be_bytes());
        out.extend_from_slice(track_data);
    }

    out
}

/// Build the tempo track (track 0) — contains tempo change meta-events.
fn build_tempo_track(timemap: &[TimemapEntry]) -> Vec<u8> {
    let mut events: Vec<MidiEvent> = Vec::new();
    let mut last_tempo: f64 = 0.0;

    for entry in timemap {
        if (entry.tempo_bpm - last_tempo).abs() > 0.01 {
            let uspq = (60_000_000.0 / entry.tempo_bpm) as u32; // microseconds per quarter
            let tick = ms_to_ticks(entry.timestamp_ms, timemap);
            // Meta event: FF 51 03 tt tt tt
            events.push(MidiEvent {
                tick,
                bytes: vec![
                    0xFF,
                    0x51,
                    0x03,
                    ((uspq >> 16) & 0xFF) as u8,
                    ((uspq >> 8) & 0xFF) as u8,
                    (uspq & 0xFF) as u8,
                ],
            });
            last_tempo = entry.tempo_bpm;
        }
    }

    encode_track(&events, "Tempo")
}

/// Encode a track's events into raw MTrk bytes (delta-time encoded).
fn encode_track(events: &[MidiEvent], name: &str) -> Vec<u8> {
    let mut data = Vec::new();

    // Track name meta event
    let name_bytes = name.as_bytes();
    data.extend_from_slice(&[0x00]); // delta time 0
    data.push(0xFF);
    data.push(0x03); // track name
    write_vlq(&mut data, name_bytes.len() as u32);
    data.extend_from_slice(name_bytes);

    // Sort events by tick
    let mut sorted: Vec<&MidiEvent> = events.iter().collect();
    sorted.sort_by_key(|e| e.tick);

    let mut last_tick: u32 = 0;
    for event in &sorted {
        let delta = event.tick.saturating_sub(last_tick);
        write_vlq(&mut data, delta);
        data.extend_from_slice(&event.bytes);
        last_tick = event.tick;
    }

    // End of track
    data.extend_from_slice(&[0x00, 0xFF, 0x2F, 0x00]);

    data
}

/// Write a variable-length quantity (VLQ) to a byte vector.
fn write_vlq(out: &mut Vec<u8>, mut value: u32) {
    if value == 0 {
        out.push(0);
        return;
    }
    let mut buf = [0u8; 5];
    let mut i = 0;
    while value > 0 {
        buf[i] = (value & 0x7F) as u8;
        value >>= 7;
        if i > 0 {
            buf[i] |= 0x80;
        }
        i += 1;
    }
    // Write in reverse order
    for j in (0..i).rev() {
        out.push(buf[j]);
    }
}

/// Convert milliseconds to MIDI ticks, respecting tempo changes in the timemap.
pub fn ms_to_ticks(target_ms: f64, timemap: &[TimemapEntry]) -> u32 {
    if timemap.is_empty() {
        return 0;
    }

    let mut ticks: f64 = 0.0;
    let mut prev_ms: f64 = 0.0;
    let mut prev_tempo: f64 = timemap[0].tempo_bpm;

    for entry in timemap {
        let entry_ms = entry.timestamp_ms;
        if target_ms <= entry_ms {
            // Target is before this entry — convert remaining ms
            let remaining = target_ms - prev_ms;
            let ticks_per_ms = (TICKS_PER_QUARTER as f64 * prev_tempo) / 60_000.0;
            ticks += remaining * ticks_per_ms;
            return ticks.round() as u32;
        }
        // Accumulate ticks for the segment from prev to this entry
        let segment_ms = entry_ms - prev_ms;
        let ticks_per_ms = (TICKS_PER_QUARTER as f64 * prev_tempo) / 60_000.0;
        ticks += segment_ms * ticks_per_ms;
        prev_ms = entry_ms;
        prev_tempo = entry.tempo_bpm;
    }

    // Target is past the last entry
    let remaining = target_ms - prev_ms;
    let ticks_per_ms = (TICKS_PER_QUARTER as f64 * prev_tempo) / 60_000.0;
    ticks += remaining * ticks_per_ms;
    ticks.round() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vlq_encoding() {
        let mut buf = Vec::new();
        write_vlq(&mut buf, 0);
        assert_eq!(buf, vec![0x00]);

        buf.clear();
        write_vlq(&mut buf, 127);
        assert_eq!(buf, vec![0x7F]);

        buf.clear();
        write_vlq(&mut buf, 128);
        assert_eq!(buf, vec![0x81, 0x00]);

        buf.clear();
        write_vlq(&mut buf, 480);
        assert_eq!(buf, vec![0x83, 0x60]);
    }

    #[test]
    fn smf_header_valid() {
        let track = encode_track(&[], "Test");
        let smf = build_smf(&[track]);
        assert_eq!(&smf[0..4], b"MThd");
        assert_eq!(&smf[8..10], &1u16.to_be_bytes()); // format 1
        assert_eq!(&smf[12..14], &TICKS_PER_QUARTER.to_be_bytes());
        // Should contain MTrk
        assert!(smf.windows(4).any(|w| w == b"MTrk"));
    }
}
