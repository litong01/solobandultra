//! JNI bindings for Android choir library.

use jni::objects::{JClass, JString};
use jni::sys::{jint, jlong, jstring};
use jni::JNIEnv;
use std::ffi::{c_char, c_void, CString};

use crate::{
    choir_client_connected, choir_client_join, choir_client_join_with_url, choir_client_leave,
    choir_discover, choir_execute_at_ms, choir_free_string, choir_leader_connect,
    choir_poll_command, choir_send_command, choir_server_start, choir_server_stop,
};

fn jstring_to_rust(env: &mut JNIEnv, s: &JString) -> Option<String> {
    env.get_string(s).ok().map(|s| s.into())
}

/// Called when the native library is loaded. Initializes Rust log -> Logcat.
#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn JNI_OnLoad(_vm: jni::JavaVM, _reserved: *mut c_void) -> jint {
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info)
            .with_tag("choirlib"),
    );
    jni::sys::JNI_VERSION_1_6
}

/// Start choir server. Returns port (0 = error).
#[no_mangle]
pub unsafe extern "system" fn Java_com_solobandultra_app_ChoirLib_choirServerStart(
    mut env: JNIEnv,
    _class: JClass,
    choir_name: JString,
    password: JString,
) -> jint {
    let name = match jstring_to_rust(&mut env, &choir_name) {
        Some(s) => s,
        None => return 0,
    };
    let pass = match jstring_to_rust(&mut env, &password) {
        Some(s) => s,
        None => return 0,
    };
    let name_c = CString::new(name).unwrap_or_default();
    let pass_c = CString::new(pass).unwrap_or_default();
    let port = choir_server_start(name_c.as_ptr(), pass_c.as_ptr());
    port as jint
}

/// Stop choir server.
#[no_mangle]
pub unsafe extern "system" fn Java_com_solobandultra_app_ChoirLib_choirServerStop(
    _env: JNIEnv,
    _class: JClass,
) {
    choir_server_stop();
}

/// Discover choirs (blocking). Returns JSON string; null on error. Caller must deleteLocalRef.
#[no_mangle]
pub unsafe extern "system" fn Java_com_solobandultra_app_ChoirLib_choirDiscover(
    env: JNIEnv,
    _class: JClass,
    timeout_secs: jint,
) -> jstring {
    let ptr = choir_discover(timeout_secs as u32);
    if ptr.is_null() {
        return std::ptr::null_mut();
    }
    let s = std::ffi::CStr::from_ptr(ptr).to_str().unwrap_or("");
    let j = env.new_string(s).ok().map(|s| s.into_raw()).unwrap_or(std::ptr::null_mut());
    choir_free_string(ptr);
    j
}

/// Join choir (blocking). Returns 1 on success.
#[no_mangle]
pub unsafe extern "system" fn Java_com_solobandultra_app_ChoirLib_choirClientJoin(
    mut env: JNIEnv,
    _class: JClass,
    choir_name: JString,
    password: JString,
) -> jint {
    let name = match jstring_to_rust(&mut env, &choir_name) {
        Some(s) => s,
        None => return 0,
    };
    let pass = match jstring_to_rust(&mut env, &password) {
        Some(s) => s,
        None => return 0,
    };
    let name_c = CString::new(name).unwrap_or_default();
    let pass_c = CString::new(pass).unwrap_or_default();
    choir_client_join(name_c.as_ptr(), pass_c.as_ptr()) as jint
}

/// Join choir by URL (blocking, no mDNS). For Android emulator use ws://10.0.2.2:PORT. Returns 1 on success.
#[no_mangle]
pub unsafe extern "system" fn Java_com_solobandultra_app_ChoirLib_choirClientJoinWithUrl(
    mut env: JNIEnv,
    _class: JClass,
    ws_url: JString,
    choir_name: JString,
    password: JString,
) -> jint {
    let url = match jstring_to_rust(&mut env, &ws_url) {
        Some(s) => s,
        None => return 0,
    };
    let name = match jstring_to_rust(&mut env, &choir_name) {
        Some(s) => s,
        None => return 0,
    };
    let pass = match jstring_to_rust(&mut env, &password) {
        Some(s) => s,
        None => return 0,
    };
    let url_c = CString::new(url).unwrap_or_default();
    let name_c = CString::new(name).unwrap_or_default();
    let pass_c = CString::new(pass).unwrap_or_default();
    choir_client_join_with_url(url_c.as_ptr(), name_c.as_ptr(), pass_c.as_ptr()) as jint
}

/// Leave choir.
#[no_mangle]
pub unsafe extern "system" fn Java_com_solobandultra_app_ChoirLib_choirClientLeave(
    _env: JNIEnv,
    _class: JClass,
) {
    choir_client_leave();
}

/// Returns true if the client connection is still alive (background task running), false if disconnected.
#[no_mangle]
pub unsafe extern "system" fn Java_com_solobandultra_app_ChoirLib_choirClientConnected(
    _env: JNIEnv,
    _class: JClass,
) -> jint {
    choir_client_connected()
}

/// Leader connect after starting server. Returns 1 on success.
#[no_mangle]
pub unsafe extern "system" fn Java_com_solobandultra_app_ChoirLib_choirLeaderConnect(
    _env: JNIEnv,
    _class: JClass,
    port: jint,
) -> jint {
    choir_leader_connect(port as u16) as jint
}

/// Send command (leader). execute_at_ms from choirExecuteAtMs.
#[no_mangle]
pub unsafe extern "system" fn Java_com_solobandultra_app_ChoirLib_choirSendCommand(
    mut env: JNIEnv,
    _class: JClass,
    command: JString,
    execute_at_ms: jlong,
) -> jint {
    let cmd = match jstring_to_rust(&mut env, &command) {
        Some(s) => s,
        None => return 0,
    };
    let cmd_c = CString::new(cmd).unwrap_or_default();
    choir_send_command(cmd_c.as_ptr(), execute_at_ms) as jint
}

/// Compute execute_at = now + delay_ms.
#[no_mangle]
pub unsafe extern "system" fn Java_com_solobandultra_app_ChoirLib_choirExecuteAtMs(
    _env: JNIEnv,
    _class: JClass,
    delay_ms: jlong,
) -> jlong {
    choir_execute_at_ms(delay_ms as i64) as jlong
}

/// Poll next command. Returns JSON "{\"command\":\"play\",\"execute_at_ms\":123}" or null.
#[no_mangle]
pub unsafe extern "system" fn Java_com_solobandultra_app_ChoirLib_choirPollCommand(
    env: JNIEnv,
    _class: JClass,
) -> jstring {
    let mut buf = [0 as c_char; 64];
    let mut execute_at: i64 = 0;
    let ok = choir_poll_command(buf.as_mut_ptr(), 64, &mut execute_at);
    if ok != 1 {
        return std::ptr::null_mut();
    }
    let cmd_str = std::ffi::CStr::from_ptr(buf.as_ptr()).to_str().unwrap_or("");
    let json = format!("{{\"command\":\"{}\",\"execute_at_ms\":{}}}", cmd_str, execute_at);
    env.new_string(&json).ok().map(|s| s.into_raw()).unwrap_or(std::ptr::null_mut())
}
