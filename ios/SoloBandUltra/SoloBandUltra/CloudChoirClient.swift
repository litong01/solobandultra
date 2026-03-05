//
//  CloudChoirClient.swift
//  SoloBandUltra
//
//  Cloud WebSocket client for choir: connect to wss://base/ws with Kinde Bearer token,
//  send join(room, password); first joiner creates the room. Protocol per docs/websocket.md.
//

import Foundation

/// Cloud choir WebSocket client. Connect with Bearer token, join room; receive/send play, stop, pause, prev, next.
final class CloudChoirClient: @unchecked Sendable {
    private let baseURL: String
    private let token: String
    private let room: String
    private let password: String
    private var task: URLSessionWebSocketTask?
    private let commandDelayMs: Int64 = 500
    private var serverOffsetMs: Int64 = 0  // server_utc_ms - client_utc_ms; convert server startAt -> local
    private let offsetLock = NSLock()
    private let onJoined: () -> Void
    private let onLeft: (String) -> Void
    private let onCommand: (String, Int64) -> Void
    private let queue = DispatchQueue(label: "CloudChoirClient", qos: .userInitiated)
    private var isShutdown = false

    /// - Parameters:
    ///   - baseURL: e.g. "https://your-server.com" (ws URL becomes wss://your-server.com/ws)
    ///   - token: Kinde access token for Authorization: Bearer
    ///   - room: Choir/room name
    ///   - password: Room password (first joiner creates room with it)
    ///   - onJoined: Called on main when join succeeds
    ///   - onLeft: Called on main when connection closes (reason)
    ///   - onCommand: Called on main with (command, executeAtMs) for play/stop/pause/prev/next
    init(
        baseURL: String,
        token: String,
        room: String,
        password: String,
        onJoined: @escaping () -> Void,
        onLeft: @escaping (String) -> Void,
        onCommand: @escaping (String, Int64) -> Void
    ) {
        self.baseURL = baseURL
        self.token = token
        self.room = room
        self.password = password
        self.onJoined = onJoined
        self.onLeft = onLeft
        self.onCommand = onCommand
    }

    /// Connect, send join, then run receive loop. Fails with string error; on success runs receive loop and calls onJoined, then onLeft when done.
    func connect() async throws {
        let wsURL = Self.makeWebSocketURL(baseURL: baseURL)
        var request = URLRequest(url: wsURL)
        request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        let session = URLSession(configuration: .default)
        let wsTask = session.webSocketTask(with: request)
        task = wsTask
        wsTask.resume()

        // Send join
        let clientUtc = Self.iso8601Utc(from: Date())
        let joinBody: [String: Any] = [
            "join": [
                "room": room,
                "password": password,
                "clientUtc": clientUtc
            ]
        ]
        let joinData = try JSONSerialization.data(withJSONObject: joinBody)
        guard let joinStr = String(data: joinData, encoding: .utf8) else { throw CloudChoirError.encode }
        try await withCheckedThrowingContinuation { (cont: CheckedContinuation<Void, Error>) in
            wsTask.send(.string(joinStr)) { err in
                if let e = err { return cont.resume(throwing: e) }
                cont.resume()
            }
        }

        // First message must be join response
        let firstStr = try await receiveMessage(wsTask)
        guard let str = firstStr,
              let data = str.data(using: .utf8),
              let msg = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw CloudChoirError.noJoinResponse
        }
        if let errMsg = msg["error"] as? String {
            throw CloudChoirError.serverError(errMsg)
        }
        guard let ok = msg["ok"] as? Bool, ok,
              let serverUtc = msg["serverUtc"] as? String else {
            throw CloudChoirError.invalidJoinResponse
        }
        let clientMs = Self.iso8601ToMs(clientUtc) ?? 0
        let serverMs = Self.iso8601ToMs(serverUtc) ?? 0
        offsetLock.withLock {
            serverOffsetMs = serverMs - clientMs
        }

        DispatchQueue.main.async { self.onJoined() }

        // Optional: refine offset once with GET /time to reduce RTT error from join (no periodic refresh).
        await refreshServerTime()

