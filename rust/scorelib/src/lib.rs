//! scorelib — MusicXML parser and score rendering library for SoloBand Ultra.
//!
//! Supports both uncompressed MusicXML (.musicxml) and compressed MXL (.mxl) files.
//!
//! # Example
//! ```no_run
//! use scorelib::parse_file;
//!
//! let score = parse_file("path/to/score.musicxml").unwrap();
//! println!("Title: {:?}", score.title);
//! println!("Parts: {}", score.parts.len());
//! println!("Measures: {}", score.measure_count());
//! ```

pub mod model;
pub mod mxl;
pub mod parser;
pub mod renderer;
pub mod unroller;
pub mod timemap;
pub mod midi;
pub mod accompaniment;
pub mod playback;
pub mod audio;
pub mod note_timeline;
pub mod top_layer;
pub mod feedback_overlay;

#[cfg(target_os = "android")]
pub mod android;

use std::path::Path;

pub use model::*;
pub use parser::parse_musicxml;
pub use mxl::parse_mxl;
pub use renderer::render_score_to_svg;
pub use midi::{generate_midi, discover_global_tracks, GlobalTrack, MidiOptions, Energy};
pub use unroller::unroll;
pub use timemap::generate_timemap;
pub use playback::{generate_playback_map, PlaybackMap};
pub use feedback_overlay::add_feedback_overlay_to_svg;

// ═══════════════════════════════════════════════════════════════════════
// Score transposition
// ═══════════════════════════════════════════════════════════════════════

/// Transpose all pitches, key signatures, and harmony symbols in a score
/// by the given number of semitones.  Positive = up, negative = down.
///
/// This modifies the `Score` in-place so that both rendering and MIDI
/// generation produce transposed output.
pub fn transpose_score(score: &mut Score, semitones: i32) {
    if semitones == 0 {
        return;
    }

    for part in &mut score.parts {
        let mut current_fifths: i32 = 0; // running key context (C major default)

        for measure in &mut part.measures {
            // --- Transpose key signature if present ---
            if let Some(ref mut attrs) = measure.attributes {
                if let Some(ref mut key) = attrs.key {
                    let old_root = (key.fifths * 7).rem_euclid(12);
                    let new_root = (old_root + semitones).rem_euclid(12);
                    key.fifths = semitone_to_fifths(new_root);
                    current_fifths = key.fifths;
                }
            }

            let use_sharps = current_fifths >= 0;

            // --- Transpose note pitches ---
            for note in &mut measure.notes {
                if let Some(ref mut pitch) = note.pitch {
                    let midi = pitch.to_midi() + semitones;
                    let octave = midi.div_euclid(12) - 1;
                    let pc = midi.rem_euclid(12);
                    let (step, alter) = semitone_to_note(pc, use_sharps);
                    pitch.step = step.to_string();
                    pitch.alter = if alter != 0.0 { Some(alter) } else { None };
                    pitch.octave = octave;
                }
            }

            // --- Transpose harmony roots and bass notes ---
            for harmony in &mut measure.harmonies {
                transpose_harmony_root(&mut harmony.root, semitones, use_sharps);
                if let Some(ref mut bass) = harmony.bass {
                    transpose_harmony_root(bass, semitones, use_sharps);
                }
            }
        }
    }
}

/// Map a semitone (0–11) to the simplest key-signature fifths value.
fn semitone_to_fifths(semi: i32) -> i32 {
    match semi.rem_euclid(12) {
        0  =>  0,  // C
        1  => -5,  // Db
        2  =>  2,  // D
        3  => -3,  // Eb
        4  =>  4,  // E
        5  => -1,  // F
        6  => -6,  // Gb
        7  =>  1,  // G
        8  => -4,  // Ab
        9  =>  3,  // A
        10 => -2,  // Bb
        11 =>  5,  // B
        _  =>  0,
    }
}

/// Convert a pitch-class semitone (0–11) to (step, alter) using sharp or flat spelling.
fn semitone_to_note(pc: i32, use_sharps: bool) -> (&'static str, f64) {
    let pc = pc.rem_euclid(12);
    if use_sharps {
        match pc {
            0  => ("C", 0.0),
            1  => ("C", 1.0),
            2  => ("D", 0.0),
            3  => ("D", 1.0),
            4  => ("E", 0.0),
            5  => ("F", 0.0),
            6  => ("F", 1.0),
            7  => ("G", 0.0),
            8  => ("G", 1.0),
            9  => ("A", 0.0),
            10 => ("A", 1.0),
            11 => ("B", 0.0),
            _  => ("C", 0.0),
        }
    } else {
        match pc {
            0  => ("C", 0.0),
            1  => ("D",-1.0),
            2  => ("D", 0.0),
            3  => ("E",-1.0),
            4  => ("E", 0.0),
            5  => ("F", 0.0),
            6  => ("G",-1.0),
            7  => ("G", 0.0),
            8  => ("A",-1.0),
            9  => ("A", 0.0),
            10 => ("B",-1.0),
            11 => ("B", 0.0),
            _  => ("C", 0.0),
        }
    }
}

/// Transpose a harmony root or bass note in-place.
fn transpose_harmony_root(root: &mut model::HarmonyRoot, semitones: i32, use_sharps: bool) {
    let step_semi = match root.step.as_str() {
        "C" => 0, "D" => 2, "E" => 4, "F" => 5,
        "G" => 7, "A" => 9, "B" => 11,
        _ => 0,
    };
    let alter = root.alter.unwrap_or(0.0) as i32;
    let old_pc = (step_semi + alter).rem_euclid(12);
    let new_pc = (old_pc + semitones).rem_euclid(12);
    let (step, alter_f) = semitone_to_note(new_pc, use_sharps);
    root.step = step.to_string();
    root.alter = if alter_f != 0.0 { Some(alter_f) } else { None };
}

