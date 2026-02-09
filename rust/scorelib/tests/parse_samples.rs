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

    let clef = attrs.clef.as_ref().expect("Should have clef");
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
