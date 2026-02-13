//! Integration tests for the MIDI generation pipeline:
//! unrolling, timemap computation, and MIDI output.

use scorelib::{
    parse_file, unroll, generate_timemap, generate_midi_from_score,
    MidiOptions, Energy,
};

/// Write bytes to a path, creating parent directories if needed.
fn write_test_output(path: &str, data: &[u8]) {
    if let Some(parent) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, data).unwrap();
}

// ═══════════════════════════════════════════════════════════════════════
// Unroller tests
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn unroll_asa_branca_expands_repeats() {
    let score = parse_file("../../sheetmusic/asa-branca.musicxml").unwrap();
    let unrolled = unroll(&score, 0);
    let raw_count = score.parts[0].measures.len();

    // asa-branca has repeat barlines with 1st/2nd endings
    assert!(
        unrolled.len() > raw_count,
        "Unrolled {} should be > raw {}",
        unrolled.len(), raw_count
    );
    println!("✓ asa-branca: {} raw → {} unrolled measures", raw_count, unrolled.len());
}

#[test]
fn unroll_blue_bag_folly_handles_ds_al_fine() {
    let score = parse_file("../../sheetmusic/blue-bag-folly.musicxml").unwrap();
    let unrolled = unroll(&score, 0);
    let raw_count = score.parts[0].measures.len();

    // Has D.S. al Fine: should replay from segno to fine
    assert!(
        unrolled.len() > raw_count,
        "Unrolled {} should be > raw {} (D.S. al Fine)",
        unrolled.len(), raw_count
    );
    println!("✓ blue-bag-folly: {} raw → {} unrolled measures", raw_count, unrolled.len());
}

#[test]
fn unroll_chopin_no_repeats() {
    let score = parse_file("../../sheetmusic/chopin-trois-valses.mxl").unwrap();
    let unrolled = unroll(&score, 0);
    let raw_count = score.parts[0].measures.len();

    // Chopin has no repeats — unrolled should equal raw
    assert_eq!(
        unrolled.len(), raw_count,
        "Chopin unrolled {} should == raw {}",
        unrolled.len(), raw_count
    );
    println!("✓ chopin: {} measures (no expansion needed)", raw_count);
}

#[test]
fn unroll_tongnian() {
    let score = parse_file("../../sheetmusic/童年.mxl").unwrap();
    let unrolled = unroll(&score, 0);
    let raw_count = score.parts[0].measures.len();

    // Should produce at least as many measures as raw
    assert!(
        unrolled.len() >= raw_count,
        "Unrolled {} should be >= raw {}",
        unrolled.len(), raw_count
    );
    println!("✓ 童年: {} raw → {} unrolled measures", raw_count, unrolled.len());
}

// ═══════════════════════════════════════════════════════════════════════
// Timemap tests
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn timemap_asa_branca_has_correct_tempo() {
    let score = parse_file("../../sheetmusic/asa-branca.musicxml").unwrap();
    let unrolled = unroll(&score, 0);
    let timemap = generate_timemap(&score, 0, &unrolled);

    assert_eq!(timemap.len(), unrolled.len());

    // First entry starts at 0
    assert!((timemap[0].timestamp_ms - 0.0).abs() < 0.01);

    // Entries are monotonically increasing
    for i in 1..timemap.len() {
        assert!(
            timemap[i].timestamp_ms > timemap[i - 1].timestamp_ms,
            "Timemap not monotonic at index {}",
            i
        );
    }

    // Total duration should be reasonable (> 30 seconds for a real piece)
    let total_ms = scorelib::timemap::total_duration_ms(&timemap);
    assert!(
        total_ms > 30_000.0,
        "Total duration {:.1}ms seems too short",
        total_ms
    );
    println!("✓ asa-branca timemap: {} entries, total {:.1}s", timemap.len(), total_ms / 1000.0);
}

#[test]
fn timemap_blue_bag_folly_tempo_changes() {
    let score = parse_file("../../sheetmusic/blue-bag-folly.musicxml").unwrap();
    let unrolled = unroll(&score, 0);
    let timemap = generate_timemap(&score, 0, &unrolled);

    // blue-bag-folly has tempo changes (120 → 90)
    let tempos: Vec<f64> = timemap.iter().map(|e| e.tempo_bpm).collect();
    let unique_tempos: std::collections::HashSet<i32> = tempos.iter().map(|t| *t as i32).collect();
    assert!(
        unique_tempos.len() >= 2,
        "Expected multiple tempos, got {:?}",
        unique_tempos
    );
    println!("✓ blue-bag-folly timemap: tempos = {:?}", unique_tempos);
}

