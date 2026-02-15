# Audio Architecture: Offline Rendering & Playback

## Overview

SoloBandUltra renders MusicXML sheet music into playable audio entirely **offline** — no real-time MIDI synthesis. The pipeline runs in a shared Rust core and produces a WAV file that both iOS and Android play back natively.

```
MusicXML → Parse → Transpose → MIDI (SMF Type 1) → SoundFont Synthesis → WAV
```

This approach eliminates real-time scheduling constraints, supports unlimited polyphony, and delivers identical audio quality on both platforms.

---

## Pipeline

### 1. MusicXML Parsing & MIDI Generation (Rust)

The entry point is `render_audio_from_bytes()` in `rust/scorelib/src/lib.rs`:

```rust
pub fn render_audio_from_bytes(
    data: &[u8],
    extension: Option<&str>,
    options: &MidiOptions,
    soundfont_data: &[u8],
) -> Result<Vec<u8>, String>
```

This function chains four steps:
1. **Parse** — `parse_bytes()` reads MusicXML (or compressed `.mxl`) into an internal `Score` structure
2. **Transpose** — `transpose_score()` shifts pitch by the requested number of semitones
3. **Generate MIDI** — `generate_midi_from_score()` produces Standard MIDI File Type 1 bytes
4. **Synthesize** — `audio::render_audio()` renders the MIDI to WAV using a SoundFont

The intermediate MIDI bytes are never exposed to the caller — the function returns finished WAV data.

#### MIDI Options

Track selection and playback options are controlled by `MidiOptions`:

| Field               | Default | Description                              |
|---------------------|---------|------------------------------------------|
| `include_melody`    | `true`  | Include the melody track                 |
| `include_piano`     | `false` | Include piano accompaniment              |
| `include_bass`      | `false` | Include bass accompaniment               |
| `include_strings`   | `false` | Include string accompaniment             |
| `include_drums`     | `false` | Include drum track                       |
| `include_metronome` | `true`  | Include metronome click track            |
| `energy`            | Medium  | Dynamics level: Soft, Medium, or Strong  |
| `transpose`         | `0`     | Semitones to transpose (positive = up)   |

Options are serialized as a JSON string and parsed on the Rust side.

### 2. SoundFont Synthesis (Rust)

The synthesis engine lives in `rust/scorelib/src/audio.rs` and uses the [rustysynth](https://crates.io/crates/rustysynth) crate (v1.3), a pure-Rust SoundFont 2 synthesizer.

#### Audio Format

| Parameter     | Value                          |
|---------------|--------------------------------|
| Sample rate   | 44,100 Hz                      |
| Channels      | 2 (stereo)                     |
| Bit depth     | 16-bit signed integer (PCM)    |
| Byte order    | Little-endian                  |
| Container     | WAV (RIFF, format code 1)      |
| Byte rate     | 176,400 bytes/sec              |
| Block align   | 4 bytes (2 channels x 2 bytes) |

#### Rendering Process

```rust
pub fn render_audio(midi_data: &[u8], soundfont_data: &[u8]) -> Result<Vec<u8>, String>
```

1. Load the SoundFont into `Arc<SoundFont>` and the MIDI data into `Arc<MidiFile>`
2. Create a `Synthesizer` and `MidiFileSequencer` at 44,100 Hz
3. Calculate total duration = MIDI length + 1.0 second (release tail for sustained notes)
4. Render in blocks of **8,192 samples** to keep peak memory low:
   - The sequencer advances the MIDI playhead and feeds events to the synthesizer
   - The synthesizer fills stereo `f32` buffers (left/right)
   - Each block is immediately converted from `f32` → interleaved `i16` and appended to the PCM output
5. Build a WAV file: 44-byte RIFF/WAVE header + PCM data

The block-based rendering avoids allocating a single massive float buffer for the entire piece. Memory usage stays proportional to the output size (PCM `i16` buffer) plus one block of float intermediates.

#### SoundFont

The bundled SoundFont is **GeneralUser GS** (`GeneralUser_GS.sf2`, ~31 MB). It provides General MIDI instrument patches for all accompaniment tracks.

- **iOS**: Bundled in the app's resource bundle, loaded once as a static `Data` property (cached for the app's lifetime)
- **Android**: Stored in `assets/GeneralUser_GS.sf2`, loaded via `context.assets.open()`

### 3. WAV Output

The WAV file is built in-memory with a hand-written RIFF header (no external WAV library needed):

```
Bytes 0–3:    "RIFF"
Bytes 4–7:    File size − 8
Bytes 8–11:   "WAVE"
Bytes 12–15:  "fmt "
Bytes 16–19:  16 (fmt chunk size)
Bytes 20–21:  1 (PCM format)
Bytes 22–23:  2 (channels)
Bytes 24–27:  44100 (sample rate)
Bytes 28–31:  176400 (byte rate)
Bytes 32–33:  4 (block align)
Bytes 34–35:  16 (bits per sample)
Bytes 36–39:  "data"
Bytes 40–43:  PCM data size
Bytes 44+:    Interleaved 16-bit PCM samples (L, R, L, R, …)
```

