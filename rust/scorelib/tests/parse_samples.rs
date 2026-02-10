//! Integration tests — parse the actual sample files in the sheetmusic/ directory.

use scorelib::{parse_file, Score};
use std::path::PathBuf;

/// Get the path to the sheetmusic directory (relative to workspace root).
fn sheetmusic_dir() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // rust/scorelib -> ../../sheetmusic
    manifest_dir.join("../../sheetmusic")
}

// ─── Uncompressed MusicXML (.musicxml) ──────────────────────────────

#[test]
fn parse_asa_branca_musicxml() {
    let path = sheetmusic_dir().join("asa-branca.musicxml");
    let score = parse_file(&path).expect("Failed to parse asa-branca.musicxml");

    assert_score_asa_branca(&score);
}

fn assert_score_asa_branca(score: &Score) {
    // Metadata
    assert_eq!(score.title.as_deref(), Some("Asa branca"));
    assert_eq!(score.composer.as_deref(), Some("Luiz Gonzaga"));
    assert_eq!(score.version.as_deref(), Some("3.1"));

    // Parts
    assert_eq!(score.parts.len(), 1);
    let part = &score.parts[0];
    assert_eq!(part.id, "P1");
    assert_eq!(part.name, "Classical Guitar");
    assert_eq!(part.abbreviation.as_deref(), Some("Guit."));
    assert_eq!(part.midi_program, Some(25));
    assert_eq!(part.midi_channel, Some(1));

    // Measures
    assert!(
        part.measures.len() > 30,
        "Expected 30+ measures, got {}",
        part.measures.len()
    );

    // First measure (pickup/anacrusis)
    let m0 = &part.measures[0];
    assert_eq!(m0.number, 0);
    assert!(m0.implicit, "Measure 0 should be implicit (anacrusis)");

    // Check attributes on first measure
    let attrs = m0.attributes.as_ref().expect("First measure should have attributes");
    assert_eq!(attrs.divisions, Some(2));

    let key = attrs.key.as_ref().expect("Should have key signature");
    assert_eq!(key.fifths, 0); // C major

    let time = attrs.time.as_ref().expect("Should have time signature");
    assert_eq!(time.beats, 2);
    assert_eq!(time.beat_type, 4);

    assert!(!attrs.clefs.is_empty(), "Should have at least one clef");
    let clef = &attrs.clefs[0];
    assert_eq!(clef.sign, "G");
    assert_eq!(clef.line, 2);
    assert_eq!(clef.octave_change, Some(-1));

    // First measure notes (two eighth notes: C4, D4)
    assert_eq!(m0.notes.len(), 2);

    let note1 = &m0.notes[0];
    assert!(!note1.rest);
    let pitch1 = note1.pitch.as_ref().expect("Note 1 should have pitch");
    assert_eq!(pitch1.step, "C");
    assert_eq!(pitch1.octave, 4);
    assert_eq!(note1.note_type.as_deref(), Some("eighth"));

    let note2 = &m0.notes[1];
    let pitch2 = note2.pitch.as_ref().expect("Note 2 should have pitch");
    assert_eq!(pitch2.step, "D");
    assert_eq!(pitch2.octave, 4);

    // Check harmonies exist in some measures
    let has_harmonies = part.measures.iter().any(|m| !m.harmonies.is_empty());
    assert!(has_harmonies, "Score should contain chord symbols");

    // Check first harmony (C major in measure 1)
    let m1 = &part.measures[1];
    assert!(!m1.harmonies.is_empty(), "Measure 1 should have harmonies");
    assert_eq!(m1.harmonies[0].root.step, "C");
    assert_eq!(m1.harmonies[0].kind, "major");

    // Check barlines (repeat signs)
    let has_repeats = part
        .measures
        .iter()
        .any(|m| m.barlines.iter().any(|b| b.repeat.is_some()));
    assert!(has_repeats, "Score should contain repeat barlines");

    // Check MIDI conversion
    assert_eq!(pitch1.to_midi(), 60); // C4 = 60
    assert_eq!(pitch2.to_midi(), 62); // D4 = 62

    println!("✓ asa-branca.musicxml parsed successfully");
    println!("  Title: {:?}", score.title);
    println!("  Composer: {:?}", score.composer);
    println!("  Parts: {}", score.parts.len());
    println!("  Measures: {}", part.measures.len());
    println!(
        "  Total notes: {}",
        part.measures.iter().map(|m| m.notes.len()).sum::<usize>()
    );
}

// ─── Compressed MXL (.mxl) ──────────────────────────────────────────

#[test]
fn parse_tongnian_mxl() {
    let path = sheetmusic_dir().join("童年.mxl");
    let score = parse_file(&path).expect("Failed to parse 童年.mxl");

    assert_score_tongnian(&score);
}