#[test]
fn debug_blue_bag_folly_structure() {
    let score = parse_file("../../sheetmusic/blue-bag-folly.musicxml").unwrap();
    let part = &score.parts[0];

    println!("=== Measure analysis ===");
    for (i, m) in part.measures.iter().enumerate() {
        let mut info = Vec::new();
        for dir in &m.directions {
            if dir.segno { info.push("SEGNO".to_string()); }
            if dir.coda { info.push("CODA".to_string()); }
            if dir.sound_dalsegno { info.push("D.S.".to_string()); }
            if dir.sound_fine { info.push("FINE".to_string()); }
            if dir.sound_tocoda { info.push("TO CODA".to_string()); }
            if let Some(t) = dir.sound_tempo { info.push(format!("tempo={}", t)); }
            if let Some(ref w) = dir.words { info.push(format!("words=\"{}\"", w)); }
        }
        for bl in &m.barlines {
            if let Some(ref r) = bl.repeat { info.push(format!("repeat-{}", r.direction)); }
            if let Some(ref e) = bl.ending { info.push(format!("ending-{}-{}", e.number, e.ending_type)); }
        }
        if !info.is_empty() {
            println!("  m[{}] (number={}): {}", i, m.number, info.join(", "));
        }
    }

    let unrolled = unroll(&score, 0);
    println!("\n=== Unrolled sequence ({} measures) ===", unrolled.len());
    for (i, um) in unrolled.iter().enumerate() {
        let m = &part.measures[um.original_index];
        let mut markers = Vec::new();
        for dir in &m.directions {
            if let Some(t) = dir.sound_tempo { markers.push(format!("tempo={}", t)); }
            if dir.segno { markers.push("SEGNO".to_string()); }
            if dir.sound_dalsegno { markers.push("D.S.".to_string()); }
            if dir.sound_fine { markers.push("FINE".to_string()); }
        }
        let marker_str = if markers.is_empty() { String::new() } else { format!(" [{}]", markers.join(", ")) };
        println!("  [{:>2}] → m[{}] (number={}){}", i, um.original_index, m.number, marker_str);
    }
}

