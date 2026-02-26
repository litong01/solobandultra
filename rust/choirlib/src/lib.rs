//! Choir library: WebSocket server/client and mDNS discovery for synchronized choir playback.

pub mod client;
pub mod discovery;
pub mod protocol;
pub mod server;

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use client::{run_client, ScheduledCommand};
use discovery::{discover_choirs, resolve_choir};
use server::{run_server, ServerHandle};

#[cfg(target_os = "android")]
pub mod android;

// ─── Global state (single server, single client per process) ─────────────────

struct ServerState {
    handle: ServerHandle,
    _port: u16,
    /// "IP:port" for display so user can try manual connect if discovery fails.
    listen_address: String,
    choir_name: String,
    password: String,
}

struct ClientState {
    send_cmd_tx: tokio::sync::mpsc::UnboundedSender<(String, i64)>,
    command_rx: Mutex<tokio::sync::mpsc::UnboundedReceiver<ScheduledCommand>>,
    shutdown_tx: tokio::sync::mpsc::UnboundedSender<()>,
}

static SERVER: Mutex<Option<ServerState>> = Mutex::new(None);
static CLIENT: Mutex<Option<ClientState>> = Mutex::new(None);
/// Last discovery error message, so UI can show "Discovery failed: ..." vs "No choir discovered".
static LAST_DISCOVERY_ERROR: Mutex<Option<String>> = Mutex::new(None);
/// Last send_command failure reason: "no client" or "channel closed" (receiver dropped).
static LAST_SEND_ERROR: Mutex<Option<String>> = Mutex::new(None);
/// Last reason the client task exited (set from client.rs when the task exits).
static LAST_CLIENT_EXIT_REASON: Mutex<Option<String>> = Mutex::new(None);

/// Called by client task when it exits so we can diagnose why leader connection dropped.
pub(crate) fn set_client_exit_reason(reason: String) {
    *LAST_CLIENT_EXIT_REASON.lock().unwrap() = Some(reason);
}

/// Called by client task when it exits so the app sees "not connected" (choir_client_connected() returns 0).
pub(crate) fn clear_client_on_disconnect() {
    let mut guard = CLIENT.lock().unwrap();
    *guard = None;
}

/// Last reason the server saw a client disconnect (server's view of why connection closed).
static LAST_SERVER_DISCONNECT_REASON: Mutex<Option<String>> = Mutex::new(None);

/// Called by server when a client disconnects so we can correlate with client-side exit reason.
pub(crate) fn set_server_disconnect_reason(reason: String) {
    *LAST_SERVER_DISCONNECT_REASON.lock().unwrap() = Some(reason);
}

fn unix_time_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

// ─── C FFI (iOS) ─────────────────────────────────────────────────────────────

/// Start choir server. Returns port number on success, 0 on error.
/// Caller owns choir_name and password (UTF-8, null-terminated).
#[no_mangle]
pub unsafe extern "C" fn choir_server_start(
    choir_name: *const c_char,
    password: *const c_char,
) -> u16 {
    if choir_name.is_null() || password.is_null() {
        return 0;
    }
    let name = match unsafe { CStr::from_ptr(choir_name).to_str() } {
        Ok(s) => s.to_string(),
        Err(_) => return 0,
    };
    let pass = match unsafe { CStr::from_ptr(password).to_str() } {
        Ok(s) => s.to_string(),
        Err(_) => return 0,
    };
    log::info!("choirlib: server_start choir_name={}", name);
    match run_server(name.clone(), pass.clone(), 0) {
        Ok((port, handle, listen_address)) => {
            let mut guard = SERVER.lock().unwrap();
            *guard = Some(ServerState {
                handle,
                _port: port,
                listen_address: listen_address.clone(),
                choir_name: name,
                password: pass,
            });
            log::info!("choirlib: server_start success port={} listen_address={}", port, listen_address);
            port
        }
        Err(e) => {
            log::error!("choirlib: server_start failed: {}", e);
            0
        }
    }
}

/// After choir_server_start, return listen address "IP:port" for display. Caller must free with choir_free_string.
#[no_mangle]
pub unsafe extern "C" fn choir_server_listen_address() -> *mut c_char {
    let guard = SERVER.lock().unwrap();
    match guard.as_ref() {
        Some(s) => CString::new(s.listen_address.as_str())
            .ok()
            .map(|c| c.into_raw())
            .unwrap_or(std::ptr::null_mut()),
        None => std::ptr::null_mut(),
    }
}

/// Stop choir server. Call after choir_server_start.
#[no_mangle]
pub unsafe extern "C" fn choir_server_stop() {
    let mut guard = SERVER.lock().unwrap();
    if let Some(s) = guard.take() {
        s.handle.stop();
    }
}