fn assert_score_tongnian(score: &Score) {
    // Metadata
    assert_eq!(score.title.as_deref(), Some("童年"));
    assert_eq!(score.composer.as_deref(), Some("罗大佑"));
    assert_eq!(score.version.as_deref(), Some("4.0"));

    // Parts
    assert!(
        !score.parts.is_empty(),
        "Score should have at least one part"
    );

    let part = &score.parts[0];

    // Measures
    assert!(
        !part.measures.is_empty(),
        "Part should have at least one measure"
    );

    // First measure with attributes should have time and key signatures
    let first_attrs = part
        .measures
        .iter()
        .find_map(|m| m.attributes.as_ref())
        .expect("Score should have attributes somewhere");

    assert!(first_attrs.divisions.is_some(), "Should have divisions");
    assert!(first_attrs.key.is_some(), "Should have key signature");
    assert!(first_attrs.time.is_some(), "Should have time signature");

    // Should have notes
    let total_notes: usize = part.measures.iter().map(|m| m.notes.len()).sum();
    assert!(
        total_notes > 0,
        "Score should have notes, got {}",
        total_notes
    );

    println!("✓ 童年.mxl parsed successfully");
    println!("  Title: {:?}", score.title);
    println!("  Composer: {:?}", score.composer);
    println!("  Version: {:?}", score.version);
    println!("  Parts: {}", score.parts.len());
    println!("  Part name: {}", part.name);
    println!("  Measures: {}", part.measures.len());
    println!("  Total notes: {}", total_notes);
}

// ─── Compressed MXL (.mxl) — Chopin ─────────────────────────────

#[test]
fn parse_chopin_trois_valses_mxl() {
    let path = sheetmusic_dir().join("chopin-trois-valses.mxl");
    let score = parse_file(&path).expect("Failed to parse chopin-trois-valses.mxl");

    // Parts
    assert!(
        !score.parts.is_empty(),
        "Score should have at least one part"
    );

    let part = &score.parts[0];

    // Measures
    assert!(
        !part.measures.is_empty(),
        "Part should have at least one measure"
    );

    // First measure with attributes should have time and key signatures
    let first_attrs = part
        .measures
        .iter()
        .find_map(|m| m.attributes.as_ref())
        .expect("Score should have attributes somewhere");

    assert!(first_attrs.divisions.is_some(), "Should have divisions");
    assert!(first_attrs.key.is_some(), "Should have key signature");
    assert!(first_attrs.time.is_some(), "Should have time signature");

    // Should have notes
    let total_notes: usize = part.measures.iter().map(|m| m.notes.len()).sum();
    assert!(
        total_notes > 0,
        "Score should have notes, got {}",
        total_notes
    );

    // Verify grand staff structure (2 staves)
    assert_eq!(first_attrs.staves, Some(2), "Piano should have 2 staves");
    assert!(first_attrs.clefs.len() >= 2, "Should have clefs for both staves");

    let treble = first_attrs.clefs.iter().find(|c| c.number == 1).expect("Should have staff 1 clef");
    assert_eq!(treble.sign, "G", "Staff 1 should be treble clef");

    let bass = first_attrs.clefs.iter().find(|c| c.number == 2).expect("Should have staff 2 clef");
    assert_eq!(bass.sign, "F", "Staff 2 should be bass clef");

    // Verify notes have staff assignments
    let staff1_notes = part.measures.iter()
        .flat_map(|m| m.notes.iter())
        .filter(|n| n.staff == Some(1))
        .count();
    let staff2_notes = part.measures.iter()
        .flat_map(|m| m.notes.iter())
        .filter(|n| n.staff == Some(2))
        .count();
    assert!(staff1_notes > 0, "Should have notes on staff 1 (treble)");
    assert!(staff2_notes > 0, "Should have notes on staff 2 (bass)");

    // Verify key signature (Db major = -5 flats)
    let key = first_attrs.key.as_ref().expect("Should have key");
    assert_eq!(key.fifths, -5, "Should be Db major (5 flats)");

    println!("✓ chopin-trois-valses.mxl parsed successfully");
    println!("  Title: {:?}", score.title);
    println!("  Composer: {:?}", score.composer);
    println!("  Version: {:?}", score.version);
    println!("  Parts: {}", score.parts.len());
    println!("  Part name: {}", part.name);
    println!("  Staves: {:?}", first_attrs.staves);
    println!("  Clefs: {} (treble={}, bass={})", first_attrs.clefs.len(), treble.sign, bass.sign);
    println!("  Key: {} fifths", key.fifths);
    println!("  Measures: {}", part.measures.len());
    println!("  Total notes: {} (staff1={}, staff2={})", total_notes, staff1_notes, staff2_notes);
}

// ─── Auto-detection ─────────────────────────────────────────────────

