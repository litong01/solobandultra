import Foundation
import AVFoundation
import WebKit
import Combine

/// Manages audio playback and cursor synchronization.
///
/// Plays pre-rendered WAV audio (synthesized offline by rustysynth in the
/// Rust library) through AVAudioEngine.  This completely replaces the old
/// MIDI-based approach — no AVAudioSequencer, no MIDISynth, no real-time
/// synthesis.
///
/// Audio graph:  AVAudioPlayerNode → AVAudioUnitTimePitch → mainMixer → output
///
/// Cursor animation still runs inside the WKWebView via `requestAnimationFrame`
/// — zero IPC during playback.
///
/// Supports:
/// - **Speed** — `AVAudioUnitTimePitch.rate` (speed without pitch change).
/// - **Mute** — `mainMixerNode.volume = 0` (player keeps running, cursor in sync).
/// - **Repeat** — replays the piece N times automatically.
class PlaybackManager: ObservableObject {
    // MARK: - Published state

    @Published var isPlaying = false
    /// Current position in *music* time (ms).
    var currentTimeMs: Double = 0
    /// Total duration in *music* time (ms).
    var durationMs: Double = 0

    // MARK: - Playback settings

    /// Playback speed multiplier (1.0 = normal, 0.5 = half, 2.0 = double).
    /// Clamped to [0.1, 5.0].  Takes effect immediately via AVAudioUnitTimePitch.
    var speed: Double = 1.0 {
        didSet {
            speed = max(0.1, min(5.0, speed))
            timePitch.rate = Float(speed)
            if isPlaying {
                // Re-sync wall-clock tracking and cursor.
                resyncPlayback()
            }
        }
    }

    /// When `true` audio is silenced but the player keeps running
    /// and the cursor still moves.
    var isMuted: Bool = false {
        didSet {
            engine.mainMixerNode.outputVolume = isMuted ? 0 : 1
        }
    }

    /// Total number of times to play (1 = play once, 2 = play twice, …).
    var repeatCount: Int = 1

    // MARK: - Dependencies

    private let audioSessionManager: AudioSessionManager
    weak var webView: WKWebView?

    // MARK: - Audio engine

    private let engine = AVAudioEngine()
    private let playerNode = AVAudioPlayerNode()
    private let timePitch = AVAudioUnitTimePitch()

    /// The loaded audio file (WAV written to a temp file).
    private var audioFile: AVAudioFile?
    /// Temp file URL for the current WAV.
    private var tempFileURL: URL?

    // MARK: - Position tracking (wall-clock based, matches cursor)

    /// Wall-clock time when playback started/resumed.
    private var wallStart: CFAbsoluteTime = 0
    /// Music-time (ms) at which playback started/resumed.
    private var musicStart: Double = 0

    /// Low-frequency timer (4 Hz) for end-of-playback detection.
    private var pollTimer: Timer?

    // MARK: - Repeat

    private var remainingRepeats: Int = 0

    // MARK: - Generation counter (prevents stale completion handlers)

    /// Incremented every time `startPlayback()` schedules new audio.
    /// The completion handler captures this value and is ignored if it
    /// no longer matches — preventing a stale handler from killing
    /// newly started playback after seek/resume/repeat.
    private var playbackGeneration: Int = 0

    // MARK: - Interruption handling

    private var interruptionObserver: Any?
    private var routeChangeObserver: Any?
    /// Whether playback was active when an interruption began (used to
    /// decide whether to auto-resume when the interruption ends).
    private var wasPlayingBeforeInterruption = false

    // MARK: - Lifecycle

    init(audioSessionManager: AudioSessionManager) {
        self.audioSessionManager = audioSessionManager
        setupAudioEngine()
        setupInterruptionHandling()
    }

    deinit {
        stopPollTimer()
        playerNode.stop()
        engine.stop()
        cleanupTempFile()
        if let obs = interruptionObserver { NotificationCenter.default.removeObserver(obs) }
        if let obs = routeChangeObserver { NotificationCenter.default.removeObserver(obs) }
    }

    // MARK: - Audio engine setup

    /// Build the audio graph once: PlayerNode → TimePitch → mainMixer → output.
    private func setupAudioEngine() {
        engine.attach(playerNode)
        engine.attach(timePitch)
        engine.connect(playerNode, to: timePitch, format: nil)
        engine.connect(timePitch, to: engine.mainMixerNode, format: nil)
    }

    /// Ensure the audio session is configured and the engine is running.
    private func ensureEngineRunning() throws {
        audioSessionManager.ensureSessionActive()

        if !engine.isRunning {
            try engine.start()
            print("[PlaybackManager] Engine started")
        }
    }

