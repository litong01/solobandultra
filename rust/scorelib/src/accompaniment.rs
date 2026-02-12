//! Accompaniment track generation: piano, bass, strings, drums, and metronome.
//!
//! Given a chord sequence derived from the score's harmony data and a timemap,
//! this module generates MIDI events for each accompaniment instrument using
//! algorithmically-generated patterns (ported from the TypeScript mysoloband
//! implementation).

use crate::midi::{Energy, MidiEvent, TICKS_PER_QUARTER, ms_to_ticks};
use crate::model::Part;
use crate::timemap::TimemapEntry;
use crate::unroller::UnrolledMeasure;

// ═══════════════════════════════════════════════════════════════════════
// Chord analysis
// ═══════════════════════════════════════════════════════════════════════

/// A chord in the play-order sequence with timing information.
#[derive(Debug, Clone)]
pub struct Chord {
    /// MIDI pitch class of the root (0=C, 1=C#, ... 11=B)
    pub root: u8,
    /// Chord quality
    pub kind: ChordKind,
    /// Start time in ms
    pub time_ms: f64,
    /// Duration in ms
    pub duration_ms: f64,
}

/// Supported chord qualities.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChordKind {
    Major,
    Minor,
    Dominant7,
    MajorSeventh,
    MinorSeventh,
    Diminished,
    HalfDiminished,
    Augmented,
}

/// Analyze chord symbols from the score to produce a timed chord sequence.
///
/// **Two modes:**
/// 1. If the score has `<harmony>` elements (explicit chord symbols), use them directly.
/// 2. If no harmonies are present, infer chords from the melody note pitch classes
///    using the key signature and diatonic priority — like a pianist reading a lead
///    sheet and creating their own chord voicings on the fly.
pub fn analyze_chords(
    part: &Part,
    unrolled: &[UnrolledMeasure],
    timemap: &[TimemapEntry],
) -> Vec<Chord> {
    // Check whether the score has *any* harmonies at all
    let has_harmonies = part.measures.iter().any(|m| !m.harmonies.is_empty());

    if has_harmonies {
        analyze_chords_from_harmonies(part, unrolled, timemap)
    } else {
        analyze_chords_from_melody(part, unrolled, timemap)
    }
}

/// Use explicit `<harmony>` elements from the MusicXML.
fn analyze_chords_from_harmonies(
    part: &Part,
    unrolled: &[UnrolledMeasure],
    timemap: &[TimemapEntry],
) -> Vec<Chord> {
    let mut chords: Vec<Chord> = Vec::new();

    for (i, um) in unrolled.iter().enumerate() {
        let measure = &part.measures[um.original_index];
        let entry = &timemap[i];

        if measure.harmonies.is_empty() {
            // No chord symbol in this measure — repeat the previous chord
            if let Some(prev) = chords.last().cloned() {
                chords.push(Chord {
                    root: prev.root,
                    kind: prev.kind,
                    time_ms: entry.timestamp_ms,
                    duration_ms: entry.duration_ms,
                });
            } else {
                chords.push(Chord {
                    root: 0,
                    kind: ChordKind::Major,
                    time_ms: entry.timestamp_ms,
                    duration_ms: entry.duration_ms,
                });
            }
            continue;
        }

        let h = &measure.harmonies[0];
        let root = step_to_pitch_class(&h.root.step, h.root.alter.unwrap_or(0.0));
        let kind = parse_chord_kind(&h.kind);

        chords.push(Chord {
            root,
            kind,
            time_ms: entry.timestamp_ms,
            duration_ms: entry.duration_ms,
        });
    }

    chords
}

