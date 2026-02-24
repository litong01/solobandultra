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
/// - **Mute** — `mainMixerNode.volume = 0`.
/// - **Repeat** — replays the piece N times automatically.
///
/// **Settings model:** Changing speed, mute, cursor visibility, or repeat
/// count stops playback and resets the cursor to the beginning.  The user
/// presses play to restart with the new settings.  This avoids all the
/// complexity (and bugs) of reconfiguring the audio graph mid-playback.
class PlaybackManager: ObservableObject {
    // MARK: - Published state

    @Published var isPlaying = false
    /// Current position in *music* time (ms).
    var currentTimeMs: Double = 0
    /// Total duration in *music* time (ms).
    var durationMs: Double = 0

    // MARK: - Playback settings

    /// Playback speed multiplier (1.0 = normal, 0.5 = half, 2.0 = double).
    /// Clamped to [0.1, 5.0].  Changing while playing stops playback and
    /// resets the cursor to the beginning — the user presses play to restart.
    var speed: Double = 1.0 {
        didSet {
            speed = max(0.1, min(5.0, speed))
            resetIfPlaying()
        }
    }

    /// When `true` audio is silenced.  Changing stops playback.
    var isMuted: Bool = false {
        didSet { resetIfPlaying() }
    }

    /// Total number of times to play (1 = play once, 2 = play twice, …).
    /// Changing stops playback.
    var repeatCount: Int = 1 {
        didSet { resetIfPlaying() }
    }

    /// Whether to show the orange cursor bar overlay during playback.
    /// Changing stops playback.
    var showCursorEnabled: Bool = true {
        didSet {
            resetIfPlaying()
            sendJS("if (typeof setCursorBarVisible === 'function') { setCursorBarVisible(\(showCursorEnabled)); }")
        }
    }

    /// Stop playback and reset to the beginning whenever a setting changes.
    /// After this the user simply presses play to hear the piece with the
    /// new settings — no mid-playback reconfiguration needed.
    private func resetIfPlaying() {
        if isPlaying || currentTimeMs > 0 {
            stop()
        }
    }

    // MARK: - Callbacks

    /// Called on every poll tick (~4 Hz) with the current music position in ms.
    /// Set by ContentView to drive FeedbackManager.update(musicMs:).
    var onTimeUpdate: ((Double) -> Void)?

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

    /// Ensure the audio session is active and the engine is running.
    ///
    /// The session category (.playAndRecord) is set once at app launch.
    /// Pre-realizing the inputNode before engine.start() ensures the hardware
    /// microphone path is configured so installMicrophoneTap always works.
    private func ensureEngineRunning() throws {
        try audioSessionManager.ensureSessionActive()
        // Pre-realize the input node so it has a valid hardware format when
        // installMicrophoneTap() is called after playback starts.
        _ = engine.inputNode
        if !engine.isRunning {
            try engine.start()
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
            sendJS("showCursor(); moveCursor(0);")

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
        // Only reset the repeat counter when starting fresh, not when resuming
        // from a paused state mid-repeat.
        if !isPlaying && currentTimeMs == 0 {
            remainingRepeats = repeatCount
        }
        startPlayback()
    }

    /// Internal: schedule audio from currentTimeMs and start the player.
    ///
    /// Always starts from a fully stopped engine (via `stop()`) so the
    /// audio graph is in a known-good state — no stale DSP buffers, no
    /// rate mismatches.
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

            // Apply current settings to the (freshly started) engine.
            timePitch.rate = Float(speed)
            engine.mainMixerNode.outputVolume = isMuted ? 0 : 1

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
    /// Stops the engine fully so resume goes through the same clean
    /// `startPlayback()` path as a fresh play.
    func pause() {
        guard isPlaying else { return }

        // Capture position before stopping.
        updateCurrentTime()

        // Full stop of engine — resume will restart cleanly.
        playbackGeneration += 1
        playerNode.stop()
        engine.stop()
        timePitch.reset()
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
        engine.stop()
        timePitch.reset()   // Flush stale buffers so the next playback starts clean.
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
            // Full stop so restart goes through the clean path.
            playbackGeneration += 1
            playerNode.stop()
            engine.stop()
            timePitch.reset()
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

    // MARK: - Microphone tap (shared engine, used by FeedbackManager)

    /// Install an input tap on this engine's input node so the microphone and
    /// the audio output share the same AVAudioEngine instance.
    /// Safe to call while the engine is already running.
    func installMicrophoneTap(handler: @escaping (AVAudioPCMBuffer) -> Void) {
        let inputNode = engine.inputNode
        inputNode.removeTap(onBus: 0)   // remove any stale tap

        // Build the format from the session's hardware sample rate.
        // Querying inputNode.outputFormat(forBus:) is unreliable here — it can
        // return a zero-sampleRate stub if the input wasn't pre-realized before
        // engine.start(), causing the installTap assertion to fire.
        let sampleRate = AVAudioSession.sharedInstance().sampleRate
        guard sampleRate > 0,
              let format = AVAudioFormat(standardFormatWithSampleRate: sampleRate,
                                        channels: 1) else {
            print("[PlaybackManager] Cannot install mic tap — invalid sample rate \(AVAudioSession.sharedInstance().sampleRate)")
            return
        }

        inputNode.installTap(onBus: 0, bufferSize: 4096, format: format) { buf, _ in
            handler(buf)
        }
    }

    /// Remove the microphone input tap.
    func removeMicrophoneTap() {
        engine.inputNode.removeTap(onBus: 0)
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

        // End-of-playback is detected by the scheduleSegment completion handler,
        // not here. The poll timer only drives cursor position updates.
        // Clamp so the UI doesn't overshoot the duration.
        if currentTimeMs > durationMs {
            currentTimeMs = durationMs
        }

        onTimeUpdate?(currentTimeMs)
    }

    // MARK: - Repeat / finish

    private func playbackDidFinish() {
        guard isPlaying else { return }

        // Full engine stop for a clean state.
        playbackGeneration += 1
        playerNode.stop()
        engine.stop()
        timePitch.reset()
        isPlaying = false
        stopPollTimer()
        sendJS("stopCursorAnimation(0)")

        remainingRepeats -= 1
        if remainingRepeats > 0 {
            print("[PlaybackManager] Repeat \(repeatCount - remainingRepeats)/\(repeatCount)")
            currentTimeMs = 0

            // Capture generation so the delayed restart is cancelled if the user
            // stops, starts a new play, loads a new score, or an interruption occurs
            // during the 150 ms gap between repeats.
            let gen = playbackGeneration
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.15) { [weak self] in
                guard let self = self, self.playbackGeneration == gen else { return }
                self.startPlayback()
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
