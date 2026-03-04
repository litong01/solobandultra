//! Comprehensive smoke test — exercises nearly every notation feature in a
//! single MusicXML piece. If this file parses, unrolls, renders, generates
//! MIDI, produces a playback map, and (optionally) renders audio correctly,
//! then the engine handles virtually all real-world scores.

use scorelib::{
    parse_file, unroll, generate_timemap, generate_midi_from_score,
    render_score_to_svg, generate_playback_map, transpose_score,
    MidiOptions, Energy,
};
use scorelib::playback::playback_map_to_json;
use std::path::PathBuf;

fn smoke_test_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../sheetmusic/smoke-test.musicxml")
}

fn output_dir() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test_output");
    std::fs::create_dir_all(&dir).ok();
    dir
}

// ═══════════════════════════════════════════════════════════════════════
// Parsing
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn smoke_parse_metadata() {
    let score = parse_file(smoke_test_path()).unwrap();
    assert_eq!(score.title.as_deref(), Some("Mysoloband Smoke Test"));
    assert_eq!(score.composer.as_deref(), Some("Auto-generated Test Piece"));
    assert_eq!(score.parts.len(), 1);
    assert_eq!(score.parts[0].name, "Piano");
    assert_eq!(score.parts[0].midi_program, Some(1));
    assert_eq!(score.parts[0].midi_channel, Some(1));
    println!("✓ smoke test metadata parsed correctly");
}

#[test]
fn smoke_parse_measures() {
    let score = parse_file(smoke_test_path()).unwrap();
    let part = &score.parts[0];

    // 70 raw measures: m0 (pickup) through m69
    assert_eq!(part.measures.len(), 70, "Should have 70 raw measures");

    // m0 is implicit (pickup)
    assert!(part.measures[0].implicit, "Measure 0 should be implicit (anacrusis)");
    assert_eq!(part.measures[0].number, 0);

    println!("✓ smoke test: 70 measures, m0 is pickup");
}

#[test]
fn smoke_parse_key_signatures() {
    let score = parse_file(smoke_test_path()).unwrap();
    let part = &score.parts[0];

    let keys: Vec<i32> = part.measures.iter()
        .filter_map(|m| m.attributes.as_ref()?.key.as_ref().map(|k| k.fifths))
        .collect();

    // C major (0), G major (1), Eb major (-3), D major (2), Db major (-5), C major (0)
    assert!(keys.contains(&0), "Should have C major (0 fifths)");
    assert!(keys.contains(&1), "Should have G major (1 sharp)");
    assert!(keys.contains(&-3), "Should have Eb major (3 flats)");
    assert!(keys.contains(&2), "Should have D major (2 sharps)");
    assert!(keys.contains(&-5), "Should have Db major (5 flats)");
    assert!(keys.len() >= 5, "Should have at least 5 key signature changes");

    println!("✓ smoke test key signatures: {:?}", keys);
}

#[test]
fn smoke_parse_time_signatures() {
    let score = parse_file(smoke_test_path()).unwrap();
    let part = &score.parts[0];

    let times: Vec<(i32, i32)> = part.measures.iter()
        .filter_map(|m| m.attributes.as_ref()?.time.as_ref().map(|t| (t.beats, t.beat_type)))
        .collect();

    assert!(times.contains(&(4, 4)), "Should have 4/4");
    assert!(times.contains(&(3, 4)), "Should have 3/4");
    assert!(times.contains(&(6, 8)), "Should have 6/8");
    assert!(times.contains(&(5, 4)), "Should have 5/4");

    println!("✓ smoke test time signatures: {:?}", times);
}

#[test]
fn smoke_parse_tempos() {
    let score = parse_file(smoke_test_path()).unwrap();
    let part = &score.parts[0];

    let tempos: Vec<f64> = part.measures.iter()
        .flat_map(|m| m.directions.iter())
        .filter_map(|d| d.sound_tempo)
        .collect();

    assert!(tempos.iter().any(|&t| (t - 120.0).abs() < 1.0), "Should have 120 BPM");
    assert!(tempos.iter().any(|&t| (t - 96.0).abs() < 1.0), "Should have 96 BPM");
    assert!(tempos.iter().any(|&t| (t - 108.0).abs() < 1.0), "Should have 108 BPM");
    assert!(tempos.iter().any(|&t| (t - 132.0).abs() < 1.0), "Should have 132 BPM");
    assert!(tempos.iter().any(|&t| (t - 180.0).abs() < 1.0), "Should have 180 BPM (waltz)");

    println!("✓ smoke test tempos: {:?}", tempos);
}