// ═══════════════════════════════════════════════════════════════════════
// Parsing
// ═══════════════════════════════════════════════════════════════════════

/// Parse a MusicXML file from a file path.
/// Automatically detects format based on file extension:
/// - `.musicxml` or `.xml` → uncompressed MusicXML
/// - `.mxl` → compressed MXL (ZIP archive)
pub fn parse_file<P: AsRef<Path>>(path: P) -> Result<Score, String> {
    let path = path.as_ref();
    let data = std::fs::read(path)
        .map_err(|e| format!("Failed to read file '{}': {e}", path.display()))?;

    parse_bytes(&data, path.extension().and_then(|e| e.to_str()))
}

/// Parse MusicXML from raw bytes with an optional format hint.
/// If `extension` is None, tries to auto-detect the format.
pub fn parse_bytes(data: &[u8], extension: Option<&str>) -> Result<Score, String> {
    match extension {
        Some("mxl") => parse_mxl(data),
        Some("musicxml") | Some("xml") => {
            let xml = std::str::from_utf8(data)
                .map_err(|e| format!("Invalid UTF-8 in MusicXML file: {e}"))?;
            parse_musicxml(xml)
        }
        _ => {
            // Auto-detect: try as XML first, then as MXL
            if let Ok(xml) = std::str::from_utf8(data) {
                if xml.trim_start().starts_with("<?xml") || xml.trim_start().starts_with('<') {
                    return parse_musicxml(xml);
                }
            }
            // Try as MXL (ZIP)
            parse_mxl(data)
        }
    }
}

/// Convert a parsed score to a JSON string.
/// Useful for passing data across FFI boundaries.
pub fn score_to_json(score: &Score) -> Result<String, String> {
    serde_json::to_string_pretty(score).map_err(|e| format!("JSON serialization error: {e}"))
}

/// Parse a MusicXML file and render it directly to SVG.
/// Convenience function combining parsing and rendering.
///
/// `page_width` sets the SVG width in user units. Pass `None` to use the
/// default (820). On phones, pass the screen width in points so the renderer
/// fits fewer measures per system and keeps notes readable.
///
/// `transpose` shifts all pitches by the given number of semitones (0 = no change).
///
/// `staff_indices_1based` limits which staves are drawn by global staff index (1 = first staff, 2 = second, etc.). Pass `None` for all.
/// `use_jianpu` when true renders in Jianpu (numbered notation); exactly one staff is used.
pub fn render_file_to_svg<P: AsRef<std::path::Path>>(
    path: P,
    page_width: Option<f64>,
    transpose: i32,
    staff_indices_1based: Option<&[usize]>,
    use_jianpu: bool,
) -> Result<String, String> {
    let mut score = parse_file(path)?;
    transpose_score(&mut score, transpose);
    Ok(render_score_to_svg(&score, page_width, staff_indices_1based, use_jianpu, transpose))
}

/// Parse MusicXML bytes and render to SVG.
///
/// `page_width` sets the SVG width in user units. Pass `None` to use the
/// default (820).
///
/// `transpose` shifts all pitches by the given number of semitones (0 = no change).
///
/// `staff_indices_1based` limits which staves are drawn by global staff index (1 = first staff, 2 = second, etc.). Pass `None` for all.
/// `use_jianpu` when true renders in Jianpu (numbered notation); exactly one staff is used.
pub fn render_bytes_to_svg(
    data: &[u8],
    extension: Option<&str>,
    page_width: Option<f64>,
    transpose: i32,
    staff_indices_1based: Option<&[usize]>,
    use_jianpu: bool,
) -> Result<String, String> {
    let mut score = parse_bytes(data, extension)?;
    transpose_score(&mut score, transpose);
    Ok(render_score_to_svg(&score, page_width, staff_indices_1based, use_jianpu, transpose))
}

/// Generate MIDI bytes from a parsed score.
///
/// Unrolls repeats/jumps, computes the timemap, extracts melody and
/// optionally generates accompaniment tracks.  Returns a Standard MIDI
/// File (SMF Type 1) as raw bytes.
pub fn generate_midi_from_score(score: &Score, options: &MidiOptions) -> Vec<u8> {
    if score.parts.is_empty() {
        return Vec::new();
    }
    let part_idx = 0; // melody from first part
    let unrolled = unroll(score, part_idx);
    let tmap = generate_timemap(score, part_idx, &unrolled);
    generate_midi(score, part_idx, &unrolled, &tmap, options)
}

/// Parse a MusicXML file and generate MIDI bytes.
pub fn generate_midi_from_file<P: AsRef<Path>>(
    path: P,
    options: &MidiOptions,
) -> Result<Vec<u8>, String> {
    let mut score = parse_file(path)?;
    transpose_score(&mut score, options.transpose);
    Ok(generate_midi_from_score(&score, options))
}

/// Parse MusicXML bytes and generate MIDI bytes.
pub fn generate_midi_from_bytes(
    data: &[u8],
    extension: Option<&str>,
    options: &MidiOptions,
) -> Result<Vec<u8>, String> {
    let mut score = parse_bytes(data, extension)?;
    transpose_score(&mut score, options.transpose);
    Ok(generate_midi_from_score(&score, options))
}

