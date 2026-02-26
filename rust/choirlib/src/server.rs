//! WebSocket server: one choir per server, auth by password. Any joined client can send commands; server broadcasts to all.

use std::collections::HashMap;
use std::sync::Arc;
use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::accept_async;

use crate::protocol::{CommandMessage, JoinRequest, JoinResponse, CHOIR_SERVICE_TYPE};
use mdns_sd::{ServiceDaemon, ServiceInfo};

type Tx = mpsc::UnboundedSender<String>;
type ClientId = u32;

struct ServerState {
    choir_name: String,
    password: String,
    clients: HashMap<ClientId, Tx>,
    _next_id: ClientId,
    broadcast_tx: broadcast::Sender<String>,
}

pub struct ServerHandle {
    shutdown_tx: mpsc::UnboundedSender<()>,
    #[allow(dead_code)] // held so the server thread stays joined, not detached
    join_handle: std::thread::JoinHandle<()>,
}

impl ServerHandle {
    /// Signal the server to stop.
    pub fn stop(&self) {
        let _ = self.shutdown_tx.send(());
    }
}

/// Run WebSocket server on a given port (0 = pick any). Returns (actual_port, handle, advertised_address).
/// Registers mDNS with choir_name so members can discover.
/// advertised_address is "IP:port" (e.g. "192.168.1.5:12345") for display; other devices need this to connect if mDNS fails.
pub fn run_server(
    choir_name: String,
    password: String,
    port: u16,
) -> Result<(u16, ServerHandle, String), String> {
    let (shutdown_tx, mut shutdown_rx) = mpsc::unbounded_channel::<()>();
    let actual_port = if port == 0 {
        let listener = std::net::TcpListener::bind("0.0.0.0:0").map_err(|e| format!("bind: {}", e))?;
        let p = listener.local_addr().map_err(|e| format!("local_addr: {}", e))?.port();
        drop(listener);
        p
    } else {
        port
    };

    // Compute advertised IP in this thread so we can return it; use same logic as in server thread for mDNS.
    let local_ip = local_ip_address::local_ip()
        .map(|ip| ip.to_string())
        .unwrap_or_else(|_| "127.0.0.1".to_string());
    let advertised_address = format!("{}:{}", local_ip, actual_port);
    if local_ip == "127.0.0.1" {
        log::warn!(
            "choirlib server: advertising on 127.0.0.1 — other devices will not find this choir via discovery; they can try ws://<this-device-IP>:{}",
            actual_port
        );
    }

    let join_handle = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");
        rt.block_on(async move {
            let listener = match TcpListener::bind(format!("0.0.0.0:{}", actual_port)).await {
                Ok(l) => l,
                Err(e) => {
                    log::error!("choirlib server: failed to bind: {}", e);
                    return;
                }
            };

            // Advertise via mDNS (instance name = choir name; members discover by name)
            let mdns = match ServiceDaemon::new() {
                Ok(d) => d,
                Err(e) => {
                    log::error!("choirlib server: mDNS daemon failed: {}", e);
                    return;
                }
            };
            let host = format!("{}.local.", local_ip.replace('.', "-"));
            let instance = choir_name.clone();
            let service = ServiceInfo::new(
                CHOIR_SERVICE_TYPE,
                &instance,
                &host,
                &local_ip,
                actual_port,
                &[] as &[(&str, &str)],
            );
            if let Err(e) = service {
                log::error!("choirlib server: ServiceInfo::new failed: {:?}", e);
            } else if mdns.register(service.unwrap()).is_err() {
                log::error!("choirlib server: mDNS register failed");
            }

            let state = Arc::new(tokio::sync::Mutex::new(ServerState {
                choir_name: choir_name.clone(),
                password,
                clients: HashMap::new(),
                _next_id: 0,
                broadcast_tx: broadcast::channel(64).0,
            }));

            let shutdown = tokio::select! {
                _ = shutdown_rx.recv() => true,
                _ = accept_loop(listener, state) => false,
            };
            mdns.shutdown().ok();
            if shutdown {
                log::info!("choirlib server: shutdown");
            }
        });
    });

    Ok((
        actual_port,
        ServerHandle {
            shutdown_tx,
            join_handle,
        },
        advertised_address,
    ))
}

async fn accept_loop(listener: TcpListener, state: Arc<tokio::sync::Mutex<ServerState>>) {
    let mut next_id = 0u32;
    loop {
        let (stream, peer) = match listener.accept().await {
            Ok(p) => p,
            Err(e) => {
                log::error!("choirlib server: accept failed: {}", e);
                continue;
            }
        };
        let is_local = peer.ip().is_loopback();
        let id = next_id;
        next_id = next_id.wrapping_add(1);
        log::info!(
            "[CHOIR-SERVER] client id={} CONNECTED peer={} is_local={} (waiting for join)",
            id,
            peer,
            is_local
        );
        let state_clone = state.clone();
        tokio::spawn(async move {
            handle_client(stream, id, state_clone).await;
        });
    }
}