        // Receive loop: get messages; parse commands and deliver executeAtMs (local).
        while !isShutdown {
            do {
                guard let text = try await receiveMessage(wsTask) else { break }
                if let (cmd, executeAtMs) = parseCommand(text) {
                    DispatchQueue.main.async { self.onCommand(cmd, executeAtMs) }
                }
            } catch {
                break
            }
        }
        DispatchQueue.main.async { self.onLeft("connection closed") }
    }

    /// Close the WebSocket (optional leave first: send {"leave":{}} then close).
    func disconnect() {
        queue.async { [weak self] in
            guard let self = self else { return }
            self.isShutdown = true
            self.task?.cancel(with: .goingAway, reason: nil)
            self.task = nil
        }
    }

    /// Fetch GET /time (optional server endpoint), update serverOffsetMs so command execution stays in sync.
    private func refreshServerTime() async {
        guard let timeURL = Self.makeTimeURL(baseURL: baseURL) else { return }
        var request = URLRequest(url: timeURL)
        request.httpMethod = "GET"
        do {
            let (data, _) = try await URLSession.shared.data(for: request)
            guard let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
                  let serverUtc = json["utc"] as? String,
                  let serverMs = Self.iso8601ToMs(serverUtc) else { return }
            let clientMs = Int64(Date().timeIntervalSince1970 * 1000)
            offsetLock.withLock {
                serverOffsetMs = serverMs - clientMs
            }
        } catch {
            // GET /time is optional; keep using offset from join
        }
    }

    /// Send a room command (play, stop, pause, prev, next). startAt = now + delay in client UTC per protocol.
    func sendCommand(_ command: String) {
        let at = Date().addingTimeInterval(TimeInterval(commandDelayMs) / 1000)
        let startAt = Self.iso8601Utc(from: at)
        let body: [String: Any] = [command: ["startAt": startAt, "comment": command]]
        guard let data = try? JSONSerialization.data(withJSONObject: body),
              let str = String(data: data, encoding: .utf8) else { return }
        queue.async { [weak self] in
            self?.task?.send(.string(str)) { _ in }
        }
    }

    private func receiveMessage(_ wsTask: URLSessionWebSocketTask) async throws -> String? {
        try await withCheckedThrowingContinuation { cont in
            wsTask.receive { result in
                switch result {
                case .success(let msg):
                    if case .string(let s) = msg { cont.resume(returning: s) }
                    else if case .data(let d) = msg, let s = String(data: d, encoding: .utf8) { cont.resume(returning: s) }
                    else { cont.resume(returning: nil) }
                case .failure(let e): cont.resume(throwing: e)
                }
            }
        }
    }

    private func parseCommand(_ json: String) -> (String, Int64)? {
        guard let data = json.data(using: .utf8),
              let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return nil }
        let cmd: String?
        let startAt: String?
        if let v = obj["play"] as? [String: Any], let at = v["startAt"] as? String { cmd = "play"; startAt = at }
        else if let v = obj["stop"] as? [String: Any], let at = v["startAt"] as? String { cmd = "stop"; startAt = at }
        else if let v = obj["pause"] as? [String: Any], let at = v["startAt"] as? String { cmd = "pause"; startAt = at }
        else if let v = obj["prev"] as? [String: Any], let at = v["startAt"] as? String { cmd = "prev"; startAt = at }
        else if let v = obj["next"] as? [String: Any], let at = v["startAt"] as? String { cmd = "next"; startAt = at }
        else { return nil }
        guard let c = cmd, let at = startAt else { return nil }
        let serverMs = Self.iso8601ToMs(at) ?? 0
        let offset = offsetLock.withLock { serverOffsetMs }
        let executeAtMs = serverMs - offset
        return (c, executeAtMs)
    }

    private static func makeWebSocketURL(baseURL: String) -> URL {
        let trimmed = baseURL.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
        let wsScheme = trimmed.lowercased().hasPrefix("https") ? "wss" : "ws"
        let host = trimmed
            .replacingOccurrences(of: "https://", with: "")
            .replacingOccurrences(of: "http://", with: "")
        let path = host.contains("/") ? "" : "/ws"
        let base = "\(wsScheme)://\(host)\(path)"
        let urlStr = path.isEmpty ? "\(base)/ws" : base
        return URL(string: urlStr)!
    }

    private static func makeTimeURL(baseURL: String) -> URL? {
        let trimmed = baseURL.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
        let scheme = trimmed.lowercased().hasPrefix("https") ? "https" : "http"
        let host = trimmed
            .replacingOccurrences(of: "https://", with: "")
            .replacingOccurrences(of: "http://", with: "")
        let path = host.contains("/") ? "" : "/time"
        let urlStr = path.isEmpty ? "\(scheme)://\(host)/time" : "\(scheme)://\(host)\(path)"
        return URL(string: urlStr)
    }

    private static func iso8601Utc(from date: Date) -> String {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        formatter.timeZone = TimeZone(identifier: "UTC")!
        return formatter.string(from: date)
    }

    private static func iso8601ToMs(_ s: String) -> Int64? {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        formatter.timeZone = TimeZone(identifier: "UTC")!
        guard let d = formatter.date(from: s) else { return nil }
        return Int64(d.timeIntervalSince1970 * 1000)
    }
}

enum CloudChoirError: LocalizedError {
    case encode
    case noJoinResponse
    case invalidJoinResponse
    case serverError(String)
    var errorDescription: String? {
        switch self {
        case .encode: return "Encode failed"
        case .noJoinResponse: return "No join response"
        case .invalidJoinResponse: return "Invalid join response"
        case .serverError(let s): return s
        }
    }
}