/// Parse MusicXML bytes, generate MIDI internally, and render to WAV audio
/// using the provided SoundFont.
///
/// Returns a complete WAV file (48 000 Hz, stereo, 16-bit) as raw bytes.
/// The MIDI is an internal intermediate — it is never returned to the caller.
pub fn render_audio_from_bytes(
    data: &[u8],
    extension: Option<&str>,
    options: &MidiOptions,
    soundfont_data: &[u8],
) -> Result<Vec<u8>, String> {
    let mut score = parse_bytes(data, extension)?;
    transpose_score(&mut score, options.transpose);
    let midi_bytes = generate_midi_from_score(&score, options);
    audio::render_audio(&midi_bytes, soundfont_data)
}

/// Add the feedback overlay layer (colored dots) to an existing score SVG.
///
/// `overlay_dots_json` is a JSON array of `{ "x", "y", "colors": string[] }`
/// in SVG coordinates. Returns the SVG with a `<g id="feedback-overlay">` inserted
/// before the closing `</svg>`. Used for the performance report.
pub fn add_feedback_overlay(svg: &str, overlay_dots_json: &str) -> Result<String, String> {
    add_feedback_overlay_to_svg(svg, overlay_dots_json)
}

/// Generate a playback map from a parsed score (JSON string).
///
/// The playback map contains measure positions, system positions and the
/// timemap — everything the WebView needs to animate a playback cursor.
/// Pass `use_jianpu: true` when the score is rendered in jianpu so cursor positions match.
/// When jianpu is true, the score is simplified to top layer only so note_positions align.
pub fn playback_map_from_score(
    score: &Score,
    page_width: Option<f64>,
    staff_indices_1based: Option<&[usize]>,
    use_jianpu: bool,
) -> String {
    let map = if use_jianpu {
        let simplified = renderer::simplify_score_for_jianpu(score, staff_indices_1based);
        generate_playback_map(&simplified, page_width, staff_indices_1based, use_jianpu)
    } else {
        generate_playback_map(score, page_width, staff_indices_1based, use_jianpu)
    };
    playback::playback_map_to_json(&map)
}

/// Parse MusicXML bytes and return a playback map JSON string.
///
/// `transpose` shifts all pitches by the given number of semitones (0 = no change).
/// This must match the transpose used for SVG rendering so positions are consistent.
/// Pass the same staff filter and `use_jianpu` as used for SVG so cursor matches.
pub fn playback_map_from_bytes(
    data: &[u8],
    extension: Option<&str>,
    page_width: Option<f64>,
    transpose: i32,
    staff_indices_1based: Option<&[usize]>,
    use_jianpu: bool,
) -> Result<String, String> {
    let mut score = parse_bytes(data, extension)?;
    transpose_score(&mut score, transpose);
    Ok(playback_map_from_score(&score, page_width, staff_indices_1based, use_jianpu))
}

// ═══════════════════════════════════════════════════════════════════════
// C FFI — for iOS (static library) and Android (JNI)
// ═══════════════════════════════════════════════════════════════════════

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

/// Parse "1,3,5" (comma-separated 1-based global staff indices) into a Vec<usize>.
/// Returns None if the string is null/empty or invalid (no valid numbers).
/// Used by FFI to pass staff filter for SVG rendering (1 = first staff, 2 = second, etc.).
pub fn parse_parts_filter(s: Option<&str>) -> Option<Vec<usize>> {
    let s = s?.trim();
    if s.is_empty() {
        return None;
    }
    let parts: Vec<usize> = s
        .split(',')
        .map(|p| p.trim().parse::<usize>().ok())
        .collect::<Option<Vec<_>>>()?;
    if parts.is_empty() {
        None
    } else {
        Some(parts)
    }
}

/// Convert a Rust string to a C string for FFI, stripping null bytes so CString::new never fails.
fn string_to_c_string(s: String) -> CString {
    let sanitized = s.replace('\0', "");
    CString::new(sanitized).unwrap_or_default()
}

/// Parse a MusicXML file and return SVG as a C string.
/// The caller must free the returned string with `scorelib_free_string`.
///
/// `page_width` sets the SVG width in user units. Pass 0.0 to use the default.
///
/// `parts_filter` optional comma-separated 1-based part indices (e.g. "1,3,5"). Pass null for all parts.
/// `use_jianpu` 1 = render in Jianpu (numbered notation), 0 = staff notation.
///
/// # Safety
/// `path` must be a valid null-terminated UTF-8 C string. `parts_filter` may be null.
#[no_mangle]
pub unsafe extern "C" fn scorelib_render_file(
    path: *const c_char,
    page_width: f64,
    transpose: i32,
    parts_filter: *const c_char,
    use_jianpu: i32,
) -> *mut c_char {
    if path.is_null() {
        return std::ptr::null_mut();
    }
    let c_str = unsafe { CStr::from_ptr(path) };
    let path_str = match c_str.to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };

    let pw = if page_width > 0.0 { Some(page_width) } else { None };
    let part_indices = if parts_filter.is_null() {
        None
    } else {
        unsafe { CStr::from_ptr(parts_filter) }.to_str().ok().and_then(|s| parse_parts_filter(Some(s)))
    };
    let parts_ref = part_indices.as_deref();
    let jianpu = use_jianpu != 0;

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        render_file_to_svg(path_str, pw, transpose, parts_ref, jianpu)
    }));

    match result {
        Ok(Ok(svg)) => string_to_c_string(svg).into_raw(),
        _ => std::ptr::null_mut(),
    }
}