    // MARK: - Interruption & route-change handling

    private func setupInterruptionHandling() {
        interruptionObserver = NotificationCenter.default.addObserver(
            forName: AVAudioSession.interruptionNotification,
            object: AVAudioSession.sharedInstance(),
            queue: .main
        ) { [weak self] notification in
            self?.handleInterruption(notification)
        }

        routeChangeObserver = NotificationCenter.default.addObserver(
            forName: AVAudioSession.routeChangeNotification,
            object: AVAudioSession.sharedInstance(),
            queue: .main
        ) { [weak self] notification in
            self?.handleRouteChange(notification)
        }
    }

    private func handleInterruption(_ notification: Notification) {
        guard let info = notification.userInfo,
              let typeValue = info[AVAudioSessionInterruptionTypeKey] as? UInt,
              let type = AVAudioSession.InterruptionType(rawValue: typeValue) else { return }

        switch type {
        case .began:
            wasPlayingBeforeInterruption = isPlaying
            if isPlaying { pause() }
            print("[PlaybackManager] Interruption began")

        case .ended:
            // Check if we should resume.
            if let optionsValue = info[AVAudioSessionInterruptionOptionKey] as? UInt {
                let options = AVAudioSession.InterruptionOptions(rawValue: optionsValue)
                if options.contains(.shouldResume) && wasPlayingBeforeInterruption {
                    try? ensureEngineRunning()
                    play()
                    print("[PlaybackManager] Interruption ended — resuming")
                }
            }
            wasPlayingBeforeInterruption = false

        @unknown default:
            break
        }
    }

    private func handleRouteChange(_ notification: Notification) {
        guard let info = notification.userInfo,
              let reasonValue = info[AVAudioSessionRouteChangeReasonKey] as? UInt,
              let reason = AVAudioSession.RouteChangeReason(rawValue: reasonValue) else { return }

        if reason == .oldDeviceUnavailable {
            // Headphones unplugged — pause to avoid blasting audio through speaker.
            if isPlaying {
                pause()
                print("[PlaybackManager] Headphones disconnected — paused")
            }
        }
    }

    // MARK: - Public API

    /// Prepare WAV audio data for playback (does not start playing).
    ///
    /// Writes the WAV to a temp file and opens it as an AVAudioFile.
    func prepareAudio(_ wavData: Data) {
        stop()

        do {
            try ensureEngineRunning()

            // Write WAV to a temp file so AVAudioFile can read it.
            cleanupTempFile()
            let url = FileManager.default.temporaryDirectory
                .appendingPathComponent("soloband_playback_\(UUID().uuidString).wav")
            tempFileURL = url  // Set first so cleanup can find it on failure
            try wavData.write(to: url)

            let file = try AVAudioFile(forReading: url)
            audioFile = file

            let sampleRate = file.processingFormat.sampleRate
            durationMs = Double(file.length) / sampleRate * 1000.0

            // Apply current mute state.
            engine.mainMixerNode.outputVolume = isMuted ? 0 : 1

            // Show cursor at the beginning.
            sendJS("if (typeof moveCursor === 'function') { showCursor(); moveCursor(0); }")

            print("[PlaybackManager] Audio prepared: \(String(format: "%.1f", durationMs / 1000.0))s, "
                + "\(file.length) frames, speed=\(speed)")
        } catch {
            print("[PlaybackManager] Failed to prepare audio: \(error.localizedDescription)")
            audioFile = nil
            durationMs = 0
        }
    }

    /// Start or resume playback.
    func play() {
        remainingRepeats = repeatCount
        startPlayback()
    }

    /// Internal: schedule audio from currentTimeMs and start the player.
    private func startPlayback() {
        guard let file = audioFile else {
            print("[PlaybackManager] No audio data loaded")
            return
        }

        do {
            try ensureEngineRunning()

            let sampleRate = file.processingFormat.sampleRate
            let startFrame = AVAudioFramePosition(currentTimeMs / 1000.0 * sampleRate)
            let totalFrames = file.length
            guard startFrame < totalFrames else {
                playbackDidFinish()
                return
            }
            let frameCount = AVAudioFrameCount(totalFrames - startFrame)

            // Stop any previous scheduling.
            playerNode.stop()

            // Bump generation so any in-flight completion handler is ignored.
            playbackGeneration += 1
            let thisGeneration = playbackGeneration

            // Schedule the segment from the current position.
            playerNode.scheduleSegment(
                file,
                startingFrame: startFrame,
                frameCount: frameCount,
                at: nil
            ) { [weak self] in
                // Completion fires on a background thread.
                DispatchQueue.main.async {
                    guard let self = self, self.playbackGeneration == thisGeneration else { return }
                    self.playbackDidFinish()
                }
            }

            timePitch.rate = Float(speed)
            playerNode.play()
            isPlaying = true

            // Start wall-clock tracking.
            wallStart = CFAbsoluteTimeGetCurrent()
            musicStart = currentTimeMs

            // Tell the WebView to start its cursor animation.
            sendJS("startCursorAnimation(\(currentTimeMs), \(speed))")

            startPollTimer()
            print("[PlaybackManager] Playing from \(String(format: "%.1f", currentTimeMs / 1000.0))s "
                + "(speed=\(speed), muted=\(isMuted))")
        } catch {
            print("[PlaybackManager] Failed to start playback: \(error.localizedDescription)")
        }
    }

