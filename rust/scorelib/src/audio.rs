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

/// Target RMS level for normalization (~-14 dBFS, typical streaming loudness target).
/// RMS normalization keeps perceived loudness consistent regardless of how many
/// accompaniment tracks are active — unlike peak normalization, a single loud drum
/// transient won't cause the whole mix to sound quieter.
const TARGET_RMS: f32 = 0.20;

/// Maximum gain allowed during RMS normalization.
/// Prevents near-silent content from being amplified to an ear-damaging level.
const MAX_GAIN: f32 = 8.0;

/// Hard peak ceiling applied after RMS gain to prevent DAC clipping.
const PEAK_CEILING: f32 = 0.95;

/// Maximum allowed MIDI duration in seconds (safety cap against malformed files).
const MAX_DURATION_SECS: f64 = 3600.0; // 1 hour

/// Render MIDI data to a WAV file using the provided SoundFont.
///
/// Returns a complete WAV file (header + PCM data) as bytes.
/// Format: 44 100 Hz, stereo, 16-bit signed integer, little-endian.
///
/// An extra second of silence is appended so release tails ring out
/// naturally instead of being abruptly cut.
///
/// The output is **peak-normalized** so the loudest sample reaches
/// ~95% of full scale.  This ensures consistent loudness regardless
/// of the SoundFont's internal gain and the piece's dynamics.
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
    if !duration_secs.is_finite() || duration_secs <= 0.0 {
        return Err("Invalid MIDI duration".to_string());
    }
    let duration_secs = duration_secs.min(MAX_DURATION_SECS);
    let total_samples = (SAMPLE_RATE as f64 * duration_secs) as usize;

    // Guard against overflow in buffer allocation.
    let float_cap = total_samples.checked_mul(2)
        .ok_or("Audio buffer size overflow (f32)")?;
    let pcm_cap = total_samples.checked_mul(4)
        .ok_or("Audio buffer size overflow (pcm)")?;

    // Start playback.
    sequencer.play(&midi_file, false);

    // ── Pass 1: Render to f32, computing RMS and peak amplitude ──
    let mut left = vec![0f32; BLOCK_SIZE];
    let mut right = vec![0f32; BLOCK_SIZE];

    // Store interleaved f32 samples for normalization.
    let mut float_buf: Vec<f32> = Vec::with_capacity(float_cap);
    let mut peak: f32 = 0.0;
    let mut sum_sq: f64 = 0.0;
    let mut rendered: usize = 0;

    while rendered < total_samples {
        let count = BLOCK_SIZE.min(total_samples - rendered);
        sequencer.render(&mut left[..count], &mut right[..count]);

        for i in 0..count {
            let l = if left[i].is_finite() { left[i] } else { 0.0 };
            let r = if right[i].is_finite() { right[i] } else { 0.0 };
            float_buf.push(l);
            float_buf.push(r);
            sum_sq += (l * l + r * r) as f64;
            let abs_l = l.abs();
            let abs_r = r.abs();
            if abs_l > peak { peak = abs_l; }
            if abs_r > peak { peak = abs_r; }
        }
        rendered += count;
    }

    // ── Pass 2: RMS-normalize, then clamp peak to ceiling ──
    //
    // RMS normalization targets a consistent perceived loudness.
    // This prevents a single drum transient from dragging the whole mix quiet
    // when accompaniment is added — the problem with pure peak normalization.
    let rms = ((sum_sq / (float_buf.len() as f64)).sqrt()) as f32;
    let rms_gain = if rms > 0.0001 {
        (TARGET_RMS / rms).min(MAX_GAIN)
    } else {
        1.0
    };
    // If the RMS gain would push the peak above the ceiling, scale back just enough.
    let gain = if peak * rms_gain > PEAK_CEILING {
        PEAK_CEILING / peak
    } else {
        rms_gain
    };

    let mut pcm_data: Vec<u8> = Vec::with_capacity(pcm_cap);
    for sample in &float_buf {
        let normalized = (sample * gain * 32767.0).clamp(-32768.0, 32767.0) as i16;
        pcm_data.extend_from_slice(&normalized.to_le_bytes());
    }

    // Drop float buffer to free memory before building WAV.
    drop(float_buf);

    let wav = build_wav(SAMPLE_RATE as u32, 2, 16, &pcm_data)?;

    #[cfg(debug_assertions)]
    eprintln!(
        "[audio] Rendered {:.1}s → {} bytes WAV ({} PCM samples, rms={:.3}, peak={:.3}, gain={:.2}x / {:.1} dB)",
        duration_secs,
        wav.len(),
        rendered,
        rms,
        peak,
        gain,
        20.0 * (gain as f64).log10()
    );

    Ok(wav)
}

/// Build a complete WAV file from raw PCM data.
fn build_wav(sample_rate: u32, channels: u16, bits_per_sample: u16, pcm_data: &[u8]) -> Result<Vec<u8>, String> {
    let byte_rate = sample_rate * channels as u32 * bits_per_sample as u32 / 8;
    let block_align = channels * bits_per_sample / 8;

    let data_size: u32 = pcm_data.len().try_into()
        .map_err(|_| format!("PCM data too large for WAV format: {} bytes (max ~4 GB)", pcm_data.len()))?;
    let file_size = 36u32.checked_add(data_size)
        .ok_or("WAV file size overflow")?;

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

    Ok(wav)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_wav_header() {
        // 1 second of silence, mono, 16-bit, 44100 Hz
        let pcm = vec![0u8; 44100 * 2]; // 1 second mono 16-bit
        let wav = build_wav(44100, 1, 16, &pcm).unwrap();

        // Check RIFF header
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[12..16], b"fmt ");
        assert_eq!(&wav[36..40], b"data");

        // Total size = 44 header + PCM data
        assert_eq!(wav.len(), 44 + pcm.len());
    }
}