/// Parse MusicXML bytes and return SVG as a C string.
/// The caller must free the returned string with `scorelib_free_string`.
///
/// `page_width` sets the SVG width in user units. Pass 0.0 to use the default.
///
/// `parts_filter` optional comma-separated 1-based part indices (e.g. "1,3,5"). Pass null for all parts.
/// `use_jianpu` 1 = render in Jianpu (numbered notation), 0 = staff notation.
///
/// # Safety
/// `data` must point to `len` valid bytes. `extension` and `parts_filter` may be null.
#[no_mangle]
pub unsafe extern "C" fn scorelib_render_bytes(
    data: *const u8,
    len: usize,
    extension: *const c_char,
    page_width: f64,
    transpose: i32,
    parts_filter: *const c_char,
    use_jianpu: i32,
) -> *mut c_char {
    if data.is_null() || len == 0 {
        return std::ptr::null_mut();
    }
    let bytes = unsafe { std::slice::from_raw_parts(data, len) };
    let ext = if extension.is_null() {
        None
    } else {
        unsafe { CStr::from_ptr(extension) }.to_str().ok()
    };

    let pw = if page_width > 0.0 { Some(page_width) } else { None };
    let part_indices = if parts_filter.is_null() {
        None
    } else {
        unsafe { CStr::from_ptr(parts_filter) }.to_str().ok().and_then(|s| parse_parts_filter(Some(s)))
    };
    let parts_ref = part_indices.as_deref();
    let jianpu = use_jianpu != 0;

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        render_bytes_to_svg(bytes, ext, pw, transpose, parts_ref, jianpu)
    }));

    match result {
        Ok(Ok(svg)) => string_to_c_string(svg).into_raw(),
        _ => std::ptr::null_mut(),
    }
}

/// Free a string previously returned by scorelib functions.
///
/// # Safety
/// `ptr` must be a string previously returned by a scorelib function, or null.
#[no_mangle]
pub unsafe extern "C" fn scorelib_free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        unsafe {
            let _ = CString::from_raw(ptr);
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// MIDI generation FFI
// ═══════════════════════════════════════════════════════════════════════

/// Generate MIDI bytes from a MusicXML file.
///
/// Returns a pointer to the MIDI data and writes the length to `out_len`.
/// The caller must free the returned buffer with `scorelib_free_midi`.
/// Returns null on error.
///
/// `options_json` is a JSON string with fields:
///   `include_melody`, `include_piano`, `include_bass`, `include_strings`,
///   `include_drums`, `include_metronome`, `energy` ("soft"/"medium"/"strong"),
///   `transpose`, `melody_tracks` (comma-separated global track numbers, e.g.
///   "1,3" — top-to-bottom lines across all parts/staves; omitted = all;
///   present but empty = no melody lines). Legacy `melody_track`:N is also accepted.
/// Pass null to use defaults.
///
/// # Safety
/// `path` must be a valid null-terminated UTF-8 C string.
/// `out_len` must point to valid writable memory.
#[no_mangle]
pub unsafe extern "C" fn scorelib_generate_midi(
    path: *const c_char,
    options_json: *const c_char,
    out_len: *mut usize,
) -> *mut u8 {
    if path.is_null() || out_len.is_null() {
        return std::ptr::null_mut();
    }
    let c_str = unsafe { CStr::from_ptr(path) };
    let path_str = match c_str.to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };

    let options = parse_midi_options_json(options_json);

    unsafe { *out_len = 0; }

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        generate_midi_from_file(path_str, &options)
    }));

    match result {
        Ok(Ok(midi_bytes)) if !midi_bytes.is_empty() => {
            let boxed = midi_bytes.into_boxed_slice();
            let len = boxed.len();
            let ptr = Box::into_raw(boxed) as *mut u8;
            unsafe { *out_len = len; }
            ptr
        }
        _ => std::ptr::null_mut(),
    }
}

/// Free a buffer previously returned by a scorelib FFI function.
///
/// # Safety
/// `ptr` must be a buffer previously returned by a scorelib function,
/// or null. `len` must be the length returned via `out_len`.
#[no_mangle]
pub unsafe extern "C" fn scorelib_free_midi(ptr: *mut u8, len: usize) {
    if !ptr.is_null() && len > 0 {
        unsafe {
            // Reconstruct the Box<[u8]> that was leaked via Box::into_raw.
            let slice = std::slice::from_raw_parts_mut(ptr, len);
            let _ = Box::from_raw(slice as *mut [u8]);
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Playback map FFI
// ═══════════════════════════════════════════════════════════════════════

/// Generate a playback map JSON string from MusicXML bytes.
///
/// The caller must free the returned string with `scorelib_free_string`.
///
/// `page_width` sets the SVG width in user units. Pass 0.0 to use the default.
/// `parts_filter` must match the filter used for SVG rendering (e.g. "1,3" for staves 1 and 3).
/// `use_jianpu` 1 = use jianpu layout so cursor matches jianpu SVG; 0 = staff layout.
///
/// # Safety
/// `data` must point to `len` valid bytes. `extension` and `parts_filter` may be null.
#[no_mangle]
pub unsafe extern "C" fn scorelib_playback_map(
    data: *const u8,
    len: usize,
    extension: *const c_char,
    page_width: f64,
    transpose: i32,
    parts_filter: *const c_char,
    use_jianpu: i32,
) -> *mut c_char {
    if data.is_null() || len == 0 {
        return std::ptr::null_mut();
    }
    let bytes = unsafe { std::slice::from_raw_parts(data, len) };
    let ext = if extension.is_null() {
        None
    } else {
        unsafe { CStr::from_ptr(extension) }.to_str().ok()
    };
    let staff_indices = if parts_filter.is_null() {
        None
    } else {
        unsafe { CStr::from_ptr(parts_filter) }
            .to_str()
            .ok()
            .and_then(|s| parse_parts_filter(Some(s)))
    };
    let staff_ref = staff_indices.as_deref();

    let pw = if page_width > 0.0 { Some(page_width) } else { None };
    let jianpu = use_jianpu != 0;

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        playback_map_from_bytes(bytes, ext, pw, transpose, staff_ref, jianpu)
    }));

    match result {
        Ok(Ok(json)) => string_to_c_string(json).into_raw(),
        _ => std::ptr::null_mut(),
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Feedback overlay FFI
// ═══════════════════════════════════════════════════════════════════════

/// Add the feedback overlay layer to a score SVG string.
///
/// `svg` and `overlay_dots_json` must be null-terminated UTF-8 strings.
/// `overlay_dots_json` is a JSON array of { "x", "y", "colors": string[] }.
/// Returns a new SVG string with the overlay inserted, or NULL on error.
/// The caller must free the result with scorelib_free_string.
///
/// # Safety
/// `svg` and `overlay_dots_json` must point to valid null-terminated UTF-8 strings.
#[no_mangle]
pub unsafe extern "C" fn scorelib_add_feedback_overlay(
    svg: *const c_char,
    overlay_dots_json: *const c_char,
) -> *mut c_char {
    if svg.is_null() || overlay_dots_json.is_null() {
        return std::ptr::null_mut();
    }
    let svg_str = match unsafe { CStr::from_ptr(svg) }.to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let dots_str = match unsafe { CStr::from_ptr(overlay_dots_json) }.to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        add_feedback_overlay_to_svg(svg_str, dots_str)
    }));
    match result {
        Ok(Ok(out)) => string_to_c_string(out).into_raw(),
        _ => std::ptr::null_mut(),
    }
}

