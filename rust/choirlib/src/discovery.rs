//! mDNS discovery: browse for choir services, resolve to host:port for WebSocket URL.

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use crate::protocol::CHOIR_SERVICE_TYPE;
use mdns_sd::{ServiceDaemon, ServiceEvent};

/// Resolved choir: display name and WebSocket URL (ws://host:port).
#[derive(Debug, Clone, serde::Serialize)]
pub struct ResolvedChoir {
    pub choir_name: String,
    pub ws_url: String,
}

fn instance_name_from_fullname(fullname: &str) -> String {
    fullname
        .split('.')
        .next()
        .unwrap_or("")
        .to_string()
}

/// Browse for choirs (blocking) for up to `timeout_secs`. Returns list of (choir_name, ws_url).
pub fn discover_choirs(timeout_secs: u64) -> Result<Vec<ResolvedChoir>, String> {
    let mdns = ServiceDaemon::new().map_err(|e| format!("mDNS: {}", e))?;
    let receiver = mdns.browse(CHOIR_SERVICE_TYPE).map_err(|e| format!("browse: {}", e))?;

    let (tx, rx) = mpsc::channel();
    let browse_thread = thread::spawn(move || {
        let mut resolved = Vec::new();
        let deadline = std::time::Instant::now() + Duration::from_secs(timeout_secs);
        while std::time::Instant::now() < deadline {
            match receiver.recv_timeout(Duration::from_millis(500)) {
                Ok(ServiceEvent::ServiceResolved(svc)) => {
                    let host = svc.host.trim_end_matches('.').to_string();
                    let choir_name = instance_name_from_fullname(&svc.fullname);
                    let ws_url = format!("ws://{}:{}", host, svc.port);
                    resolved.push(ResolvedChoir { choir_name, ws_url });
                }
                Ok(_) => {}
                Err(flume::RecvTimeoutError::Timeout) => continue,
                Err(flume::RecvTimeoutError::Disconnected) => break,
            }
        }
        let _ = tx.send(resolved);
    });

    let result = rx.recv_timeout(Duration::from_secs(timeout_secs + 2));
    let _ = browse_thread.join();
    match result {
        Ok(list) => Ok(list),
        Err(_) => Ok(Vec::new()),
    }
}

/// Resolve a single choir by instance name (blocking). Returns ws_url.
pub fn resolve_choir(choir_name: &str, timeout_secs: u64) -> Result<String, String> {
    let mdns = ServiceDaemon::new().map_err(|e| format!("mDNS: {}", e))?;
    let receiver = mdns.browse(CHOIR_SERVICE_TYPE).map_err(|e| format!("browse: {}", e))?;

    let deadline = std::time::Instant::now() + Duration::from_secs(timeout_secs);
    while std::time::Instant::now() < deadline {
        match receiver.recv_timeout(Duration::from_millis(500)) {
            Ok(ServiceEvent::ServiceResolved(svc)) => {
                if instance_name_from_fullname(&svc.fullname) == choir_name {
                    let host = svc.host.trim_end_matches('.').to_string();
                    return Ok(format!("ws://{}:{}", host, svc.port));
                }
            }
            Ok(_) => {}
            Err(flume::RecvTimeoutError::Timeout) => continue,
            Err(flume::RecvTimeoutError::Disconnected) => break,
        }
    }
    Err("choir not found or timeout".to_string())
}
