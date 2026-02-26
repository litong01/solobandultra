//
//  ChoirManager.swift
//  SoloBandUltra
//
//  Manages choir client state (iOS cannot host; only join). Discovery, join, and scheduled command execution.
//

import Foundation
import Combine

/// Choir state and sync. iOS joins only; any joined member can send play/pause/stop/prev/next.
final class ChoirManager: ObservableObject {
    /// We are a member (client connected to a choir).
    @Published private(set) var isJoined = false
    /// Last discovered choir name (from mDNS) for "Join" section.
    @Published var discoveredChoirName = ""
    /// When discovery returns nothing, error message if any (e.g. "mDNS: ..."); nil means no error or empty result.
    @Published var discoveryError: String?

    private var pollTimer: Timer?
    private let commandDelayMs: Int64 = 500

    /// When a scheduled command is received, this is called with (command, executeAtMs). Schedule execution for executeAtMs.
    var onScheduledCommand: ((String, Int64) -> Void)?

    func discover() {
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            let json = ChoirLib.discover(timeoutSecs: 5)
            let errorMsg = ChoirLib.discoverLastError()
            DispatchQueue.main.async {
                self?.discoveryError = errorMsg
                guard let json = json,
                      let data = json.data(using: .utf8),
                      let list = try? JSONDecoder().decode([DiscoveredChoir].self, from: data),
                      let first = list.first else {
                    self?.discoveredChoirName = ""
                    return
                }
                self?.discoveredChoirName = first.choir_name
            }
        }
    }

    func join(choirName: String, password: String) {
        guard !choirName.isEmpty else { return }
        print("[Choir] join choirName=\(choirName)")
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            let ok = ChoirLib.clientJoin(choirName: choirName, password: password)
            print("[Choir] choirClientJoin ok=\(ok)")
            DispatchQueue.main.async {
                if ok {
                    self?.isJoined = true
                    self?.startPolling()
                }
            }
        }
    }

    func leave() {
        print("[Choir] leave")
        ChoirLib.clientLeave()
        isJoined = false
        stopPolling()
    }

    /// Send command to choir (play, pause, stop, prev, next). Any joined member can send; server broadcasts to all.
    func sendCommand(_ command: String) {
        guard isJoined else { print("[Choir] sendCommand ignored (not joined)"); return }
        print("[Choir] sendCommand(\(command))")
        let executeAt = ChoirLib.executeAtMs(delayMs: commandDelayMs)
        let ok = ChoirLib.sendCommand(command, executeAtMs: executeAt)
        if !ok {
            let reason = ChoirLib.sendCommandLastError() ?? "unknown"
            print("[Choir] choirSendCommand failed: \(reason)")
        }
    }

    private func startPolling() {
        stopPolling()
        let timer = Timer.scheduledTimer(withTimeInterval: 0.1, repeats: true) { [weak self] _ in
            self?.pollOnce()
        }
        // Fire even when run loop is in modal/sheet mode (e.g. keyboard or sheet up on iPad).
        RunLoop.main.add(timer, forMode: .common)
        pollTimer = timer
    }

    private func stopPolling() {
        pollTimer?.invalidate()
        pollTimer = nil
    }

    private func pollOnce() {
        // If we think we're joined but the client thread has exited, sync UI to show Join again.
        if isJoined && !ChoirLib.clientConnected() {
            print("[Choir] poll: client disconnected (thread ended), setting isJoined=false")
            DispatchQueue.main.async { [weak self] in
                self?.isJoined = false
                self?.stopPolling()
            }
            return
        }
        guard let (command, executeAtMs) = ChoirLib.pollCommand() else { return }
        print("[Choir] pollCommand received cmd=\(command) executeAtMs=\(executeAtMs)")
        onScheduledCommand?(command, executeAtMs)
    }
}

private struct DiscoveredChoir: Codable {
    let choir_name: String
    let ws_url: String
}
