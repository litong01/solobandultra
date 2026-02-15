import Foundation
import AVFoundation
import AudioToolbox
import WebKit
import Combine

/// Manages MIDI playback and cursor synchronization.
///
/// Uses AVAudioEngine + AVAudioSequencer for native MIDI playback.
/// Cursor animation runs entirely inside the WKWebView via
/// `requestAnimationFrame` — **zero** `evaluateJavaScript` IPC calls
/// during continuous playback.  Swift only sends one-shot commands
/// (play/pause/seek/speed) and uses a low-frequency timer for
/// end-of-playback detection.
///
/// Supports:
/// - **Speed** — native `sequencer.rate` multiplier (no MIDI byte manipulation).
/// - **Mute** — `engine.mainMixerNode.volume = 0` (player keeps running, cursor stays in sync).
/// - **Repeat** — replays the piece N times automatically.
class PlaybackManager: ObservableObject {
    // MARK: - Published state

    @Published var isPlaying = false
    /// Current position in *music* time (ms).
    /// NOT @Published — updated by the end-of-playback timer, but only
    /// consumed internally.
    var currentTimeMs: Double = 0
    /// Total duration in *music* time (ms).  Stays constant regardless of speed.
    var durationMs: Double = 0

    // MARK: - Playback settings

    /// Playback speed multiplier (1.0 = normal, 0.5 = half, 2.0 = double).
    /// Clamped to [0.1, 5.0].  Takes effect immediately, even mid-playback.
    var speed: Double = 1.0 {
        didSet {
            speed = max(0.1, min(5.0, speed))
            sequencer?.rate = Float(speed)
            // Tell the WebView about the speed change so it can adjust
            // its requestAnimationFrame interpolation.
            if isPlaying {
                syncCursorSpeed()
            }
        }
    }

    /// When `true` audio is silenced but the sequencer keeps running
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
    private var midiSynth: AVAudioUnitMIDIInstrument?
    private var sequencer: AVAudioSequencer?

    /// Low-frequency timer (4 Hz) for end-of-playback detection.
    /// Replaces the old 60fps CADisplayLink — all cursor animation
    /// now runs inside the WebView via requestAnimationFrame.
    private var pollTimer: Timer?

    // MARK: - Repeat

    private var remainingRepeats: Int = 0

    // MARK: - Lifecycle

    init(audioSessionManager: AudioSessionManager) {
        self.audioSessionManager = audioSessionManager
        setupAudioEngine()
    }

    deinit {
        stopPollTimer()
        // Must nil-out the sequencer BEFORE stopping the engine to avoid a
        // CoreAudio crash (AVAudioSequencer references the engine's outputNode).
        let seq = sequencer
        sequencer = nil
        seq?.stop()
        engine.stop()
    }

    // MARK: - Audio engine setup

    /// Build the audio graph once: MIDISynth → mainMixerNode → output.
    private func setupAudioEngine() {
        // Create Apple's built-in multi-timbral DLS synth (handles all 16 MIDI channels).
        let description = AudioComponentDescription(
            componentType: kAudioUnitType_MusicDevice,
            componentSubType: kAudioUnitSubType_MIDISynth,
            componentManufacturer: kAudioUnitManufacturer_Apple,
            componentFlags: 0,
            componentFlagsMask: 0
        )
        let synth = AVAudioUnitMIDIInstrument(audioComponentDescription: description)
        midiSynth = synth

        engine.attach(synth)
        engine.connect(synth, to: engine.mainMixerNode, format: nil)

        // Load a General MIDI sound bank.  On real iOS devices the DLS synth
        // has no built-in instrument sounds (unlike the macOS Simulator), so
        // a bundled SoundFont / DLS file is required for audible playback.
        let bankURL: URL? =
            Bundle.main.url(forResource: "GeneralUser_GS", withExtension: "sf2")
            ?? Bundle.main.url(forResource: "gs_instruments", withExtension: "dls")

        if let bankURL = bankURL {
            // Use withExtendedLifetime to silence the "object reference in
            // UnsafeRawPointer" warning while keeping the CFURL alive.
            let cfurl = bankURL as CFURL
            let status = withExtendedLifetime(cfurl) { () -> OSStatus in
                var ref = cfurl
                return withUnsafeMutableBytes(of: &ref) { buf in
                    AudioUnitSetProperty(
                        synth.audioUnit,
                        kMusicDeviceProperty_SoundBankURL,
                        kAudioUnitScope_Global,
                        0,
                        buf.baseAddress!,
                        UInt32(buf.count)
                    )
                }
            }
            if status == noErr {
                print("[PlaybackManager] Loaded sound bank: \(bankURL.lastPathComponent)")
            } else {
                print("[PlaybackManager] Sound bank load failed (status \(status))")
            }
        } else {
            print("[PlaybackManager] WARNING: No sound bank found — audio will be silent on real devices")
        }

        // Reduce render quality to give the synth more headroom for polyphony.
        // kRenderQuality_Min = 0, _Low = 32, _Medium = 64, _High = 96, _Max = 127
        var quality: UInt32 = 32  // Low quality — less CPU per voice
        AudioUnitSetProperty(
            synth.audioUnit,
            kAudioUnitProperty_RenderQuality,
            kAudioUnitScope_Global,
            0,
            &quality,
            UInt32(MemoryLayout<UInt32>.size)
        )
    }

