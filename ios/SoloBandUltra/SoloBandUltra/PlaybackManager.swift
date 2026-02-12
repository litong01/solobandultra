import Foundation
import AVFoundation
import WebKit
import Combine

/// Manages MIDI playback and cursor synchronization.
///
/// Uses AVMIDIPlayer for native MIDI playback (no audio through WebView)
/// and CADisplayLink for 60fps cursor position updates.
///
/// Supports:
/// - **Speed** — scales MIDI tempo events so the player runs faster/slower.
/// - **Mute** — plays silently using a virtual timer (AVMIDIPlayer has no volume API).
/// - **Repeat** — replays the piece N times automatically.
class PlaybackManager: ObservableObject {
    // MARK: - Published state

    @Published var isPlaying = false
    /// Current position in *music* time (ms).  Accounts for speed scaling.
    @Published var currentTimeMs: Double = 0
    /// Total duration in *music* time (ms).  Stays constant regardless of speed.
    @Published var durationMs: Double = 0

    // MARK: - Playback settings

    /// Playback speed multiplier (1.0 = normal, 0.5 = half, 2.0 = double).
    /// Clamped to [0.1, 5.0].
    var speed: Double = 1.0 {
        didSet {
            speed = max(0.1, min(5.0, speed))
            applySpeedChange()
        }
    }

    /// When `true` the cursor still moves but no audio is produced.
    var isMuted: Bool = false {
        didSet { applyMuteChange() }
    }

    /// Total number of times to play (1 = play once, 2 = play twice, …).
    var repeatCount: Int = 1

    // MARK: - Dependencies

    private let audioSessionManager: AudioSessionManager
    weak var webView: WKWebView?

    // MARK: - MIDI playback

    private var midiPlayer: AVMIDIPlayer?
    /// Original (un-scaled) MIDI bytes.  Kept so we can re-scale when speed changes.
    private var originalMidiData: Data?
    private var displayLink: CADisplayLink?

    /// Flag to suppress the completion handler when we intentionally stop/pause.
    private var stoppingIntentionally = false

    // MARK: - Mute: virtual-time tracking

    /// When muted we don't run the AVMIDIPlayer.  Instead we track elapsed
    /// wall-clock time manually so the cursor still advances.
    private var virtualStartDate: Date?
    private var virtualStartPositionMs: Double = 0

    // MARK: - Repeat

    private var remainingRepeats: Int = 0

    // MARK: - Lifecycle

    init(audioSessionManager: AudioSessionManager) {
        self.audioSessionManager = audioSessionManager
    }

    deinit {
        stopDisplayLink()
        stoppingIntentionally = true
        midiPlayer?.stop()
    }

    // MARK: - MIDI tempo scaling

    /// Scale every tempo meta-event (`FF 51 03 tt tt tt`) in the MIDI data
    /// by dividing the microseconds-per-quarter value by `speed`.
    ///
    /// This makes the player play faster (speed > 1) or slower (speed < 1)
    /// without re-generating MIDI from Rust.
    private static func scaleMidiTempo(_ data: Data, speed: Double) -> Data {
        guard speed > 0, speed != 1.0 else { return data }
        var bytes = [UInt8](data)
        var i = 0
        while i < bytes.count - 5 {
            if bytes[i] == 0xFF && bytes[i + 1] == 0x51 && bytes[i + 2] == 0x03 {
                let uspq = UInt32(bytes[i + 3]) << 16
                       | UInt32(bytes[i + 4]) << 8
                       | UInt32(bytes[i + 5])
                let newUspq = max(1, UInt32(Double(uspq) / speed))
                bytes[i + 3] = UInt8((newUspq >> 16) & 0xFF)
                bytes[i + 4] = UInt8((newUspq >>  8) & 0xFF)
                bytes[i + 5] = UInt8( newUspq        & 0xFF)
                i += 6
            } else {
                i += 1
            }
        }
        return Data(bytes)
    }

    // MARK: - Public API

