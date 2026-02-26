//! Wire protocol for choir WebSocket messages.

use serde::{Deserialize, Serialize};

/// Service type for mDNS (choir discovery on local network).
pub const CHOIR_SERVICE_TYPE: &str = "_solobandchoir._tcp.local.";

/// Client → Server: join request (first message after connect).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinRequest {
    #[serde(rename = "type")]
    pub typ: String, // "join"
    pub choir_name: String,
    pub password: String,
}

/// Server → Client: join result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinResponse {
    #[serde(rename = "type")]
    pub typ: String, // "join_ok" | "join_err"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Leader → Server (then Server → All): playback command with target time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandMessage {
    #[serde(rename = "type")]
    pub typ: String, // "command"
    /// One of: "play", "pause", "stop", "next", "prev"
    pub command: String,
    /// Unix timestamp in milliseconds when all clients should execute.
    pub execute_at: i64,
}

impl JoinRequest {
    pub fn new(choir_name: String, password: String) -> Self {
        Self {
            typ: "join".to_string(),
            choir_name,
            password,
        }
    }
}

impl JoinResponse {
    pub fn ok() -> Self {
        Self {
            typ: "join_ok".to_string(),
            message: None,
        }
    }
    pub fn err(message: String) -> Self {
        Self {
            typ: "join_err".to_string(),
            message: Some(message),
        }
    }
}

impl CommandMessage {
    pub fn new(command: &str, execute_at: i64) -> Self {
        Self {
            typ: "command".to_string(),
            command: command.to_string(),
            execute_at,
        }
    }
}