Typical sizes: ~10 MB per minute of music (stereo 16-bit at 44.1 kHz).

---

## FFI Layer

### iOS (C FFI)

Declared in `ios/SoloBandUltra/include/scorelib.h`:

```c
uint8_t* scorelib_render_audio_from_bytes(
    const uint8_t* data, size_t len,
    const char* extension,
    const char* options_json,
    const uint8_t* sf_data, size_t sf_len,
    size_t* out_len
);
```

Returns a heap-allocated WAV buffer. The caller must free it with:

```c
void scorelib_free_midi(uint8_t* ptr, size_t len);
```

### Android (JNI)

Declared in `rust/scorelib/src/android.rs`:

```rust
pub extern "system" fn Java_com_solobandultra_app_ScoreLib_renderAudio(
    mut env: JNIEnv,
    _class: JClass,
    data: JByteArray,
    extension: JString,
    options_json: JString,
    soundfont_data: JByteArray,
) -> jni::sys::jbyteArray
```

Converts JNI byte arrays to Rust `Vec<u8>`, calls `render_audio_from_bytes`, and returns the result as a JNI `jbyteArray`.

---

## Platform Bridges

### iOS: ScoreLibBridge.swift

```swift
static func renderAudio(_ data: Data, extension ext: String? = nil, optionsJson: String? = nil) -> Data?
```

- Loads the SoundFont **once** from the bundle (lazy static `soundfontData` property)
- Passes both MusicXML and SoundFont bytes to the C FFI via `withUnsafeBytes`
- Returns WAV data as `Data`, or `nil` on error
- Frees the Rust-allocated buffer immediately after copying into `Data`

### Android: ScoreLib.kt

```kotlin
external fun renderAudio(
    data: ByteArray,
    extension: String?,
    optionsJson: String?,
    soundfontData: ByteArray
): ByteArray?
```

Convenience wrappers:
- `renderAudioFromAsset(context, assetPath, soundfontAssetPath, optionsJson)` — loads both files from assets
- `renderAudioFromData(data, ext, soundfontData, optionsJson)` — for pre-loaded bytes

---

## Playback

### iOS: PlaybackManager.swift

**Audio graph:**

```
AVAudioPlayerNode → AVAudioUnitTimePitch → mainMixerNode → output
```

| Component              | Role                                           |
|------------------------|-------------------------------------------------|
| `AVAudioPlayerNode`    | Plays the WAV audio buffer                     |
| `AVAudioUnitTimePitch` | Speed control without pitch change (`.rate`)   |
| `AVAudioEngine`        | Manages the audio graph and output              |

**Key behaviors:**

- **Prepare**: WAV data is written to a temp file (`soloband_playback_<UUID>.wav`), opened as `AVAudioFile`, and scheduled on the player node
- **Speed**: `timePitch.rate` is set to the desired multiplier (0.1–5.0). No pitch distortion.
- **Mute**: `engine.mainMixerNode.outputVolume = 0`. The player keeps running so cursor stays synced.
- **Seek**: Stops the player, recalculates position, and reschedules with `scheduleSegment()`
- **Repeat**: On completion, if repeats remain, reschedules from the beginning after a 150ms gap
- **Position tracking**: Wall-clock based using `CFAbsoluteTimeGetCurrent()`, accounting for current speed
- **End detection**: 4 Hz timer polls the player node's position to detect natural completion
- **Audio session**: `AVAudioSession` configured for `.playback` category (overrides silent switch)

**Cursor synchronization (iOS):**

The WebView drives its own cursor animation via a `requestAnimationFrame` loop. Swift sends only two commands:
- `startCursorAnimation(ms, speed)` — begins the JS animation loop
- `stopCursorAnimation(ms)` — halts and freezes the cursor

Zero IPC during active playback — the JS loop interpolates position using its own timestamps.

### Android: PlaybackManager.kt

Uses `MediaPlayer` with `PlaybackParams` for speed control.

| Component        | Role                                            |
|------------------|-------------------------------------------------|
| `MediaPlayer`    | Plays the WAV temp file                         |
| `PlaybackParams` | Speed control (`.setSpeed()` + `.setPitch(1.0)`) |
| `Choreographer`  | Frame-accurate cursor position updates          |

**Key behaviors:**

- **Prepare**: WAV bytes written to a temp file in `context.cacheDir`, loaded into `MediaPlayer`
- **Speed**: `PlaybackParams().setSpeed(speed).setPitch(1.0f)` — time-stretch without pitch change
- **Mute**: `player.setVolume(0f, 0f)`
- **Seek**: `player.seekTo(ms)` — `currentPosition` reports music time directly
- **Repeat**: On completion callback, seeks to 0 and restarts if repeats remain
- **Audio session**: `AudioSessionManager` configures `USAGE_MEDIA` + `CONTENT_TYPE_MUSIC` (plays through media stream, independent of ringer)