    /// Ensure the audio session is configured and the engine is running.
    /// Must be called before any sequencer operations.
    private func ensureEngineRunning() throws {
        // Always configure the audio session first — the engine's internal
        // render format depends on the active session category/mode.
        audioSessionManager.ensureSessionActive()

        // Request a larger IO buffer — gives the MIDISynth more time per
        // render callback to process voices.  Default is ~5ms (256 samples
        // at 44.1 kHz); 0.02s (882 samples) gives ~4× more headroom.
        // Tradeoff: 20ms extra latency (imperceptible for MIDI playback).
        try? AVAudioSession.sharedInstance().setPreferredIOBufferDuration(0.02)

        if !engine.isRunning {
            try engine.start()
            print("[PlaybackManager] Engine started (IO buffer: "
                + "\(String(format: "%.1f", AVAudioSession.sharedInstance().ioBufferDuration * 1000))ms)")
        }
    }

    // MARK: - Public API

    /// Prepare MIDI data for playback (does not start playing).
    func prepareMidi(_ data: Data) {
        // Stop any current playback and release old sequencer.
        stop()
        sequencer = nil

        do {
            // Engine must be running before we create the sequencer.
            try ensureEngineRunning()

            // Create a fresh sequencer attached to the engine and load the MIDI data.
            let seq = AVAudioSequencer(audioEngine: engine)
            try seq.load(from: data, options: .smf_ChannelsToTracks)

            // Route all tracks to our synth explicitly.
            if let synth = midiSynth {
                for track in seq.tracks {
                    track.destinationAudioUnit = synth
                }
            }

            seq.prepareToPlay()
            seq.rate = Float(speed)
            sequencer = seq

            // Compute duration from the longest track (beats → seconds via tempo map).
            var maxSeconds: Double = 0
            for track in seq.tracks {
                let trackSec = seq.seconds(forBeats: track.lengthInBeats)
                maxSeconds = max(maxSeconds, trackSec)
            }
            durationMs = maxSeconds * 1000.0

            // Apply current mute state to the mixer.
            engine.mainMixerNode.outputVolume = isMuted ? 0 : 1

            // Show cursor at the beginning.
            sendJS("if (typeof moveCursor === 'function') { showCursor(); moveCursor(0); }")

            print("[PlaybackManager] MIDI prepared: \(String(format: "%.1f", durationMs / 1000.0))s, speed=\(speed)")
        } catch {
            print("[PlaybackManager] Failed to prepare MIDI: \(error.localizedDescription)")
            sequencer = nil
            durationMs = 0
        }
    }

    /// Start or resume playback.
    func play() {
        // Set remaining repeats at the start of a fresh play (position near 0).
        if currentTimeMs < 1.0 {
            remainingRepeats = repeatCount
        }
        startSequencer()
    }

    /// Internal: start the sequencer without touching the repeat counter.
    /// Used by both `play()` and repeat-restart in `playbackDidFinish()`.
    private func startSequencer() {
        guard let seq = sequencer else {
            print("[PlaybackManager] No MIDI data loaded")
            return
        }

        do {
            try ensureEngineRunning()
            seq.prepareToPlay()
            try seq.start()
            isPlaying = true

            // Tell the WebView to start its own cursor animation loop.
            // This is the ONLY evaluateJavaScript call during playback —
            // the WebView then drives the cursor via requestAnimationFrame.
            let posMs = seq.currentPositionInSeconds * 1000.0
            sendJS("startCursorAnimation(\(posMs), \(speed))")

            startPollTimer()
            print("[PlaybackManager] Playing (speed=\(speed), muted=\(isMuted))")
        } catch {
            print("[PlaybackManager] Failed to start sequencer: \(error.localizedDescription)")
        }
    }

