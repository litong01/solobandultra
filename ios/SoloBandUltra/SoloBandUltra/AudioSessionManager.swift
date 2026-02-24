import Foundation
import AVFoundation
import Combine

/// Manages the AVAudioSession for the app.
/// Ensures audio plays through speakers even when the device mute/silent switch is on.
class AudioSessionManager: ObservableObject {
    @Published var isSessionActive = false
    @Published var currentRoute: String = ""

    private var routeChangeObserver: AnyCancellable?
    private var interruptionObserver: AnyCancellable?

    init() {
        setupObservers()
        updateRouteInfo()
    }

    deinit {
        routeChangeObserver?.cancel()
        interruptionObserver?.cancel()
    }

    // MARK: - Public API

    /// Re-activate the audio session.
    ///
    /// The session category (.playAndRecord + .defaultToSpeaker) is set once at
    /// app launch in SoloBandUltraApp.configureAudioSession().  This method only
    /// re-activates the session (e.g. after an interruption or route change) without
    /// touching the category — changing categories after AVAudioEngine has started
    /// invalidates the inputNode format and crashes the engine.
    func ensureSessionActive() throws {
        try AVAudioSession.sharedInstance().setActive(true)
        isSessionActive = true
        updateRouteInfo()
    }

    /// Deactivates the audio session.
    func deactivateSession() {
        do {
            try AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
            isSessionActive = false
            print("[AudioSessionManager] Session deactivated")
        } catch {
            print("[AudioSessionManager] Failed to deactivate session: \(error.localizedDescription)")
        }
    }

    // MARK: - Private

    private func setupObservers() {
        // Observe audio route changes (headphones plugged/unplugged, etc.)
        routeChangeObserver = NotificationCenter.default
            .publisher(for: AVAudioSession.routeChangeNotification)
            .receive(on: DispatchQueue.main)
            .sink { [weak self] notification in
                self?.handleRouteChange(notification)
            }

        // Observe audio interruptions (phone calls, Siri, etc.)
        interruptionObserver = NotificationCenter.default
            .publisher(for: AVAudioSession.interruptionNotification)
            .receive(on: DispatchQueue.main)
            .sink { [weak self] notification in
                self?.handleInterruption(notification)
            }
    }

    private func handleRouteChange(_ notification: Notification) {
        guard let userInfo = notification.userInfo,
              let reasonValue = userInfo[AVAudioSessionRouteChangeReasonKey] as? UInt,
              let reason = AVAudioSession.RouteChangeReason(rawValue: reasonValue) else {
            return
        }

        switch reason {
        case .newDeviceAvailable:
            print("[AudioSessionManager] New audio device connected")
        case .oldDeviceUnavailable:
            print("[AudioSessionManager] Audio device disconnected")
            // Re-activate session when headphones are unplugged
            try? ensureSessionActive()
        default:
            break
        }

        updateRouteInfo()
    }

    private func handleInterruption(_ notification: Notification) {
        guard let userInfo = notification.userInfo,
              let typeValue = userInfo[AVAudioSessionInterruptionTypeKey] as? UInt,
              let type = AVAudioSession.InterruptionType(rawValue: typeValue) else {
            return
        }

        switch type {
        case .began:
            print("[AudioSessionManager] Audio session interrupted")
            isSessionActive = false
        case .ended:
            if let optionsValue = userInfo[AVAudioSessionInterruptionOptionKey] as? UInt {
                let options = AVAudioSession.InterruptionOptions(rawValue: optionsValue)
                if options.contains(.shouldResume) {
                    try? ensureSessionActive()
                    print("[AudioSessionManager] Audio session interruption ended, resuming")
                }
            }
        @unknown default:
            break
        }
    }

    private func updateRouteInfo() {
        let route = AVAudioSession.sharedInstance().currentRoute
        let outputs = route.outputs.map { $0.portName }.joined(separator: ", ")
        currentRoute = outputs.isEmpty ? "No output" : outputs
    }
}