/// Infer chords from melody notes when no explicit harmonies exist.
///
/// For each measure, collects the pitch classes of all sounding notes,
/// then picks the most likely chord root and quality using the key
/// signature and standard diatonic harmony rules.
fn analyze_chords_from_melody(
    part: &Part,
    unrolled: &[UnrolledMeasure],
    timemap: &[TimemapEntry],
) -> Vec<Chord> {
    // Detect key from the first key signature found
    let key_root = detect_key_root(part);

    let mut chords: Vec<Chord> = Vec::new();

    for (i, um) in unrolled.iter().enumerate() {
        let measure = &part.measures[um.original_index];
        let entry = &timemap[i];

        // Collect unique pitch classes from all sounding notes in this measure
        let mut pitch_classes: Vec<u8> = Vec::new();
        for note in &measure.notes {
            if note.rest || note.grace || note.chord {
                continue;
            }
            if let Some(ref pitch) = note.pitch {
                let pc = (pitch.to_midi().rem_euclid(12)) as u8;
                if !pitch_classes.contains(&pc) {
                    pitch_classes.push(pc);
                }
            }
        }

        if pitch_classes.is_empty() {
            // Rest-only measure — repeat previous chord or use tonic
            if let Some(prev) = chords.last().cloned() {
                chords.push(Chord {
                    root: prev.root,
                    kind: prev.kind,
                    time_ms: entry.timestamp_ms,
                    duration_ms: entry.duration_ms,
                });
            } else {
                chords.push(Chord {
                    root: key_root,
                    kind: ChordKind::Major,
                    time_ms: entry.timestamp_ms,
                    duration_ms: entry.duration_ms,
                });
            }
            continue;
        }

        let root = find_most_likely_root(&pitch_classes, key_root);
        let kind = infer_chord_kind(&pitch_classes, root);

        chords.push(Chord {
            root,
            kind,
            time_ms: entry.timestamp_ms,
            duration_ms: entry.duration_ms,
        });
    }

    chords
}

/// Detect the key root pitch class from the first key signature in the part.
/// Maps the `fifths` value (circle of fifths position) to a pitch class.
/// Falls back to C major (0) if no key signature is found.
fn detect_key_root(part: &Part) -> u8 {
    for m in &part.measures {
        if let Some(ref attrs) = m.attributes {
            if let Some(ref key) = attrs.key {
                return fifths_to_pitch_class(key.fifths);
            }
        }
    }
    0 // Default: C major
}

/// Convert a circle-of-fifths position to a pitch class.
/// -7=Cb, -6=Gb, ... 0=C, 1=G, 2=D, ... 7=C#
fn fifths_to_pitch_class(fifths: i32) -> u8 {
    // Each step on the circle of fifths adds 7 semitones
    ((fifths * 7).rem_euclid(12)) as u8
}

/// Find the most likely chord root from a set of pitch classes.
///
/// Checks diatonic scale degrees in priority order: I, V, IV, vi, ii, iii.
/// This ordering reflects common harmonic patterns — tonic and dominant are
/// most frequent, followed by subdominant and relative minor.
fn find_most_likely_root(pitches: &[u8], key_root: u8) -> u8 {
    // Diatonic scale degree roots in priority order (semitones from key root)
    let diatonic_offsets: [u8; 6] = [
        0,  // I   (tonic)
        7,  // V   (dominant)
        5,  // IV  (subdominant)
        9,  // vi  (relative minor / submediant)
        2,  // ii  (supertonic)
        4,  // iii (mediant)
    ];

    for &offset in &diatonic_offsets {
        let candidate = (key_root + offset) % 12;
        if pitches.contains(&candidate) {
            return candidate;
        }
    }

    // Fallback: use the first pitch class encountered
    pitches[0]
}

/// Infer the chord quality from the pitch classes present relative to the root.
///
/// Checks for triad patterns by looking at intervals above the root.
/// Note: we check dominant7 BEFORE major (unlike the TypeScript version which
/// had a bug where dominant7 was unreachable).
fn infer_chord_kind(pitches: &[u8], root: u8) -> ChordKind {
    let intervals: Vec<u8> = pitches
        .iter()
        .map(|&p| (p as i32 - root as i32).rem_euclid(12) as u8)
        .collect();

    let has = |interval: u8| intervals.contains(&interval);

    // Check for dominant 7th first (0, 4, 7, 10) — BEFORE major
    if has(4) && has(7) && has(10) {
        return ChordKind::Dominant7;
    }

    // Check for diminished (0, 3, 6)
    if has(3) && has(6) {
        return ChordKind::Diminished;
    }

    // Check for minor triad (0, 3, 7)
    if has(3) && has(7) {
        return ChordKind::Minor;
    }

    // Check for major triad (0, 4, 7)
    if has(4) && has(7) {
        return ChordKind::Major;
    }

    // Check for just a minor 3rd present (lean toward minor)
    if has(3) {
        return ChordKind::Minor;
    }

    // Default to major
    ChordKind::Major
}

