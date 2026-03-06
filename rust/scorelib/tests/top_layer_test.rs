//! Test top-layer note extraction on a simple two-measure MusicXML file.
//!
//! Fixture: `tests/fixtures/test.musicxml` (Rust convention: test data under `tests/`).
//! Content:
//! - Measure 1: Staff 1 voice 1 (Bb5, Ab5, Gb5, F5), voice 2 (Gb4 quarters); Staff 2 voice 5 (A2, then A3+C4 chord, then A3+C4 chord).
//! - Measure 2: Staff 1 voice 1 (F5, Eb5, Eb5, D5, Eb5), voice 2 (Gb4); Staff 2 voice 5 (Eb3, then A3+C4 chord, then A3+C4 chord).
//!
//! For note selection we use **first staff only** (staff_filter = Some(1)) so staff 2 never overrides.
//! Top layer = that staff's top voice + at each beat the highest note (top of chord).
//! Expected with staff_filter Some(1): M1: Bb5, Ab5, Gb5, F5; M2: F5, Eb5, Eb5, D5, Eb5 (no staff 2 notes).

use scorelib::{parse_file, top_layer};
use std::path::PathBuf;

fn test_musicxml_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/test.musicxml")
}

#[test]
fn test_top_layer_parses_test_musicxml() {
    let path = test_musicxml_path();
    let score = parse_file(&path).unwrap();
    assert_eq!(score.parts.len(), 1);
    assert_eq!(score.parts[0].measures.len(), 2, "test.musicxml has exactly 2 measures");
}

#[test]
fn test_top_layer_detects_multi_voice_and_chord() {
    let score = parse_file(test_musicxml_path()).unwrap();
    let part = &score.parts[0];
    let results = top_layer::extract_top_layer_from_part(part, None);

    assert_eq!(results.len(), 2);

    // Measure 0: has voice 1 and 2 on staff 1, voice 5 on staff 2; staff 2 has chords
    assert!(
        results[0].has_multiple_voices,
        "Measure 1 should have multiple voices (staff1: 1,2; staff2: 5)"
    );
    assert!(
        results[0].has_chord,
        "Measure 1 should have chords (staff 2 A3+C4)"
    );

    // Measure 1: same
    assert!(results[1].has_multiple_voices, "Measure 2 should have multiple voices");
    assert!(results[1].has_chord, "Measure 2 should have chords");
}

#[test]
fn test_top_layer_first_staff_only() {
    let score = parse_file(test_musicxml_path()).unwrap();
    let part = &score.parts[0];
    let results = top_layer::extract_top_layer_from_part(part, Some(1));

    assert_eq!(results.len(), 2);
    // Only staff 1 notes; staff 2 must not appear
    let m0: Vec<String> = results[0].top_notes.iter().map(|n| n.pitch_name.clone()).collect();
    let m1: Vec<String> = results[1].top_notes.iter().map(|n| n.pitch_name.clone()).collect();
    assert!(
        results[0].top_notes.iter().all(|n| n.staff == 1),
        "With staff_filter Some(1), measure 1 must contain only staff 1 notes"
    );
    assert!(
        results[1].top_notes.iter().all(|n| n.staff == 1),
        "With staff_filter Some(1), measure 2 must contain only staff 1 notes"
    );
    assert_eq!(m0, ["Bb5", "Ab5", "Gb5", "F5"], "Measure 1 first-staff top layer");
    assert_eq!(m1, ["F5", "Eb5", "Eb5", "D5", "Eb5"], "Measure 2 first-staff top layer");
}

#[test]
fn test_top_layer_all_staves_includes_staff2() {
    let score = parse_file(test_musicxml_path()).unwrap();
    let part = &score.parts[0];
    let results = top_layer::extract_top_layer_from_part(part, None);

    let staff2_m0: Vec<String> = results[0]
        .top_notes
        .iter()
        .filter(|n| n.staff == 2)
        .map(|n| n.pitch_name.clone())
        .collect();
    let staff2_m1: Vec<String> = results[1]
        .top_notes
        .iter()
        .filter(|n| n.staff == 2)
        .map(|n| n.pitch_name.clone())
        .collect();
    assert_eq!(staff2_m0, ["Ab2", "C4", "C4"], "Without filter, staff 2 top layer M1");
    assert_eq!(staff2_m1, ["Eb3", "C4", "C4"], "Without filter, staff 2 top layer M2");
}

#[test]
fn test_top_layer_print_summary() {
    let score = parse_file(test_musicxml_path()).unwrap();
    let part = &score.parts[0];
    let results = top_layer::extract_top_layer_from_part(part, Some(1));

    eprintln!("=== Top-layer extraction test.musicxml (first staff only) ===\n");
    for m in &results {
        eprintln!(
            "Measure {}: multi_voice={} has_chord={} divisions={}",
            m.measure_idx + 1,
            m.has_multiple_voices,
            m.has_chord,
            m.divisions
        );
        for n in &m.top_notes {
            eprintln!("  staff {} beat {:.2} -> {} (midi {})", n.staff, n.beat_time, n.pitch_name, n.midi);
        }
        eprintln!();
    }
    eprintln!("Total top-layer notes: {}", results.iter().map(|m| m.top_notes.len()).sum::<usize>());
}