#[test]
fn smoke_parse_harmonies() {
    let score = parse_file(smoke_test_path()).unwrap();
    let part = &score.parts[0];

    let harmony_kinds: Vec<&str> = part.measures.iter()
        .flat_map(|m| m.harmonies.iter())
        .map(|h| h.kind.as_str())
        .collect();

    assert!(harmony_kinds.contains(&"major"), "Should have major chords");
    assert!(harmony_kinds.contains(&"minor"), "Should have minor chords");
    assert!(harmony_kinds.contains(&"dominant"), "Should have dominant 7th chords");
    assert!(harmony_kinds.contains(&"diminished"), "Should have diminished chords");
    assert!(harmony_kinds.contains(&"augmented"), "Should have augmented chords");

    // Check specific roots
    let roots: Vec<&str> = part.measures.iter()
        .flat_map(|m| m.harmonies.iter())
        .map(|h| h.root.step.as_str())
        .collect();
    assert!(roots.contains(&"C"));
    assert!(roots.contains(&"F"));
    assert!(roots.contains(&"G"));
    assert!(roots.contains(&"D"));
    assert!(roots.contains(&"A"));
    assert!(roots.contains(&"E"));
    assert!(roots.contains(&"B"));

    println!("✓ smoke test harmonies: {} total, kinds={:?}",
        harmony_kinds.len(), harmony_kinds);
}

#[test]
fn smoke_parse_repeats_and_navigation() {
    let score = parse_file(smoke_test_path()).unwrap();
    let part = &score.parts[0];

    // Check repeat barlines
    let has_forward_repeat = part.measures.iter().any(|m|
        m.barlines.iter().any(|b| b.repeat.as_ref().map_or(false, |r| r.direction == "forward"))
    );
    let has_backward_repeat = part.measures.iter().any(|m|
        m.barlines.iter().any(|b| b.repeat.as_ref().map_or(false, |r| r.direction == "backward"))
    );
    assert!(has_forward_repeat, "Should have forward repeat");
    assert!(has_backward_repeat, "Should have backward repeat");

    // Check 1st/2nd endings
    let ending_numbers: Vec<&str> = part.measures.iter()
        .flat_map(|m| m.barlines.iter())
        .filter_map(|b| b.ending.as_ref())
        .map(|e| e.number.as_str())
        .collect();
    assert!(ending_numbers.contains(&"1"), "Should have 1st ending");
    assert!(ending_numbers.contains(&"2"), "Should have 2nd ending");

    // Check segno
    let has_segno = part.measures.iter().any(|m|
        m.directions.iter().any(|d| d.segno)
    );
    assert!(has_segno, "Should have segno");

    // Check fine
    let has_fine = part.measures.iter().any(|m|
        m.directions.iter().any(|d| d.sound_fine)
    );
    assert!(has_fine, "Should have fine");

    // Check D.S.
    let has_ds = part.measures.iter().any(|m|
        m.directions.iter().any(|d| d.sound_dalsegno)
    );
    assert!(has_ds, "Should have D.S. al Fine");

    println!("✓ smoke test navigation: repeats, 1st/2nd endings, segno, fine, D.S.");
}

