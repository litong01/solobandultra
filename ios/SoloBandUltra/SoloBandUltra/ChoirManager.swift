//
//  ChoirManager.swift
//  SoloBandUltra
//
//  Manages cloud choir client: join by room name + password; scheduled command execution.
//  Auto-reconnects on connection loss; does not reconnect when the user taps Leave.
//  WebSocket endpoint is from Info.plist CHOIR_WS_BASE_URL.
//

import Foundation
import Combine

/// Choir state and sync. Join cloud room with room name + password; any member can send play/pause/stop/prev/next.
/// Reconnects automatically when the connection drops (network/server); does not reconnect after user taps Leave.
final class ChoirManager: ObservableObject {
    /// We are a member (connected to a choir room).
    @Published private(set) var isJoined = false
    /// Last join error (wrong password, network, etc.); clear when user taps Join again.
    @Published var joinError: String?
    /// When true, we are reconnecting after a drop (show optional “Reconnecting…” in UI).
    @Published private(set) var isReconnecting = false

    private var client: CloudChoirClient?
    private let commandDelayMs: Int64 = 500

    /// Set to true only when the user explicitly taps Leave. When onLeft fires, we reconnect only if this is false.
    private var userRequestedLeave = false
    /// True while performJoin is in progress (initial or reconnect). When onLeft fires in that case, don’t schedule another reconnect.
    private var joiningInProgress = false
    /// Stored for auto-reconnect; cleared on leave().
    private var reconnectRoom: String?
    private var reconnectPassword: String?
    private var reconnectTokenProvider: (() async throws -> String)?
    private var reconnectTask: Task<Void, Never>?
    private var reconnectAttempt = 0
    private let maxReconnectDelay: TimeInterval = 30

    /// When a scheduled command is received, this is called with (command, executeAtMs). Schedule execution for executeAtMs.
    var onScheduledCommand: ((String, Int64) -> Void)?

    /// Base URL for cloud WebSocket (from Info.plist CHOIR_WS_BASE_URL). Empty = not configured.
    static var choirWSBaseURL: String {
        (Bundle.main.object(forInfoDictionaryKey: "CHOIR_WS_BASE_URL") as? String)?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
    }

    /// Join cloud choir: room name + password. Token provider (e.g. Kinde) is required. First to join creates the room.
    func join(choirName: String, password: String, tokenProvider: @escaping () async throws -> String) {
        guard !choirName.isEmpty else { return }
        let baseURL = Self.choirWSBaseURL
        guard !baseURL.isEmpty else {
            DispatchQueue.main.async { self.joinError = "Choir server URL not configured (CHOIR_WS_BASE_URL)." }
            return
        }
        joinError = nil
        userRequestedLeave = true
        leave()
        userRequestedLeave = false

        let room = choirName.trimmingCharacters(in: .whitespacesAndNewlines)
        let pass = password
        reconnectRoom = room
        reconnectPassword = pass
        reconnectTokenProvider = tokenProvider
        reconnectAttempt = 0

        performJoin(room: room, password: pass, tokenProvider: tokenProvider)
    }

    /// Performs one connect+join. Used for initial join and for auto-reconnect.
    private func performJoin(room: String, password: String, tokenProvider: @escaping () async throws -> String) {
        let baseURL = Self.choirWSBaseURL
        guard !baseURL.isEmpty else { return }

        joiningInProgress = true
        Task { @MainActor in
            do {
                let token = try await tokenProvider()
                let c = CloudChoirClient(
                    baseURL: baseURL,
                    token: token,
                    room: room,
                    password: password,
                    onJoined: { [weak self] in
                        DispatchQueue.main.async {
                            guard let self = self else { return }
                            self.isJoined = true
                            self.joinError = nil
                            self.isReconnecting = false
                            self.reconnectAttempt = 0
                            self.joiningInProgress = false
                        }
                    },
                    onLeft: { [weak self] _ in
                        DispatchQueue.main.async {
                            self?.handleLeft()
                        }
                    },
                    onCommand: { [weak self] cmd, executeAtMs in
                        DispatchQueue.main.async { self?.onScheduledCommand?(cmd, executeAtMs) }
                    }
                )
                client = c
                try await c.connect()
            } catch {
                await MainActor.run {
                    if userRequestedLeave { return }
                    joiningInProgress = false
                    joinError = error.localizedDescription
                    isJoined = false
                    client = nil
                    isReconnecting = true
                    scheduleReconnect()
                }
            }
        }
    }

    /// Called when the socket closes (network drop or user Leave). Reconnect only if user did not tap Leave.
    private func handleLeft() {
        let wasUserLeave = userRequestedLeave
        isJoined = false
        client = nil
        if wasUserLeave {
            userRequestedLeave = false
            return
        }
        if joiningInProgress {
            return
        }
        isReconnecting = true
        scheduleReconnect()
    }

    private func scheduleReconnect() {
        reconnectTask?.cancel()
        guard let room = reconnectRoom,
              let password = reconnectPassword,
              let tokenProvider = reconnectTokenProvider,
              !userRequestedLeave else { return }

        let delay = min(pow(2.0, Double(reconnectAttempt)), maxReconnectDelay)
        reconnectAttempt += 1

        reconnectTask = Task { @MainActor in
            try? await Task.sleep(nanoseconds: UInt64(delay * 1_000_000_000))
            if userRequestedLeave { return }
            performJoin(room: room, password: password, tokenProvider: tokenProvider)
        }
    }

    /// User explicitly left the choir; do not auto-reconnect.
    func leave() {
        userRequestedLeave = true
        reconnectTask?.cancel()
        reconnectTask = nil
        reconnectRoom = nil
        reconnectPassword = nil
        reconnectTokenProvider = nil
        client?.disconnect()
        client = nil
        isJoined = false
        isReconnecting = false
    }

    /// Send command to choir (play, pause, stop, prev, next).
    func sendCommand(_ command: String) {
        guard isJoined else { return }
        client?.sendCommand(command)
    }
}