    /// Pause playback — cursor stays at the current position.
    func pause() {
        guard let seq = sequencer else { return }

        let positionSec = seq.currentPositionInSeconds
        seq.stop()
        allNotesOff()

        // Preserve position after stop.
        seq.currentPositionInSeconds = positionSec
        currentTimeMs = positionSec * 1000.0

        isPlaying = false
        stopPollTimer()

        // Tell the WebView to stop its animation loop and hold position.
        sendJS("stopCursorAnimation(\(currentTimeMs))")

        print("[PlaybackManager] Paused at \(String(format: "%.1f", positionSec))s")
    }

    /// Stop playback and reset cursor to the beginning.
    func stop() {
        sequencer?.stop()
        allNotesOff()
        sequencer?.currentPositionInSeconds = 0
        isPlaying = false
        currentTimeMs = 0
        remainingRepeats = 0
        stopPollTimer()
        sendJS("stopCursorAnimation(0)")
    }

    /// Toggle play/pause.
    func togglePlayPause() {
        if isPlaying {
            pause()
        } else {
            play()
        }
    }

    /// Seek to a specific *music* time in milliseconds.
    func seek(to musicTimeMs: Double) {
        guard let seq = sequencer else { return }

        let clampedMs = max(0, min(musicTimeMs, durationMs))

        allNotesOff()

        seq.currentPositionInSeconds = clampedMs / 1000.0
        currentTimeMs = clampedMs

        if isPlaying {
            // Re-sync the WebView animation from the new position.
            sendJS("startCursorAnimation(\(clampedMs), \(speed))")
        } else {
            sendJS("stopCursorAnimation(\(clampedMs))")
        }
        print("[PlaybackManager] Seeked to \(String(format: "%.1f", clampedMs / 1000.0))s")
    }

    /// Send CC#123 (All Notes Off) + CC#120 (All Sound Off) on all 16 channels.
    private func allNotesOff() {
        guard let au = midiSynth?.audioUnit else { return }
        for ch: UInt32 in 0..<16 {
            MusicDeviceMIDIEvent(au, 0xB0 | ch, 123, 0, 0)
            MusicDeviceMIDIEvent(au, 0xB0 | ch, 120, 0, 0)
        }
    }

    // MARK: - WebView communication (one-shot commands only)

    /// Fire-and-forget JS execution — used only for one-shot commands
    /// (play/pause/seek/speed), never in a per-frame loop.
    private func sendJS(_ js: String) {
        guard let webView = webView else { return }
        webView.evaluateJavaScript(js, completionHandler: nil)
    }

    /// Notify the WebView of a speed change mid-playback.
    private func syncCursorSpeed() {
        guard let seq = sequencer else { return }
        let posMs = seq.currentPositionInSeconds * 1000.0
        sendJS("startCursorAnimation(\(posMs), \(speed))")
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
        guard let seq = sequencer, isPlaying else { return }

        let positionSec = seq.currentPositionInSeconds
        let musicMs = positionSec * 1000.0
        currentTimeMs = musicMs

        // Detect end of playback.
        if !seq.isPlaying || (durationMs > 0 && musicMs >= durationMs - 1) {
            playbackDidFinish()
        }
    }

    // MARK: - Private helpers

    /// Called when the sequencer reaches the end of the MIDI data.
    private func playbackDidFinish() {
        // Immediately stop everything so the timer can't re-enter.
        sequencer?.stop()
        isPlaying = false
        stopPollTimer()
        sendJS("stopCursorAnimation(0)")

        remainingRepeats -= 1
        if remainingRepeats > 0 {
            // ── More repeats to go — restart from the beginning ──
            print("[PlaybackManager] Repeat \(repeatCount - remainingRepeats)/\(repeatCount)")
            sequencer?.currentPositionInSeconds = 0
            currentTimeMs = 0

            // Delay to let the audio IO thread settle before restarting.
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.15) { [weak self] in
                self?.startSequencer()
            }
            return
        }

        // ── All repeats done ──
        sequencer?.currentPositionInSeconds = 0
        currentTimeMs = 0
        print("[PlaybackManager] Playback finished (all repeats done)")
    }
}
