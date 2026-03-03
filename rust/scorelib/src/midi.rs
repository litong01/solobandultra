//! MIDI file generation from a parsed and unrolled score.
//!
//! Produces a Standard MIDI File (SMF) Type 1 as raw bytes.
//! Track 0 is the tempo map; subsequent tracks are the melody (one per
//! staff for multi-staff parts like piano — each on its own MIDI channel
//! to prevent note-off conflicts on shared pitches).  Accompaniment
//! tracks (piano, bass, strings, drums, metronome) follow.

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
    /// Transposition in semitones (applied to the Score before generation).
    pub transpose: i32,
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
            transpose: 0,
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

    if unrolled.len() != timemap.len() {
        eprintln!(
            "[scorelib] WARNING: generate_midi: unrolled ({} measures) and timemap ({} entries) length mismatch — returning empty MIDI",
            unrolled.len(),
            timemap.len()
        );
        return Vec::new();
    }

    let mut tracks: Vec<Vec<u8>> = Vec::new();

    // ── Track 0: Tempo map ──────────────────────────────────────────
    tracks.push(build_tempo_track(timemap));

    // ── Track 1+ : Melody (one track per staff) ────────────────────
    // For multi-staff parts (e.g. piano with treble + bass), each staff
    // gets its own MIDI channel to prevent note-off/note-on conflicts
    // when both staves play the same pitch at overlapping times.
    if options.include_melody {
        let num_staves = detect_staves(part);
        let program = part.midi_program.unwrap_or(0).max(0).min(127) as u8;
        // Mask melody channel to valid MIDI range (0-15) and avoid the drum
        // channel (9) and accompaniment channels (1, 2, 3) to prevent conflicts.
        let melody_ch = options.melody_channel & 0x0F;

        if num_staves <= 1 {
            // Single-staff part: all notes on one channel/track.
            let melody_events = extract_melody(part, unrolled, timemap, melody_ch, None);
            let mut track_events = Vec::new();
            track_events.push(MidiEvent {
                tick: 0,
                bytes: vec![0xC0 | melody_ch, program],
            });
            track_events.extend(melody_events);
            tracks.push(encode_track(&track_events, "Melody"));
        } else {
            // Multi-staff part: one track per staff, each on its own channel.
            // Channels 0, 7, 8, 11, 12… (avoiding 1-3 for accompaniment, 9 for drums).
            let staff_channels: Vec<u8> = (0..num_staves as u8)
                .map(|s| match s {
                    0 => melody_ch,
                    1 => 7,
                    2 => 8,
                    3 => 11,
                    _ => (12 + s - 4).min(15),
                })
                .collect();

            for staff_num in 1..=num_staves {
                let ch = staff_channels[staff_num - 1];
                let events = extract_melody(
                    part, unrolled, timemap, ch, Some(staff_num as i32),
                );
                let mut track_events = Vec::new();
                track_events.push(MidiEvent {
                    tick: 0,
                    bytes: vec![0xC0 | ch, program],
                });
                track_events.extend(events);
                let name = if staff_num == 1 { "Treble" } else { "Bass" };
                tracks.push(encode_track(&track_events, name));
            }
        }
    }

    // ── Accompaniment tracks ────────────────────────────────────────
    let chords = accompaniment::analyze_chords(part, unrolled, timemap);

    if options.include_metronome {
        let events = accompaniment::generate_metronome(timemap);
        tracks.push(encode_track(&events, "Metronome"));
    }
    if options.include_piano {
        let events = accompaniment::generate_piano(&chords, options.energy, timemap);
        let mut te = vec![MidiEvent {
            tick: 0,
            bytes: vec![0xC1, 0], // Channel 1, Acoustic Grand Piano
        }];
        te.extend(events);
        tracks.push(encode_track(&te, "Piano"));
    }
    if options.include_bass {
        let events = accompaniment::generate_bass(&chords, options.energy, timemap);
        let mut te = vec![MidiEvent {
            tick: 0,
            bytes: vec![0xC2, 32], // Channel 2, Acoustic Bass
        }];
        te.extend(events);
        tracks.push(encode_track(&te, "Bass"));
    }
    if options.include_strings {
        let events = accompaniment::generate_strings(&chords, options.energy, timemap);
        let mut te = vec![MidiEvent {
            tick: 0,
            bytes: vec![0xC3, 48], // Channel 3, String Ensemble 1
        }];
        te.extend(events);
        tracks.push(encode_track(&te, "Strings"));
    }
    if options.include_drums {
        let events = accompaniment::generate_drums(&chords, options.energy, timemap);
        tracks.push(encode_track(&events, "Drums"));
    }

    // ── Assemble SMF ────────────────────────────────────────────────
    build_smf(&tracks)
}

