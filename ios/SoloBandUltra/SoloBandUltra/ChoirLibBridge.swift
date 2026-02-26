//
//  ChoirLibBridge.swift
//  SoloBandUltra
//
//  Swift wrapper around the Rust choirlib C FFI for choir WebSocket server/client and mDNS.
//

import Foundation

/// Bridge to Rust choirlib: choir server, client, discovery, and command sync.
enum ChoirLib {

    // MARK: - Server (leader)

    /// Start choir server. Returns port (0 = error).
    static func serverStart(choirName: String, password: String) -> UInt16 {
        choirName.withCString { namePtr in
            password.withCString { passPtr in
                UInt16(choir_server_start(namePtr, passPtr))
            }
        }
    }

    /// Stop choir server.
    static func serverStop() {
        choir_server_stop()
    }

    /// When hosting, returns listen address "IP:port" for display (e.g. "192.168.1.5:12345"). Nil when not hosting.
    static func serverListenAddress() -> String? {
        let ptr = choir_server_listen_address()
        guard ptr != nil else { return nil }
        defer { choir_free_string(ptr) }
        return String(cString: ptr!)
    }

    /// Leader: connect as client to own server (call after serverStart). Returns true on success.
    static func leaderConnect(port: UInt16) -> Bool {
        choir_leader_connect(port) != 0
    }

    // MARK: - Discovery

    /// Discover choirs (blocking, up to timeoutSecs). Returns JSON array of {choir_name, ws_url}; nil on error.
    static func discover(timeoutSecs: UInt32 = 5) -> String? {
        let ptr = choir_discover(timeoutSecs)
        guard ptr != nil else { return nil }
        defer { choir_free_string(ptr) }
        return String(cString: ptr!)
    }

    /// After discover(timeoutSecs:) returned nil, returns the error message (e.g. "mDNS: ..."). Nil if no error or success.
    static func discoverLastError() -> String? {
        let ptr = choir_discover_last_error()
        guard ptr != nil else { return nil }
        defer { choir_free_string(ptr) }
        return String(cString: ptr!)
    }

    // MARK: - Client (member)

    /// Join choir (blocking). Resolves via mDNS, connects, authenticates. Call from background. Returns true on success.
    static func clientJoin(choirName: String, password: String) -> Bool {
        choirName.withCString { namePtr in
            password.withCString { passPtr in
                choir_client_join(namePtr, passPtr) != 0
            }
        }
    }

    /// Leave choir.
    static func clientLeave() {
        choir_client_leave()
    }

    /// True if the client connection is still alive (background task running). When false, UI should show Join not Leave.
    static func clientConnected() -> Bool {
        choir_client_connected() != 0
    }

    // MARK: - Commands

    /// Compute execute_at = now + delayMs (for sync).
    static func executeAtMs(delayMs: Int64) -> Int64 {
        choir_execute_at_ms(delayMs)
    }

    /// Send command as leader. executeAtMs from executeAtMs(delayMs).
    static func sendCommand(_ command: String, executeAtMs: Int64) -> Bool {
        command.withCString { ptr in
            choir_send_command(ptr, executeAtMs) != 0
        }
    }

    /// When sendCommand returned false, returns the failure reason (e.g. "channel closed").
    static func sendCommandLastError() -> String? {
        let ptr = choir_send_command_last_error()
        guard ptr != nil else { return nil }
        defer { choir_free_string(ptr) }
        return String(cString: ptr!)
    }

    /// Last reason the leader client task exited (why the connection dropped). Use to diagnose reconnects.
    static func clientExitReason() -> String? {
        let ptr = choir_client_exit_reason()
        guard ptr != nil else { return nil }
        defer { choir_free_string(ptr) }
        return String(cString: ptr!)
    }

    /// Last reason the server saw a client disconnect (server's view of why connection closed).
    static func serverLastDisconnectReason() -> String? {
        let ptr = choir_server_last_disconnect_reason()
        guard ptr != nil else { return nil }
        defer { choir_free_string(ptr) }
        return String(cString: ptr!)
    }

    /// Poll next scheduled command. Returns (command, executeAtMs) or nil.
    static func pollCommand() -> (command: String, executeAtMs: Int64)? {
        var buf = [CChar](repeating: 0, count: 64)
        var executeAt: Int64 = 0
        let ok = buf.withUnsafeMutableBufferPointer { b in
            choir_poll_command(b.baseAddress, 64, &executeAt)
        }
        guard ok != 0 else { return nil }
        let cmd = String(cString: buf)
        return (cmd, executeAt)
    }
}