async fn handle_client(
    stream: TcpStream,
    id: ClientId,
    state: Arc<tokio::sync::Mutex<ServerState>>,
) {
    let ws = match accept_async(stream).await {
        Ok(w) => w,
        Err(e) => {
            log::error!("choirlib server: websocket accept failed id={}: {}", id, e);
            return;
        }
    };
    log::info!("choirlib server: websocket accepted id={}", id);
    let (mut ws_sender, mut ws_receiver) = ws.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();

    // Register client
    {
        let mut st = state.lock().await;
        st.clients.insert(id, tx);
    }

    // Forward broadcast to this client
    let mut broadcast_rx = {
        let st = state.lock().await;
        st.broadcast_tx.subscribe()
    };
    let client_tx = {
        let st = state.lock().await;
        st.clients.get(&id).cloned()
    };
    let client_tx = match client_tx {
        Some(t) => t,
        None => return,
    };
    let client_tx_for_spawn = client_tx.clone();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                Some(msg) = rx.recv() => {
                    if ws_sender.send(Message::Text(msg)).await.is_err() {
                        log::info!("[CHOIR-SERVER] forwarder for id={} EXITING — ws_sender.send failed (connection closed?); dropping send half will close connection", id);
                        break;
                    }
                }
                Ok(msg) = broadcast_rx.recv() => {
                    if client_tx_for_spawn.send(msg).is_err() {
                        log::info!("[CHOIR-SERVER] forwarder for id={} EXITING — client_tx.send failed (client unregistered?)", id);
                        break;
                    }
                }
            }
        }
    });

    // Receive from client: first message must be join; then only leader can send commands
    while let Some(Ok(msg)) = ws_receiver.next().await {
        let text = match msg {
            Message::Text(t) => t,
            Message::Close(_) => break,
            _ => continue,
        };
        let join_req: JoinRequest = match serde_json::from_str(&text) {
            Ok(r) => r,
            Err(_) => {
                let _ = client_tx.send(
                    serde_json::to_string(&JoinResponse::err("invalid message".to_string()))
                        .unwrap(),
                );
                continue;
            }
        };
        if join_req.typ != "join" {
            let _ = client_tx.send(
                serde_json::to_string(&JoinResponse::err("first message must be join".to_string()))
                    .unwrap(),
            );
            continue;
        }
        let st = state.lock().await;
        if join_req.choir_name != st.choir_name || join_req.password != st.password {
            log::info!(
                "choirlib server: client id={} join rejected (name or password mismatch)",
                id
            );
            let _ = client_tx.send(
                serde_json::to_string(&JoinResponse::err("invalid choir name or password".to_string()))
                    .unwrap(),
            );
            continue;
        }
        drop(st);
        let _ = client_tx.send(serde_json::to_string(&JoinResponse::ok()).unwrap());
        log::info!("[CHOIR-SERVER] client id={} JOINED", id);

        break;
    }

    // Any joined client can send commands; server broadcasts to all (including sender).
    loop {
        match ws_receiver.next().await {
            Some(Ok(Message::Text(text))) => {
                let cmd: CommandMessage = match serde_json::from_str(&text) {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                if cmd.typ != "command" {
                    continue;
                }
                log::info!(
                    "[CHOIR-SERVER] broadcast command from client id={} cmd={} execute_at={}",
                    id,
                    cmd.command,
                    cmd.execute_at
                );
                let broadcast_msg = serde_json::to_string(&cmd).unwrap();
                let st = state.lock().await;
                let _ = st.broadcast_tx.send(broadcast_msg);
            }
            Some(Ok(Message::Close(frame))) => {
                let reason = format!("Close frame: {:?}", frame);
                crate::set_server_disconnect_reason(reason.clone());
                log::info!("[CHOIR-SERVER] CONNECTION DROPPED id={} — {}", id, reason);
                break;
            }
            Some(Ok(_)) => {}
            Some(Err(e)) => {
                let reason = format!("WebSocket read error: {}", e);
                crate::set_server_disconnect_reason(reason.clone());
                log::info!("[CHOIR-SERVER] CONNECTION DROPPED id={} — {}", id, reason);
                break;
            }
            None => {
                let reason = "stream ended (client closed or TCP closed)";
                crate::set_server_disconnect_reason(reason.to_string());
                log::info!("[CHOIR-SERVER] CONNECTION DROPPED id={} — {}", id, reason);
                break;
            }
        }
    }

    // Disconnect: remove client
    let mut st = state.lock().await;
    st.clients.remove(&id);
    log::info!("[CHOIR-SERVER] client id={} REMOVED from roster", id);
}

