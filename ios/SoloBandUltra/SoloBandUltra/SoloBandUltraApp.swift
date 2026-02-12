import SwiftUI
import AVFoundation
import KindeSDK

@main
struct SoloBandUltraApp: App {
    @StateObject private var audioSessionManager: AudioSessionManager
    @StateObject private var playbackManager: PlaybackManager
    @StateObject private var midiSettings = MidiSettings()
    @StateObject private var authManager: AuthManager

    init() {
        // Configure the Kinde authentication SDK FIRST — AuthManager.init() checks isAuthenticated.
        KindeSDKAPI.configure()

        // Initialize shared AudioSessionManager
        let asm = AudioSessionManager()
        _audioSessionManager = StateObject(wrappedValue: asm)
        _playbackManager = StateObject(wrappedValue: PlaybackManager(audioSessionManager: asm))
        _authManager = StateObject(wrappedValue: AuthManager())

        // Configure audio session after all stored properties are initialized
        Self.configureAudioSession()
    }

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environmentObject(audioSessionManager)
                .environmentObject(playbackManager)
                .environmentObject(midiSettings)
                .environmentObject(authManager)
                .onOpenURL { url in
                    handleIncomingFile(url)
                }
        }
    }

    /// Handle a file URL passed via "Open With" / file association.
    private func handleIncomingFile(_ url: URL) {
        // Security-scoped access may or may not be required depending on the source
        let didStart = url.startAccessingSecurityScopedResource()
        defer { if didStart { url.stopAccessingSecurityScopedResource() } }

        guard let data = try? Data(contentsOf: url) else { return }

        let filename = url.lastPathComponent
        let ext = (filename as NSString).pathExtension.lowercased()
        guard ext == "musicxml" || ext == "mxl" || ext == "xml" else { return }

        if authManager.isAuthenticated {
            // User is logged in — load the file immediately.
            midiSettings.externalFileData = data
            midiSettings.externalFileName = filename
            midiSettings.externalFileVersion += 1
            midiSettings.selectedSourceId = "external"
            midiSettings.selectedFileUrl = "external://\(filename)"
        } else {
            // Not logged in — defer the file load until after login.
            authManager.login(then: .loadExternal(data, filename))
        }
    }

    /// Configure AVAudioSession for playback category.
    /// This ensures audio plays even when the device silent/mute switch is on.
    private static func configureAudioSession() {
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