/// Generate MIDI bytes from MusicXML bytes.
///
/// Returns a pointer to the MIDI data and writes the length to `out_len`.
/// The caller must free the returned buffer with `scorelib_free_midi`.
///
/// # Safety
/// `data` must point to `len` valid bytes. `extension` may be null.
/// `out_len` must point to valid writable memory.
#[no_mangle]
pub unsafe extern "C" fn scorelib_generate_midi_from_bytes(
    data: *const u8,
    len: usize,
    extension: *const c_char,
    options_json: *const c_char,
    out_len: *mut usize,
) -> *mut u8 {
    if data.is_null() || len == 0 || out_len.is_null() {
        return std::ptr::null_mut();
    }
    let bytes = unsafe { std::slice::from_raw_parts(data, len) };
    let ext = if extension.is_null() {
        None
    } else {
        unsafe { CStr::from_ptr(extension) }.to_str().ok()
    };

    let options = parse_midi_options_json(options_json);

    unsafe { *out_len = 0; }

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        generate_midi_from_bytes(bytes, ext, &options)
    }));

    match result {
        Ok(Ok(midi_bytes)) if !midi_bytes.is_empty() => {
            let boxed = midi_bytes.into_boxed_slice();
            let len = boxed.len();
            let ptr = Box::into_raw(boxed) as *mut u8;
            unsafe { *out_len = len; }
            ptr
        }
        _ => std::ptr::null_mut(),
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Audio rendering FFI
// ═══════════════════════════════════════════════════════════════════════

/// Render MusicXML bytes to WAV audio using a SoundFont.
///
/// Internally generates MIDI and synthesizes it offline via rustysynth.
/// Returns a pointer to WAV data and writes the length to `out_len`.
/// The caller must free the returned buffer with `scorelib_free_midi`.
///
/// # Safety
/// `data` must point to `len` valid bytes. `sf_data` must point to `sf_len`
/// valid bytes. `extension` and `options_json` may be null.
/// `out_len` must point to valid writable memory.
#[no_mangle]
pub unsafe extern "C" fn scorelib_render_audio_from_bytes(
    data: *const u8,
    len: usize,
    extension: *const c_char,
    options_json: *const c_char,
    sf_data: *const u8,
    sf_len: usize,
    out_len: *mut usize,
) -> *mut u8 {
    if data.is_null() || len == 0 || sf_data.is_null() || sf_len == 0 || out_len.is_null() {
        return std::ptr::null_mut();
    }
    let bytes = unsafe { std::slice::from_raw_parts(data, len) };
    let sf_bytes = unsafe { std::slice::from_raw_parts(sf_data, sf_len) };
    let ext = if extension.is_null() {
        None
    } else {
        unsafe { CStr::from_ptr(extension) }.to_str().ok()
    };

    let options = parse_midi_options_json(options_json);

    unsafe { *out_len = 0; }

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        render_audio_from_bytes(bytes, ext, &options, sf_bytes)
    }));

    match result {
        Ok(Ok(wav_bytes)) if !wav_bytes.is_empty() => {
            let boxed = wav_bytes.into_boxed_slice();
            let len = boxed.len();
            let ptr = Box::into_raw(boxed) as *mut u8;
            unsafe { *out_len = len; }
            ptr
        }
        Ok(Err(e)) => {
            eprintln!("[scorelib FFI] render_audio error: {e}");
            std::ptr::null_mut()
        }
        _ => std::ptr::null_mut(),
    }
}

