//! JNI bindings for Android.
//!
//! These functions are called from Kotlin via the JNI bridge.
//!
//! Each function extracts data from JNIEnv first (which cannot panic across
//! FFI), then wraps the core Rust computation in `catch_unwind` to prevent
//! panics from unwinding through the JNI boundary (which is undefined
//! behavior and would crash the ART VM).

use jni::objects::{JByteArray, JClass, JString};
use jni::sys::{jfloat, jint, jstring};
use jni::JNIEnv;

use crate::{render_bytes_to_svg, render_file_to_svg, playback_map_from_bytes, generate_midi_from_bytes, render_audio_from_bytes, parse_midi_options_from_json_str, MidiOptions};

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

    // catch_unwind around the core computation to prevent panics crossing JNI.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        render_file_to_svg(&path_str, pw, transpose)
    }));

    match result {
        Ok(Ok(svg)) => match env.new_string(&svg) {
            Ok(js) => js.into_raw(),
            Err(_) => std::ptr::null_mut(),
        },
        _ => std::ptr::null_mut(),
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

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        render_bytes_to_svg(&bytes, ext.as_deref(), pw, transpose)
    }));

    match result {
        Ok(Ok(svg)) => match env.new_string(&svg) {
            Ok(js) => js.into_raw(),
            Err(_) => std::ptr::null_mut(),
        },
        _ => std::ptr::null_mut(),
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

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        playback_map_from_bytes(&bytes, ext.as_deref(), pw, transpose)
    }));

    match result {
        Ok(Ok(json)) => match env.new_string(&json) {
            Ok(js) => js.into_raw(),
            Err(_) => std::ptr::null_mut(),
        },
        _ => std::ptr::null_mut(),
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
            Ok(s) => parse_midi_options_from_json_str(&String::from(s)),
            Err(_) => MidiOptions::default(),
        }
    };

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        generate_midi_from_bytes(&bytes, ext.as_deref(), &options)
    }));

    match result {
        Ok(Ok(midi_bytes)) => {
            match env.byte_array_from_slice(&midi_bytes) {
                Ok(arr) => arr.into_raw(),
                Err(_) => std::ptr::null_mut() as jni::sys::jbyteArray,
            }
        }
        _ => std::ptr::null_mut() as jni::sys::jbyteArray,
    }
}

/// Render MusicXML bytes to WAV audio using a SoundFont.
///
/// Called from Kotlin as:
///   external fun renderAudio(data: ByteArray, extension: String?, optionsJson: String?, soundfontData: ByteArray): ByteArray?
#[no_mangle]
pub extern "system" fn Java_com_solobandultra_app_ScoreLib_renderAudio(
    mut env: JNIEnv,
    _class: JClass,
    data: JByteArray,
    extension: JString,
    options_json: JString,
    soundfont_data: JByteArray,
) -> jni::sys::jbyteArray {
    let bytes = match env.convert_byte_array(&data) {
        Ok(b) => b,
        Err(_) => return std::ptr::null_mut() as jni::sys::jbyteArray,
    };

    let sf_bytes = match env.convert_byte_array(&soundfont_data) {
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
            Ok(s) => parse_midi_options_from_json_str(&String::from(s)),
            Err(_) => MidiOptions::default(),
        }
    };

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        render_audio_from_bytes(&bytes, ext.as_deref(), &options, &sf_bytes)
    }));

    match result {
        Ok(Ok(wav_bytes)) => {
            match env.byte_array_from_slice(&wav_bytes) {
                Ok(arr) => arr.into_raw(),
                Err(_) => std::ptr::null_mut() as jni::sys::jbyteArray,
            }
        }
        _ => std::ptr::null_mut() as jni::sys::jbyteArray,
    }
}

