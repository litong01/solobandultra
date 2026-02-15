//! Offline MIDI-to-audio rendering using rustysynth.
//!
//! Takes MIDI bytes (SMF) + SoundFont bytes (SF2) and produces a WAV file
//! in memory.  All synthesis happens offline — no real-time constraints,
//! unlimited polyphony (bounded only by CPU time).

use std::io::Cursor;
use std::sync::Arc;

use rustysynth::{MidiFile, MidiFileSequencer, SoundFont, Synthesizer, SynthesizerSettings};

/// Output sample rate (CD quality).
const SAMPLE_RATE: i32 = 44100;

/// Render block size (samples per block).  Keeps peak memory low by
/// converting f32→i16 incrementally instead of allocating the entire
/// float buffer up front.
const BLOCK_SIZE: usize = 8192;

/// Render MIDI data to a WAV file using the provided SoundFont.
///
/// Returns a complete WAV file (header + PCM data) as bytes.
/// Format: 44 100 Hz, stereo, 16-bit signed integer, little-endian.
///
/// An extra second of silence is appended so release tails ring out
/// naturally instead of being abruptly cut.
pub fn render_audio(midi_data: &[u8], soundfont_data: &[u8]) -> Result<Vec<u8>, String> {
    // ── Load SoundFont ──
    let mut sf_cursor = Cursor::new(soundfont_data);
    let soundfont = Arc::new(
        SoundFont::new(&mut sf_cursor).map_err(|e| format!("SoundFont load error: {e:?}"))?,
    );

    // ── Load MIDI ──
    let mut midi_cursor = Cursor::new(midi_data);
    let midi_file = Arc::new(
        MidiFile::new(&mut midi_cursor).map_err(|e| format!("MIDI load error: {e:?}"))?,
    );

    // ── Create synthesizer + sequencer ──
    let settings = SynthesizerSettings::new(SAMPLE_RATE);
    let synthesizer =
        Synthesizer::new(&soundfont, &settings).map_err(|e| format!("Synth init error: {e:?}"))?;
    let mut sequencer = MidiFileSequencer::new(synthesizer);

    // Total samples = MIDI duration + 1 s for release tails.
    let duration_secs = midi_file.get_length() + 1.0;
    let total_samples = (SAMPLE_RATE as f64 * duration_secs) as usize;

    // Start playback.
    sequencer.play(&midi_file, false);

    // ── Render in blocks, converting f32 → i16 incrementally ──
    let mut left = vec![0f32; BLOCK_SIZE];
    let mut right = vec![0f32; BLOCK_SIZE];

    // Pre-allocate PCM output: stereo × 2 bytes per sample.
    let mut pcm_data: Vec<u8> = Vec::with_capacity(total_samples * 4);
    let mut rendered: usize = 0;

    while rendered < total_samples {
        let count = BLOCK_SIZE.min(total_samples - rendered);
        sequencer.render(&mut left[..count], &mut right[..count]);

        for i in 0..count {
            let l = (left[i] * 32767.0).clamp(-32768.0, 32767.0) as i16;
            let r = (right[i] * 32767.0).clamp(-32768.0, 32767.0) as i16;
            pcm_data.extend_from_slice(&l.to_le_bytes());
            pcm_data.extend_from_slice(&r.to_le_bytes());
        }
        rendered += count;
    }

    let wav = build_wav(SAMPLE_RATE as u32, 2, 16, &pcm_data);

    #[cfg(debug_assertions)]
    eprintln!(
        "[audio] Rendered {:.1}s → {} bytes WAV ({} PCM samples)",
        duration_secs,
        wav.len(),
        rendered
    );

    Ok(wav)
}

/// Build a complete WAV file from raw PCM data.
fn build_wav(sample_rate: u32, channels: u16, bits_per_sample: u16, pcm_data: &[u8]) -> Vec<u8> {
    let byte_rate = sample_rate * channels as u32 * bits_per_sample as u32 / 8;
    let block_align = channels * bits_per_sample / 8;
    let data_size = pcm_data.len() as u32;
    let file_size = 36 + data_size;

    let mut wav = Vec::with_capacity(44 + pcm_data.len());

    // RIFF header
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&file_size.to_le_bytes());
    wav.extend_from_slice(b"WAVE");

    // fmt sub-chunk
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes()); // sub-chunk size
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM format
    wav.extend_from_slice(&channels.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&bits_per_sample.to_le_bytes());

    // data sub-chunk
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_size.to_le_bytes());
    wav.extend_from_slice(pcm_data);

    wav
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_wav_header() {
        // 1 second of silence, mono, 16-bit, 44100 Hz
        let pcm = vec![0u8; 44100 * 2]; // 1 second mono 16-bit
        let wav = build_wav(44100, 1, 16, &pcm);

        // Check RIFF header
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[12..16], b"fmt ");
        assert_eq!(&wav[36..40], b"data");

        // Total size = 44 header + PCM data
        assert_eq!(wav.len(), 44 + pcm.len());
    }
}