/// Discover choirs (blocking, up to timeout_secs). Returns JSON array of {"choir_name":"...","ws_url":"..."}.
/// Caller must free with choir_free_string. On error returns null; call choir_discover_last_error() for message.
#[no_mangle]
pub unsafe extern "C" fn choir_discover(timeout_secs: u32) -> *mut c_char {
    match discover_choirs(timeout_secs as u64) {
        Ok(list) => {
            *LAST_DISCOVERY_ERROR.lock().unwrap() = None;
            let json = serde_json::to_string(&list).unwrap_or_default();
            CString::new(json).ok().map(|c| c.into_raw()).unwrap_or(std::ptr::null_mut())
        }
        Err(e) => {
            *LAST_DISCOVERY_ERROR.lock().unwrap() = Some(e);
            std::ptr::null_mut()
        }
    }
}

/// After choir_discover returned null, return last error message. Caller must free with choir_free_string.
#[no_mangle]
pub unsafe extern "C" fn choir_discover_last_error() -> *mut c_char {
    let guard = LAST_DISCOVERY_ERROR.lock().unwrap();
    guard
        .as_deref()
        .and_then(|s| CString::new(s.to_string()).ok())
        .map(|c| c.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

/// Join choir: resolve choir_name via mDNS, connect, authenticate with password.
/// Returns 1 on success, 0 on failure. Should be called from a background thread (blocking).
#[no_mangle]
pub unsafe extern "C" fn choir_client_join(
    choir_name: *const c_char,
    password: *const c_char,
) -> i32 {
    if choir_name.is_null() || password.is_null() {
        return 0;
    }
    let name = match unsafe { CStr::from_ptr(choir_name).to_str() } {
        Ok(s) => s.to_string(),
        Err(_) => return 0,
    };
    let pass = match unsafe { CStr::from_ptr(password).to_str() } {
        Ok(s) => s.to_string(),
        Err(_) => return 0,
    };
    let ws_url = match resolve_choir(&name, 5) {
        Ok(u) => u,
        Err(_) => return 0,
    };
    let (command_tx, command_rx) = tokio::sync::mpsc::unbounded_channel();
    match run_client(ws_url, name, pass, command_tx) {
        Ok((send_cmd_tx, shutdown_tx)) => {
            *LAST_CLIENT_EXIT_REASON.lock().unwrap() = None;
            let mut guard = CLIENT.lock().unwrap();
            *guard = Some(ClientState {
                send_cmd_tx,
                command_rx: Mutex::new(command_rx),
                shutdown_tx,
            });
            1
        }
        Err(_) => 0,
    }
}

/// Join choir by direct WebSocket URL (no mDNS). Use when discovery fails, e.g. Android emulator → host: use ws://10.0.2.2:PORT.
/// Returns 1 on success, 0 on failure. Call from a background thread (blocking).
#[no_mangle]
pub unsafe extern "C" fn choir_client_join_with_url(
    ws_url: *const c_char,
    choir_name: *const c_char,
    password: *const c_char,
) -> i32 {
    if ws_url.is_null() || choir_name.is_null() || password.is_null() {
        return 0;
    }
    let url = match unsafe { CStr::from_ptr(ws_url).to_str() } {
        Ok(s) => s.to_string(),
        Err(_) => return 0,
    };
    let name = match unsafe { CStr::from_ptr(choir_name).to_str() } {
        Ok(s) => s.to_string(),
        Err(_) => return 0,
    };
    let pass = match unsafe { CStr::from_ptr(password).to_str() } {
        Ok(s) => s.to_string(),
        Err(_) => return 0,
    };
    log::info!("choirlib: join_with_url url={} choir_name={}", url, name);
    let (command_tx, command_rx) = tokio::sync::mpsc::unbounded_channel();
    match run_client(url, name, pass, command_tx) {
        Ok((send_cmd_tx, shutdown_tx)) => {
            *LAST_CLIENT_EXIT_REASON.lock().unwrap() = None;
            let mut guard = CLIENT.lock().unwrap();
            *guard = Some(ClientState {
                send_cmd_tx,
                command_rx: Mutex::new(command_rx),
                shutdown_tx,
            });
            log::info!("choirlib: join_with_url success");
            1
        }
        Err(e) => {
            log::error!("choirlib: join_with_url failed: {}", e);
            0
        }
    }
}

/// Leave choir (disconnect client).
#[no_mangle]
pub unsafe extern "C" fn choir_client_leave() {
    let mut guard = CLIENT.lock().unwrap();
    if let Some(c) = guard.take() {
        let _ = c.shutdown_tx.send(());
    }
}

/// Send a command as leader (only valid if this device is the choir leader).
/// execute_at_ms is Unix time in ms when all clients should execute; use choir_execute_at_ms(delay_ms) to compute.
/// Returns 1 on success, 0 if not joined or not leader.
#[no_mangle]
pub unsafe extern "C" fn choir_send_command(
    command: *const c_char,
    execute_at_ms: i64,
) -> i32 {
    if command.is_null() {
        return 0;
    }
    let cmd = match unsafe { CStr::from_ptr(command).to_str() } {
        Ok(s) => s.to_string(),
        Err(_) => return 0,
    };
    let guard = CLIENT.lock().unwrap();
    if let Some(ref c) = *guard {
        if c.send_cmd_tx.send((cmd.clone(), execute_at_ms)).is_ok() {
            *LAST_SEND_ERROR.lock().unwrap() = None;
            log::info!("choirlib: send_command cmd={} execute_at_ms={}", cmd, execute_at_ms);
            return 1;
        }
        *LAST_SEND_ERROR.lock().unwrap() = Some("channel closed (leader connection dropped)".to_string());
    } else {
        *LAST_SEND_ERROR.lock().unwrap() = Some("no client (not joined)".to_string());
    }
    log::warn!("choirlib: send_command dropped cmd={} reason={:?}", cmd, *LAST_SEND_ERROR.lock().unwrap());
    0
}

/// Returns 1 if the client is still connected (background task running), 0 if disconnected or never joined.
/// Use this to keep UI in sync: when it returns 0, set isJoined = false so the button shows "Join" not "Leave".
#[no_mangle]
pub extern "C" fn choir_client_connected() -> i32 {
    let guard = CLIENT.lock().unwrap();
    if guard.is_some() {
        1
    } else {
        0
    }
}

/// After choir_send_command returned 0, return last failure reason. Caller must free with choir_free_string.
#[no_mangle]
pub unsafe extern "C" fn choir_send_command_last_error() -> *mut c_char {
    let guard = LAST_SEND_ERROR.lock().unwrap();
    guard
        .as_deref()
        .and_then(|s| CString::new(s.to_string()).ok())
        .map(|c| c.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

/// Last reason the client/leader connection task exited (e.g. "WebSocket stream ended"). Caller must free with choir_free_string.
#[no_mangle]
pub unsafe extern "C" fn choir_client_exit_reason() -> *mut c_char {
    let guard = LAST_CLIENT_EXIT_REASON.lock().unwrap();
    guard
        .as_deref()
        .and_then(|s| CString::new(s.to_string()).ok())
        .map(|c| c.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

/// Last reason the server saw a client disconnect (server's view). Caller must free with choir_free_string.
#[no_mangle]
pub unsafe extern "C" fn choir_server_last_disconnect_reason() -> *mut c_char {
    let guard = LAST_SERVER_DISCONNECT_REASON.lock().unwrap();
    guard
        .as_deref()
        .and_then(|s| CString::new(s.to_string()).ok())
        .map(|c| c.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

/// Compute execute_at timestamp: now + delay_ms. Use for choir_send_command.
#[no_mangle]
pub extern "C" fn choir_execute_at_ms(delay_ms: i64) -> i64 {
    unix_time_ms() + delay_ms
}

/// Poll for next scheduled command. Returns command name in out_command (caller allocates at least 32 bytes), execute_at_ms in out_execute_at.
/// Returns 1 if a command was available, 0 otherwise.
#[no_mangle]
pub unsafe extern "C" fn choir_poll_command(
    out_command: *mut c_char,
    out_command_len: usize,
    out_execute_at_ms: *mut i64,
) -> i32 {
    if out_command.is_null() || out_command_len == 0 || out_execute_at_ms.is_null() {
        return 0;
    }
    let mut guard = CLIENT.lock().unwrap();
    if let Some(ref mut c) = *guard {
        let mut rx = c.command_rx.lock().unwrap();
        if let Ok(cmd) = rx.try_recv() {
            log::info!(
                "choirlib: poll_command returning cmd={} execute_at_ms={}",
                cmd.command,
                cmd.execute_at_ms
            );
            let cmd_str = cmd.command.as_bytes();
            let copy_len = (out_command_len - 1).min(cmd_str.len());
            unsafe {
                std::ptr::copy_nonoverlapping(cmd_str.as_ptr(), out_command as *mut u8, copy_len);
                *out_command.add(copy_len) = 0;
                *out_execute_at_ms = cmd.execute_at_ms;
            }
            return 1;
        }
    }
    0
}

/// Free string returned by choir_discover.
#[no_mangle]
pub unsafe extern "C" fn choir_free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        let _ = unsafe { CString::from_raw(ptr) };
    }
}

/// Start server and connect as leader client to localhost so leader receives own commands.
/// Returns 1 on success, 0 on failure. Call after choir_server_start (so port is known).
#[no_mangle]
pub unsafe extern "C" fn choir_leader_connect(port: u16) -> i32 {
    let (name, pass) = {
        let guard = SERVER.lock().unwrap();
        let s = match guard.as_ref() {
            Some(s) => s,
            None => return 0,
        };
        (s.choir_name.clone(), s.password.clone())
    };
    let ws_url = format!("ws://127.0.0.1:{}", port);
    log::info!("choirlib: leader_connect url={}", ws_url);
    let (command_tx, command_rx) = tokio::sync::mpsc::unbounded_channel();
    match run_client(ws_url, name, pass, command_tx) {
        Ok((send_cmd_tx, shutdown_tx)) => {
            *LAST_CLIENT_EXIT_REASON.lock().unwrap() = None;
            let mut guard = CLIENT.lock().unwrap();
            *guard = Some(ClientState {
                send_cmd_tx,
                command_rx: Mutex::new(command_rx),
                shutdown_tx,
            });
            log::info!("choirlib: leader_connect success");
            1
        }
        Err(e) => {
            log::error!("choirlib: leader_connect failed: {}", e);
            0
        }
    }
}
