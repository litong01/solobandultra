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
    if unrolled.len() != timemap.len() {
        eprintln!(
            "[scorelib] WARNING: analyze_chords: unrolled ({} measures) and timemap ({} entries) length mismatch — returning empty chord list",
            unrolled.len(),
            timemap.len()
        );
        return Vec::new();
    }

    // Check whether the score has *any* harmonies at all
    let has_harmonies = part.measures.iter().any(|m| !m.harmonies.is_empty());

    if has_harmonies {
        analyze_chords_from_harmonies(part, unrolled, timemap)
    } else {
        analyze_chords_from_melody(part, unrolled, timemap)
    }
}

/// Use explicit `<harmony>` elements from the MusicXML.
///
/// Supports multiple chord symbols per measure: if a measure has N harmonies
/// at different beat offsets, N chords are emitted with durations that span
/// from one harmony to the next (or to the end of the measure).
fn analyze_chords_from_harmonies(
    part: &Part,
    unrolled: &[UnrolledMeasure],
    timemap: &[TimemapEntry],
) -> Vec<Chord> {
    let mut chords: Vec<Chord> = Vec::new();

    for (i, um) in unrolled.iter().enumerate() {
        let measure = &part.measures[um.original_index];
        let entry = &timemap[i];
        let divisions = entry.divisions.max(1) as f64;
        let denom = entry.effective_quarters * divisions;
        let ms_per_division = if denom > 0.0 { entry.duration_ms / denom } else { 0.0 };

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

        // Process all harmonies in this measure, splitting the measure
        // duration among them based on their beat offsets.
        let n = measure.harmonies.len();
        for (j, h) in measure.harmonies.iter().enumerate() {
            let root = step_to_pitch_class(&h.root.step, h.root.alter.unwrap_or(0.0));
            let kind = parse_chord_kind(&h.kind);

            let offset_ms = h.offset_divisions as f64 * ms_per_division;
            // Duration runs until the next harmony's offset, or end of measure
            let end_ms = if j + 1 < n {
                let next_offset = measure.harmonies[j + 1].offset_divisions as f64;
                next_offset * ms_per_division
            } else {
                entry.duration_ms
            };
            let dur_ms = (end_ms - offset_ms).max(1.0);

            chords.push(Chord {
                root,
                kind,
                time_ms: entry.timestamp_ms + offset_ms,
                duration_ms: dur_ms,
            });
        }
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

        // Collect unique pitch classes from all sounding notes in this measure.
        // Include chord notes (simultaneous notes marked with <chord/>) — these
        // carry the intervals that define chord quality (e.g. C-E-G written out
        // as a chordal texture).  Only skip grace notes and rests.
        let mut pitch_classes: Vec<u8> = Vec::new();
        for note in &measure.notes {
            if note.rest || note.grace {
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
/// Checks for triad and seventh-chord patterns by looking at intervals above
/// the root.  Ordered from most specific (4-note) to least specific (2-note)
/// to avoid false matches.
fn infer_chord_kind(pitches: &[u8], root: u8) -> ChordKind {
    let intervals: Vec<u8> = pitches
        .iter()
        .map(|&p| (p as i32 - root as i32).rem_euclid(12) as u8)
        .collect();

    let has = |interval: u8| intervals.contains(&interval);

    // ── 4-note chords (check first for specificity) ──────────────

    // Major seventh (0, 4, 7, 11)
    if has(4) && has(7) && has(11) {
        return ChordKind::MajorSeventh;
    }
    // Dominant seventh (0, 4, 7, 10)
    if has(4) && has(7) && has(10) {
        return ChordKind::Dominant7;
    }
    // Minor seventh (0, 3, 7, 10)
    if has(3) && has(7) && has(10) {
        return ChordKind::MinorSeventh;
    }
    // Half-diminished seventh (0, 3, 6, 10)
    if has(3) && has(6) && has(10) {
        return ChordKind::HalfDiminished;
    }

    // ── 3-note chords (triads) ───────────────────────────────────

    // Augmented triad (0, 4, 8)
    if has(4) && has(8) && !has(7) {
        return ChordKind::Augmented;
    }
    // Diminished triad (0, 3, 6)
    if has(3) && has(6) {
        return ChordKind::Diminished;
    }
    // Minor triad (0, 3, 7)
    if has(3) && has(7) {
        return ChordKind::Minor;
    }
    // Major triad (0, 4, 7)
    if has(4) && has(7) {
        return ChordKind::Major;
    }

    // ── Partial matches ──────────────────────────────────────────

    // Just a minor 3rd → lean toward minor
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
///
/// When voicings have different lengths (e.g. triad → 7th chord), we compare
/// only the overlapping positions to avoid garbage comparisons from `.cycle()`.
fn get_smoother_voicing(voicing: &[u8], previous: &[u8]) -> Vec<u8> {
    if previous.is_empty() || voicing.is_empty() {
        return voicing.to_vec();
    }

    let mut best = voicing.to_vec();
    let mut best_distance = i32::MAX;

    let n = voicing.len();
    let compare_len = n.min(previous.len());
    let mut current = voicing.to_vec();

    for _ in 0..n {
        // Compare only the overlapping positions — don't .cycle() the shorter
        // voicing, which would wrap around and produce meaningless comparisons.
        let dist: i32 = current
            .iter()
            .take(compare_len)
            .zip(previous.iter().take(compare_len))
            .map(|(&a, &b)| (a as i32 - b as i32).abs())
            .sum();
        if dist < best_distance {
            best_distance = dist;
            best = current.clone();
        }
        // Rotate: move lowest note up an octave
        if let Some(&lowest) = current.first() {
            current.remove(0);
            current.push(lowest.saturating_add(12));
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

/// Determine the number of "felt" beats for a time signature.
///
/// Compound meters (6/8, 9/8, 12/8) are felt in groups of 3 eighth notes,
/// so 6/8 has 2 felt beats, 9/8 has 3, 12/8 has 4.
/// Simple meters use the numerator directly: 4/4 = 4 beats, 3/4 = 3 beats.
fn felt_beats(beats: i32, beat_type: i32) -> i32 {
    // Compound: numerator divisible by 3, denominator is 8, and more than 3
    // (3/8 is simple — 3 eighth-note beats, not 1 dotted-quarter beat)
    if beat_type == 8 && beats > 3 && beats % 3 == 0 {
        beats / 3
    } else {
        beats
    }
}

/// Generate metronome click events from the timemap.
pub fn generate_metronome(timemap: &[TimemapEntry]) -> Vec<MidiEvent> {
    let mut events = Vec::new();
    let click_dur_ms = 100.0; // each click lasts 100ms

    for entry in timemap.iter() {
        let (beats, beat_type) = entry.time_sig;
        let num_clicks = felt_beats(beats, beat_type).max(1);
        let beat_dur_ms = entry.duration_ms / num_clicks as f64;

        for b in 0..num_clicks {
            let beat_time_ms = entry.timestamp_ms + b as f64 * beat_dur_ms;
            let note = if b == 0 { CLICK_HI } else { CLICK_LO };
            let vel = if b == 0 { 85 } else { 65 };

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

        // Sustained pad: play all notes for nearly the full chord duration.
        // Use 98% to avoid MIDI note-on collision — if we overlap into the next
        // chord, a shared pitch gets its note-off from chord N killing chord N+1.
        let dur_ms = chord.duration_ms * 0.98;
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

/// Generate drum pattern events using the timemap's time signature to derive
/// the correct number of beats per measure (instead of the old hardcoded 500ms
/// that only worked at 120 BPM).
///
/// Drums play a steady pattern per measure regardless of chord changes, so this
/// iterates over timemap entries — not chords.
pub fn generate_drums(_: &[Chord], energy: Energy, timemap: &[TimemapEntry]) -> Vec<MidiEvent> {
    let em = energy_multipliers(energy);
    let mut events = Vec::new();

    for entry in timemap.iter() {
        let (ts_beats, ts_beat_type) = entry.time_sig;
        let beats = felt_beats(ts_beats, ts_beat_type).max(1);
        let beat_dur_ms = entry.duration_ms / beats as f64;

        for b in 0..beats {
            let beat_time = entry.timestamp_ms + b as f64 * beat_dur_ms;
            let on_tick = ms_to_ticks(beat_time, timemap);
            let dur_ticks = (TICKS_PER_QUARTER as f64 * 0.25) as u32;
            let off_tick = on_tick + dur_ticks;

            // Kick on beat 1 and 3 (or beat 1 only if < 4 beats)
            if b == 0 || (beats >= 4 && b == 2) {
                let vel = velocity(82.0, em.drums);
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
                let vel = velocity(72.0, em.drums);
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
            let hh_vel = velocity(55.0, em.drums);
            events.push(MidiEvent {
                tick: on_tick,
                bytes: vec![0x99, HIHAT_CLOSED, hh_vel],
            });
            events.push(MidiEvent {
                tick: off_tick,
                bytes: vec![0x89, HIHAT_CLOSED, 0],
            });

            // Hi-hat eighth notes between beats (only if the beat is long enough
            // to hear the subdivision, i.e. > 300ms per beat)
            if beat_dur_ms > 300.0 {
                let eighth_time = beat_time + beat_dur_ms * 0.5;
                let eighth_tick = ms_to_ticks(eighth_time, timemap);
                let eighth_vel = velocity(40.0, em.drums);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn velocity_clamps_to_valid_range() {
        // Normal case
        assert_eq!(velocity(100.0, 0.7), 70);
        // High value should clamp to 127
        assert_eq!(velocity(200.0, 1.0), 127);
        // Very low value should clamp to 1 (not 0)
        assert_eq!(velocity(0.5, 0.1), 1);
        // Zero base should still clamp to 1
        assert_eq!(velocity(0.0, 1.0), 1);
    }

    #[test]
    fn energy_multipliers_ordering() {
        let soft = energy_multipliers(Energy::Soft);
        let med = energy_multipliers(Energy::Medium);
        let strong = energy_multipliers(Energy::Strong);

        // Strong > Medium > Soft for all instruments
        assert!(strong.piano > med.piano);
        assert!(med.piano > soft.piano);
        assert!(strong.bass > med.bass);
        assert!(med.bass > soft.bass);
        assert!(strong.strings > med.strings);
        assert!(med.strings > soft.strings);
        assert!(strong.drums > med.drums);
        assert!(med.drums > soft.drums);
    }

    #[test]
    fn energy_multipliers_in_valid_range() {
        for energy in [Energy::Soft, Energy::Medium, Energy::Strong] {
            let em = energy_multipliers(energy);
            assert!(em.piano > 0.0 && em.piano <= 1.0, "piano={}", em.piano);
            assert!(em.bass > 0.0 && em.bass <= 1.0, "bass={}", em.bass);
            assert!(em.strings > 0.0 && em.strings <= 1.0, "strings={}", em.strings);
            assert!(em.drums > 0.0 && em.drums <= 1.0, "drums={}", em.drums);
        }
    }

    #[test]
    fn chord_voicing_intervals() {
        // Major: root, major third (+4), fifth (+7)
        let major = get_chord_voicing(0, ChordKind::Major); // C = 0
        assert_eq!(major, vec![48, 52, 55]); // C3, E3, G3

        // Minor: root, minor third (+3), fifth (+7)
        let minor = get_chord_voicing(0, ChordKind::Minor);
        assert_eq!(minor, vec![48, 51, 55]); // C3, Eb3, G3

        // Dominant 7: root, major third, fifth, flat seventh
        let dom7 = get_chord_voicing(0, ChordKind::Dominant7);
        assert_eq!(dom7, vec![48, 52, 55, 58]);

        // Diminished: root, minor third, tritone
        let dim = get_chord_voicing(0, ChordKind::Diminished);
        assert_eq!(dim, vec![48, 51, 54]);

        // Augmented: root, major third, augmented fifth
        let aug = get_chord_voicing(0, ChordKind::Augmented);
        assert_eq!(aug, vec![48, 52, 56]);
    }

    #[test]
    fn chord_voicing_transposes_with_root() {
        // G major (root = 7)
        let g_major = get_chord_voicing(7, ChordKind::Major);
        assert_eq!(g_major, vec![55, 59, 62]); // G3, B3, D4

        // D minor (root = 2)
        let d_minor = get_chord_voicing(2, ChordKind::Minor);
        assert_eq!(d_minor, vec![50, 53, 57]); // D3, F3, A3
    }

    #[test]
    fn parse_chord_kind_standard_strings() {
        assert_eq!(parse_chord_kind("major"), ChordKind::Major);
        assert_eq!(parse_chord_kind("minor"), ChordKind::Minor);
        assert_eq!(parse_chord_kind("dominant"), ChordKind::Dominant7);
        assert_eq!(parse_chord_kind("dominant-seventh"), ChordKind::Dominant7);
        assert_eq!(parse_chord_kind("major-seventh"), ChordKind::MajorSeventh);
        assert_eq!(parse_chord_kind("minor-seventh"), ChordKind::MinorSeventh);
        assert_eq!(parse_chord_kind("diminished"), ChordKind::Diminished);
        assert_eq!(parse_chord_kind("half-diminished"), ChordKind::HalfDiminished);
        assert_eq!(parse_chord_kind("augmented"), ChordKind::Augmented);
        // Unknown defaults to Major
        assert_eq!(parse_chord_kind("unknown-quality"), ChordKind::Major);
    }

    #[test]
    fn infer_chord_kind_from_pitches() {
        // C major triad: C(0), E(4), G(7)
        assert_eq!(infer_chord_kind(&[0, 4, 7], 0), ChordKind::Major);
        // C minor triad: C(0), Eb(3), G(7)
        assert_eq!(infer_chord_kind(&[0, 3, 7], 0), ChordKind::Minor);
        // C dominant 7: C(0), E(4), G(7), Bb(10)
        assert_eq!(infer_chord_kind(&[0, 4, 7, 10], 0), ChordKind::Dominant7);
        // C diminished: C(0), Eb(3), Gb(6)
        assert_eq!(infer_chord_kind(&[0, 3, 6], 0), ChordKind::Diminished);
        // C augmented: C(0), E(4), G#(8)
        assert_eq!(infer_chord_kind(&[0, 4, 8], 0), ChordKind::Augmented);
    }

    #[test]
    fn analyze_chords_asa_branca_produces_chords() {
        let score = crate::parse_file("../../sheetmusic/asa-branca.musicxml").unwrap();
        let unrolled = crate::unroller::unroll(&score, 0);
        let timemap = crate::timemap::generate_timemap(&score, 0, &unrolled);

        let chords = analyze_chords(&score.parts[0], &unrolled, &timemap);

        assert!(!chords.is_empty(), "asa-branca should produce chords");

        // All chords should have valid root (0-11) and positive duration
        for c in &chords {
            assert!(c.root < 12, "Root {} should be < 12", c.root);
            assert!(c.duration_ms > 0.0, "Duration should be positive");
            assert!(c.time_ms >= 0.0, "Time should be non-negative");
        }

        // Chords should be sorted by time
        for i in 1..chords.len() {
            assert!(
                chords[i].time_ms >= chords[i - 1].time_ms,
                "Chords should be sorted by time"
            );
        }

        // asa-branca has explicit harmony <C major> in measure 1
        let first_chord = &chords[0];
        assert_eq!(first_chord.root, 0, "First chord should be C (root=0)");
        assert_eq!(first_chord.kind, ChordKind::Major, "First chord should be Major");

        println!("✓ asa-branca chords: {} total, first = C major", chords.len());
    }

    #[test]
    fn analyze_chords_chopin_infers_from_melody() {
        let score = crate::parse_file("../../sheetmusic/chopin-trois-valses.mxl").unwrap();
        let unrolled = crate::unroller::unroll(&score, 0);
        let timemap = crate::timemap::generate_timemap(&score, 0, &unrolled);

        // Chopin has no explicit harmonies
        let total_harmonies: usize = score.parts[0].measures.iter()
            .map(|m| m.harmonies.len()).sum();
        assert_eq!(total_harmonies, 0, "Chopin should have no explicit harmonies");

        let chords = analyze_chords(&score.parts[0], &unrolled, &timemap);

        // Should still infer chords from melody
        assert!(!chords.is_empty(), "Should infer chords from melody");

        for c in &chords {
            assert!(c.root < 12);
            assert!(c.duration_ms > 0.0);
        }

        println!("✓ chopin inferred chords: {}", chords.len());
    }
}
