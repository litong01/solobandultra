import Foundation
import AVFoundation
import WebKit
import Combine

/// Manages MIDI playback and cursor synchronization.
///
/// Uses AVMIDIPlayer for native MIDI playback (no audio through WebView)
/// and CADisplayLink for 60fps cursor position updates.
class PlaybackManager: ObservableObject {
    // MARK: - Published state

    @Published var isPlaying = false
    @Published var currentTimeMs: Double = 0
    @Published var durationMs: Double = 0

    // MARK: - Dependencies

    private let audioSessionManager: AudioSessionManager
    weak var webView: WKWebView?

    // MARK: - MIDI playback

    private var midiPlayer: AVMIDIPlayer?
    private var midiData: Data?
    private var displayLink: CADisplayLink?

    /// Flag to suppress the completion handler when we intentionally stop/pause.
    /// AVMIDIPlayer.stop() fires the completion block, so we need to distinguish
    /// a user-initiated pause/stop from playback naturally reaching the end.
    private var stoppingIntentionally = false

    // MARK: - Lifecycle

    init(audioSessionManager: AudioSessionManager) {
        self.audioSessionManager = audioSessionManager
    }

    deinit {
        stopDisplayLink()
        stoppingIntentionally = true
        midiPlayer?.stop()
    }

    // MARK: - Public API

    /// Prepare MIDI data for playback (does not start playing).
    func prepareMidi(_ data: Data) {
        // Stop any current playback
        stop()

        midiData = data

        do {
            // AVMIDIPlayer needs a soundbank URL. Use the system default (GS/DLS).
            // On iOS the default General MIDI sound bank is built into the OS.
            let soundBankURL = Bundle.main.url(forResource: "gs_instruments", withExtension: "dls")

            if let bankURL = soundBankURL {
                midiPlayer = try AVMIDIPlayer(data: data, soundBankURL: bankURL)
            } else {
                // Fall back to no explicit sound bank — iOS uses the built-in one
                midiPlayer = try AVMIDIPlayer(data: data, soundBankURL: nil)
            }

            midiPlayer?.prepareToPlay()
            durationMs = (midiPlayer?.duration ?? 0) * 1000.0

            // Show cursor at the beginning
            updateCursor(timeMs: 0)

            print("[PlaybackManager] MIDI prepared: \(String(format: "%.1f", durationMs / 1000.0))s")
        } catch {
            print("[PlaybackManager] Failed to create AVMIDIPlayer: \(error.localizedDescription)")
            midiPlayer = nil
            durationMs = 0
        }
    }

    /// Start or resume playback.
    func play() {
        guard let player = midiPlayer else {
            print("[PlaybackManager] No MIDI data loaded")
            return
        }

        audioSessionManager.ensureSessionActive()

        stoppingIntentionally = false

        player.play {
            // Completion handler — called when playback finishes OR stop() is called
            DispatchQueue.main.async { [weak self] in
                self?.playbackDidFinish()
            }
        }

        isPlaying = true
        startDisplayLink()

        print("[PlaybackManager] Playing")
    }

    /// Pause playback — cursor stays at the current position.
    func pause() {
        guard let player = midiPlayer else { return }

        // Save position before stop — AVMIDIPlayer.stop() resets currentPosition
        let savedPositionSec = player.currentPosition

        // Tell the completion handler to ignore this stop
        stoppingIntentionally = true

        player.stop()

        // Restore position so resume works correctly
        player.currentPosition = savedPositionSec

        isPlaying = false
        currentTimeMs = savedPositionSec * 1000.0
        stopDisplayLink()

        // Keep cursor at the paused position
        updateCursor(timeMs: currentTimeMs)

        print("[PlaybackManager] Paused at \(String(format: "%.1f", savedPositionSec))s")
    }

    /// Stop playback and reset cursor to the beginning.
    func stop() {
        // Tell the completion handler to ignore this stop
        stoppingIntentionally = true

        midiPlayer?.stop()
        midiPlayer?.currentPosition = 0
        isPlaying = false
        currentTimeMs = 0
        stopDisplayLink()

        // Show cursor at the beginning (not hidden)
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

    /// Seek to a specific time in milliseconds.
    func seek(to timeMs: Double) {
        guard let player = midiPlayer else { return }

        let timeSec = timeMs / 1000.0
        let clampedSec = max(0, min(timeSec, player.duration))
        player.currentPosition = clampedSec

        currentTimeMs = clampedSec * 1000.0

        // Update cursor immediately at the seek position
        updateCursor(timeMs: currentTimeMs)

        print("[PlaybackManager] Seeked to \(String(format: "%.1f", clampedSec))s")
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
        guard let player = midiPlayer, isPlaying else { return }

        let posMs = player.currentPosition * 1000.0
        currentTimeMs = posMs
        updateCursor(timeMs: posMs)
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

    // MARK: - Private

    private func playbackDidFinish() {
        // Ignore if we intentionally stopped/paused — we handle cursor ourselves
        if stoppingIntentionally {
            stoppingIntentionally = false
            return
        }

        // Playback reached the end naturally
        isPlaying = false
        stopDisplayLink()
        midiPlayer?.currentPosition = 0
        currentTimeMs = 0

        // Reset cursor to the beginning (keep it visible)
        updateCursor(timeMs: 0)

        print("[PlaybackManager] Playback finished")
    }
}