    /// Pause playback — cursor stays at the current position.
    func pause() {
        guard isPlaying else { return }

        // Capture position before pausing.
        updateCurrentTime()
        playerNode.pause()
        isPlaying = false
        stopPollTimer()

        sendJS("stopCursorAnimation(\(currentTimeMs))")
        print("[PlaybackManager] Paused at \(String(format: "%.1f", currentTimeMs / 1000.0))s")
    }

    /// Stop playback and reset cursor to the beginning.
    func stop() {
        // Bump generation to invalidate any pending completion handlers.
        playbackGeneration += 1

        playerNode.stop()
        isPlaying = false
        currentTimeMs = 0
        remainingRepeats = 0
        stopPollTimer()
        sendJS("stopCursorAnimation(0)")
    }

    /// Toggle play/pause.
    func togglePlayPause() {
        if isPlaying { pause() } else { play() }
    }

    /// Seek to a specific *music* time in milliseconds.
    func seek(to musicTimeMs: Double) {
        guard audioFile != nil else { return }

        let wasPlaying = isPlaying
        let clampedMs = max(0, min(musicTimeMs, durationMs))

        if wasPlaying {
            playerNode.stop()
            isPlaying = false
            stopPollTimer()
        }

        currentTimeMs = clampedMs

        if wasPlaying {
            startPlayback()
        } else {
            sendJS("stopCursorAnimation(\(clampedMs))")
        }
        print("[PlaybackManager] Seeked to \(String(format: "%.1f", clampedMs / 1000.0))s")
    }

    // MARK: - Position tracking

    /// Update currentTimeMs from the wall clock.
    private func updateCurrentTime() {
        guard isPlaying else { return }
        let elapsed = CFAbsoluteTimeGetCurrent() - wallStart
        currentTimeMs = min(musicStart + elapsed * speed * 1000.0, durationMs)
    }

    /// Re-sync wall-clock tracking after a speed change.
    private func resyncPlayback() {
        updateCurrentTime()
        wallStart = CFAbsoluteTimeGetCurrent()
        musicStart = currentTimeMs
        let posMs = currentTimeMs
        sendJS("startCursorAnimation(\(posMs), \(speed))")
    }

    // MARK: - WebView communication (one-shot commands only)

    private func sendJS(_ js: String) {
        guard let webView = webView else { return }
        webView.evaluateJavaScript(js, completionHandler: nil)
    }

    // MARK: - Poll timer (end-of-playback detection, ~4 Hz)

    private func startPollTimer() {
        stopPollTimer()
        pollTimer = Timer.scheduledTimer(withTimeInterval: 0.25, repeats: true) { [weak self] _ in
            self?.pollPlayback()
        }
    }

    private func stopPollTimer() {
        pollTimer?.invalidate()
        pollTimer = nil
    }

    private func pollPlayback() {
        guard isPlaying else { return }
        updateCurrentTime()

        // Detect end of playback.
        if currentTimeMs >= durationMs - 50 {
            playbackDidFinish()
        }
    }

    // MARK: - Repeat / finish

    private func playbackDidFinish() {
        guard isPlaying else { return }

        playerNode.stop()
        isPlaying = false
        stopPollTimer()
        sendJS("stopCursorAnimation(0)")

        remainingRepeats -= 1
        if remainingRepeats > 0 {
            print("[PlaybackManager] Repeat \(repeatCount - remainingRepeats)/\(repeatCount)")
            currentTimeMs = 0

            DispatchQueue.main.asyncAfter(deadline: .now() + 0.15) { [weak self] in
                self?.startPlayback()
            }
            return
        }

        currentTimeMs = 0
        print("[PlaybackManager] Playback finished (all repeats done)")
    }

    // MARK: - Cleanup

    private func cleanupTempFile() {
        if let url = tempFileURL {
            try? FileManager.default.removeItem(at: url)
            tempFileURL = nil
        }
    }
}
