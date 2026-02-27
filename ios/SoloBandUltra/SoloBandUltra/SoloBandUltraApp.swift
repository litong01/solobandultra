import SwiftUI
import AVFoundation
import KindeSDK

@main
struct SoloBandUltraApp: App {
    @StateObject private var audioSessionManager: AudioSessionManager
    @StateObject private var playbackManager: PlaybackManager
    @StateObject private var midiSettings = MidiSettings()
    @StateObject private var authManager: AuthManager
    @StateObject private var feedbackManager = FeedbackManager()

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
                .environmentObject(feedbackManager)
                .onOpenURL { url in
                    handleIncomingFile(url)
                }
                .task { await loadBundledMbkFiles() }
        }
    }

    /// Scan the app bundle's sheetmusic folder for .mbk files and register each one
    /// as a playlist source in `midiSettings.activeBundles`.  Runs on app launch via
    /// `.task`; heavy work (file I/O + ZIP extraction) is offloaded to a background task
    /// so the main actor is never blocked.
    private func loadBundledMbkFiles() async {
        guard let folder = Bundle.main.url(forResource: "sheetmusic", withExtension: nil) else { return }

        let allFiles: [URL]
        do {
            allFiles = try FileManager.default.contentsOfDirectory(
                at: folder,
                includingPropertiesForKeys: nil
            )
        } catch { return }

        let mbkFiles = allFiles.filter { $0.pathExtension.lowercased() == "mbk" }
        guard !mbkFiles.isEmpty else { return }

        for fileURL in mbkFiles {
            do {
                let bundle = try await Task.detached(priority: .utility) {
                    guard let data = try? Data(contentsOf: fileURL) else {
                        throw NSError(domain: "MbkExtractor", code: 1,
                                      userInfo: [NSLocalizedDescriptionKey: "Could not read \(fileURL.lastPathComponent)"])
                    }
                    return try SoloBandUltraApp.extractAndParseMbk(data: data)
                }.value
                if midiSettings.activeBundles[bundle.bookId] == nil {
                    midiSettings.activeBundles[bundle.bookId] = bundle
                }
            } catch {
                // Silently skip malformed bundled .mbk files.
            }
        }
    }

    /// Handle a file URL passed via "Open With" / file association.
    private func handleIncomingFile(_ url: URL) {
        let didStart = url.startAccessingSecurityScopedResource()
        defer { if didStart { url.stopAccessingSecurityScopedResource() } }

        guard let data = try? Data(contentsOf: url) else { return }

        let filename = url.lastPathComponent
        let ext = (filename as NSString).pathExtension.lowercased()

        if ext == "mbk" {
            handleIncomingMbk(data: data, filename: filename)
        } else if ext == "musicxml" || ext == "mxl" || ext == "xml" {
            if authManager.isAuthenticated {
                midiSettings.externalFileData = data
                midiSettings.externalFileName = filename
                midiSettings.externalFileVersion += 1
                midiSettings.selectedSourceId = "external"
                midiSettings.selectedFileUrl = "external://\(filename)"
            } else {
                authManager.login(then: .loadExternal(data, filename))
            }
        }
    }

    /// Extract an .mbk bundle, parse its index, and activate it as the playlist source.
    private func handleIncomingMbk(data: Data, filename: String) {
        DispatchQueue.global(qos: .userInitiated).async {
            do {
                let bundle = try Self.extractAndParseMbk(data: data)
                DispatchQueue.main.async {
                    midiSettings.activeBundles[bundle.bookId] = bundle
                    let sourceId = "mbk:\(bundle.bookId)"
                    midiSettings.selectedSourceId = sourceId
                    // Select the first unlocked piece, falling back to first piece.
                    if let first = bundle.unlockedPieces.first ?? bundle.allPieces.first {
                        midiSettings.selectedFileUrl = "mbk://\(bundle.bookId)/\(first.xml)"
                    }
                }
            } catch {
                let msg = "Could not open \"\(filename)\": \(error.localizedDescription)"
                DispatchQueue.main.async { midiSettings.errorMessage = msg }
            }
        }
    }

    /// Unzip the archive, parse book.json, and return a BookBundle.
    ///
    /// Strategy: extract once into a UUID-named staging directory, read book.json to
    /// learn the bookId, then atomically move the staging dir to its canonical cache
    /// location.  This avoids the old double-extraction approach and is immune to
    /// errors on individual entries (e.g. files with multi-byte UTF-8 names).
    nonisolated static func extractAndParseMbk(data: Data) throws -> BookBundle {
        let fm = FileManager.default
        let cacheRoot = fm.urls(for: .cachesDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("mbk")

        // Staging directory — cleaned up on any failure.
        let stagingDir = cacheRoot.appendingPathComponent("_staging_\(UUID().uuidString.prefix(8))")
        var movedToFinal = false
        defer { if !movedToFinal { try? fm.removeItem(at: stagingDir) } }

        // Extract the whole archive into staging.
        if let err = mbk_extract(data, stagingDir) { throw err }

        // Read book.json from the extracted staging dir.
        let stagingJson = stagingDir.appendingPathComponent("book.json")
        guard let jsonData = try? Data(contentsOf: stagingJson), !jsonData.isEmpty else {
            throw NSError(domain: "MbkExtractor", code: 10,
                          userInfo: [NSLocalizedDescriptionKey: "book.json not found or empty in bundle"])
        }

        // Parse to discover bookId (cacheDir is irrelevant at this stage).
        let parsed = try BookBundle.parse(jsonData: jsonData, cacheDir: stagingDir)
        let cacheDir = cacheRoot.appendingPathComponent(parsed.bookId)

        // Replace any stale cached copy then move staging → canonical location.
        try? fm.removeItem(at: cacheDir)
        try fm.moveItem(at: stagingDir, to: cacheDir)
        movedToFinal = true

        // Return a bundle that points to the now-canonical cacheDir.
        return BookBundle(bookId:   parsed.bookId,
                          version:  parsed.version,
                          title:    parsed.title,
                          pages:    parsed.pages,
                          cacheDir: cacheDir)
    }

    /// Configure AVAudioSession once at app launch.
    ///
    /// Using `.playAndRecord` with `.defaultToSpeaker` so audio plays from the speaker
    /// and the mic is available for the feedback pitch tap.
    ///
    /// We use mode `.default` (not `.measurement` — that gives very low/no mic input,
    /// and not `.videoRecording` — that could stop playback when capture starts).
    /// With a separate input engine and starting input before playback, `.default`
    /// keeps both playback and capture working.
    ///
    /// We avoid `.mixWithOthers` so our app owns the session and playback isn't ducked.
    /// preferredSampleRate 48000 matches our WAV and typical hardware.
    private static func configureAudioSession() {
        do {
            let session = AVAudioSession.sharedInstance()
            try session.setCategory(.playAndRecord, mode: .default,
                                    options: [.defaultToSpeaker, .allowBluetooth])
            try session.setPreferredSampleRate(48000)
            try session.setActive(true)
        } catch {
            print("[AudioSession] Failed to configure: \(error.localizedDescription)")
        }
    }
}
