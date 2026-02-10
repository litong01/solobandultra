//! JNI bindings for Android.
//!
//! These functions are called from Kotlin via the JNI bridge.

use jni::objects::{JByteArray, JClass, JString};
use jni::sys::{jfloat, jstring};
use jni::JNIEnv;

use crate::{render_bytes_to_svg, render_file_to_svg};

/// Render a MusicXML file at the given path to SVG.
///
/// Called from Kotlin as:
///   external fun renderFile(path: String, pageWidth: Float): String?
#[no_mangle]
pub extern "system" fn Java_com_solobandultra_app_ScoreLib_renderFile(
    mut env: JNIEnv,
    _class: JClass,
    path: JString,
    page_width: jfloat,
) -> jstring {
    let path_str: String = match env.get_string(&path) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };

    let pw = if page_width > 0.0 { Some(page_width as f64) } else { None };

    match render_file_to_svg(&path_str, pw) {
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
///   external fun renderBytes(data: ByteArray, extension: String?, pageWidth: Float): String?
#[no_mangle]
pub extern "system" fn Java_com_solobandultra_app_ScoreLib_renderBytes(
    mut env: JNIEnv,
    _class: JClass,
    data: JByteArray,
    extension: JString,
    page_width: jfloat,
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

    match render_bytes_to_svg(&bytes, ext.as_deref(), pw) {
        Ok(svg) => match env.new_string(&svg) {
            Ok(js) => js.into_raw(),
            Err(_) => std::ptr::null_mut(),
        },
        Err(_) => std::ptr::null_mut(),
    }
}