#[test]
fn smoke_parse_note_variety() {
    let score = parse_file(smoke_test_path()).unwrap();
    let part = &score.parts[0];

    let all_notes: Vec<&scorelib::Note> = part.measures.iter()
        .flat_map(|m| m.notes.iter())
        .collect();

    // Note types
    let types: std::collections::HashSet<&str> = all_notes.iter()
        .filter_map(|n| n.note_type.as_deref())
        .collect();
    assert!(types.contains("whole"), "Should have whole notes");
    assert!(types.contains("half"), "Should have half notes");
    assert!(types.contains("quarter"), "Should have quarter notes");
    assert!(types.contains("eighth"), "Should have eighth notes");
    assert!(types.contains("16th"), "Should have 16th notes");

    // Dotted notes
    let has_dots = all_notes.iter().any(|n| n.dot);
    assert!(has_dots, "Should have dotted notes");

    // Rests
    let has_rests = all_notes.iter().any(|n| n.rest);
    assert!(has_rests, "Should have rests");

    // Ties
    let has_ties = all_notes.iter().any(|n| n.tie_start || n.tie_stop);
    assert!(has_ties, "Should have ties");

    // Grace notes
    let has_grace = all_notes.iter().any(|n| n.grace);
    assert!(has_grace, "Should have grace notes");

    // Slurs
    let has_slurs = all_notes.iter().any(|n| !n.slurs.is_empty());
    assert!(has_slurs, "Should have slurs");

    // Lyrics
    let has_lyrics = all_notes.iter().any(|n| !n.lyrics.is_empty());
    assert!(has_lyrics, "Should have lyrics");

    // Triplets — duration=8 at divisions=12 means a quarter-note triplet
    // (normal quarter = 12, triplet quarter = 8 = 12 * 2/3)
    let has_triplet_durations = all_notes.iter().any(|n| n.duration == 8);
    assert!(has_triplet_durations, "Should have triplet note durations (duration=8)");

    // Octave range (Db4 to Db6 with waltz section)
    let midi_values: Vec<i32> = all_notes.iter()
        .filter_map(|n| n.pitch.as_ref())
        .map(|p| p.to_midi())
        .collect();
    let min_midi = *midi_values.iter().min().unwrap();
    let max_midi = *midi_values.iter().max().unwrap();
    assert!(min_midi <= 52, "Should have notes as low as E3 (52), got {}", min_midi);
    assert!(max_midi >= 81, "Should have notes as high as Db6 (85), got {}", max_midi);

    println!("✓ smoke test notes: {} total, types={:?}, range=MIDI {}-{}",
        all_notes.len(), types, min_midi, max_midi);
}

// ═══════════════════════════════════════════════════════════════════════
// Unrolling
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn smoke_unroll_correct_count() {
    let score = parse_file(smoke_test_path()).unwrap();
    let unrolled = unroll(&score, 0);

    // Expected: 10 (A) + 3 (B 1st) + 3 (B 2nd) + 4 (C) + 3 (D) + 48 (F waltz) + 1 (E) + 4 (D.S.) = 76
    assert_eq!(unrolled.len(), 76,
        "Should have 76 unrolled measures (70 raw with repeat + D.S.), got {}",
        unrolled.len());

    println!("✓ smoke test unroll: 70 raw → {} unrolled measures", unrolled.len());
}

#[test]
fn smoke_unroll_correct_sequence() {
    let score = parse_file(smoke_test_path()).unwrap();
    let unrolled = unroll(&score, 0);

    // Verify the playback order by original_index
    let indices: Vec<usize> = unrolled.iter().map(|u| u.original_index).collect();

    // Section A: m0-m9 (original + quarter-note pairs + beamed eighth pairs)
    assert_eq!(&indices[0..10], &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9], "Section A");
    // Section B 1st pass: m10, m11, m12
    assert_eq!(&indices[10..13], &[10, 11, 12], "Section B 1st ending");
    // Section B 2nd pass: m10, m11, m13
    assert_eq!(&indices[13..16], &[10, 11, 13], "Section B 2nd ending");
    // Section C: m14-m17
    assert_eq!(&indices[16..20], &[14, 15, 16, 17], "Section C");
    // Section D: m18-m20
    assert_eq!(&indices[20..23], &[18, 19, 20], "Section D");
    // Section F (waltz): m21-m68 (48 measures)
    let waltz_indices: Vec<usize> = (21..=68).collect();
    assert_eq!(&indices[23..71], &waltz_indices[..], "Section F (waltz)");
    // Section E: m69
    assert_eq!(indices[71], 69, "Section E (D.S.)");
    // D.S. replay: m14-m17
    assert_eq!(&indices[72..76], &[14, 15, 16, 17], "D.S. replay to Fine");

    println!("✓ smoke test unroll sequence verified ({} measures)", indices.len());
}

