import SwiftUI
import AVFoundation

@main
struct SoloBandUltraApp: App {
    @StateObject private var audioSessionManager = AudioSessionManager()

    init() {
        configureAudioSession()
    }

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environmentObject(audioSessionManager)
        }
    }

    /// Configure AVAudioSession for playback category.
    /// This ensures audio plays even when the device silent/mute switch is on.
    private func configureAudioSession() {
        do {
            let session = AVAudioSession.sharedInstance()
            try session.setCategory(.playback, mode: .default, options: [])
            try session.setActive(true)
            print("[AudioSession] Configured for playback (silent mode override enabled)")
        } catch {
            print("[AudioSession] Failed to configure: \(error.localizedDescription)")
        }
    }
}
