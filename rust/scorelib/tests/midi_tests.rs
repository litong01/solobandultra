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
    // tempo + treble + bass = 3 tracks (piano piece has 2 staves, each on its own channel)
    assert_eq!(track_count, 3, "Expected 3 tracks (tempo + treble + bass), got {}", track_count);

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
    // All 8 tracks: tempo + treble + bass + piano + bass_acc + strings + drums + metronome
    assert_eq!(track_count, 8, "Expected 8 tracks (2 staves + 5 accompaniment + tempo), got {}", track_count);

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

// ═══════════════════════════════════════════════════════════════════════
// parse_midi_options_from_json_str tests
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn parse_options_defaults() {
    let opts = scorelib::parse_midi_options_from_json_str("{}");
    assert!(opts.include_melody);
    assert!(!opts.include_piano);
    assert!(!opts.include_bass);
    assert!(!opts.include_strings);
    assert!(!opts.include_drums);
    assert!(opts.include_metronome);
    assert_eq!(opts.transpose, 0);
    assert!(matches!(opts.energy, Energy::Medium));
    println!("✓ parse_midi_options defaults correct");
}

#[test]
fn parse_options_all_accompaniment_enabled() {
    let json = r#"{
        "include_melody": true,
        "include_piano": true,
        "include_bass": true,
        "include_strings": true,
        "include_drums": true,
        "include_metronome": true,
        "energy": "strong",
        "transpose": 3
    }"#;
    let opts = scorelib::parse_midi_options_from_json_str(json);
    assert!(opts.include_melody);
    assert!(opts.include_piano);
    assert!(opts.include_bass);
    assert!(opts.include_strings);
    assert!(opts.include_drums);
    assert!(opts.include_metronome);
    assert!(matches!(opts.energy, Energy::Strong));
    assert_eq!(opts.transpose, 3);
    println!("✓ parse_midi_options with all accompaniment enabled");
}

#[test]
fn parse_options_melody_disabled() {
    let json = r#"{"include_melody": false, "include_metronome": false}"#;
    let opts = scorelib::parse_midi_options_from_json_str(json);
    assert!(!opts.include_melody);
    assert!(!opts.include_metronome);
    println!("✓ parse_midi_options melody disabled");
}

#[test]
fn parse_options_soft_energy() {
    let json = r#"{"energy": "soft"}"#;
    let opts = scorelib::parse_midi_options_from_json_str(json);
    assert!(matches!(opts.energy, Energy::Soft));
    println!("✓ parse_midi_options soft energy");
}

#[test]
fn parse_options_negative_transpose() {
    let json = r#"{"transpose": -5}"#;
    let opts = scorelib::parse_midi_options_from_json_str(json);
    assert_eq!(opts.transpose, -5);
    println!("✓ parse_midi_options negative transpose");
}

#[test]
fn parse_options_compact_json() {
    // No spaces after colons
    let json = r#"{"include_piano":true,"include_bass":true,"energy":"soft","transpose":-2}"#;
    let opts = scorelib::parse_midi_options_from_json_str(json);
    assert!(opts.include_piano);
    assert!(opts.include_bass);
    assert!(matches!(opts.energy, Energy::Soft));
    assert_eq!(opts.transpose, -2);
    println!("✓ parse_midi_options compact JSON (no spaces)");
}

// ═══════════════════════════════════════════════════════════════════════
// generate_midi_from_bytes tests
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn generate_midi_from_musicxml_bytes() {
    let data = std::fs::read("../../sheetmusic/asa-branca.musicxml").unwrap();
    let options = MidiOptions::default();
    let midi = scorelib::generate_midi_from_bytes(&data, Some("musicxml"), &options)
        .expect("generate_midi_from_bytes should succeed");

    assert_eq!(&midi[0..4], b"MThd");
    let track_count = u16::from_be_bytes([midi[10], midi[11]]);
    assert!(track_count >= 3);
    println!("✓ generate_midi_from_bytes (musicxml): {} bytes, {} tracks", midi.len(), track_count);
}

#[test]
fn generate_midi_from_mxl_bytes() {
    let data = std::fs::read("../../sheetmusic/童年.mxl").unwrap();
    let options = MidiOptions::default();
    let midi = scorelib::generate_midi_from_bytes(&data, Some("mxl"), &options)
        .expect("generate_midi_from_bytes (MXL) should succeed");

    assert_eq!(&midi[0..4], b"MThd");
    println!("✓ generate_midi_from_bytes (mxl): {} bytes", midi.len());
}