    /// Prepare MIDI data for playback (does not start playing).
    func prepareMidi(_ data: Data) {
        // Stop any current playback
        stop()

        originalMidiData = data
        rebuildPlayer()
    }

    /// Start or resume playback.
    func play() {
        guard originalMidiData != nil else {
            print("[PlaybackManager] No MIDI data loaded")
            return
        }

        // Set remaining repeats at the start of a fresh play (position 0).
        if currentTimeMs < 1.0 {
            remainingRepeats = repeatCount
        }

        if isMuted {
            // ── Muted mode: advance time via virtual timer, no audio ──
            virtualStartDate = Date()
            virtualStartPositionMs = currentTimeMs
            isPlaying = true
            startDisplayLink()
            print("[PlaybackManager] Playing (muted)")
        } else {
            // ── Normal mode: play through AVMIDIPlayer ──
            guard let player = midiPlayer else {
                print("[PlaybackManager] AVMIDIPlayer not ready")
                return
            }
            audioSessionManager.ensureSessionActive()
            stoppingIntentionally = false

            player.play {
                DispatchQueue.main.async { [weak self] in
                    self?.playbackDidFinish()
                }
            }
            isPlaying = true
            startDisplayLink()
            print("[PlaybackManager] Playing")
        }
    }

    /// Pause playback — cursor stays at the current position.
    func pause() {
        // 1. Capture current music-time position from whichever source is active.
        if let _ = virtualStartDate {
            // Virtual-timer mode was active — use it for the authoritative position.
            updateVirtualTime()
        } else if let player = midiPlayer {
            // Real-player mode — read from AVMIDIPlayer.
            currentTimeMs = player.currentPosition * 1000.0 * speed
        }

        // 2. Always stop the AVMIDIPlayer (safe even if it wasn't playing).
        stoppingIntentionally = true
        if let player = midiPlayer {
            player.stop()
            // Restore the player's position so a subsequent unmuted play() resumes correctly.
            let playerSec = currentTimeMs / 1000.0 / speed
            player.currentPosition = max(0, min(playerSec, player.duration))
        }

        isPlaying = false
        stopDisplayLink()
        virtualStartDate = nil
        updateCursor(timeMs: currentTimeMs)

        print("[PlaybackManager] Paused at \(String(format: "%.1f", currentTimeMs / 1000.0))s (music time)")
    }

    /// Stop playback and reset cursor to the beginning.
    func stop() {
        stoppingIntentionally = true
        midiPlayer?.stop()
        midiPlayer?.currentPosition = 0
        isPlaying = false
        currentTimeMs = 0
        remainingRepeats = 0
        virtualStartDate = nil
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
        let clampedMs = max(0, min(musicTimeMs, durationMs))
        currentTimeMs = clampedMs

        // Always keep the AVMIDIPlayer position in sync so that toggling
        // mute off later resumes from the correct spot.
        if let player = midiPlayer {
            let playerSec = clampedMs / 1000.0 / speed
            player.currentPosition = max(0, min(playerSec, player.duration))
        }

        // If the virtual timer is active, reset its baseline.
        if virtualStartDate != nil {
            virtualStartDate = Date()
            virtualStartPositionMs = clampedMs
        }

        updateCursor(timeMs: clampedMs)
        print("[PlaybackManager] Seeked to \(String(format: "%.1f", clampedMs / 1000.0))s (music time)")
    }

    // MARK: - Display Link (60fps cursor updates)