fn step_to_pitch_class(step: &str, alter: f64) -> u8 {
    let base = match step {
        "C" => 0, "D" => 2, "E" => 4, "F" => 5,
        "G" => 7, "A" => 9, "B" => 11,
        _ => 0,
    };
    ((base as i32 + alter.round() as i32).rem_euclid(12)) as u8
}

fn parse_chord_kind(kind: &str) -> ChordKind {
    match kind {
        "major" => ChordKind::Major,
        "minor" => ChordKind::Minor,
        "dominant" | "dominant-seventh" => ChordKind::Dominant7,
        "major-seventh" => ChordKind::MajorSeventh,
        "minor-seventh" => ChordKind::MinorSeventh,
        "diminished" => ChordKind::Diminished,
        "half-diminished" => ChordKind::HalfDiminished,
        "augmented" => ChordKind::Augmented,
        _ => ChordKind::Major,
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Chord voicing
// ═══════════════════════════════════════════════════════════════════════

/// Get MIDI notes for a chord voicing rooted around MIDI note 48 (C3).
fn get_chord_voicing(root: u8, kind: ChordKind) -> Vec<u8> {
    let base = 48 + root;
    match kind {
        ChordKind::Major => vec![base, base + 4, base + 7],
        ChordKind::Minor => vec![base, base + 3, base + 7],
        ChordKind::Dominant7 => vec![base, base + 4, base + 7, base + 10],
        ChordKind::MajorSeventh => vec![base, base + 4, base + 7, base + 11],
        ChordKind::MinorSeventh => vec![base, base + 3, base + 7, base + 10],
        ChordKind::Diminished => vec![base, base + 3, base + 6],
        ChordKind::HalfDiminished => vec![base, base + 3, base + 6, base + 10],
        ChordKind::Augmented => vec![base, base + 4, base + 8],
    }
}

/// Add a 7th to a voicing if it doesn't already have one.
fn add_seventh(voicing: &[u8], kind: ChordKind) -> Vec<u8> {
    let mut v = voicing.to_vec();
    let seventh_interval = match kind {
        ChordKind::Major => 11,
        ChordKind::Minor => 10,
        ChordKind::Dominant7 | ChordKind::MinorSeventh | ChordKind::HalfDiminished => 10,
        ChordKind::MajorSeventh => 11,
        ChordKind::Diminished => 9,
        ChordKind::Augmented => 11,
    };
    if v.len() < 4 {
        v.push(v[0] + seventh_interval);
    }
    v
}

/// Find the smoothest inversion of `voicing` relative to `previous`.
/// Tries all rotations and picks the one with minimum total pitch movement.
fn get_smoother_voicing(voicing: &[u8], previous: &[u8]) -> Vec<u8> {
    if previous.is_empty() || voicing.is_empty() {
        return voicing.to_vec();
    }

    let mut best = voicing.to_vec();
    let mut best_distance = i32::MAX;

    let n = voicing.len();
    let mut current = voicing.to_vec();

    for _ in 0..n {
        // Compute total distance
        let dist: i32 = current
            .iter()
            .zip(previous.iter().cycle())
            .map(|(&a, &b)| (a as i32 - b as i32).abs())
            .sum();
        if dist < best_distance {
            best_distance = dist;
            best = current.clone();
        }
        // Rotate: move lowest note up an octave
        if let Some(&lowest) = current.first() {
            current.remove(0);
            current.push(lowest + 12);
        }
    }

    best
}

// ═══════════════════════════════════════════════════════════════════════
// Energy multipliers
// ═══════════════════════════════════════════════════════════════════════

struct EnergyMultipliers {
    piano: f64,
    bass: f64,
    strings: f64,
    drums: f64,
}

fn energy_multipliers(energy: Energy) -> EnergyMultipliers {
    match energy {
        Energy::Soft => EnergyMultipliers { piano: 0.5, bass: 0.6, strings: 0.4, drums: 0.4 },
        Energy::Medium => EnergyMultipliers { piano: 0.7, bass: 0.75, strings: 0.6, drums: 0.6 },
        Energy::Strong => EnergyMultipliers { piano: 0.85, bass: 0.9, strings: 0.75, drums: 0.8 },
    }
}

fn velocity(base: f64, multiplier: f64) -> u8 {
    (base * multiplier).round().max(1.0).min(127.0) as u8
}

// ═══════════════════════════════════════════════════════════════════════
// Metronome
// ═══════════════════════════════════════════════════════════════════════

/// MIDI drum notes for metronome clicks.
const CLICK_HI: u8 = 76; // Hi Wood Block — downbeat
const CLICK_LO: u8 = 77; // Lo Wood Block — other beats
#[allow(dead_code)]
const DRUM_CHANNEL: u8 = 9;

/// Generate metronome click events from the timemap.
pub fn generate_metronome(timemap: &[TimemapEntry]) -> Vec<MidiEvent> {
    let mut events = Vec::new();
    let click_dur_ms = 100.0; // each click lasts 100ms

    for (i, entry) in timemap.iter().enumerate() {
        let beats = entry.time_sig.0.max(1);
        let beat_dur_ms = entry.duration_ms / beats as f64;

        // Detect pickup measure: compare duration to next measure
        let is_pickup = if i == 0 && timemap.len() > 1 {
            entry.duration_ms / timemap[1].duration_ms < 0.95
        } else {
            false
        };

        let actual_beats = if is_pickup {
            (entry.duration_ms / beat_dur_ms).round() as i32
        } else {
            beats
        };

        for b in 0..actual_beats {
            let beat_time_ms = entry.timestamp_ms + b as f64 * beat_dur_ms;
            let note = if b == 0 { CLICK_HI } else { CLICK_LO };
            let vel = if b == 0 { 127 } else { 100 };

            let on_tick = ms_to_ticks(beat_time_ms, timemap);
            let off_tick = ms_to_ticks(beat_time_ms + click_dur_ms, timemap);

            events.push(MidiEvent {
                tick: on_tick,
                bytes: vec![0x99, note, vel], // Channel 9 note on
            });
            events.push(MidiEvent {
                tick: off_tick,
                bytes: vec![0x89, note, 0], // Channel 9 note off
            });
        }
    }

    events
}

// ═══════════════════════════════════════════════════════════════════════
// Piano accompaniment
// ═══════════════════════════════════════════════════════════════════════

const PIANO_CHANNEL: u8 = 1;

/// Generate piano accompaniment events (broken chord / arpeggio pattern).
pub fn generate_piano(chords: &[Chord], energy: Energy, timemap: &[TimemapEntry]) -> Vec<MidiEvent> {
    let em = energy_multipliers(energy);
    let mut events = Vec::new();
    let mut prev_voicing: Vec<u8> = Vec::new();

    for chord in chords {
        let raw_voicing = get_chord_voicing(chord.root, chord.kind);
        let voicing_7 = add_seventh(&raw_voicing, chord.kind);
        let voicing = get_smoother_voicing(&voicing_7, &prev_voicing);

        // Skip bass note (index 0) — leave that for the bass track
        let piano_notes: Vec<u8> = if voicing.len() > 1 {
            voicing[1..].to_vec()
        } else {
            voicing.clone()
        };

        let base_vel = velocity(80.0, em.piano);
        let dur_ms = chord.duration_ms * 0.5;

        // Arpeggio: stagger each note slightly
        for (j, &note) in piano_notes.iter().enumerate() {
            let stagger_ms = j as f64 * 15.0;
            let note_time_ms = chord.time_ms + stagger_ms;
            let on_tick = ms_to_ticks(note_time_ms, timemap);
            let off_tick = ms_to_ticks(note_time_ms + dur_ms, timemap);

            let note_vel = base_vel.min(127);
            events.push(MidiEvent {
                tick: on_tick,
                bytes: vec![0x90 | PIANO_CHANNEL, note.min(127), note_vel],
            });
            events.push(MidiEvent {
                tick: off_tick,
                bytes: vec![0x80 | PIANO_CHANNEL, note.min(127), 0],
            });
        }

        // Second sweep if chord is long enough (> 1 second)
        if chord.duration_ms > 1000.0 {
            let sweep_time = chord.time_ms + chord.duration_ms * 0.5;
            for (j, &note) in piano_notes.iter().enumerate() {
                let stagger_ms = j as f64 * 15.0;
                let note_time_ms = sweep_time + stagger_ms;
                let on_tick = ms_to_ticks(note_time_ms, timemap);
                let off_tick = ms_to_ticks(note_time_ms + dur_ms * 0.8, timemap);

                let note_vel = (base_vel as f64 * 0.85).round().max(1.0).min(127.0) as u8;
                events.push(MidiEvent {
                    tick: on_tick,
                    bytes: vec![0x90 | PIANO_CHANNEL, note.min(127), note_vel],
                });
                events.push(MidiEvent {
                    tick: off_tick,
                    bytes: vec![0x80 | PIANO_CHANNEL, note.min(127), 0],
                });
            }
        }

        prev_voicing = voicing;
    }

    events
}

// ═══════════════════════════════════════════════════════════════════════
// Bass accompaniment
// ═══════════════════════════════════════════════════════════════════════

const BASS_CHANNEL: u8 = 2;

/// Generate walking bass events.
pub fn generate_bass(chords: &[Chord], energy: Energy, timemap: &[TimemapEntry]) -> Vec<MidiEvent> {
    let em = energy_multipliers(energy);
    let mut events = Vec::new();
    let base_vel = velocity(90.0, em.bass);

    for chord in chords {
        // Root note in bass range (E1-D#2 → MIDI 36-47)
        let bass_note = 36 + (chord.root % 12);

        // Beat 1: root
        let dur1 = chord.duration_ms * 0.45;
        let on1 = ms_to_ticks(chord.time_ms, timemap);
        let off1 = ms_to_ticks(chord.time_ms + dur1, timemap);
        events.push(MidiEvent {
            tick: on1,
            bytes: vec![0x90 | BASS_CHANNEL, bass_note, base_vel],
        });
        events.push(MidiEvent {
            tick: off1,
            bytes: vec![0x80 | BASS_CHANNEL, bass_note, 0],
        });

        // Beat 2/3: fifth
        let fifth_time = chord.time_ms + chord.duration_ms * 0.5;
        let fifth_note = bass_note + 7;
        let dur2 = chord.duration_ms * 0.35;
        let on2 = ms_to_ticks(fifth_time, timemap);
        let off2 = ms_to_ticks(fifth_time + dur2, timemap);
        events.push(MidiEvent {
            tick: on2,
            bytes: vec![0x90 | BASS_CHANNEL, fifth_note.min(127), base_vel],
        });
        events.push(MidiEvent {
            tick: off2,
            bytes: vec![0x80 | BASS_CHANNEL, fifth_note.min(127), 0],
        });

        // Approach note (octave) if chord is long enough
        if chord.duration_ms > 1200.0 {
            let oct_time = chord.time_ms + chord.duration_ms * 0.75;
            let oct_note = bass_note + 12;
            let dur3 = chord.duration_ms * 0.20;
            let on3 = ms_to_ticks(oct_time, timemap);
            let off3 = ms_to_ticks(oct_time + dur3, timemap);
            events.push(MidiEvent {
                tick: on3,
                bytes: vec![0x90 | BASS_CHANNEL, oct_note.min(127), base_vel],
            });
            events.push(MidiEvent {
                tick: off3,
                bytes: vec![0x80 | BASS_CHANNEL, oct_note.min(127), 0],
            });
        }
    }

    events
}

// ═══════════════════════════════════════════════════════════════════════
// String accompaniment
// ═══════════════════════════════════════════════════════════════════════

const STRING_CHANNEL: u8 = 3;

/// Generate sustained string pad events.
pub fn generate_strings(chords: &[Chord], energy: Energy, timemap: &[TimemapEntry]) -> Vec<MidiEvent> {
    let em = energy_multipliers(energy);
    let mut events = Vec::new();
    let base_vel = velocity(65.0, em.strings);
    let mut prev_voicing: Vec<u8> = Vec::new();

    for chord in chords {
        let raw_voicing = get_chord_voicing(chord.root, chord.kind);
        let voicing = get_smoother_voicing(&raw_voicing, &prev_voicing);

        // Sustained pad: play all notes for the chord duration (slight overlap)
        let dur_ms = chord.duration_ms * 1.05;
        let on_tick = ms_to_ticks(chord.time_ms, timemap);
        let off_tick = ms_to_ticks(chord.time_ms + dur_ms, timemap);

        for &note in &voicing {
            events.push(MidiEvent {
                tick: on_tick,
                bytes: vec![0x90 | STRING_CHANNEL, note.min(127), base_vel],
            });
            events.push(MidiEvent {
                tick: off_tick,
                bytes: vec![0x80 | STRING_CHANNEL, note.min(127), 0],
            });
        }

        prev_voicing = voicing;
    }

    events
}

// ═══════════════════════════════════════════════════════════════════════
// Drum accompaniment
// ═══════════════════════════════════════════════════════════════════════

const KICK: u8 = 36;
const SNARE: u8 = 38;
const HIHAT_CLOSED: u8 = 42;

/// Generate drum pattern events.
pub fn generate_drums(chords: &[Chord], energy: Energy, timemap: &[TimemapEntry]) -> Vec<MidiEvent> {
    let em = energy_multipliers(energy);
    let mut events = Vec::new();

    for chord in chords {
        let beats = (chord.duration_ms / 500.0).round().max(2.0) as i32;
        let beat_dur_ms = chord.duration_ms / beats as f64;

        for b in 0..beats {
            let beat_time = chord.time_ms + b as f64 * beat_dur_ms;
            let on_tick = ms_to_ticks(beat_time, timemap);
            let dur_ticks = (TICKS_PER_QUARTER as f64 * 0.25) as u32;
            let off_tick = on_tick + dur_ticks;

            // Kick on beat 1 and 3
            if b == 0 || (beats >= 4 && b == 2) {
                let vel = velocity(100.0, em.drums);
                events.push(MidiEvent {
                    tick: on_tick,
                    bytes: vec![0x99, KICK, vel],
                });
                events.push(MidiEvent {
                    tick: off_tick,
                    bytes: vec![0x89, KICK, 0],
                });
            }

            // Snare on backbeats (2, 4)
            if b % 2 == 1 {
                let vel = velocity(90.0, em.drums);
                events.push(MidiEvent {
                    tick: on_tick,
                    bytes: vec![0x99, SNARE, vel],
                });
                events.push(MidiEvent {
                    tick: off_tick,
                    bytes: vec![0x89, SNARE, 0],
                });
            }

            // Hi-hat on every beat
            let hh_vel = velocity(70.0, em.drums);
            events.push(MidiEvent {
                tick: on_tick,
                bytes: vec![0x99, HIHAT_CLOSED, hh_vel],
            });
            events.push(MidiEvent {
                tick: off_tick,
                bytes: vec![0x89, HIHAT_CLOSED, 0],
            });

            // Hi-hat eighth notes between beats
            if beat_dur_ms > 300.0 {
                let eighth_time = beat_time + beat_dur_ms * 0.5;
                let eighth_tick = ms_to_ticks(eighth_time, timemap);
                let eighth_vel = velocity(50.0, em.drums);
                events.push(MidiEvent {
                    tick: eighth_tick,
                    bytes: vec![0x99, HIHAT_CLOSED, eighth_vel],
                });
                events.push(MidiEvent {
                    tick: eighth_tick + dur_ticks,
                    bytes: vec![0x89, HIHAT_CLOSED, 0],
                });
            }
        }
    }

    events
}