**Cursor synchronization (Android):**

Uses `Choreographer.FrameCallback` (vsync-aligned) to call `evaluateJavascript("moveCursor($timeMs)")` into the WebView every frame.

---

## UI Integration

### iOS: SheetMusicView.swift

The `loadScore(width:)` method runs on a background queue and makes three Rust calls:

```swift
let svg   = ScoreLib.renderData(data, extension: ext, pageWidth: pw, transpose: t)
let pmap  = ScoreLib.playbackMap(data, extension: ext, pageWidth: pw, transpose: t)
let audio = ScoreLib.renderAudio(data, extension: ext, optionsJson: json)
```

On the main thread, calls `playbackManager.prepareAudio(wavData)`.

When track toggles change (melody, piano, bass, etc.), only audio is re-rendered — the SVG and playback map stay the same. A **generation counter** (`midiGeneration`) discards stale results from concurrent background renders.

### Android: SheetMusicScreen.kt

The `loadScore()` function dispatches to `Dispatchers.IO`:

```kotlin
val sfBytes = context.assets.open("GeneralUser_GS.sf2").use { it.readBytes() }
val svg   = ScoreLib.renderData(bytes, ext, pageWidth, transpose)
val pmap  = ScoreLib.playbackMapFromData(bytes, ext, pageWidth, transpose)
val audio = ScoreLib.renderAudioFromData(bytes, ext, sfBytes, optionsJson)
```

A separate `LaunchedEffect(optionsJson)` block regenerates only audio when accompaniment settings change. Uses the same generation counter pattern as iOS.

---

## Dependencies

### Rust (`rust/scorelib/Cargo.toml`)

| Crate        | Version | Purpose                              |
|--------------|---------|--------------------------------------|
| `rustysynth` | 1.3     | SoundFont 2 MIDI synthesizer        |
| `roxmltree`  | —       | MusicXML parsing                     |
| `zip`        | —       | .mxl decompression                   |
| `serde`      | —       | Serialization                        |
| `serde_json` | —       | JSON for playback map output         |
| `jni`        | 0.21    | Android JNI bridge                   |

### iOS

- `AVAudioEngine`, `AVAudioPlayerNode`, `AVAudioUnitTimePitch` (AVFoundation)
- `AVAudioSession` (playback category, silent switch override)

### Android

- `MediaPlayer` with `PlaybackParams` (android.media)
- `Choreographer` (android.view, cursor sync)
- `AudioManager` / `AudioFocusRequest` (audio session)

### Shared Asset

- `GeneralUser_GS.sf2` (~31 MB) — General MIDI SoundFont bundled on both platforms

---

## File Map

```
rust/scorelib/
├── Cargo.toml                 # Crate config, rustysynth dependency
├── src/
│   ├── lib.rs                 # Public API: render_audio_from_bytes(), C FFI
│   ├── audio.rs               # SoundFont synthesis: render_audio()
│   ├── midi.rs                # MIDI generation, MidiOptions
│   └── android.rs             # JNI bindings for Android

ios/SoloBandUltra/
├── include/
│   └── scorelib.h             # C FFI header
├── SoloBandUltra/
│   ├── ScoreLibBridge.swift   # Swift ↔ C bridge, SoundFont caching
│   ├── PlaybackManager.swift  # AVAudioEngine playback
│   ├── SheetMusicView.swift   # UI integration, loadScore()
│   └── GeneralUser_GS.sf2    # Bundled SoundFont

android/app/src/main/
├── assets/
│   ├── GeneralUser_GS.sf2    # Bundled SoundFont
│   └── sheetmusic/            # Sample MusicXML files
├── java/com/solobandultra/app/
│   ├── ScoreLib.kt            # JNI bridge + convenience wrappers
│   ├── audio/
│   │   ├── PlaybackManager.kt      # MediaPlayer playback
│   │   └── AudioSessionManager.kt  # Audio focus & session
│   └── ui/screens/
│       └── SheetMusicScreen.kt     # UI integration, loadScore()
```

---

## Performance Characteristics

| Metric                | Typical Value                           |
|-----------------------|-----------------------------------------|
| Synthesis time        | 1–5 seconds (depending on piece length) |
| Peak memory (render)  | ~200 MB (SoundFont + MIDI + PCM buffer) |
| Steady-state memory   | ~115 MB (WAV in player, buffers freed)  |
| CPU during playback   | ~5% (just streaming PCM samples)        |
| WAV size              | ~10 MB per minute of music              |
| Speed control range   | 0.1x – 5.0x (no pitch distortion)       |
| Max polyphony         | Unlimited (offline, no real-time limit)  |

Tested with Chopin Trois Valses (complex, fast, high polyphony) — zero dropped notes at all speed levels including 4x.