/// Parse MidiOptions from a JSON string.
///
/// Simple string-matching parser (no serde_json::Value overhead).
/// Handles both compact (`"key":value`) and spaced (`"key": value`) JSON.
/// Used by both the C FFI layer and the Android JNI layer.
pub fn parse_midi_options_from_json_str(json_str: &str) -> MidiOptions {
    let mut opts = MidiOptions::default();
    if json_str.contains("\"include_melody\":false") || json_str.contains("\"include_melody\": false") {
        opts.include_melody = false;
    }
    if json_str.contains("\"include_piano\":true") || json_str.contains("\"include_piano\": true") {
        opts.include_piano = true;
    }
    if json_str.contains("\"include_bass\":true") || json_str.contains("\"include_bass\": true") {
        opts.include_bass = true;
    }
    if json_str.contains("\"include_strings\":true") || json_str.contains("\"include_strings\": true") {
        opts.include_strings = true;
    }
    if json_str.contains("\"include_drums\":true") || json_str.contains("\"include_drums\": true") {
        opts.include_drums = true;
    }
    if json_str.contains("\"include_metronome\":false") || json_str.contains("\"include_metronome\": false") {
        opts.include_metronome = false;
    }
    if json_str.contains("\"energy\":\"soft\"") || json_str.contains("\"energy\": \"soft\"") {
        opts.energy = Energy::Soft;
    }
    if json_str.contains("\"energy\":\"strong\"") || json_str.contains("\"energy\": \"strong\"") {
        opts.energy = Energy::Strong;
    }
    // Parse "transpose":N — extract the integer value after the key
    if let Some(pos) = json_str.find("\"transpose\":") {
        let after = &json_str[pos + "\"transpose\":".len()..];
        let num_str: String = after.trim().chars()
            .take_while(|c| *c == '-' || c.is_ascii_digit())
            .collect();
        if let Ok(val) = num_str.parse::<i32>() {
            opts.transpose = val;
        }
    } else if let Some(pos) = json_str.find("\"transpose\": ") {
        let after = &json_str[pos + "\"transpose\": ".len()..];
        let num_str: String = after.trim().chars()
            .take_while(|c| *c == '-' || c.is_ascii_digit())
            .collect();
        if let Ok(val) = num_str.parse::<i32>() {
            opts.transpose = val;
        }
    }
    // Parse "melody_tracks":"1,2,3" — comma-separated global track numbers.
    // Key present with empty string → Some([]) (no melody). Key absent → None (all).
    if let Some(tracks) = extract_json_string_value(json_str, "melody_tracks") {
        if tracks.trim().is_empty() {
            opts.melody_tracks = Some(vec![]);
        } else {
            opts.melody_tracks = parse_melody_tracks_filter(Some(&tracks));
        }
    } else if let Some(pos) = json_str.find("\"melody_track\":") {
        // Legacy single-track form: "melody_track":N
        let after = &json_str[pos + "\"melody_track\":".len()..];
        let num_str: String = after.trim().chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        if let Ok(val) = num_str.parse::<i32>() {
            if val >= 1 {
                opts.melody_tracks = Some(vec![val]);
            }
        }
    }
    opts
}

/// Extract a JSON string value for `key` from a compact/spaced object
/// (`"key":"value"` or `"key": "value"`). Returns None if absent/malformed.
fn extract_json_string_value(json_str: &str, key: &str) -> Option<String> {
    let patterns = [
        format!("\"{key}\":\""),
        format!("\"{key}\": \""),
    ];
    for pat in &patterns {
        if let Some(pos) = json_str.find(pat) {
            let after = &json_str[pos + pat.len()..];
            let end = after.find('"')?;
            return Some(after[..end].to_string());
        }
    }
    None
}

/// Parse "1,3" (comma-separated global track numbers) into a Vec.
/// Returns None if empty/invalid. Accepts any positive integers.
pub fn parse_melody_tracks_filter(s: Option<&str>) -> Option<Vec<i32>> {
    let s = s?.trim();
    if s.is_empty() {
        return None;
    }
    let mut tracks: Vec<i32> = Vec::new();
    for part in s.split(',') {
        let t = part.trim();
        if t.is_empty() {
            continue;
        }
        let n = t.parse::<i32>().ok()?;
        if n < 1 {
            return None;
        }
        if !tracks.contains(&n) {
            tracks.push(n);
        }
    }
    if tracks.is_empty() {
        None
    } else {
        Some(tracks)
    }
}

