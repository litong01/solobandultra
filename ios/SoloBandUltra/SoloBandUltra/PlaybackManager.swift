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

    /// When true, playback and capture use a single full-duplex RemoteIO unit instead of
    /// AVAudioEngine + separate capture. Set by ContentView from midiSettings.includeFeedback.
    var useDuplexForFeedback: Bool = false

    /// Called on every poll tick (~4 Hz) with the current music position in ms.
    /// Set by ContentView to drive FeedbackManager.update(musicMs:).
    var onTimeUpdate: ((Double) -> Void)?

    /// Called at the start of startPlayback(), after the engine is running but before
    /// scheduling/play. Use this to install the mic tap before playback starts so the
    /// engine doesn't reconfigure mid-stream (which can stop playback).
    var beforePlaybackStarts: (() -> Void)?

    // MARK: - Dependencies

    private let audioSessionManager: AudioSessionManager
    weak var webView: WKWebView?

    // MARK: - Audio engine

    private let engine = AVAudioEngine()
    private let playerNode = AVAudioPlayerNode()
    private let timePitch = AVAudioUnitTimePitch()

    /// Microphone capture via RemoteIO Audio Unit (cpal-style). Not AVAudioEngine, so
    /// playback is never reconfigured when capture starts.
    private var remoteIOCapture: RemoteIOCapture?

    /// Full-duplex path: one RemoteIO for both playback and capture when Feedback is on.
    private var duplex: RemoteIODuplex?
    private var duplexPlaybackBuffer: [Float]?
    private var duplexCaptureHandler: ((AVAudioPCMBuffer) -> Void)?

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
        remoteIOCapture?.stop()
        remoteIOCapture = nil
        duplex?.stop()
        duplex = nil
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

    /// Ensure the audio session is active and the playback engine is running.
    /// The input engine is started only when installMicrophoneTap is called.
    private func ensureEngineRunning() throws {
        try audioSessionManager.ensureSessionActive()
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
    /// Writes the WAV to a temp file and opens it as an AVAudioFile. Does not start
    /// the engine — that happens on play() so that when Feedback is on we can start
    /// capture first, then the engine, avoiding session reconfiguration that stops output.
    func prepareAudio(_ wavData: Data) {
        stop()
        duplexPlaybackBuffer = nil

        do {
            // Do NOT start the engine here. Start it only in startPlayback() so that
            // we can start RemoteIO capture first (when Feedback is on), then the
            // engine — both in one go — avoiding "input after output" session reconfigure.
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

            // Apply current mute state (engine may not be running yet).
            engine.mainMixerNode.outputVolume = isMuted ? 0 : 1

            // Show cursor at the beginning.
            sendJS("showCursor(); moveCursor(0);")

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
    /// When useDuplexForFeedback is true and we have a capture handler, use a single
    /// full-duplex RemoteIO for both playback and capture so the session has one client.
    private func startPlayback() {
        guard let file = audioFile else {
            print("[PlaybackManager] No audio data loaded")
            return
        }

        let sr = file.processingFormat.sampleRate
        let totalFrames = Int(file.length)

        // So that duplex path can use the capture handler, run beforePlaybackStarts first.
        beforePlaybackStarts?()

        // Duplex path: one RemoteIO for play + capture when Feedback is on.
        if useDuplexForFeedback, let captureHandler = duplexCaptureHandler {
            do {
                if duplexPlaybackBuffer == nil {
                    try loadPlaybackBufferIntoDuplex(file: file, totalFrames: totalFrames)
                }
                guard let buffer = duplexPlaybackBuffer else { return }
                let startFrame = Int(currentTimeMs / 1000.0 * sr)
                let duplexObj = RemoteIODuplex(
                    sampleRate: sr,
                    playbackBuffer: buffer,
                    totalFrames: totalFrames,
                    startFrame: startFrame,
                    isMuted: isMuted,
                    onPlaybackFinished: { [weak self] in
                        DispatchQueue.main.async { self?.playbackDidFinish() }
                    },
                    captureHandler: captureHandler
                )
                try duplexObj.start()
                duplex = duplexObj
                isPlaying = true
                wallStart = CFAbsoluteTimeGetCurrent()
                musicStart = currentTimeMs
                sendJS("startCursorAnimation(\(currentTimeMs), \(speed))")
                startPollTimer()
                return
            } catch {
                print("[PlaybackManager] Duplex start failed: \(error.localizedDescription)")
                duplexCaptureHandler = nil
            }
        }

        do {
            try ensureEngineRunning()

            let startFrame = AVAudioFramePosition(currentTimeMs / 1000.0 * sr)
            guard startFrame < file.length else {
                playbackDidFinish()
                return
            }
            let frameCount = AVAudioFrameCount(totalFrames - Int(startFrame))

            timePitch.rate = Float(speed)
            engine.mainMixerNode.outputVolume = isMuted ? 0 : 1

            playbackGeneration += 1
            let thisGeneration = playbackGeneration

            playerNode.scheduleSegment(
                file,
                startingFrame: startFrame,
                frameCount: frameCount,
                at: nil
            ) { [weak self] in
                DispatchQueue.main.async {
                    guard let self = self, self.playbackGeneration == thisGeneration else { return }
                    self.playbackDidFinish()
                }
            }

            playerNode.play()
            isPlaying = true
            wallStart = CFAbsoluteTimeGetCurrent()
            musicStart = currentTimeMs
            sendJS("startCursorAnimation(\(currentTimeMs), \(speed))")
            startPollTimer()
        } catch {
            print("[PlaybackManager] Failed to start playback: \(error.localizedDescription)")
        }
    }

    /// Load WAV into stereo Float buffer for duplex playback. Call once per score.
    private func loadPlaybackBufferIntoDuplex(file: AVAudioFile, totalFrames: Int) throws {
        let format = file.processingFormat
        guard format.channelCount == 2 else {
            throw NSError(domain: "PlaybackManager", code: -1, userInfo: [NSLocalizedDescriptionKey: "Duplex expects stereo"])
        }
        file.framePosition = 0
        let capacity = AVAudioFrameCount(totalFrames)
        guard let buf = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: capacity) else {
            throw NSError(domain: "PlaybackManager", code: -1, userInfo: [NSLocalizedDescriptionKey: "Failed to allocate buffer"])
        }
        try file.read(into: buf)
        let L = buf.floatChannelData?[0]
        let R = buf.floatChannelData?[1]
        guard let left = L, let right = R else { return }
        var stereo: [Float] = []
        stereo.reserveCapacity(totalFrames * 2)
        for i in 0..<totalFrames {
            stereo.append(left[i])
            stereo.append(right[i])
        }
        duplexPlaybackBuffer = stereo
    }

    /// Pause playback — cursor stays at the current position.
    /// Stops the engine fully so resume goes through the same clean
    /// `startPlayback()` path as a fresh play.
    func pause() {
        guard isPlaying else { return }

        updateCurrentTime()

        if duplex != nil {
            duplex?.isPlaying = false
            duplex?.stop()
            duplex = nil
        } else {
            playbackGeneration += 1
            playerNode.stop()
            engine.stop()
            timePitch.reset()
        }
        isPlaying = false
        stopPollTimer()

        sendJS("stopCursorAnimation(\(currentTimeMs))")
    }

    /// Stop playback and reset cursor to the beginning.
    func stop() {
        playbackGeneration += 1

        if duplex != nil {
            duplex?.stop()
            duplex = nil
        } else {
            playerNode.stop()
            engine.stop()
            timePitch.reset()
        }
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
            playbackGeneration += 1
            if let du = duplex {
                du.stop()
                duplex = nil
            } else {
                playerNode.stop()
                engine.stop()
                timePitch.reset()
            }
            isPlaying = false
            stopPollTimer()
        }

        currentTimeMs = clampedMs

        if wasPlaying {
            startPlayback()
        } else {
            sendJS("stopCursorAnimation(\(clampedMs))")
        }
    }

    // MARK: - Position tracking

    /// Update currentTimeMs from the wall clock (engine path) or duplex playhead (duplex path).
    private func updateCurrentTime() {
        guard isPlaying else { return }
        if let du = duplex, let file = audioFile {
            let frame = du.currentFrame
            currentTimeMs = Double(frame) / file.processingFormat.sampleRate * 1000.0
            currentTimeMs = min(currentTimeMs, durationMs)
        } else {
            let elapsed = CFAbsoluteTimeGetCurrent() - wallStart
            currentTimeMs = min(musicStart + elapsed * speed * 1000.0, durationMs)
        }
    }

    // MARK: - Microphone tap (RemoteIO Audio Unit, cpal-style)

    /// Install microphone capture. When useDuplexForFeedback is true, stores the handler
    /// for the duplex path (playback will use the same unit). Otherwise starts RemoteIOCapture.
    func installMicrophoneTap(handler: @escaping (AVAudioPCMBuffer) -> Void) {
        if useDuplexForFeedback {
            duplexCaptureHandler = handler
            return
        }

        remoteIOCapture?.stop()
        remoteIOCapture = nil

        let session = AVAudioSession.sharedInstance()
        let sampleRate = session.sampleRate > 0 ? session.sampleRate : 48000

        let capture = RemoteIOCapture(sampleRate: sampleRate, handler: handler)
        do {
            try capture.start()
            remoteIOCapture = capture
        } catch {
            print("[PlaybackManager] RemoteIO capture failed to start: \(error.localizedDescription)")
        }
    }

    /// Remove the microphone capture and stop the RemoteIO/duplex unit.
    func removeMicrophoneTap() {
        duplexCaptureHandler = nil
        if duplex != nil {
            duplex?.stop()
            duplex = nil
        } else {
            remoteIOCapture?.stop()
            remoteIOCapture = nil
        }
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

        playbackGeneration += 1
        if duplex != nil {
            duplex?.stop()
            duplex = nil
        } else {
            playerNode.stop()
            engine.stop()
            timePitch.reset()
        }
        isPlaying = false
        stopPollTimer()
        sendJS("stopCursorAnimation(0)")

        remainingRepeats -= 1
        if remainingRepeats > 0 {
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
    }

    // MARK: - Cleanup

    private func cleanupTempFile() {
        if let url = tempFileURL {
            try? FileManager.default.removeItem(at: url)
            tempFileURL = nil
        }
    }
}