#[test]
fn generate_midi_from_bytes_auto_detect() {
    let data = std::fs::read("../../sheetmusic/asa-branca.musicxml").unwrap();
    let options = MidiOptions::default();
    let midi = scorelib::generate_midi_from_bytes(&data, None, &options)
        .expect("generate_midi_from_bytes (auto-detect) should succeed");

    assert_eq!(&midi[0..4], b"MThd");
    println!("✓ generate_midi_from_bytes (auto-detect): {} bytes", midi.len());
}

#[test]
fn generate_midi_from_bytes_with_transpose() {
    let data = std::fs::read("../../sheetmusic/asa-branca.musicxml").unwrap();
    let options_no_trans = MidiOptions::default();
    let options_trans = MidiOptions { transpose: 5, ..MidiOptions::default() };

    let midi1 = scorelib::generate_midi_from_bytes(&data, Some("musicxml"), &options_no_trans).unwrap();
    let midi2 = scorelib::generate_midi_from_bytes(&data, Some("musicxml"), &options_trans).unwrap();

    assert_ne!(midi1, midi2, "Transposed MIDI should differ from original");
    println!("✓ generate_midi_from_bytes with transpose: orig={} bytes, trans={} bytes", midi1.len(), midi2.len());
}

// ═══════════════════════════════════════════════════════════════════════
// render_bytes_to_svg tests
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn render_bytes_to_svg_musicxml() {
    let data = std::fs::read("../../sheetmusic/asa-branca.musicxml").unwrap();
    let svg = scorelib::render_bytes_to_svg(&data, Some("musicxml"), None, 0, None, false)
        .expect("render_bytes_to_svg should succeed");

    assert!(svg.starts_with("<svg"));
    assert!(svg.contains("<ellipse"));
    println!("✓ render_bytes_to_svg (musicxml): {} bytes", svg.len());
}

#[test]
fn render_bytes_to_svg_mxl() {
    let data = std::fs::read("../../sheetmusic/童年.mxl").unwrap();
    let svg = scorelib::render_bytes_to_svg(&data, Some("mxl"), None, 0, None, false)
        .expect("render_bytes_to_svg (MXL) should succeed");

    assert!(svg.starts_with("<svg"));
    assert!(svg.contains("<ellipse"));
    println!("✓ render_bytes_to_svg (mxl): {} bytes", svg.len());
}

#[test]
fn render_bytes_to_svg_with_transpose() {
    let data = std::fs::read("../../sheetmusic/asa-branca.musicxml").unwrap();
    let svg_orig = scorelib::render_bytes_to_svg(&data, Some("musicxml"), None, 0, None, false).unwrap();
    let svg_trans = scorelib::render_bytes_to_svg(&data, Some("musicxml"), None, 3, None, false).unwrap();

    assert!(svg_orig.starts_with("<svg"));
    assert!(svg_trans.starts_with("<svg"));
    // Transposed version should differ (key signature, note positions)
    assert_ne!(svg_orig, svg_trans, "Transposed SVG should differ from original");
    println!("✓ render_bytes_to_svg with transpose: orig={} bytes, trans={} bytes",
        svg_orig.len(), svg_trans.len());
}

#[test]
fn render_bytes_to_svg_with_page_width() {
    let data = std::fs::read("../../sheetmusic/asa-branca.musicxml").unwrap();
    let svg_wide = scorelib::render_bytes_to_svg(&data, Some("musicxml"), Some(820.0), 0, None, false).unwrap();
    let svg_narrow = scorelib::render_bytes_to_svg(&data, Some("musicxml"), Some(390.0), 0, None, false).unwrap();

    assert!(svg_wide.contains("viewBox=\"0 0 820"));
    assert!(svg_narrow.contains("viewBox=\"0 0 390"));
    println!("✓ render_bytes_to_svg with page width: wide vs narrow");
}

// ═══════════════════════════════════════════════════════════════════════
// playback_map_from_bytes test
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn playback_map_from_bytes_returns_valid_json() {
    let data = std::fs::read("../../sheetmusic/asa-branca.musicxml").unwrap();
    let json = scorelib::playback_map_from_bytes(&data, Some("musicxml"), None, 0, None, false)
        .expect("playback_map_from_bytes should succeed");

    let parsed: serde_json::Value = serde_json::from_str(&json).expect("Should be valid JSON");
    assert!(parsed["measures"].is_array());
    assert!(parsed["systems"].is_array());
    assert!(parsed["timemap"].is_array());
    println!("✓ playback_map_from_bytes: {} bytes JSON", json.len());
}