// ═══════════════════════════════════════════════════════════════════════
// Melody extraction
// ═══════════════════════════════════════════════════════════════════════

/// Extract melody note events from a part, optionally filtering by staff.
///
/// When `staff_filter` is `None`, all notes are included (single-staff parts).
/// When `Some(n)`, only notes belonging to staff `n` are included — used for
/// multi-staff parts where each staff gets its own MIDI channel to prevent
/// note-off/note-on conflicts on identical pitches.
fn extract_melody(
    part: &crate::model::Part,
    unrolled: &[UnrolledMeasure],
    timemap: &[TimemapEntry],
    channel: u8,
    staff_filter: Option<i32>,
) -> Vec<MidiEvent> {
    let mut events: Vec<MidiEvent> = Vec::new();

    // Instrument transposition from the MusicXML <transpose> element.
    // Converts written pitch (what appears on the page) to concert pitch
    // (what actually sounds). Per MusicXML spec: sounding = written + chromatic.
    // Examples: Alto Sax (Eb instrument) = -9, Bb Clarinet = -2, Piano = 0.
    // Carried forward across measures and only updated when attributes change.
    let mut instrument_transpose: i32 = 0;

    for (i, um) in unrolled.iter().enumerate() {
        let measure = &part.measures[um.original_index];
        let entry = &timemap[i];

        // Update instrument transposition whenever attributes carry a new
        // <transpose> element (key/time/clef changes share the same block).
        if let Some(ref attrs) = measure.attributes {
            if let Some(ref t) = attrs.transpose {
                instrument_transpose =
                    t.chromatic + t.octave_change.unwrap_or(0) * 12;
            }
        }
        let divisions = entry.divisions.max(1) as f64;
        // Guard against zero/negative effective_quarters from degenerate measures
        // to prevent NaN/Infinity propagation into MIDI tick calculations.
        let quarter_notes_in_measure = if entry.effective_quarters > 0.0 {
            entry.effective_quarters
        } else {
            4.0 // default to 4/4
        };

        // Per-(staff, voice) position tracking for correct multi-voice timing.
        // MusicXML lists notes in document order; after a <backup> element
        // (which our parser ignores), a new voice's notes appear at the same
        // beat positions.  Using (staff, voice) as the key handles the case
        // where voice numbers overlap across staves (common in MuseScore exports).
        use std::collections::HashMap;
        type VoiceKey = (i32, i32); // (staff, voice)
        let mut voice_positions: HashMap<VoiceKey, f64> = HashMap::new();
        let mut voice_last_onset: HashMap<VoiceKey, f64> = HashMap::new();

        // Buffer for grace notes preceding a main note.
        let mut pending_graces: Vec<&crate::model::Note> = Vec::new();

        for note in &measure.notes {
            if note.grace {
                pending_graces.push(note);
                continue;
            }

            let note_staff = note.staff.unwrap_or(1);
            let vk: VoiceKey = (note_staff, note.voice.unwrap_or(1));
            let pos_div = voice_positions.entry(vk).or_insert(0.0);

            // Staff filter: always advance position tracking (so timing
            // stays correct for notes we DO include), but only emit MIDI
            // events for notes on the target staff.
            let emit = staff_filter.map_or(true, |sf| note_staff == sf);

            // Emit pending grace notes just before a principal pitched note.
            // Each grace note is ~1/8 of a beat, capped at 30-80ms, placed
            // immediately before the main note's onset with softer velocity.
            if !pending_graces.is_empty() && emit && !note.rest && !note.chord {
                let main_onset_ms = entry.timestamp_ms
                    + (*pos_div / divisions / quarter_notes_in_measure)
                        * entry.duration_ms;
                let beat_ms = 60_000.0 / entry.tempo_bpm;
                let grace_dur_ms = (beat_ms / 8.0).clamp(30.0, 80.0);
                let num_graces = pending_graces.len();

                for (gi, gnote) in pending_graces.iter().enumerate() {
                    if let Some(ref pitch) = gnote.pitch {
                        let midi_note = (pitch.to_midi() + instrument_transpose).max(0).min(127) as u8;
                        let offset = (num_graces - gi) as f64 * grace_dur_ms;
                        let onset_ms = (main_onset_ms - offset).max(0.0);
                        let off_ms = onset_ms + grace_dur_ms;

                        let on_tick = ms_to_ticks(onset_ms, timemap);
                        let off_tick = ms_to_ticks(off_ms, timemap);

                        events.push(MidiEvent {
                            tick: on_tick,
                            bytes: vec![0x90 | channel, midi_note, 90],
                        });
                        events.push(MidiEvent {
                            tick: off_tick,
                            bytes: vec![0x80 | channel, midi_note, 0],
                        });
                    }
                }
                pending_graces.clear();
            } else if !note.chord {
                // Clear grace notes before rests (no main note to attach to)
                pending_graces.clear();
            }

            // Chord notes share the same onset as their principal note
            if note.chord {
                if emit && !note.rest {
                    if let Some(ref pitch) = note.pitch {
                        let midi_note = (pitch.to_midi() + instrument_transpose).max(0).min(127) as u8;
                        let onset = voice_last_onset.get(&vk).copied().unwrap_or(0.0);
                        let note_time_ms = entry.timestamp_ms
                            + (onset / divisions / quarter_notes_in_measure)
                                * entry.duration_ms;
                        let note_dur_ms = (note.duration as f64 / divisions
                            / quarter_notes_in_measure)
                            * entry.duration_ms;
                        let on_tick = ms_to_ticks(note_time_ms, timemap);
                        let off_tick = ms_to_ticks(note_time_ms + note_dur_ms, timemap);
                        // Only emit note-on for the FIRST note in a tie chain.
                        // Middle notes (tie_stop && tie_start) must NOT re-trigger
                        // — that would leak a synth voice with no matching note-off.
                        if !note.tie_stop {
                            events.push(MidiEvent {
                                tick: on_tick,
                                bytes: vec![0x90 | channel, midi_note, 100],
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
                *pos_div += note.duration as f64;
                continue;
            }

            if let Some(ref pitch) = note.pitch {
                let midi_note = (pitch.to_midi() + instrument_transpose).max(0).min(127) as u8;
                voice_last_onset.insert(vk, *pos_div);

                if emit {
                    let note_time_ms = entry.timestamp_ms
                        + (*pos_div / divisions / quarter_notes_in_measure)
                            * entry.duration_ms;
                    let note_dur_ms =
                        (note.duration as f64 / divisions / quarter_notes_in_measure)
                            * entry.duration_ms;

                    let on_tick = ms_to_ticks(note_time_ms, timemap);
                    let off_tick = ms_to_ticks(note_time_ms + note_dur_ms, timemap);

                    // For tied notes: only emit note-on for the FIRST note in
                    // a tie chain (!tie_stop), and note-off for the LAST
                    // (!tie_start).  Middle notes (tie_stop && tie_start)
                    // emit neither — the pitch sustains through.
                    if !note.tie_stop {
                        events.push(MidiEvent {
                            tick: on_tick,
                            bytes: vec![0x90 | channel, midi_note, 100],
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

            *pos_div += note.duration as f64;
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
    const MIN_TEMPO_BPM: f64 = 20.0;
    const MAX_TEMPO_BPM: f64 = 600.0;
    let mut events: Vec<MidiEvent> = Vec::new();
    let mut last_tempo: f64 = 0.0;

    for entry in timemap {
        let tempo_bpm = entry.tempo_bpm.clamp(MIN_TEMPO_BPM, MAX_TEMPO_BPM);
        if (tempo_bpm - last_tempo).abs() > 0.01 {
            let uspq = (60_000_000.0 / tempo_bpm) as u32; // microseconds per quarter
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
            last_tempo = tempo_bpm;
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
/// Standard MIDI VLQ uses at most 5 bytes for u32.
fn write_vlq(out: &mut Vec<u8>, mut value: u32) {
    if value == 0 {
        out.push(0);
        return;
    }
    let mut buf = [0u8; 5];
    let mut i = 0;
    while value > 0 && i < 5 {
        buf[i] = (value & 0x7F) as u8;
        value >>= 7;
        if i > 0 {
            buf[i] |= 0x80;
        }
        i += 1;
    }
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

/// Detect the number of staves in a part by scanning attributes and note specifc parts.
/// Clamped to [1, 8] to prevent OOM from malformed input (negative values wrapping
/// to huge usize, or absurdly large stave counts).
fn detect_staves(part: &crate::model::Part) -> usize {
    const MAX_STAVES: usize = 8;
    let mut max_staff = 1usize;
    for measure in &part.measures {
        if let Some(ref attrs) = measure.attributes {
            if let Some(s) = attrs.staves {
                if s > 0 {
                    max_staff = max_staff.max((s as usize).min(MAX_STAVES));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instrument_transpose_shifts_midi_notes() {
        // Verify that a part with <transpose chromatic="-9"/> (alto saxophone)
        // produces MIDI notes 9 semitones lower than the written pitch.
        // Written C4 (MIDI 60) should sound as Eb3 (MIDI 51).
        use crate::model::*;
        use crate::unroller::UnrolledMeasure;
        use crate::timemap::TimemapEntry;

        let mut part = Part {
            id: "P1".into(),
            name: "Alto Saxophone".into(),
            abbreviation: None,
            instrument_name: None,
            midi_program: Some(65), // Alto Sax GM program
            midi_channel: Some(1),
            measures: vec![],
        };

        // Build one measure with a written C4 note and <transpose chromatic="-9">
        let mut measure = Measure {
            number: 1,
            implicit: false,
            width: None,
            attributes: Some(Attributes {
                divisions: Some(2),
                key: Some(Key { fifths: 0, mode: Some("major".into()) }),
                time: Some(TimeSignature { beats: 4, beat_type: 4 }),
                clefs: vec![],
                transpose: Some(Transpose { diatonic: -6, chromatic: -9, octave_change: None }),
                staves: None,
            }),
            notes: vec![Note {
                pitch: Some(Pitch { step: "C".into(), alter: None, octave: 4 }),
                duration: 2,
                voice: Some(1),
                note_type: Some("quarter".into()),
                stem: None,
                beams: vec![],
                rest: false,
                measure_rest: false,
                chord: false,
                dot: false,
                accidental: None,
                grace: false,
                grace_slash: false,
                staff: Some(1),
                tie_start: false,
                tie_stop: false,
                default_x: None,
                default_y: None,
                lyrics: vec![],
                slurs: vec![],
            }],
            harmonies: vec![],
            barlines: vec![],
            directions: vec![],
            new_system: false,
            new_page: false,
        };
        part.measures.push(measure.clone());

        // Also add a measure without attributes (transpose must carry forward)
        measure.attributes = None;
        part.measures.push(measure);

        let score = Score {
            title: Some("Test".into()),
            title_style: None,
            subtitle: None,
            subtitle_style: None,
            composer: None,
            composer_style: None,
            arranger: None,
            version: None,
            software: None,
            defaults: None,
            parts: vec![part],
        };

        let unrolled: Vec<UnrolledMeasure> = (0..2)
            .map(|i| UnrolledMeasure { original_index: i })
            .collect();

        let timemap: Vec<TimemapEntry> = (0..2)
            .map(|i| TimemapEntry {
                index: i,
                original_index: i,
                timestamp_ms: i as f64 * 2000.0,
                duration_ms: 2000.0,
                tempo_bpm: 120.0,
                time_sig: (4, 4),
                divisions: 2,
                effective_quarters: 4.0,
            })
            .collect();

        let options = MidiOptions { include_metronome: false, ..MidiOptions::default() };
        let midi = generate_midi(&score, 0, &unrolled, &timemap, &options);

        // Scan for note-on events (status byte 0x90 on channel 0)
        // and collect all MIDI note numbers played.
        let mut note_ons: Vec<u8> = Vec::new();
        let mut pos = 14; // skip MThd (14 bytes)
        while pos + 4 < midi.len() {
            if &midi[pos..pos+4] == b"MTrk" {
                let track_len = u32::from_be_bytes([midi[pos+4], midi[pos+5], midi[pos+6], midi[pos+7]]) as usize;
                let track_start = pos + 8;
                let track_end = (track_start + track_len).min(midi.len());
                let mut p = track_start;
                while p < track_end.saturating_sub(1) {
                    // Skip VLQ delta time
                    while p < track_end && midi[p] & 0x80 != 0 { p += 1; }
                    p += 1; // last VLQ byte
                    if p >= track_end { break; }
                    let status = midi[p];
                    if status & 0xF0 == 0x90 && p + 2 < track_end {
                        // Note-on: status, note, velocity
                        note_ons.push(midi[p + 1]);
                        p += 3;
                    } else {
                        p += 1;
                    }
                }
                pos = track_end;
            } else {
                pos += 1;
            }
        }

        // Written C4 = MIDI 60; with alto sax transpose (-9) = MIDI 51 (Eb3)
        assert!(
            note_ons.iter().any(|&n| n == 51),
            "Expected MIDI note 51 (Eb3, concert pitch of written C4 for alto sax), got: {:?}",
            note_ons
        );
        assert!(
            !note_ons.iter().any(|&n| n == 60),
            "MIDI note 60 (written C4) should NOT appear — must be transposed to concert pitch"
        );
        println!("✓ Alto sax instrument_transpose(-9): written C4 → concert Eb3 (MIDI 51)");
    }

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

    #[test]
    fn ms_to_ticks_at_120bpm() {
        use crate::timemap::TimemapEntry;
        // At 120 BPM: 1 quarter = 500ms, so 480 ticks = 500ms
        // → ticks_per_ms = 480 * 120 / 60000 = 0.96
        let timemap = vec![
            TimemapEntry {
                index: 0,
                original_index: 0,
                timestamp_ms: 0.0,
                duration_ms: 2000.0,
                tempo_bpm: 120.0,
                time_sig: (4, 4),
                divisions: 1,
                effective_quarters: 4.0,
            },
        ];

        let t0 = ms_to_ticks(0.0, &timemap);
        assert_eq!(t0, 0, "0 ms should be 0 ticks");

        let t500 = ms_to_ticks(500.0, &timemap);
        assert_eq!(t500, 480, "500ms at 120 BPM should be 480 ticks (1 quarter)");

        let t1000 = ms_to_ticks(1000.0, &timemap);
        assert_eq!(t1000, 960, "1000ms at 120 BPM should be 960 ticks (2 quarters)");
    }

    #[test]
    fn ms_to_ticks_at_60bpm() {
        use crate::timemap::TimemapEntry;
        // At 60 BPM: 1 quarter = 1000ms, so 480 ticks = 1000ms
        // → ticks_per_ms = 480 * 60 / 60000 = 0.48
        let timemap = vec![
            TimemapEntry {
                index: 0,
                original_index: 0,
                timestamp_ms: 0.0,
                duration_ms: 4000.0,
                tempo_bpm: 60.0,
                time_sig: (4, 4),
                divisions: 1,
                effective_quarters: 4.0,
            },
        ];

        let t1000 = ms_to_ticks(1000.0, &timemap);
        assert_eq!(t1000, 480, "1000ms at 60 BPM should be 480 ticks (1 quarter)");

        let t2000 = ms_to_ticks(2000.0, &timemap);
        assert_eq!(t2000, 960, "2000ms at 60 BPM should be 960 ticks (2 quarters)");
    }

    #[test]
    fn ms_to_ticks_tempo_change_mid_piece() {
        use crate::timemap::TimemapEntry;
        // First measure at 120 BPM (2000ms), second at 60 BPM (4000ms)
        let timemap = vec![
            TimemapEntry {
                index: 0,
                original_index: 0,
                timestamp_ms: 0.0,
                duration_ms: 2000.0,
                tempo_bpm: 120.0,
                time_sig: (4, 4),
                divisions: 1,
                effective_quarters: 4.0,
            },
            TimemapEntry {
                index: 1,
                original_index: 1,
                timestamp_ms: 2000.0,
                duration_ms: 4000.0,
                tempo_bpm: 60.0,
                time_sig: (4, 4),
                divisions: 1,
                effective_quarters: 4.0,
            },
        ];

        // Within first measure (120 BPM)
        let t500 = ms_to_ticks(500.0, &timemap);
        assert_eq!(t500, 480, "500ms in 120 BPM section");

        // At the boundary (2000ms = end of first measure)
        let t2000 = ms_to_ticks(2000.0, &timemap);
        assert_eq!(t2000, 1920, "2000ms = 4 quarters at 120 BPM = 1920 ticks");

        // Into second measure (60 BPM): 2000ms + 1000ms = 3000ms
        // 1920 ticks (first measure) + 480 ticks (1 quarter at 60 BPM)
        let t3000 = ms_to_ticks(3000.0, &timemap);
        assert_eq!(t3000, 2400, "3000ms should be 1920 + 480 = 2400 ticks");
    }

    #[test]
    fn ms_to_ticks_empty_timemap() {
        let timemap: Vec<crate::timemap::TimemapEntry> = vec![];
        let t = ms_to_ticks(1000.0, &timemap);
        assert_eq!(t, 0, "Empty timemap should return 0");
    }

    #[test]
    fn ms_to_ticks_monotonically_increasing() {
        use crate::timemap::TimemapEntry;
        let timemap = vec![
            TimemapEntry {
                index: 0,
                original_index: 0,
                timestamp_ms: 0.0,
                duration_ms: 2000.0,
                tempo_bpm: 120.0,
                time_sig: (4, 4),
                divisions: 1,
                effective_quarters: 4.0,
            },
            TimemapEntry {
                index: 1,
                original_index: 1,
                timestamp_ms: 2000.0,
                duration_ms: 4000.0,
                tempo_bpm: 60.0,
                time_sig: (4, 4),
                divisions: 1,
                effective_quarters: 4.0,
            },
        ];

        let mut prev_ticks = 0;
        for ms in (0..6000).step_by(100) {
            let ticks = ms_to_ticks(ms as f64, &timemap);
            assert!(
                ticks >= prev_ticks,
                "ms_to_ticks should be monotonically increasing: {}ms → {} ticks (prev={})",
                ms, ticks, prev_ticks
            );
            prev_ticks = ticks;
        }
    }
}
