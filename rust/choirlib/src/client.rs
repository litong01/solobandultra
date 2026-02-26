//! WebSocket client: connect to choir server (host:port from mDNS resolve), join with password, queue incoming commands.

use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;
use tokio::time::interval;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::connect_async;
use futures_util::{SinkExt, StreamExt};

use crate::protocol::{CommandMessage, JoinRequest, JoinResponse};

/// Command to execute at a given time (for FFI poll).
#[derive(Debug, Clone)]
pub struct ScheduledCommand {
    pub command: String,
    pub execute_at_ms: i64,
}

/// Connect, join, then spawn background thread for send/receive. Blocks until join completes.
/// Returns (tx to send leader commands, shutdown_tx) on success.
pub fn run_client(
    ws_url: String,
    choir_name: String,
    password: String,
    command_tx: mpsc::UnboundedSender<ScheduledCommand>,
) -> Result<(mpsc::UnboundedSender<(String, i64)>, mpsc::UnboundedSender<()>), String> {
    let (send_cmd_tx, mut send_cmd_rx) = mpsc::unbounded_channel::<(String, i64)>();
    let (shutdown_tx, mut shutdown_rx) = mpsc::unbounded_channel::<()>();

    log::info!("choirlib client: connecting to {}", ws_url);

    // Block on connect + join in a small runtime
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("runtime: {}", e))?;

    let ws_stream = rt.block_on(async {
        let (stream, _) = connect_async(&ws_url).await.map_err(|e| format!("connect: {}", e))?;
        Ok::<_, String>(stream)
    })?;

    log::info!("choirlib client: connected, sending join choir_name={}", choir_name);

    let (mut ws_sender, mut ws_receiver) = ws_stream.split();

    // Send join
    let join_req = JoinRequest::new(choir_name.clone(), password);
    let join_json = serde_json::to_string(&join_req).unwrap();
    rt.block_on(ws_sender.send(Message::Text(join_json))).map_err(|e| format!("send join: {}", e))?;

    // Wait for join_ok
    let first = rt.block_on(ws_receiver.next());
    let first = match first {
        Some(Ok(Message::Text(t))) => t,
        _ => {
            log::error!("choirlib client: no join response from server");
            return Err("no join response".to_string());
        }
    };
    let resp: JoinResponse = serde_json::from_str(&first).map_err(|_| "invalid join response")?;
    if resp.typ != "join_ok" {
        let msg = resp.message.unwrap_or_else(|| "join failed".to_string());
        log::error!("choirlib client: join failed: {}", msg);
        return Err(msg);
    }
    log::info!("choirlib client: joined successfully");

    // Spawn thread to run send + receive loops (so we don't block the caller forever).
    // Catch panic so we can report it (panic would drop the stream → "Connection reset" on server).
    std::thread::spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("choirlib client runtime");
            rt.block_on(async move {
            // Send commands and periodic Ping keepalive (some stacks close idle connections; Ping prevents that).
            let send_loop = async {
                let mut keepalive = interval(Duration::from_secs(15));
                keepalive.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                loop {
                    tokio::select! {
                        msg = send_cmd_rx.recv() => {
                            let Some((cmd, execute_at)) = msg else { break };
                            let msg = CommandMessage::new(&cmd, execute_at);
                            let json = serde_json::to_string(&msg).unwrap();
                            if ws_sender.send(Message::Text(json)).await.is_err() {
                                log::warn!("choirlib client: send_loop ending — ws_sender.send failed (connection closed?)");
                                break;
                            }
                        }
                        _ = keepalive.tick() => {
                            if ws_sender.send(Message::Ping(vec![].into())).await.is_err() {
                                log::warn!("choirlib client: send_loop ending — Ping send failed");
                                break;
                            }
                        }
                    }
                }
            };
            let recv_loop = async {
                while let Some(result) = ws_receiver.next().await {
                    match result {
                        Ok(Message::Text(text)) => {
                            if let Ok(cmd) = serde_json::from_str::<CommandMessage>(&text) {
                                if cmd.typ == "command" {
                                    log::info!(
                                        "choirlib client: command received cmd={} execute_at={}",
                                        cmd.command,
                                        cmd.execute_at
                                    );
                                    let _ = command_tx.send(ScheduledCommand {
                                        command: cmd.command,
                                        execute_at_ms: cmd.execute_at,
                                    });
                                }
                            }
                        }
                        Ok(Message::Close(frame)) => {
                            log::info!(
                                "[CHOIR-CLIENT] recv_loop ending — received Close frame from server: {:?}",
                                frame
                            );
                            break;
                        }
                        Ok(_) => {}
                        Err(e) => {
                            log::error!("[CHOIR-CLIENT] recv_loop ending — WebSocket error: {}", e);
                            break;
                        }
                    }
                }
                // Stream ended (None) without explicit Close
                log::info!("[CHOIR-CLIENT] recv_loop ending — WebSocket stream ended (connection closed by peer or network)");
            };
            tokio::select! {
                _ = send_loop => {
                    let reason = "send_loop ended (send_cmd_rx closed or ws_sender failed)".to_string();
                    crate::set_client_exit_reason(reason.clone());
                    crate::clear_client_on_disconnect();
                    log::info!("[CHOIR-CLIENT] CONNECTION DROPPED — {} (thread exiting; app should show Join)", reason);
                }
                _ = recv_loop => {
                    let reason = "recv_loop ended (WebSocket stream closed or error)".to_string();
                    crate::set_client_exit_reason(reason.clone());
                    crate::clear_client_on_disconnect();
                    log::info!("[CHOIR-CLIENT] CONNECTION DROPPED — {} (thread exiting; app should show Join)", reason);
                }
                _ = shutdown_rx.recv() => {
                    let reason = "shutdown received (clientLeave called)".to_string();
                    crate::set_client_exit_reason(reason.clone());
                    crate::clear_client_on_disconnect();
                    log::info!("[CHOIR-CLIENT] CONNECTION DROPPED — {}", reason);
                }
            }
        });
        }));
        if let Err(e) = result {
            let reason = format!(
                "client task panicked (causes Connection reset on server): {:?}",
                e
            );
            crate::set_client_exit_reason(reason.clone());
            crate::clear_client_on_disconnect();
            log::error!("[CHOIR-CLIENT] CONNECTION DROPPED — {}", reason);
        }
    });

    log::info!("[CHOIR-CLIENT] CONNECTED and joined (background task running)");
    Ok((send_cmd_tx, shutdown_tx))
}

/// Current time as Unix milliseconds (for execute_at).
pub fn unix_time_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