/// Parse MidiOptions from a JSON C string (C FFI helper).
unsafe fn parse_midi_options_json(json_ptr: *const c_char) -> MidiOptions {
    if json_ptr.is_null() {
        return MidiOptions::default();
    }
    let c_str = unsafe { CStr::from_ptr(json_ptr) };
    match c_str.to_str() {
        Ok(s) => parse_midi_options_from_json_str(s),
        Err(_) => MidiOptions::default(),
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Harmony, HarmonyRoot};

    /// Minimal valid MusicXML as bytes for parsing tests.
    const MINIMAL_XML: &str = r#"<?xml version="1.0"?><score-partwise version="3.1"><part-list><score-part id="P1"><part-name>Part</part-name></score-part></part-list><part id="P1"><measure number="1"><attributes><divisions>1</divisions><key><fifths>0</fifths></key><time><beats>4</beats><beat-type>4</beat-type></time></attributes><note><pitch><step>C</step><octave>4</octave></pitch><duration>1</duration><type>quarter</type></note></measure></part></score-partwise>"#;

    #[test]
    fn transpose_score_zero_is_noop() {
        let mut score = parse_bytes(MINIMAL_XML.as_bytes(), Some("xml")).unwrap();
        let before = score.parts[0].measures[0].notes[0].pitch.as_ref().unwrap().step.clone();
        transpose_score(&mut score, 0);
        let after = score.parts[0].measures[0].notes[0].pitch.as_ref().unwrap().step.clone();
        assert_eq!(before, "C");
        assert_eq!(after, "C");
    }

    #[test]
    fn transpose_score_up_shifts_pitch_and_key() {
        let mut score = parse_bytes(MINIMAL_XML.as_bytes(), Some("xml")).unwrap();
        let key_before = score.parts[0].measures[0]
            .attributes
            .as_ref()
            .and_then(|a| a.key.as_ref())
            .map(|k| k.fifths);
        let pitch_before = score.parts[0].measures[0].notes[0]
            .pitch
            .as_ref()
            .map(|p| (p.step.clone(), p.octave));
        transpose_score(&mut score, 2);
        let key_after = score.parts[0].measures[0]
            .attributes
            .as_ref()
            .and_then(|a| a.key.as_ref())
            .map(|k| k.fifths);
        let pitch_after = score.parts[0].measures[0].notes[0]
            .pitch
            .as_ref()
            .map(|p| (p.step.clone(), p.octave));
        assert_eq!(pitch_before, Some(("C".to_string(), 4)));
        assert_eq!(pitch_after, Some(("D".to_string(), 4)));
        assert_ne!(key_before, key_after);
    }

    #[test]
    fn transpose_score_with_harmony_transposes_root() {
        let mut score = parse_bytes(MINIMAL_XML.as_bytes(), Some("xml")).unwrap();
        score.parts[0].measures[0].harmonies.push(Harmony {
            root: HarmonyRoot { step: "C".to_string(), alter: None },
            kind: "major".to_string(),
            bass: Some(HarmonyRoot { step: "E".to_string(), alter: None }),
            offset_divisions: 0,
        });
        transpose_score(&mut score, -2); // C -> Bb (or A#), E -> D
        let harm = &score.parts[0].measures[0].harmonies[0];
        // Root was C (0), now 10 (Bb or A#); bass was E (4), now 2 (D)
        assert_ne!(harm.root.step, "C");
        let bass = harm.bass.as_ref().unwrap();
        assert_eq!(bass.step, "D");
        assert!(bass.alter.is_none() || bass.alter == Some(0.0));
    }

    #[test]
    fn parse_file_nonexistent_returns_err() {
        let result = parse_file("/nonexistent/path/12345.musicxml");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("Failed to read") || err.contains("read") || err.contains("No such file") || err.contains("path")
        );
    }

    #[test]
    fn parse_bytes_musicxml_extension() {
        let result = parse_bytes(MINIMAL_XML.as_bytes(), Some("musicxml"));
        assert!(result.is_ok());
        assert_eq!(result.unwrap().measure_count(), 1);
    }

    #[test]
    fn parse_bytes_invalid_utf8_xml_returns_err() {
        let bad = b"\xff\xfe"; // invalid UTF-8
        let result = parse_bytes(bad, Some("xml"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("UTF-8"));
    }

    #[test]
    fn parse_bytes_mxl_extension_invalid_data_returns_err() {
        let result = parse_bytes(b"not a zip", Some("mxl"));
        assert!(result.is_err());
    }

    #[test]
    fn render_bytes_to_svg_produces_svg() {
        let result = render_bytes_to_svg(
            MINIMAL_XML.as_bytes(),
            Some("xml"),
            None,
            0,
            None,
            false,
        );
        assert!(result.is_ok());
        let svg = result.unwrap();
        assert!(svg.contains("<svg") || svg.contains("svg"));
    }

    #[test]
    fn generate_midi_from_score_produces_smf() {
        let score = parse_bytes(MINIMAL_XML.as_bytes(), Some("xml")).unwrap();
        let midi = generate_midi_from_score(&score, &MidiOptions::default());
        assert!(!midi.is_empty());
        assert!(midi.len() >= 14);
        assert_eq!(&midi[0..4], b"MThd");
    }

    #[test]
    fn generate_midi_from_bytes_produces_midi() {
        let result = generate_midi_from_bytes(
            MINIMAL_XML.as_bytes(),
            Some("xml"),
            &MidiOptions::default(),
        );
        assert!(result.is_ok());
        let midi = result.unwrap();
        assert!(!midi.is_empty());
        assert_eq!(&midi[0..4], b"MThd");
    }

    #[test]
    fn add_feedback_overlay_injects_group() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg"><rect width="100" height="100"/></svg>"#;
        let overlay = r#"[{"x":50,"y":50,"colors":["green"]}]"#;
        let result = add_feedback_overlay(svg, overlay);
        assert!(result.is_ok());
        let out = result.unwrap();
        assert!(out.contains("feedback-overlay"));
    }

    #[test]
    fn playback_map_from_score_produces_json() {
        let score = parse_bytes(MINIMAL_XML.as_bytes(), Some("xml")).unwrap();
        let json = playback_map_from_score(&score, None, None, false);
        assert!(!json.is_empty());
        assert!(json.contains("measures") || json.contains("timemap") || json.contains('{'));
    }

    #[test]
    fn playback_map_from_bytes_produces_json() {
        let result = playback_map_from_bytes(
            MINIMAL_XML.as_bytes(),
            Some("xml"),
            None,
            0,
            None,
            false,
        );
        assert!(result.is_ok());
        let json = result.unwrap();
        assert!(!json.is_empty());
    }

    #[test]
    fn score_to_json_with_parsed_score() {
        let score = parse_bytes(MINIMAL_XML.as_bytes(), Some("xml")).unwrap();
        let json = score_to_json(&score).unwrap();
        assert!(json.contains("Part") || json.contains("parts"));
    }

    #[test]
    fn render_audio_from_bytes_invalid_soundfont_returns_err() {
        let result = render_audio_from_bytes(
            MINIMAL_XML.as_bytes(),
            Some("xml"),
            &MidiOptions::default(),
            b"invalid soundfont bytes",
        );
        assert!(result.is_err());
    }

    #[test]
    fn parse_midi_options_include_metronome_and_spaced_json() {
        let opts = parse_midi_options_from_json_str(r#"{"include_metronome": false}"#);
        assert!(!opts.include_metronome);
        let opts = parse_midi_options_from_json_str(r#"{"include_strings": true}"#);
        assert!(opts.include_strings);
        let opts = parse_midi_options_from_json_str(r#"{"include_drums": true}"#);
        assert!(opts.include_drums);
    }

    #[test]
    fn parse_parts_filter_valid() {
        assert_eq!(parse_parts_filter(Some("1")), Some(vec![1]));
        assert_eq!(parse_parts_filter(Some("1,3,5")), Some(vec![1, 3, 5]));
        assert_eq!(parse_parts_filter(Some(" 2 , 4 ")), Some(vec![2, 4]));
    }

    #[test]
    fn parse_parts_filter_empty_or_invalid() {
        assert_eq!(parse_parts_filter(None), None);
        assert_eq!(parse_parts_filter(Some("")), None);
        assert_eq!(parse_parts_filter(Some("   ")), None);
        assert_eq!(parse_parts_filter(Some("1,abc,3")), None);
    }

    #[test]
    fn parse_midi_options_defaults() {
        let opts = parse_midi_options_from_json_str("{}");
        assert!(opts.include_melody);
        assert!(!opts.include_piano);
        assert_eq!(opts.transpose, 0);
    }

    #[test]
    fn parse_midi_options_from_json_include_flags() {
        let opts = parse_midi_options_from_json_str(
            r#"{"include_melody":false,"include_piano":true,"include_bass":true}"#,
        );
        assert!(!opts.include_melody);
        assert!(opts.include_piano);
        assert!(opts.include_bass);
    }

    #[test]
    fn parse_midi_options_transpose() {
        let opts = parse_midi_options_from_json_str(r#"{"transpose":2}"#);
        assert_eq!(opts.transpose, 2);
        let opts = parse_midi_options_from_json_str(r#"{"transpose": -3}"#);
        assert_eq!(opts.transpose, -3);
    }

    #[test]
    fn parse_midi_options_melody_tracks() {
        let opts = parse_midi_options_from_json_str(r#"{}"#);
        assert_eq!(opts.melody_tracks, None);
        let opts = parse_midi_options_from_json_str(r#"{"melody_tracks":""}"#);
        assert_eq!(opts.melody_tracks, Some(vec![]));
        let opts = parse_midi_options_from_json_str(r#"{"melody_tracks":"2"}"#);
        assert_eq!(opts.melody_tracks, Some(vec![2]));
        let opts = parse_midi_options_from_json_str(r#"{"melody_tracks": "1,3"}"#);
        assert_eq!(opts.melody_tracks, Some(vec![1, 3]));
        // Legacy single-voice form
        let opts = parse_midi_options_from_json_str(r#"{"melody_track":2}"#);
        assert_eq!(opts.melody_tracks, Some(vec![2]));
        let opts = parse_midi_options_from_json_str(r#"{"melody_track":0}"#);
        assert_eq!(opts.melody_tracks, None);
    }

    #[test]
    fn parse_melody_tracks_filter_valid() {
        assert_eq!(parse_melody_tracks_filter(Some("1")), Some(vec![1]));
        assert_eq!(parse_melody_tracks_filter(Some("1,3")), Some(vec![1, 3]));
        assert_eq!(parse_melody_tracks_filter(Some(" 2 , 4 ")), Some(vec![2, 4]));
        assert_eq!(parse_melody_tracks_filter(Some("1,1,2")), Some(vec![1, 2]));
    }

    #[test]
    fn parse_melody_tracks_filter_empty_or_invalid() {
        assert_eq!(parse_melody_tracks_filter(None), None);
        assert_eq!(parse_melody_tracks_filter(Some("")), None);
        assert_eq!(parse_melody_tracks_filter(Some("   ")), None);
        assert_eq!(parse_melody_tracks_filter(Some("1,abc")), None);
        assert_eq!(parse_melody_tracks_filter(Some("0")), None);
    }

    #[test]
    fn parse_midi_options_energy() {
        let opts = parse_midi_options_from_json_str(r#"{"energy":"soft"}"#);
        assert!(matches!(opts.energy, Energy::Soft));
        let opts = parse_midi_options_from_json_str(r#"{"energy": "strong"}"#);
        assert!(matches!(opts.energy, Energy::Strong));
    }

    #[test]
    fn parse_bytes_xml_extension() {
        let minimal = r#"<?xml version="1.0"?><score-partwise version="3.1"><part-list><score-part id="P1"><part-name>Part</part-name></score-part></part-list><part id="P1"><measure number="1"><attributes><divisions>1</divisions><key><fifths>0</fifths></key><time><beats>4</beats><beat-type>4</beat-type></time></attributes><note><pitch><step>C</step><octave>4</octave></pitch><duration>1</duration><type>quarter</type></note></measure></part></score-partwise>"#;
        let result = parse_bytes(minimal.as_bytes(), Some("xml"));
        assert!(result.is_ok());
        let score = result.unwrap();
        assert_eq!(score.measure_count(), 1);
    }

    #[test]
    fn parse_bytes_auto_detect_xml() {
        let minimal = r#"<?xml version="1.0"?><score-partwise version="3.1"><part-list><score-part id="P1"><part-name>P</part-name></score-part></part-list><part id="P1"><measure number="1"><attributes><divisions>1</divisions><key><fifths>0</fifths></key><time><beats>4</beats><beat-type>4</beat-type></time></attributes><note><rest/><duration>1</duration><type>quarter</type></note></measure></part></score-partwise>"#;
        let result = parse_bytes(minimal.as_bytes(), None);
        assert!(result.is_ok());
    }

    #[test]
    fn score_to_json_roundtrip() {
        let score = Score::new();
        let json = score_to_json(&score).unwrap();
        // serde_json pretty-prints "parts": [] (with space); compact uses "parts":[]
        assert!(json.contains("\"parts\": []") || json.contains("\"parts\":[]"));
    }
}
