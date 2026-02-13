//! Playback map tests — verify cursor synchronization data for all sample files.

use scorelib::{parse_file, generate_playback_map};
use scorelib::playback::playback_map_to_json;
use std::path::PathBuf;

fn sheetmusic_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../sheetmusic")
}

fn output_dir() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test_output");
    std::fs::create_dir_all(&dir).ok();
    dir
}

#[test]
fn playback_map_asa_branca() {
    let path = sheetmusic_dir().join("asa-branca.musicxml");
    let score = parse_file(&path).expect("Failed to parse asa-branca");

    let pmap = generate_playback_map(&score, None);

    // Asa branca has 34 original measures
    assert!(!pmap.measures.is_empty(), "Should have measures");
    assert_eq!(pmap.measures.len(), 34, "Should have 34 original measures");

    // Should have at least one system
    assert!(!pmap.systems.is_empty(), "Should have at least one system");

    // Timemap should have 65 entries (34 raw -> 65 unrolled due to repeats)
    assert_eq!(pmap.timemap.len(), 65, "Should have 65 unrolled timemap entries");

    // Measures should have valid positions
    for m in &pmap.measures {
        assert!(m.x > 0.0, "Measure {} should have positive x", m.measure_idx);
        assert!(m.width > 0.0, "Measure {} should have positive width", m.measure_idx);
        assert!(m.system_idx < pmap.systems.len(),
            "Measure {} system_idx {} should be valid", m.measure_idx, m.system_idx);
    }

    // Systems should have valid positions
    for (i, sys) in pmap.systems.iter().enumerate() {
        assert!(sys.y > 0.0, "System {} should have positive y", i);
        assert!(sys.height > 0.0, "System {} should have positive height", i);
    }

    // Timemap should be monotonically increasing in timestamp
    for i in 1..pmap.timemap.len() {
        assert!(pmap.timemap[i].timestamp_ms >= pmap.timemap[i-1].timestamp_ms,
            "Timemap should be monotonically increasing at index {}", i);
    }

    // Each timemap entry's original_index should have a corresponding measure position
    for entry in &pmap.timemap {
        let has_measure = pmap.measures.iter().any(|m| m.measure_idx == entry.original_index);
        assert!(has_measure,
            "Timemap entry original_index {} should have a measure position", entry.original_index);
    }

    // Write JSON for inspection
    let json = playback_map_to_json(&pmap);
    let out = output_dir().join("asa-branca-playback-map.json");
    std::fs::write(&out, &json).expect("Failed to write playback map");
    println!("✓ asa-branca playback map: {} measures, {} systems, {} timemap entries",
        pmap.measures.len(), pmap.systems.len(), pmap.timemap.len());
    println!("  Output: {}", out.display());
}

#[test]
fn playback_map_blue_bag_folly() {
    let path = sheetmusic_dir().join("blue-bag-folly.musicxml");
    let score = parse_file(&path).expect("Failed to parse blue-bag-folly");

    let pmap = generate_playback_map(&score, None);

    assert_eq!(pmap.measures.len(), 27, "Should have 27 original measures");
    assert_eq!(pmap.timemap.len(), 52, "Should have 52 unrolled timemap entries (D.S. al Fine)");

    // Verify tempo changes in the timemap
    let has_120 = pmap.timemap.iter().any(|e| (e.tempo_bpm - 120.0).abs() < 0.01);
    let has_90 = pmap.timemap.iter().any(|e| (e.tempo_bpm - 90.0).abs() < 0.01);
    assert!(has_120, "Should have 120 BPM");
    assert!(has_90, "Should have 90 BPM");

    // Write JSON for inspection
    let json = playback_map_to_json(&pmap);
    let out = output_dir().join("blue-bag-folly-playback-map.json");
    std::fs::write(&out, &json).expect("Failed to write playback map");
    println!("✓ blue-bag-folly playback map: {} measures, {} systems, {} timemap entries",
        pmap.measures.len(), pmap.systems.len(), pmap.timemap.len());
}

