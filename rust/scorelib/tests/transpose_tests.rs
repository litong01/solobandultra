//! Tests for score transposition — pitches, key signatures, and harmony roots.

use scorelib::{parse_file, transpose_score, render_score_to_svg, generate_midi_from_score, MidiOptions};
use std::path::PathBuf;

fn sheetmusic_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../sheetmusic")
}

// ═══════════════════════════════════════════════════════════════════════
// Pitch transposition
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn transpose_zero_is_identity() {
    let original = parse_file(sheetmusic_dir().join("asa-branca.musicxml")).unwrap();
    let mut transposed = original.clone();
    transpose_score(&mut transposed, 0);

    // All pitches should be identical
    let part_orig = &original.parts[0];
    let part_trans = &transposed.parts[0];
    for (mi, (mo, mt)) in part_orig.measures.iter().zip(part_trans.measures.iter()).enumerate() {
        for (ni, (no, nt)) in mo.notes.iter().zip(mt.notes.iter()).enumerate() {
            if let (Some(po), Some(pt)) = (&no.pitch, &nt.pitch) {
                assert_eq!(
                    po.to_midi(), pt.to_midi(),
                    "Transpose(0) should be identity: measure {} note {} differ ({} vs {})",
                    mi, ni, po.to_midi(), pt.to_midi()
                );
            }
        }
    }
    println!("✓ transpose(0) is identity");
}

#[test]
fn transpose_up_2_semitones_shifts_pitches() {
    let mut score = parse_file(sheetmusic_dir().join("asa-branca.musicxml")).unwrap();
    let original = score.clone();

    transpose_score(&mut score, 2);

    // Every pitch should be exactly 2 semitones higher
    let part_orig = &original.parts[0];
    let part_trans = &score.parts[0];
    let mut checked = 0;
    for (mo, mt) in part_orig.measures.iter().zip(part_trans.measures.iter()) {
        for (no, nt) in mo.notes.iter().zip(mt.notes.iter()) {
            if let (Some(po), Some(pt)) = (&no.pitch, &nt.pitch) {
                assert_eq!(
                    pt.to_midi(), po.to_midi() + 2,
                    "Expected MIDI {} + 2 = {}, got {}",
                    po.to_midi(), po.to_midi() + 2, pt.to_midi()
                );
                checked += 1;
            }
        }
    }
    assert!(checked > 0, "Should have checked at least one pitch");
    println!("✓ transpose(+2): verified {} pitches shifted correctly", checked);
}

#[test]
fn transpose_down_3_semitones_shifts_pitches() {
    let mut score = parse_file(sheetmusic_dir().join("asa-branca.musicxml")).unwrap();
    let original = score.clone();

    transpose_score(&mut score, -3);

    let part_orig = &original.parts[0];
    let part_trans = &score.parts[0];
    let mut checked = 0;
    for (mo, mt) in part_orig.measures.iter().zip(part_trans.measures.iter()) {
        for (no, nt) in mo.notes.iter().zip(mt.notes.iter()) {
            if let (Some(po), Some(pt)) = (&no.pitch, &nt.pitch) {
                assert_eq!(
                    pt.to_midi(), po.to_midi() - 3,
                    "Expected MIDI {} - 3 = {}, got {}",
                    po.to_midi(), po.to_midi() - 3, pt.to_midi()
                );
                checked += 1;
            }
        }
    }
    assert!(checked > 0);
    println!("✓ transpose(-3): verified {} pitches shifted correctly", checked);
}