#[test]
fn timemap_blue_bag_folly_tempo_reverts_after_ds() {
    // CRITICAL: After D.S. jumps back to segno (original measure index 10,
    // which is at 120 BPM), the tempo must revert to 120, not stay at 90
    // (which was set at original measure 14).
    let score = parse_file("../../sheetmusic/blue-bag-folly.musicxml").unwrap();
    let unrolled = unroll(&score, 0);
    let timemap = generate_timemap(&score, 0, &unrolled);

    // Find the D.S. jump: the point where original_index jumps back to
    // the segno (measure index 10).  This is NOT the repeat backward jump.
    let segno_measure = 10; // original measure index where segno is
    let mut jump_idx = None;
    for i in 1..timemap.len() {
        if timemap[i].original_index == segno_measure
            && timemap[i - 1].original_index > segno_measure
        {
            // Found a jump back to the segno from a later measure
            jump_idx = Some(i);
            break;
        }
    }
    let jump_idx = jump_idx.expect("Expected a D.S. jump back to segno in unrolled sequence");

    // Before the jump: should be 90 BPM (the later section)
    let before_jump = &timemap[jump_idx - 1];
    // After the jump: should revert to 120 BPM (the segno section)
    let after_jump = &timemap[jump_idx];

    println!("  Before D.S. jump (unrolled idx {}): original m[{}] @ {} BPM",
        jump_idx - 1, before_jump.original_index, before_jump.tempo_bpm);
    println!("  After D.S. jump  (unrolled idx {}): original m[{}] @ {} BPM",
        jump_idx, after_jump.original_index, after_jump.tempo_bpm);

    assert!(
        (before_jump.tempo_bpm - 90.0).abs() < 1.0,
        "Before D.S. jump should be ~90 BPM, got {}",
        before_jump.tempo_bpm
    );
    assert!(
        (after_jump.tempo_bpm - 120.0).abs() < 1.0,
        "After D.S. jump should revert to ~120 BPM, got {}",
        after_jump.tempo_bpm
    );

    // Print full tempo trace for verification
    println!("✓ blue-bag-folly tempo trace:");
    let mut last_tempo = 0.0;
    for e in &timemap {
        if (e.tempo_bpm - last_tempo).abs() > 0.1 {
            println!("    Unrolled[{}] = original m[{}] → {} BPM",
                e.index, e.original_index, e.tempo_bpm);
            last_tempo = e.tempo_bpm;
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// MIDI output tests
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn midi_asa_branca_valid_smf() {
    let score = parse_file("../../sheetmusic/asa-branca.musicxml").unwrap();
    let options = MidiOptions::default(); // melody + metronome
    let midi = generate_midi_from_score(&score, &options);

    // Check SMF header
    assert_eq!(&midi[0..4], b"MThd", "Missing MThd header");
    assert_eq!(&midi[8..10], &1u16.to_be_bytes(), "Should be format 1");

    // Should contain at least tempo track + melody + metronome = 3 tracks
    let track_count = u16::from_be_bytes([midi[10], midi[11]]);
    assert!(track_count >= 3, "Expected >= 3 tracks, got {}", track_count);

    // Should contain MTrk chunks
    let mtrk_count = midi.windows(4).filter(|w| *w == b"MTrk").count();
    assert_eq!(
        mtrk_count, track_count as usize,
        "MTrk count {} doesn't match header {}",
        mtrk_count, track_count
    );

    // Write to test output for manual inspection
    let output_path = "test_output/asa-branca.mid";
    write_test_output(output_path, &midi);
    println!("✓ asa-branca MIDI: {} bytes, {} tracks → {}", midi.len(), track_count, output_path);
}

#[test]
fn midi_blue_bag_folly_valid_smf() {
    let score = parse_file("../../sheetmusic/blue-bag-folly.musicxml").unwrap();
    let options = MidiOptions {
        include_melody: true,
        include_piano: true,
        include_bass: true,
        include_strings: true,
        include_drums: true,
        include_metronome: true,
        melody_channel: 0,
        energy: Energy::Medium,
        transpose: 0,
    };
    let midi = generate_midi_from_score(&score, &options);

    assert_eq!(&midi[0..4], b"MThd");
    let track_count = u16::from_be_bytes([midi[10], midi[11]]);
    // tempo + melody + metronome + piano + bass + strings + drums = 7
    assert_eq!(track_count, 7, "Expected 7 tracks (all enabled), got {}", track_count);

    let output_path = "test_output/blue-bag-folly.mid";
    write_test_output(output_path, &midi);
    println!("✓ blue-bag-folly MIDI: {} bytes, {} tracks → {}", midi.len(), track_count, output_path);
}

#[test]
fn midi_chopin_melody_only() {
    let score = parse_file("../../sheetmusic/chopin-trois-valses.mxl").unwrap();
    let options = MidiOptions {
        include_metronome: false,
        ..MidiOptions::default()
    };
    let midi = generate_midi_from_score(&score, &options);

    assert_eq!(&midi[0..4], b"MThd");
    let track_count = u16::from_be_bytes([midi[10], midi[11]]);
    // tempo + melody = 2 tracks
    assert_eq!(track_count, 2, "Expected 2 tracks (melody only), got {}", track_count);

    let output_path = "test_output/chopin-trois-valses.mid";
    write_test_output(output_path, &midi);
    println!("✓ chopin MIDI: {} bytes, {} tracks → {}", midi.len(), track_count, output_path);
}

#[test]
fn midi_chopin_with_inferred_accompaniment() {
    // Chopin has NO explicit <harmony> elements — chords must be inferred
    // from the melody notes. This tests the pitch-class analysis fallback.
    let score = parse_file("../../sheetmusic/chopin-trois-valses.mxl").unwrap();

    // Verify no harmonies exist
    let total_harmonies: usize = score.parts[0].measures.iter()
        .map(|m| m.harmonies.len()).sum();
    assert_eq!(total_harmonies, 0, "Chopin should have no explicit harmonies");

    let options = MidiOptions {
        include_melody: true,
        include_piano: true,
        include_bass: true,
        include_strings: true,
        include_drums: true,
        include_metronome: true,
        melody_channel: 0,
        energy: Energy::Medium,
        transpose: 0,
    };
    let midi = generate_midi_from_score(&score, &options);

    assert_eq!(&midi[0..4], b"MThd");
    let track_count = u16::from_be_bytes([midi[10], midi[11]]);
    // All 7 tracks should be present even without explicit chords
    assert_eq!(track_count, 7, "Expected 7 tracks (all enabled with inferred chords), got {}", track_count);

    // The file should be larger than melody-only version (has accompaniment data)
    assert!(midi.len() > 43000, "Full accompaniment MIDI should be larger than melody-only");

    let output_path = "test_output/chopin-trois-valses-full.mid";
    write_test_output(output_path, &midi);
    println!("✓ chopin (inferred chords) MIDI: {} bytes, {} tracks → {}", midi.len(), track_count, output_path);
}

#[test]
fn midi_tongnian_with_accompaniment() {
    let score = parse_file("../../sheetmusic/童年.mxl").unwrap();
    let options = MidiOptions {
        include_melody: true,
        include_piano: true,
        include_bass: true,
        include_metronome: true,
        ..MidiOptions::default()
    };
    let midi = generate_midi_from_score(&score, &options);

    assert_eq!(&midi[0..4], b"MThd");
    let track_count = u16::from_be_bytes([midi[10], midi[11]]);
    assert!(track_count >= 4, "Expected >= 4 tracks, got {}", track_count);

    // File should be reasonably sized
    assert!(midi.len() > 100, "MIDI seems too small: {} bytes", midi.len());

    let output_path = "test_output/tongnian.mid";
    write_test_output(output_path, &midi);
    println!("✓ 童年 MIDI: {} bytes, {} tracks → {}", midi.len(), track_count, output_path);
}