    private func startDisplayLink() {
        stopDisplayLink()
        let link = CADisplayLink(target: self, selector: #selector(displayLinkFired))
        link.add(to: .main, forMode: .common)
        displayLink = link
    }

    private func stopDisplayLink() {
        displayLink?.invalidate()
        displayLink = nil
    }

    @objc private func displayLinkFired() {
        guard isPlaying else { return }

        if isMuted {
            updateVirtualTime()
            if currentTimeMs >= durationMs {
                playbackDidFinish()
                return
            }
        } else {
            guard let player = midiPlayer else { return }
            let musicMs = player.currentPosition * 1000.0 * speed
            currentTimeMs = musicMs
        }

        updateCursor(timeMs: currentTimeMs)
    }

    // MARK: - WebView cursor communication

    private func updateCursor(timeMs: Double) {
        guard let webView = webView else { return }
        let js = "if (typeof moveCursor === 'function') { showCursor(); moveCursor(\(timeMs)); }"
        webView.evaluateJavaScript(js, completionHandler: nil)
    }

    private func hideCursor() {
        guard let webView = webView else { return }
        let js = "if (typeof hideCursor === 'function') { hideCursor(); }"
        webView.evaluateJavaScript(js, completionHandler: nil)
    }

    // MARK: - Private helpers

    /// Re-create the AVMIDIPlayer from `originalMidiData` using the current `speed`.
    private func rebuildPlayer() {
        guard let original = originalMidiData else { return }
        let scaled = PlaybackManager.scaleMidiTempo(original, speed: speed)
        do {
            let soundBankURL = Bundle.main.url(forResource: "gs_instruments", withExtension: "dls")
            if let bankURL = soundBankURL {
                midiPlayer = try AVMIDIPlayer(data: scaled, soundBankURL: bankURL)
            } else {
                midiPlayer = try AVMIDIPlayer(data: scaled, soundBankURL: nil)
            }
            midiPlayer?.prepareToPlay()
            // Duration in *music* time = player's wall-clock duration * speed
            durationMs = (midiPlayer?.duration ?? 0) * 1000.0 * speed
            updateCursor(timeMs: 0)
            print("[PlaybackManager] MIDI prepared: \(String(format: "%.1f", durationMs / 1000.0))s (music time), speed=\(speed)")
        } catch {
            print("[PlaybackManager] Failed to create AVMIDIPlayer: \(error.localizedDescription)")
            midiPlayer = nil
            durationMs = 0
        }
    }

    /// Called when speed changes at runtime.
    private func applySpeedChange() {
        guard originalMidiData != nil else { return }
        let wasPlaying = isPlaying
        let savedMusicTimeMs = currentTimeMs

        if wasPlaying { pause() }
        rebuildPlayer()

        // Restore position
        if savedMusicTimeMs > 0 {
            seek(to: savedMusicTimeMs)
        }
        if wasPlaying { play() }
    }

    /// Called when mute changes at runtime.
    private func applyMuteChange() {
        guard isPlaying else { return }
        let savedMusicTimeMs = currentTimeMs
        pause()
        seek(to: savedMusicTimeMs)
        play()
    }

    /// Advance `currentTimeMs` from the virtual clock (muted mode).
    private func updateVirtualTime() {
        guard let start = virtualStartDate else { return }
        let elapsedWallSec = max(0, Date().timeIntervalSince(start))
        let musicMs = virtualStartPositionMs + elapsedWallSec * speed * 1000.0
        currentTimeMs = min(musicMs, durationMs)
    }

    /// Called when playback reaches the end — either naturally (AVMIDIPlayer
    /// completion handler) or when the virtual timer hits durationMs.
    private func playbackDidFinish() {
        if stoppingIntentionally {
            stoppingIntentionally = false
            return
        }

        remainingRepeats -= 1
        if remainingRepeats > 0 {
            // ── More repeats to go — restart from the beginning ──
            print("[PlaybackManager] Repeat \(repeatCount - remainingRepeats)/\(repeatCount)")
            midiPlayer?.currentPosition = 0
            currentTimeMs = 0
            virtualStartDate = nil

            // Small delay to allow the player to reset cleanly
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) { [weak self] in
                self?.play()
            }
            return
        }

        // ── All repeats done ──
        isPlaying = false
        stopDisplayLink()
        midiPlayer?.currentPosition = 0
        currentTimeMs = 0
        virtualStartDate = nil
        updateCursor(timeMs: 0)
        print("[PlaybackManager] Playback finished (all repeats done)")
    }
}
