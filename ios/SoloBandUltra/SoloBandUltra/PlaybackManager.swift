import Foundation
import AVFoundation
import AudioToolbox
import WebKit
import Combine

/// Manages MIDI playback and cursor synchronization.
///
/// Uses AVAudioEngine + AVAudioSequencer for native MIDI playback with
/// CADisplayLink for 60fps cursor position updates.
///
/// Supports:
/// - **Speed** — native `sequencer.rate` multiplier (no MIDI byte manipulation).
/// - **Mute** — `engine.mainMixerNode.volume = 0` (player keeps running, cursor stays in sync).
/// - **Repeat** — replays the piece N times automatically.
class PlaybackManager: ObservableObject {
    // MARK: - Published state

    @Published var isPlaying = false
    /// Current position in *music* time (ms).
    /// NOT @Published — updated at 60fps by CADisplayLink, but only consumed
    /// internally (cursor updates via JS).  Making this @Published would cause
    /// every observing SwiftUI view to re-evaluate its body 60× per second.
    var currentTimeMs: Double = 0
    /// Total duration in *music* time (ms).  Stays constant regardless of speed.
    /// NOT @Published — only read internally by seek() and playbackDidFinish().
    var durationMs: Double = 0

    // MARK: - Playback settings

    /// Playback speed multiplier (1.0 = normal, 0.5 = half, 2.0 = double).
    /// Clamped to [0.1, 5.0].  Takes effect immediately, even mid-playback.
    var speed: Double = 1.0 {
        didSet {
            speed = max(0.1, min(5.0, speed))
            sequencer?.rate = Float(speed)
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
    private var displayLink: CADisplayLink?

    // MARK: - Repeat

    private var remainingRepeats: Int = 0

    // MARK: - Lifecycle

    init(audioSessionManager: AudioSessionManager) {
        self.audioSessionManager = audioSessionManager
        setupAudioEngine()
    }

    deinit {
        stopDisplayLink()
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
            var url: CFURL = bankURL as CFURL
            let status = AudioUnitSetProperty(
                synth.audioUnit,
                kMusicDeviceProperty_SoundBankURL,
                kAudioUnitScope_Global,
                0,
                &url,
                UInt32(MemoryLayout<CFURL>.size)
            )
            if status == noErr {
                print("[PlaybackManager] Loaded sound bank: \(bankURL.lastPathComponent)")
            } else {
                print("[PlaybackManager] Sound bank load failed (status \(status))")
            }
        } else {
            print("[PlaybackManager] WARNING: No sound bank found — audio will be silent on real devices")
        }
    }

    /// Ensure the audio session is configured and the engine is running.
    /// Must be called before any sequencer operations.
    private func ensureEngineRunning() throws {
        // Always configure the audio session first — the engine's internal
        // render format depends on the active session category/mode.
        audioSessionManager.ensureSessionActive()

        if !engine.isRunning {
            try engine.start()
            print("[PlaybackManager] Engine started")
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
                let trackSec = try seq.seconds(forBeats: track.lengthInBeats)
                maxSeconds = max(maxSeconds, trackSec)
            }
            durationMs = maxSeconds * 1000.0

            // Apply current mute state to the mixer.
            engine.mainMixerNode.outputVolume = isMuted ? 0 : 1

            // Show cursor at the beginning.
            updateCursor(timeMs: 0)

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
            startDisplayLink()
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
        allNotesOff()          // silence any ringing notes immediately

        // Preserve position after stop.
        seq.currentPositionInSeconds = positionSec
        currentTimeMs = positionSec * 1000.0

        isPlaying = false
        stopDisplayLink()
        updateCursor(timeMs: currentTimeMs)

        print("[PlaybackManager] Paused at \(String(format: "%.1f", positionSec))s")
    }

    /// Stop playback and reset cursor to the beginning.
    func stop() {
        sequencer?.stop()
        allNotesOff()              // free all synth voices immediately
        sequencer?.currentPositionInSeconds = 0
        isPlaying = false
        currentTimeMs = 0
        remainingRepeats = 0
        stopDisplayLink()
        updateCursor(timeMs: 0)
        print("[PlaybackManager] Stopped")
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

        // Send "All Notes Off" (CC#123) on every channel before changing
        // position.  Without this, notes that were on at the old position
        // keep sounding (stuck notes) and compete with notes at the new
        // position, worsening voice-stealing.
        allNotesOff()

        seq.currentPositionInSeconds = clampedMs / 1000.0
        currentTimeMs = clampedMs

        updateCursor(timeMs: clampedMs)
        print("[PlaybackManager] Seeked to \(String(format: "%.1f", clampedMs / 1000.0))s")
    }

    /// Send CC#123 (All Notes Off) on all 16 MIDI channels.
    /// Ensures no voices are "stuck" from a previous position.
    private func allNotesOff() {
        guard let au = midiSynth?.audioUnit else { return }
        for ch: UInt32 in 0..<16 {
            // CC#123 = All Notes Off
            MusicDeviceMIDIEvent(au, 0xB0 | ch, 123, 0, 0)
            // CC#120 = All Sound Off (immediate silence, no release tail)
            MusicDeviceMIDIEvent(au, 0xB0 | ch, 120, 0, 0)
        }
    }

    // MARK: - Display Link (cursor updates)

    /// Guard: true while a `evaluateJavaScript` call is in-flight.
    /// Prevents IPC calls from piling up if the WebView is slow.
    private var cursorUpdateInFlight = false

    private func startDisplayLink() {
        stopDisplayLink()
        let link = CADisplayLink(target: self, selector: #selector(displayLinkFired))

        // 20 fps is plenty for a smoothly moving cursor (movies run at 24).
        // The default 60 fps floods the WKWebView with IPC traffic, and the
        // accumulated load (especially on the 5 MB Chopin SVG) causes the
        // system to throttle the audio render thread after ~20 measures.
        link.preferredFramesPerSecond = 20

        link.add(to: .main, forMode: .common)
        displayLink = link
    }

    private func stopDisplayLink() {
        displayLink?.invalidate()
        displayLink = nil
    }

    @objc private func displayLinkFired() {
        guard let seq = sequencer, isPlaying else { return }

        let positionSec = seq.currentPositionInSeconds
        let musicMs = positionSec * 1000.0

        // Detect end of playback: the sequencer may stop itself (isPlaying
        // becomes false), OR the position may reach/exceed the duration while
        // isPlaying stays true — handle both cases.
        if !seq.isPlaying || (durationMs > 0 && musicMs >= durationMs - 1) {
            playbackDidFinish()
            return
        }

        currentTimeMs = musicMs
        updateCursor(timeMs: musicMs)
    }

    // MARK: - WebView cursor communication

    private func updateCursor(timeMs: Double) {
        guard let webView = webView else { return }

        // Skip this update if the previous JS call hasn't returned yet.
        // This prevents IPC calls from piling up when the WebView is slow
        // (e.g. large SVG DOM) — queued evaluateJavaScript calls compete
        // with the audio system for CPU/memory and cause note dropouts.
        guard !cursorUpdateInFlight else { return }
        cursorUpdateInFlight = true

        let js = "if (typeof moveCursor === 'function') { showCursor(); moveCursor(\(timeMs)); }"
        webView.evaluateJavaScript(js) { [weak self] _, _ in
            self?.cursorUpdateInFlight = false
        }
    }

    private func hideCursor() {
        guard let webView = webView else { return }
        let js = "if (typeof hideCursor === 'function') { hideCursor(); }"
        webView.evaluateJavaScript(js, completionHandler: nil)
    }

    // MARK: - Private helpers

    /// Called when the sequencer reaches the end of the MIDI data.
    private func playbackDidFinish() {
        // Immediately stop everything so the display link can't re-enter.
        sequencer?.stop()
        isPlaying = false
        stopDisplayLink()

        remainingRepeats -= 1
        if remainingRepeats > 0 {
            // ── More repeats to go — restart from the beginning ──
            print("[PlaybackManager] Repeat \(repeatCount - remainingRepeats)/\(repeatCount)")
            sequencer?.currentPositionInSeconds = 0
            currentTimeMs = 0
            updateCursor(timeMs: 0)

            // Delay to let the audio IO thread settle before restarting.
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.15) { [weak self] in
                self?.startSequencer()
            }
            return
        }

        // ── All repeats done ──
        sequencer?.currentPositionInSeconds = 0
        currentTimeMs = 0
        updateCursor(timeMs: 0)
        print("[PlaybackManager] Playback finished (all repeats done)")
    }
}