#[test]
fn transpose_roundtrip_is_identity() {
    let original = parse_file(sheetmusic_dir().join("asa-branca.musicxml")).unwrap();
    let mut score = original.clone();

    // Transpose up 5, then down 5 — should return to original
    transpose_score(&mut score, 5);
    transpose_score(&mut score, -5);

    let part_orig = &original.parts[0];
    let part_trans = &score.parts[0];
    let mut checked = 0;
    for (mo, mt) in part_orig.measures.iter().zip(part_trans.measures.iter()) {
        for (no, nt) in mo.notes.iter().zip(mt.notes.iter()) {
            if let (Some(po), Some(pt)) = (&no.pitch, &nt.pitch) {
                assert_eq!(
                    po.to_midi(), pt.to_midi(),
                    "Roundtrip should be identity: got {} vs {}",
                    po.to_midi(), pt.to_midi()
                );
                checked += 1;
            }
        }
    }
    assert!(checked > 0);
    println!("✓ transpose(+5, -5) roundtrip: {} pitches verified", checked);
}

#[test]
fn transpose_octave_preserves_pitch_class() {
    let mut score = parse_file(sheetmusic_dir().join("asa-branca.musicxml")).unwrap();
    let original = score.clone();

    // Transpose up 12 (one octave) — pitch classes should be the same
    transpose_score(&mut score, 12);

    let part_orig = &original.parts[0];
    let part_trans = &score.parts[0];
    let mut checked = 0;
    for (mo, mt) in part_orig.measures.iter().zip(part_trans.measures.iter()) {
        for (no, nt) in mo.notes.iter().zip(mt.notes.iter()) {
            if let (Some(po), Some(pt)) = (&no.pitch, &nt.pitch) {
                assert_eq!(
                    po.to_midi() % 12, pt.to_midi() % 12,
                    "Octave transposition should preserve pitch class"
                );
                assert_eq!(
                    pt.to_midi(), po.to_midi() + 12,
                    "Should be exactly one octave higher"
                );
                checked += 1;
            }
        }
    }
    assert!(checked > 0);
    println!("✓ transpose(+12): {} pitches verified, pitch classes preserved", checked);
}

// ═══════════════════════════════════════════════════════════════════════
// Key signature transposition
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn transpose_changes_key_signature() {
    let mut score = parse_file(sheetmusic_dir().join("asa-branca.musicxml")).unwrap();

    // asa-branca starts in C major (0 fifths)
    let original_fifths = score.parts[0].measures[0]
        .attributes.as_ref().unwrap()
        .key.as_ref().unwrap().fifths;
    assert_eq!(original_fifths, 0, "Should start in C major");

    // Transpose up 2 semitones (C → D major = 2 sharps)
    transpose_score(&mut score, 2);

    let new_fifths = score.parts[0].measures[0]
        .attributes.as_ref().unwrap()
        .key.as_ref().unwrap().fifths;
    assert_eq!(new_fifths, 2, "D major should have 2 sharps (fifths=2)");

    println!("✓ transpose(+2): key C major (0) → D major ({})", new_fifths);
}

#[test]
fn transpose_key_signature_wraps_correctly() {
    let mut score = parse_file(sheetmusic_dir().join("asa-branca.musicxml")).unwrap();

    // Transpose up 7 semitones (C → G major = 1 sharp)
    transpose_score(&mut score, 7);

    let new_fifths = score.parts[0].measures[0]
        .attributes.as_ref().unwrap()
        .key.as_ref().unwrap().fifths;
    assert_eq!(new_fifths, 1, "G major should have 1 sharp (fifths=1)");

    println!("✓ transpose(+7): C major → G major (fifths={})", new_fifths);
}

// ═══════════════════════════════════════════════════════════════════════
// Harmony root transposition
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn transpose_shifts_harmony_roots() {
    let mut score = parse_file(sheetmusic_dir().join("asa-branca.musicxml")).unwrap();
    let original = score.clone();

    // asa-branca measure 1 has C major harmony
    let orig_root = &original.parts[0].measures[1].harmonies[0].root.step;
    assert_eq!(orig_root, "C", "First harmony should be C");

    // Transpose up 2 semitones → C should become D
    transpose_score(&mut score, 2);

    let new_root = &score.parts[0].measures[1].harmonies[0].root.step;
    assert_eq!(new_root, "D", "After +2, C harmony should become D");

    println!("✓ transpose(+2): harmony root C → {}", new_root);
}