// ═══════════════════════════════════════════════════════════════════════
// Timemap
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn smoke_timemap_properties() {
    let score = parse_file(smoke_test_path()).unwrap();
    let unrolled = unroll(&score, 0);
    let timemap = generate_timemap(&score, 0, &unrolled);

    assert_eq!(timemap.len(), 76, "Timemap should have 76 entries");

    // Starts at 0
    assert!(timemap[0].timestamp_ms.abs() < 0.01, "Should start at 0ms");

    // Monotonically increasing
    for i in 1..timemap.len() {
        assert!(timemap[i].timestamp_ms > timemap[i - 1].timestamp_ms,
            "Timemap should be monotonically increasing at index {}", i);
    }

    // Check that all 4 tempos appear
    let tempos: std::collections::HashSet<i32> = timemap.iter()
        .map(|e| e.tempo_bpm as i32)
        .collect();
    assert!(tempos.contains(&120), "Should have 120 BPM");
    assert!(tempos.contains(&96), "Should have 96 BPM");
    assert!(tempos.contains(&108), "Should have 108 BPM");
    assert!(tempos.contains(&132), "Should have 132 BPM");
    assert!(tempos.contains(&180), "Should have 180 BPM (waltz)");

    // Pickup measure should have shorter duration than next measure
    assert!(timemap[0].duration_ms < timemap[1].duration_ms,
        "Pickup measure should be shorter than full measure");

    // Total duration should be reasonable (15-60 seconds for this short piece)
    let total_ms = scorelib::timemap::total_duration_ms(&timemap);
    assert!(total_ms > 10_000.0 && total_ms < 120_000.0,
        "Total duration {:.1}s should be reasonable", total_ms / 1000.0);

    println!("✓ smoke test timemap: {} entries, tempos={:?}, total={:.1}s",
        timemap.len(), tempos, total_ms / 1000.0);
}

#[test]
fn smoke_timemap_tempo_reverts_after_ds() {
    let score = parse_file(smoke_test_path()).unwrap();
    let unrolled = unroll(&score, 0);
    let timemap = generate_timemap(&score, 0, &unrolled);

    // m69 is at 120 BPM, D.S. jumps to m14 which should be 108 BPM
    let m69_entry = &timemap[71]; // index 71 = m69 (D.S. al Fine)
    let ds_target = &timemap[72]; // index 72 = m14 (after D.S.)

    assert_eq!(m69_entry.original_index, 69);
    assert_eq!(ds_target.original_index, 14);

    assert!((m69_entry.tempo_bpm - 120.0).abs() < 1.0,
        "m69 should be 120 BPM, got {}", m69_entry.tempo_bpm);
    assert!((ds_target.tempo_bpm - 108.0).abs() < 1.0,
        "After D.S. to segno, should revert to 108 BPM, got {}", ds_target.tempo_bpm);

    println!("✓ smoke test D.S. tempo revert: {} BPM → {} BPM",
        m69_entry.tempo_bpm, ds_target.tempo_bpm);
}

// ═══════════════════════════════════════════════════════════════════════
// SVG Rendering
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn smoke_render_svg() {
    let score = parse_file(smoke_test_path()).unwrap();
    let svg = render_score_to_svg(&score, None, None, false, 0);

    assert!(svg.starts_with("<svg"), "Should produce valid SVG");
    assert!(svg.contains("</svg>"), "SVG should be closed");
    assert!(svg.contains("Mysoloband Smoke Test"), "SVG should contain title");
    assert!(svg.contains("<ellipse"), "SVG should contain noteheads");
    assert!(svg.contains("<line"), "SVG should contain staff lines");

    // Should be substantial
    assert!(svg.len() > 10_000, "SVG should be substantial: {} bytes", svg.len());

    let out = output_dir().join("smoke-test.svg");
    std::fs::write(&out, &svg).unwrap();
    println!("✓ smoke test SVG: {} bytes → {}", svg.len(), out.display());
}

#[test]
fn smoke_render_svg_phone_width() {
    let score = parse_file(smoke_test_path()).unwrap();
    let svg = render_score_to_svg(&score, Some(390.0), None, false, 0);

    assert!(svg.starts_with("<svg"));
    assert!(svg.contains("viewBox=\"0 0 390"));

    let out = output_dir().join("smoke-test-phone.svg");
    std::fs::write(&out, &svg).unwrap();
    println!("✓ smoke test phone SVG: {} bytes", svg.len());
}

// ═══════════════════════════════════════════════════════════════════════
// MIDI Generation
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn smoke_midi_melody_only() {
    let score = parse_file(smoke_test_path()).unwrap();
    let opts = MidiOptions::default(); // melody + metronome
    let midi = generate_midi_from_score(&score, &opts);

    assert_eq!(&midi[0..4], b"MThd", "Should be valid SMF");
    let track_count = u16::from_be_bytes([midi[10], midi[11]]);
    assert!(track_count >= 2, "Should have at least tempo + melody tracks");
    assert!(midi.len() > 500, "MIDI should be substantial");

    let out = output_dir().join("smoke-test.mid");
    std::fs::write(&out, &midi).unwrap();
    println!("✓ smoke test MIDI (melody): {} bytes, {} tracks", midi.len(), track_count);
}

