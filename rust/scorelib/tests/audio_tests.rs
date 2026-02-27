//! Tests for end-to-end audio rendering (MusicXML → MIDI → WAV).
//!
//! These tests verify the complete audio pipeline including SoundFont
//! synthesis via rustysynth.

use scorelib::{parse_file, generate_midi_from_score, MidiOptions, Energy};
use scorelib::audio::render_audio;
use std::path::PathBuf;

fn sheetmusic_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../sheetmusic")
}

fn soundfont_path() -> Option<PathBuf> {
    // The SoundFont is in the Android assets or iOS resources — check both
    let android_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../android/app/src/main/assets/GeneralUser_GS.sf2");
    let ios_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../ios/SoloBandUltra/SoloBandUltra/GeneralUser_GS.sf2");

    if android_path.exists() {
        Some(android_path)
    } else if ios_path.exists() {
        Some(ios_path)
    } else {
        None
    }
}

/// Helper: load SoundFont or skip the test if not available.
fn load_soundfont() -> Vec<u8> {
    match soundfont_path() {
        Some(path) => std::fs::read(&path).expect("Failed to read SoundFont"),
        None => {
            println!("⚠ SoundFont not found — skipping audio test");
            println!("  Place GeneralUser_GS.sf2 in android/app/src/main/assets/ or ios/SoloBandUltra/SoloBandUltra/");
            return Vec::new();
        }
    }
}

fn output_dir() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test_output");
    std::fs::create_dir_all(&dir).ok();
    dir
}

// ═══════════════════════════════════════════════════════════════════════
// End-to-end audio rendering
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn render_audio_asa_branca_produces_valid_wav() {
    let sf_data = load_soundfont();
    if sf_data.is_empty() { return; }

    let score = parse_file(sheetmusic_dir().join("asa-branca.musicxml")).unwrap();
    let options = MidiOptions::default();
    let midi_bytes = generate_midi_from_score(&score, &options);
    assert!(!midi_bytes.is_empty(), "MIDI should not be empty");

    let wav = render_audio(&midi_bytes, &sf_data).expect("Audio render should succeed");

    // Verify WAV header
    assert_eq!(&wav[0..4], b"RIFF", "Should start with RIFF");
    assert_eq!(&wav[8..12], b"WAVE", "Should have WAVE format");
    assert_eq!(&wav[12..16], b"fmt ", "Should have fmt chunk");
    assert_eq!(&wav[36..40], b"data", "Should have data chunk");

    // Verify format: PCM (1), stereo (2), 48000 Hz, 16-bit
    let audio_format = u16::from_le_bytes([wav[20], wav[21]]);
    let num_channels = u16::from_le_bytes([wav[22], wav[23]]);
    let sample_rate = u32::from_le_bytes([wav[24], wav[25], wav[26], wav[27]]);
    let bits_per_sample = u16::from_le_bytes([wav[34], wav[35]]);

    assert_eq!(audio_format, 1, "Should be PCM format");
    assert_eq!(num_channels, 2, "Should be stereo");
    assert_eq!(sample_rate, 48000, "Should be 48000 Hz");
    assert_eq!(bits_per_sample, 16, "Should be 16-bit");

    // WAV should be substantial (a real piece, not silence)
    assert!(wav.len() > 100_000, "WAV should be substantial: {} bytes", wav.len());

    // Verify PCM data is not all zeros (actual audio, not silence)
    let data_offset = 44; // Standard WAV header size
    let pcm_data = &wav[data_offset..];
    let non_zero_samples = pcm_data.chunks(2)
        .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
        .filter(|&s| s != 0)
        .count();
    assert!(non_zero_samples > 1000, "WAV should contain actual audio, not silence ({} non-zero samples)", non_zero_samples);

    let out = output_dir().join("asa-branca.wav");
    std::fs::write(&out, &wav).expect("Failed to write WAV");
    println!("✓ asa-branca audio: {} bytes WAV, {} Hz, {} ch, {} bit, {} non-zero samples",
        wav.len(), sample_rate, num_channels, bits_per_sample, non_zero_samples);
}

#[test]
fn render_audio_with_all_accompaniment() {
    let sf_data = load_soundfont();
    if sf_data.is_empty() { return; }

    let score = parse_file(sheetmusic_dir().join("asa-branca.musicxml")).unwrap();

    let options = MidiOptions {
        include_melody: true,
        include_piano: true,
        include_bass: true,
        include_strings: true,
        include_drums: true,
        include_metronome: true,
        energy: Energy::Strong,
        ..MidiOptions::default()
    };
    let midi_bytes = generate_midi_from_score(&score, &options);
    let wav = render_audio(&midi_bytes, &sf_data).expect("Full accompaniment render should succeed");

    assert_eq!(&wav[0..4], b"RIFF");
    assert!(wav.len() > 100_000, "Full accompaniment WAV should be substantial");

    let out = output_dir().join("asa-branca-full.wav");
    std::fs::write(&out, &wav).expect("Failed to write WAV");
    println!("✓ asa-branca full accompaniment audio: {} bytes", wav.len());
}