#[test]
fn transpose_harmony_with_accidentals() {
    let mut score = parse_file(sheetmusic_dir().join("asa-branca.musicxml")).unwrap();

    // Transpose up 1 semitone — C should become C# or Db depending on key context
    transpose_score(&mut score, 1);

    let root = &score.parts[0].measures[1].harmonies[0].root;
    // After transposing C major up 1 semitone, the root could be C# or Db
    let midi_pc = match root.step.as_str() {
        "C" => 0, "D" => 2, "E" => 4, "F" => 5,
        "G" => 7, "A" => 9, "B" => 11, _ => 0,
    } + root.alter.unwrap_or(0.0) as i32;
    assert_eq!(midi_pc.rem_euclid(12), 1, "Transposed root should be pitch class 1 (C#/Db)");

    println!("✓ transpose(+1): harmony root C → {}{}", root.step,
        match root.alter {
            Some(a) if a > 0.0 => "#",
            Some(a) if a < 0.0 => "b",
            _ => "",
        });
}

// ═══════════════════════════════════════════════════════════════════════
// Transposition with rendering and MIDI
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn transposed_svg_renders_successfully() {
    let mut score = parse_file(sheetmusic_dir().join("asa-branca.musicxml")).unwrap();
    transpose_score(&mut score, 5);

    let svg = render_score_to_svg(&score, None, None);
    assert!(svg.starts_with("<svg"), "Transposed score should produce valid SVG");
    assert!(svg.contains("<ellipse"), "Transposed SVG should contain noteheads");

    println!("✓ Transposed score renders to valid SVG ({} bytes)", svg.len());
}

#[test]
fn transposed_midi_has_shifted_notes() {
    let original = parse_file(sheetmusic_dir().join("asa-branca.musicxml")).unwrap();
    let mut transposed = original.clone();
    transpose_score(&mut transposed, 3);

    let opts = MidiOptions { include_metronome: false, ..MidiOptions::default() };
    let midi_orig = generate_midi_from_score(&original, &opts);
    let midi_trans = generate_midi_from_score(&transposed, &opts);

    // Both should be valid SMFs
    assert_eq!(&midi_orig[0..4], b"MThd");
    assert_eq!(&midi_trans[0..4], b"MThd");

    // The transposed MIDI should be different (different note values)
    assert_ne!(midi_orig, midi_trans, "Transposed MIDI should differ from original");

    println!("✓ Transposed MIDI differs from original (orig={} bytes, trans={} bytes)",
        midi_orig.len(), midi_trans.len());
}

#[test]
fn transpose_multi_staff_chopin() {
    // Verify transposition works on a multi-staff piece
    let mut score = parse_file(sheetmusic_dir().join("chopin-trois-valses.mxl")).unwrap();
    let original = score.clone();

    transpose_score(&mut score, 4);

    // Check both staves have shifted pitches
    let mut staff1_checked = 0;
    let mut staff2_checked = 0;
    for (mo, mt) in original.parts[0].measures.iter().zip(score.parts[0].measures.iter()) {
        for (no, nt) in mo.notes.iter().zip(mt.notes.iter()) {
            if let (Some(po), Some(pt)) = (&no.pitch, &nt.pitch) {
                assert_eq!(pt.to_midi(), po.to_midi() + 4);
                match no.staff {
                    Some(1) => staff1_checked += 1,
                    Some(2) => staff2_checked += 1,
                    _ => {}
                }
            }
        }
    }
    assert!(staff1_checked > 0, "Should verify treble staff notes");
    assert!(staff2_checked > 0, "Should verify bass staff notes");

    println!("✓ Chopin transpose(+4): staff1={} notes, staff2={} notes verified",
        staff1_checked, staff2_checked);
}
