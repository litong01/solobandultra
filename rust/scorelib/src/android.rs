//! JNI bindings for Android.
//!
//! These functions are called from Kotlin via the JNI bridge.

use jni::objects::{JByteArray, JClass, JString};
use jni::sys::{jfloat, jint, jstring};
use jni::JNIEnv;

use crate::{render_bytes_to_svg, render_file_to_svg, playback_map_from_bytes, generate_midi_from_bytes, MidiOptions, Energy};

/// Render a MusicXML file at the given path to SVG.
///
/// Called from Kotlin as:
///   external fun renderFile(path: String, pageWidth: Float, transpose: Int): String?
#[no_mangle]
pub extern "system" fn Java_com_solobandultra_app_ScoreLib_renderFile(
    mut env: JNIEnv,
    _class: JClass,
    path: JString,
    page_width: jfloat,
    transpose: jint,
) -> jstring {
    let path_str: String = match env.get_string(&path) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };

    let pw = if page_width > 0.0 { Some(page_width as f64) } else { None };

    match render_file_to_svg(&path_str, pw, transpose) {
        Ok(svg) => match env.new_string(&svg) {
            Ok(js) => js.into_raw(),
            Err(_) => std::ptr::null_mut(),
        },
        Err(_) => std::ptr::null_mut(),
    }
}

/// Render MusicXML bytes to SVG.
///
/// Called from Kotlin as:
///   external fun renderBytes(data: ByteArray, extension: String?, pageWidth: Float, transpose: Int): String?
#[no_mangle]
pub extern "system" fn Java_com_solobandultra_app_ScoreLib_renderBytes(
    mut env: JNIEnv,
    _class: JClass,
    data: JByteArray,
    extension: JString,
    page_width: jfloat,
    transpose: jint,
) -> jstring {
    let bytes = match env.convert_byte_array(&data) {
        Ok(b) => b,
        Err(_) => return std::ptr::null_mut(),
    };

    let ext: Option<String> = if extension.is_null() {
        None
    } else {
        env.get_string(&extension).ok().map(|s| s.into())
    };

    let pw = if page_width > 0.0 { Some(page_width as f64) } else { None };

    match render_bytes_to_svg(&bytes, ext.as_deref(), pw, transpose) {
        Ok(svg) => match env.new_string(&svg) {
            Ok(js) => js.into_raw(),
            Err(_) => std::ptr::null_mut(),
        },
        Err(_) => std::ptr::null_mut(),
    }
}

/// Generate a playback map JSON from MusicXML bytes.
///
/// Called from Kotlin as:
///   external fun playbackMap(data: ByteArray, extension: String?, pageWidth: Float, transpose: Int): String?
#[no_mangle]
pub extern "system" fn Java_com_solobandultra_app_ScoreLib_playbackMap(
    mut env: JNIEnv,
    _class: JClass,
    data: JByteArray,
    extension: JString,
    page_width: jfloat,
    transpose: jint,
) -> jstring {
    let bytes = match env.convert_byte_array(&data) {
        Ok(b) => b,
        Err(_) => return std::ptr::null_mut(),
    };

    let ext: Option<String> = if extension.is_null() {
        None
    } else {
        env.get_string(&extension).ok().map(|s| s.into())
    };

    let pw = if page_width > 0.0 { Some(page_width as f64) } else { None };

    match playback_map_from_bytes(&bytes, ext.as_deref(), pw, transpose) {
        Ok(json) => match env.new_string(&json) {
            Ok(js) => js.into_raw(),
            Err(_) => std::ptr::null_mut(),
        },
        Err(_) => std::ptr::null_mut(),
    }
}

/// Generate MIDI bytes from MusicXML bytes.
///
/// Called from Kotlin as:
///   external fun generateMidi(data: ByteArray, extension: String?, optionsJson: String?): ByteArray?
#[no_mangle]
pub extern "system" fn Java_com_solobandultra_app_ScoreLib_generateMidi(
    mut env: JNIEnv,
    _class: JClass,
    data: JByteArray,
    extension: JString,
    options_json: JString,
) -> jni::sys::jbyteArray {
    let bytes = match env.convert_byte_array(&data) {
        Ok(b) => b,
        Err(_) => return std::ptr::null_mut() as jni::sys::jbyteArray,
    };

    let ext: Option<String> = if extension.is_null() {
        None
    } else {
        env.get_string(&extension).ok().map(|s| s.into())
    };

    let options = if options_json.is_null() {
        MidiOptions::default()
    } else {
        match env.get_string(&options_json) {
            Ok(s) => parse_midi_options_str(&String::from(s)),
            Err(_) => MidiOptions::default(),
        }
    };

    match generate_midi_from_bytes(&bytes, ext.as_deref(), &options) {
        Ok(midi_bytes) => {
            match env.byte_array_from_slice(&midi_bytes) {
                Ok(arr) => arr.into_raw(),
                Err(_) => std::ptr::null_mut() as jni::sys::jbyteArray,
            }
        }
        Err(_) => std::ptr::null_mut() as jni::sys::jbyteArray,
    }
}

/// Simple MIDI options parser from a JSON string (mirrors lib.rs helper).
fn parse_midi_options_str(json_str: &str) -> MidiOptions {
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
    opts
}