#[test]
fn auto_detect_format() {
    // Test that parse_bytes auto-detects format without extension hint
    let musicxml_path = sheetmusic_dir().join("asa-branca.musicxml");
    let data = std::fs::read(&musicxml_path).unwrap();
    let score = scorelib::parse_bytes(&data, None).expect("Should auto-detect musicxml");
    assert_eq!(score.title.as_deref(), Some("Asa branca"));

    let mxl_path = sheetmusic_dir().join("童年.mxl");
    let data = std::fs::read(&mxl_path).unwrap();
    let score = scorelib::parse_bytes(&data, None).expect("Should auto-detect mxl");
    assert_eq!(score.title.as_deref(), Some("童年"));
}

// ─── Blue Bag Folly (mid-piece changes) ─────────────────────────────

#[test]
fn parse_blue_bag_folly_musicxml() {
    let path = sheetmusic_dir().join("blue-bag-folly.musicxml");
    let score = parse_file(&path).expect("Failed to parse blue-bag-folly.musicxml");

    println!("✓ blue-bag-folly.musicxml parsed successfully");
    println!("  Title: {:?}", score.title);
    println!("  Parts: {}", score.parts.len());

    assert_eq!(score.parts.len(), 1);
    let part = &score.parts[0];
    assert!(!part.measures.is_empty());
    println!("  Measures: {}", part.measures.len());

    // ── Verify key signature changes ──
    // The file has 3 different key signatures: -1, -6, +1 fifths
    let key_values: Vec<i32> = part.measures.iter()
        .filter_map(|m| m.attributes.as_ref().and_then(|a| a.key.as_ref()).map(|k| k.fifths))
        .collect();
    println!("  Key changes: {:?}", key_values);
    assert!(key_values.len() >= 3, "Should have at least 3 key signature declarations");
    assert!(key_values.contains(&-1), "Should have -1 flats");
    assert!(key_values.contains(&-6), "Should have -6 flats");
    assert!(key_values.contains(&1), "Should have +1 sharps");

    // ── Verify time signature changes ──
    // The file has 3 different time signatures: 4/4, 5/4, 3/4
    let time_values: Vec<(i32, i32)> = part.measures.iter()
        .filter_map(|m| m.attributes.as_ref().and_then(|a| a.time.as_ref()).map(|t| (t.beats, t.beat_type)))
        .collect();
    println!("  Time signature changes: {:?}", time_values);
    assert!(time_values.len() >= 3, "Should have at least 3 time signature declarations");
    assert!(time_values.contains(&(4, 4)), "Should have 4/4");
    assert!(time_values.contains(&(5, 4)), "Should have 5/4");
    assert!(time_values.contains(&(3, 4)), "Should have 3/4");

    // ── Verify direction/tempo parsing ──
    // The file has 2 tempo markings: 120 BPM and 90 BPM
    let tempo_directions: Vec<&scorelib::Direction> = part.measures.iter()
        .flat_map(|m| m.directions.iter())
        .filter(|d| d.sound_tempo.is_some() || d.metronome.is_some())
        .collect();
    println!("  Tempo directions: {}", tempo_directions.len());
    assert!(tempo_directions.len() >= 2, "Should have at least 2 tempo markings");

    // Verify specific BPM values
    let bpm_values: Vec<f64> = tempo_directions.iter()
        .filter_map(|d| d.sound_tempo)
        .collect();
    println!("  BPM values: {:?}", bpm_values);
    assert!(bpm_values.iter().any(|&b| (b - 120.0).abs() < 1.0), "Should have tempo 120");
    assert!(bpm_values.iter().any(|&b| (b - 90.0).abs() < 1.0), "Should have tempo 90");

    // Verify metronome marks
    let metronome_count = tempo_directions.iter()
        .filter(|d| d.metronome.is_some())
        .count();
    println!("  Metronome marks: {}", metronome_count);
    assert!(metronome_count >= 2, "Should have at least 2 metronome marks");

    // Verify quarter-note beat unit
    for dir in &tempo_directions {
        if let Some(ref m) = dir.metronome {
            assert_eq!(m.beat_unit, "quarter", "Beat unit should be quarter note");
        }
    }
}

// ─── JSON serialization ─────────────────────────────────────────────

#[test]
fn score_to_json_roundtrip() {
    let path = sheetmusic_dir().join("asa-branca.musicxml");
    let score = parse_file(&path).unwrap();
    let json = scorelib::score_to_json(&score).expect("Should serialize to JSON");

    // Verify it's valid JSON by deserializing
    let deserialized: Score =
        serde_json::from_str(&json).expect("Should deserialize from JSON");
    assert_eq!(deserialized.title, score.title);
    assert_eq!(deserialized.composer, score.composer);
    assert_eq!(deserialized.parts.len(), score.parts.len());
}