#[test]
fn playback_map_tongnian() {
    let path = sheetmusic_dir().join("童年.mxl");
    let score = parse_file(&path).expect("Failed to parse 童年");

    let pmap = generate_playback_map(&score, None);

    assert!(!pmap.measures.is_empty(), "Should have original measures");
    assert!(!pmap.systems.is_empty());
    assert!(!pmap.timemap.is_empty());

    // Each timemap entry should reference a valid original measure
    for entry in &pmap.timemap {
        assert!(pmap.measures.iter().any(|m| m.measure_idx == entry.original_index),
            "Timemap original_index {} should reference a measure", entry.original_index);
    }

    let json = playback_map_to_json(&pmap);
    let out = output_dir().join("tongnian-playback-map.json");
    std::fs::write(&out, &json).expect("Failed to write playback map");
    println!("✓ 童年 playback map: {} measures, {} systems, {} timemap entries",
        pmap.measures.len(), pmap.systems.len(), pmap.timemap.len());
}

#[test]
fn playback_map_chopin() {
    let path = sheetmusic_dir().join("chopin-trois-valses.mxl");
    let score = parse_file(&path).expect("Failed to parse chopin");

    let pmap = generate_playback_map(&score, None);

    assert_eq!(pmap.measures.len(), 506, "Chopin should have 506 measures");
    assert!(pmap.systems.len() > 10, "Chopin should have many systems");

    // No repeats, so timemap length should equal measure count
    assert_eq!(pmap.timemap.len(), 506,
        "Chopin has no repeats, timemap should match measure count");

    let json = playback_map_to_json(&pmap);
    let out = output_dir().join("chopin-playback-map.json");
    std::fs::write(&out, &json).expect("Failed to write playback map");
    println!("✓ chopin playback map: {} measures, {} systems, {} timemap entries",
        pmap.measures.len(), pmap.systems.len(), pmap.timemap.len());
}

#[test]
fn playback_map_narrow_width_changes_systems() {
    let path = sheetmusic_dir().join("asa-branca.musicxml");
    let score = parse_file(&path).expect("Failed to parse asa-branca");

    // Generate at default width and narrow width
    let pmap_wide = generate_playback_map(&score, Some(820.0));
    let pmap_narrow = generate_playback_map(&score, Some(390.0));

    // Narrow width should have more systems (more line breaks)
    assert!(pmap_narrow.systems.len() > pmap_wide.systems.len(),
        "Narrow width ({} systems) should have more systems than wide ({} systems)",
        pmap_narrow.systems.len(), pmap_wide.systems.len());

    // Both should have the same number of measures
    assert_eq!(pmap_wide.measures.len(), pmap_narrow.measures.len());

    // Both should have the same timemap (timing is independent of layout)
    assert_eq!(pmap_wide.timemap.len(), pmap_narrow.timemap.len());

    println!("✓ Responsive layout: wide={} systems, narrow={} systems",
        pmap_wide.systems.len(), pmap_narrow.systems.len());
}

#[test]
fn playback_map_json_roundtrip() {
    let path = sheetmusic_dir().join("asa-branca.musicxml");
    let score = parse_file(&path).expect("Failed to parse asa-branca");

    let pmap = generate_playback_map(&score, None);
    let json = playback_map_to_json(&pmap);

    // Verify JSON is valid and contains expected keys
    assert!(json.contains("\"measures\""), "JSON should contain measures key");
    assert!(json.contains("\"systems\""), "JSON should contain systems key");
    assert!(json.contains("\"timemap\""), "JSON should contain timemap key");
    assert!(json.contains("\"measure_idx\""), "JSON should contain measure_idx");
    assert!(json.contains("\"timestamp_ms\""), "JSON should contain timestamp_ms");
    assert!(json.contains("\"system_idx\""), "JSON should contain system_idx");

    // Parse JSON back to verify structure
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("JSON should be valid");
    assert!(parsed["measures"].is_array());
    assert!(parsed["systems"].is_array());
    assert!(parsed["timemap"].is_array());

    println!("✓ Playback map JSON roundtrip OK ({} bytes)", json.len());
}