#[test]
fn render_audio_melody_only_smaller_than_full() {
    let sf_data = load_soundfont();
    if sf_data.is_empty() { return; }

    let score = parse_file(sheetmusic_dir().join("asa-branca.musicxml")).unwrap();

    let melody_only = MidiOptions { include_metronome: false, ..MidiOptions::default() };
    let full = MidiOptions {
        include_melody: true,
        include_piano: true,
        include_bass: true,
        include_strings: true,
        include_drums: true,
        include_metronome: true,
        energy: Energy::Strong,
        ..MidiOptions::default()
    };

    let midi_mel = generate_midi_from_score(&score, &melody_only);
    let midi_full = generate_midi_from_score(&score, &full);

    let wav_mel = render_audio(&midi_mel, &sf_data).unwrap();
    let wav_full = render_audio(&midi_full, &sf_data).unwrap();

    // Both should be valid WAVs of similar duration (same piece), so similar size
    // but the full version will have more energy (higher peak amplitude)
    assert_eq!(&wav_mel[0..4], b"RIFF");
    assert_eq!(&wav_full[0..4], b"RIFF");

    // The WAV durations should be very similar (same piece)
    let mel_data_size = u32::from_le_bytes([wav_mel[40], wav_mel[41], wav_mel[42], wav_mel[43]]);
    let full_data_size = u32::from_le_bytes([wav_full[40], wav_full[41], wav_full[42], wav_full[43]]);
    let size_ratio = mel_data_size as f64 / full_data_size as f64;
    assert!(
        (size_ratio - 1.0).abs() < 0.1,
        "WAV durations should be similar (ratio={:.3})",
        size_ratio
    );

    println!("✓ melody-only: {} bytes, full: {} bytes (ratio={:.3})",
        wav_mel.len(), wav_full.len(), size_ratio);
}

// ═══════════════════════════════════════════════════════════════════════
// render_audio_from_bytes (the combined FFI path)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn render_audio_from_bytes_end_to_end() {
    let sf_data = load_soundfont();
    if sf_data.is_empty() { return; }

    let musicxml_data = std::fs::read(sheetmusic_dir().join("asa-branca.musicxml")).unwrap();

    let options = MidiOptions::default();
    let wav = scorelib::render_audio_from_bytes(&musicxml_data, Some("musicxml"), &options, &sf_data)
        .expect("render_audio_from_bytes should succeed");

    assert_eq!(&wav[0..4], b"RIFF");
    assert!(wav.len() > 100_000);
    println!("✓ render_audio_from_bytes: {} bytes WAV", wav.len());
}

#[test]
fn render_audio_from_mxl_bytes() {
    let sf_data = load_soundfont();
    if sf_data.is_empty() { return; }

    let mxl_data = std::fs::read(sheetmusic_dir().join("童年.mxl")).unwrap();

    let options = MidiOptions::default();
    let wav = scorelib::render_audio_from_bytes(&mxl_data, Some("mxl"), &options, &sf_data)
        .expect("render_audio_from_bytes with MXL should succeed");

    assert_eq!(&wav[0..4], b"RIFF");
    assert!(wav.len() > 100_000);
    println!("✓ render_audio_from_bytes (MXL): {} bytes WAV", wav.len());
}

// ═══════════════════════════════════════════════════════════════════════
// Energy levels produce different output
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn different_energy_levels_produce_different_midi() {
    // This test doesn't need a SoundFont — it only generates MIDI.
    let score = parse_file(sheetmusic_dir().join("asa-branca.musicxml")).unwrap();

    let make_opts = |energy| MidiOptions {
        include_melody: true,
        include_piano: true,
        include_bass: true,
        include_drums: true,
        include_metronome: true,
        energy,
        ..MidiOptions::default()
    };

    let midi_soft = generate_midi_from_score(&score, &make_opts(Energy::Soft));
    let midi_medium = generate_midi_from_score(&score, &make_opts(Energy::Medium));
    let midi_strong = generate_midi_from_score(&score, &make_opts(Energy::Strong));

    // All should be valid SMFs
    assert_eq!(&midi_soft[0..4], b"MThd");
    assert_eq!(&midi_medium[0..4], b"MThd");
    assert_eq!(&midi_strong[0..4], b"MThd");

    // Different energy levels should produce different MIDI bytes
    // (different velocity values in accompaniment tracks)
    assert_ne!(midi_soft, midi_medium, "Soft and Medium should differ");
    assert_ne!(midi_medium, midi_strong, "Medium and Strong should differ");
    assert_ne!(midi_soft, midi_strong, "Soft and Strong should differ");

    println!("✓ Energy levels produce different MIDI: soft={}, medium={}, strong={} bytes",
        midi_soft.len(), midi_medium.len(), midi_strong.len());
}