#[test]
fn smoke_midi_full_accompaniment() {
    let score = parse_file(smoke_test_path()).unwrap();
    let opts = MidiOptions {
        include_melody: true,
        include_piano: true,
        include_bass: true,
        include_strings: true,
        include_drums: true,
        include_metronome: true,
        energy: Energy::Medium,
        ..MidiOptions::default()
    };
    let midi = generate_midi_from_score(&score, &opts);

    assert_eq!(&midi[0..4], b"MThd");
    let track_count = u16::from_be_bytes([midi[10], midi[11]]);
    // tempo + melody + piano + bass + strings + drums + metronome = 7
    assert_eq!(track_count, 7, "Full accompaniment should have 7 tracks, got {}", track_count);

    let out = output_dir().join("smoke-test-full.mid");
    std::fs::write(&out, &midi).unwrap();
    println!("✓ smoke test MIDI (full): {} bytes, {} tracks", midi.len(), track_count);
}

// ═══════════════════════════════════════════════════════════════════════
// Playback Map
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn smoke_playback_map() {
    let score = parse_file(smoke_test_path()).unwrap();
    let pmap = generate_playback_map(&score, None, None, false);

    assert_eq!(pmap.measures.len(), 70, "Should have 70 original measures");
    assert_eq!(pmap.timemap.len(), 76, "Should have 76 unrolled timemap entries");
    assert!(!pmap.systems.is_empty(), "Should have at least one system");

    // Every measure should have note_positions
    for m in &pmap.measures {
        assert!(!m.note_positions.is_empty(),
            "Measure {} should have note_positions", m.measure_idx);
    }

    // Timemap should reference valid measures
    for entry in &pmap.timemap {
        assert!(pmap.measures.iter().any(|m| m.measure_idx == entry.original_index),
            "Timemap entry {} should reference a valid measure", entry.original_index);
    }

    let json = playback_map_to_json(&pmap);
    let out = output_dir().join("smoke-test-playback-map.json");
    std::fs::write(&out, &json).unwrap();
    println!("✓ smoke test playback map: {} measures, {} systems, {} timemap entries",
        pmap.measures.len(), pmap.systems.len(), pmap.timemap.len());
}

// ═══════════════════════════════════════════════════════════════════════
// Transposition
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn smoke_transpose_roundtrip() {
    let original = parse_file(smoke_test_path()).unwrap();
    let mut score = original.clone();

    transpose_score(&mut score, 5);
    transpose_score(&mut score, -5);

    // All pitches should match
    let part_orig = &original.parts[0];
    let part_trans = &score.parts[0];
    let mut checked = 0;
    for (mo, mt) in part_orig.measures.iter().zip(part_trans.measures.iter()) {
        for (no, nt) in mo.notes.iter().zip(mt.notes.iter()) {
            if let (Some(po), Some(pt)) = (&no.pitch, &nt.pitch) {
                assert_eq!(po.to_midi(), pt.to_midi(),
                    "Transpose roundtrip should be identity");
                checked += 1;
            }
        }
    }
    assert!(checked > 0);
    println!("✓ smoke test transpose roundtrip: {} pitches verified", checked);
}

// ═══════════════════════════════════════════════════════════════════════
// End-to-end audio (if SoundFont available)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn smoke_audio_render() {
    let sf_paths = [
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../android/app/src/main/assets/GeneralUser_GS.sf2"),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../ios/SoloBandUltra/SoloBandUltra/GeneralUser_GS.sf2"),
    ];
    let sf_data = sf_paths.iter()
        .find(|p| p.exists())
        .map(|p| std::fs::read(p).unwrap());

    let Some(sf_data) = sf_data else {
        println!("⚠ SoundFont not found — skipping audio test");
        return;
    };

    let score = parse_file(smoke_test_path()).unwrap();
    let opts = MidiOptions {
        include_melody: true,
        include_piano: true,
        include_bass: true,
        include_drums: true,
        include_metronome: true,
        energy: Energy::Medium,
        ..MidiOptions::default()
    };
    let midi = generate_midi_from_score(&score, &opts);
    let wav = scorelib::audio::render_audio(&midi, &sf_data)
        .expect("Audio render should succeed");

    assert_eq!(&wav[0..4], b"RIFF");
    assert!(wav.len() > 100_000, "WAV should be substantial");

    let out = output_dir().join("smoke-test.wav");
    std::fs::write(&out, &wav).unwrap();
    println!("✓ smoke test audio: {} bytes WAV", wav.len());
}
