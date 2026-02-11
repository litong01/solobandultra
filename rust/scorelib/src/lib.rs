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

#[cfg(target_os = "android")]
pub mod android;

use std::path::Path;

pub use model::*;
pub use parser::parse_musicxml;
pub use mxl::parse_mxl;
pub use renderer::render_score_to_svg;
pub use midi::{generate_midi, MidiOptions, Energy};
pub use unroller::unroll;
pub use timemap::generate_timemap;
pub use playback::{generate_playback_map, PlaybackMap};

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
pub fn render_file_to_svg<P: AsRef<std::path::Path>>(
    path: P,
    page_width: Option<f64>,
) -> Result<String, String> {
    let score = parse_file(path)?;
    Ok(render_score_to_svg(&score, page_width))
}

/// Parse MusicXML bytes and render to SVG.
///
/// `page_width` sets the SVG width in user units. Pass `None` to use the
/// default (820).
pub fn render_bytes_to_svg(
    data: &[u8],
    extension: Option<&str>,
    page_width: Option<f64>,
) -> Result<String, String> {
    let score = parse_bytes(data, extension)?;
    Ok(render_score_to_svg(&score, page_width))
}

/// Generate MIDI bytes from a parsed score.
///
/// Unrolls repeats/jumps, computes the timemap, extracts melody and
/// optionally generates accompaniment tracks.  Returns a Standard MIDI
/// File (SMF Type 1) as raw bytes.
pub fn generate_midi_from_score(score: &Score, options: &MidiOptions) -> Vec<u8> {
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
    let score = parse_file(path)?;
    Ok(generate_midi_from_score(&score, options))
}

/// Parse MusicXML bytes and generate MIDI bytes.
pub fn generate_midi_from_bytes(
    data: &[u8],
    extension: Option<&str>,
    options: &MidiOptions,
) -> Result<Vec<u8>, String> {
    let score = parse_bytes(data, extension)?;
    Ok(generate_midi_from_score(&score, options))
}

/// Generate a playback map from a parsed score (JSON string).
///
/// The playback map contains measure positions, system positions and the
/// timemap — everything the WebView needs to animate a playback cursor.
pub fn playback_map_from_score(score: &Score, page_width: Option<f64>) -> String {
    let map = generate_playback_map(score, page_width);
    playback::playback_map_to_json(&map)
}

/// Parse MusicXML bytes and return a playback map JSON string.
pub fn playback_map_from_bytes(
    data: &[u8],
    extension: Option<&str>,
    page_width: Option<f64>,
) -> Result<String, String> {
    let score = parse_bytes(data, extension)?;
    Ok(playback_map_from_score(&score, page_width))
}

// ═══════════════════════════════════════════════════════════════════════
// C FFI — for iOS (static library) and Android (JNI)
// ═══════════════════════════════════════════════════════════════════════

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

/// Parse a MusicXML file and return SVG as a C string.
/// The caller must free the returned string with `scorelib_free_string`.
///
/// `page_width` sets the SVG width in user units. Pass 0.0 to use the default.
///
/// # Safety
/// `path` must be a valid null-terminated UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn scorelib_render_file(
    path: *const c_char,
    page_width: f64,
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

    match render_file_to_svg(path_str, pw) {
        Ok(svg) => CString::new(svg).unwrap_or_default().into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Parse MusicXML bytes and return SVG as a C string.
/// The caller must free the returned string with `scorelib_free_string`.
///
/// `page_width` sets the SVG width in user units. Pass 0.0 to use the default.
///
/// # Safety
/// `data` must point to `len` valid bytes. `extension` may be null.
#[no_mangle]
pub unsafe extern "C" fn scorelib_render_bytes(
    data: *const u8,
    len: usize,
    extension: *const c_char,
    page_width: f64,
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

    match render_bytes_to_svg(bytes, ext, pw) {
        Ok(svg) => CString::new(svg).unwrap_or_default().into_raw(),
        Err(_) => std::ptr::null_mut(),
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
///   `include_drums`, `include_metronome`, `energy` ("soft"/"medium"/"strong").
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

    match generate_midi_from_file(path_str, &options) {
        Ok(midi_bytes) => {
            let len = midi_bytes.len();
            let ptr = midi_bytes.leak().as_mut_ptr();
            unsafe { *out_len = len; }
            ptr
        }
        Err(_) => std::ptr::null_mut(),
    }
}

/// Free MIDI bytes previously returned by `scorelib_generate_midi`.
///
/// # Safety
/// `ptr` must be a buffer previously returned by a scorelib MIDI function,
/// or null. `len` must be the length returned via `out_len`.
#[no_mangle]
pub unsafe extern "C" fn scorelib_free_midi(ptr: *mut u8, len: usize) {
    if !ptr.is_null() && len > 0 {
        unsafe {
            let _ = Vec::from_raw_parts(ptr, len, len);
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
///
/// # Safety
/// `data` must point to `len` valid bytes. `extension` may be null.
#[no_mangle]
pub unsafe extern "C" fn scorelib_playback_map(
    data: *const u8,
    len: usize,
    extension: *const c_char,
    page_width: f64,
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

    match playback_map_from_bytes(bytes, ext, pw) {
        Ok(json) => CString::new(json).unwrap_or_default().into_raw(),
        Err(_) => std::ptr::null_mut(),
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

    match generate_midi_from_bytes(bytes, ext, &options) {
        Ok(midi_bytes) => {
            let len = midi_bytes.len();
            let ptr = midi_bytes.leak().as_mut_ptr();
            unsafe { *out_len = len; }
            ptr
        }
        Err(_) => std::ptr::null_mut(),
    }
}

/// Parse MidiOptions from a JSON C string (internal helper).
unsafe fn parse_midi_options_json(json_ptr: *const c_char) -> MidiOptions {
    if json_ptr.is_null() {
        return MidiOptions::default();
    }
    let c_str = unsafe { CStr::from_ptr(json_ptr) };
    let json_str = match c_str.to_str() {
        Ok(s) => s,
        Err(_) => return MidiOptions::default(),
    };

    // Simple JSON parsing without serde_json::Value dependency overhead.
    // We look for known keys with simple string matching.
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
    opts
}
