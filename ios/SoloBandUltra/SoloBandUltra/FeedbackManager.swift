import Foundation
import AVFoundation

// MARK: - Constants

private enum FeedbackConstants {
    /// Audio capture sample rate (Hz). Matches iOS hardware and WAV playback (48 kHz).
    static let sampleRate: Double = 48000
    /// Number of samples per YIN analysis buffer (~85 ms at 48000 Hz).
    static let bufferSize: AVAudioFrameCount = 4096
    /// YIN threshold — lower = more accurate but may miss notes.
    static let yinThreshold: Double = 0.15
    /// RMS threshold below which silence is declared (−40 dBFS ≈ 0.01 linear).
    static let silenceGate: Float = 0.01
    /// Pitch match tolerance in semitones (50 cents = 1 semitone).
    static let pitchToleranceSemitones: Int = 1
    /// Timing window in ms for "correct" vs "wrong timing" (±200 ms).
    static let timingWindowMs: Double = 200
}

// MARK: - FeedbackManager

/// Manages real-time pitch detection and post-performance reporting.
///
/// Usage:
/// 1. Call `loadTimeline(_:)` when a new score is loaded.
/// 2. Call `startListening()` when playback begins with Feedback on.
/// 3. Call `update(musicMs:)` on every playback frame (tied to PlaybackManager).
/// 4. Call `stopListening()` when playback ends; `report` is then populated.
/// 5. Read `state` for the current cursor color hint; read `report` for the summary.
@MainActor
final class FeedbackManager: ObservableObject {

    // MARK: Published state

    /// The real-time feedback state, updated ~10 Hz during playback.
    @Published private(set) var state: FeedbackState = .silent
    /// The completed post-performance report (nil until playback finishes).
    @Published private(set) var report: FeedbackReport? = nil

    // MARK: Tap wiring (set by ContentView)

    /// Called with the buffer handler when permission is granted and capture should start.
    /// Should call `playbackManager.installMicrophoneTap(handler:)`.
    var tapInstaller: ((@escaping (AVAudioPCMBuffer) -> Void) -> Void)?

    /// Called when capture should stop.
    /// Should call `playbackManager.removeMicrophoneTap()`.
    var tapRemover: (() -> Void)?

    // MARK: Private capture state

    private var isListening = false
    /// Set when record permission is granted (so we can install tap before play next time).
    private var hasRecordPermission = false

    // MARK: Score data

    private var timeline: [NoteEvent] = []
    /// Index of the note currently "active" based on musicMs.
    private var expectedNoteIndex: Int = 0

    // MARK: Accumulation for report

    /// Keyed by NoteEvent.id (startMs) → result collected so far.
    private var collected: [Double: NoteResult] = [:]
    /// Detected frequency buffer from the most recent YIN run.
    private var detectedHz: Double? = nil
    /// Music-time of the most recent onset detection.
    private var lastOnsetMs: Double? = nil

    // MARK: - Public API

    /// Load the note timeline for the current score.
    /// Call this when a score is loaded or changed, before starting playback.
    func loadTimeline(_ events: [NoteEvent]) {
        timeline = events
        reset()
    }

    /// Request microphone permission and, if granted, install an input tap via
    /// `tapInstaller` on PlaybackManager's shared AVAudioEngine.
    /// - Parameter onPermissionDenied: Called if the user denies microphone access.
    func startListening(onPermissionDenied: @escaping () -> Void = {}) {
        AVAudioSession.sharedInstance().requestRecordPermission { [weak self] granted in
            DispatchQueue.main.async {
                guard let self = self else { return }
                guard granted else {
                    onPermissionDenied()
                    return
                }
                self.hasRecordPermission = true
                // Install the tap on PlaybackManager's running engine (only if not already from installTapIfReady).
                if !self.isListening {
                    self.tapInstaller? { [weak self] buffer in
                        self?.processBuffer(buffer)
                    }
                    self.isListening = true
                }
            }
        }
    }

    /// Install the mic tap now if we already have record permission.
    /// Called from PlaybackManager.beforePlaybackStarts so the tap is in place before
    /// playback starts, avoiding engine reconfiguration that can stop output.
    func installTapIfReady() {
        guard hasRecordPermission else { return }
        tapInstaller? { [weak self] buffer in
            self?.processBuffer(buffer)
        }
        isListening = true
    }

    /// Remove the microphone tap and build the final report.
    func stopListening() {
        guard isListening else { return }
        tapRemover?()
        isListening = false
        state = .silent
        buildReport()
    }

    /// Called every playback frame with the current music position in milliseconds.
    /// Updates `expectedNoteIndex` and emits feedback for the active note.
    func update(musicMs: Double) {
        guard isListening, !timeline.isEmpty else { return }

        // Advance expected note pointer.
        advanceExpectedNote(to: musicMs)

        guard expectedNoteIndex < timeline.count else { return }
        let expected = timeline[expectedNoteIndex]

        // Determine feedback state from latest detected pitch.
        let newState: FeedbackState
        if let hz = detectedHz {
            let detectedMidi = frequencyToMidi(hz)
            let pitchOk = abs(detectedMidi - expected.midi) <= FeedbackConstants.pitchToleranceSemitones

            if pitchOk {
                // Check timing: how far is the current musicMs from the note's start?
                let timingDelta = abs(musicMs - expected.startMs)
                newState = timingDelta <= FeedbackConstants.timingWindowMs ? .correct : .wrongTiming
            } else {
                newState = .wrongPitch
            }

            // Record onset for the report (first detection within this note's window).
            let noteId = expected.startMs
            if collected[noteId] == nil {
                let result = NoteResult(
                    expected: expected,
                    detectedMidi: detectedMidi,
                    detectedStartMs: musicMs,
                    status: newState
                )
                collected[noteId] = result
            }
        } else {
            newState = .silent
        }

        if newState != state {
            state = newState
        }
    }

    // MARK: - Private helpers

    private func reset() {
        expectedNoteIndex = 0
        collected = [:]
        detectedHz = nil
        lastOnsetMs = nil
        report = nil
        state = .silent
    }

    private func advanceExpectedNote(to musicMs: Double) {
        // Move forward through the timeline until we find the note whose window
        // contains musicMs, or the last note before musicMs.
        while expectedNoteIndex + 1 < timeline.count &&
              timeline[expectedNoteIndex].endMs <= musicMs {
            expectedNoteIndex += 1
        }
    }

    /// Process one audio buffer — called on the AVAudioEngine real-time audio thread.
    /// `nonisolated` allows safe cross-thread calls; state writes are dispatched to main.
    nonisolated func processBuffer(_ buffer: AVAudioPCMBuffer) {
        guard let channelData = buffer.floatChannelData?[0] else { return }
        let frameCount = Int(buffer.frameLength)
        guard frameCount > 0 else { return }

        // Copy samples out of the buffer immediately — the buffer memory is reused
        // by the engine after this callback returns.
        let samples = Array(UnsafeBufferPointer(start: channelData, count: frameCount))

        // RMS silence gate runs on the audio thread: it is O(n) and extremely fast
        // (~0.1 ms), so it doesn't risk blocking the real-time thread.
        let rms = computeRMS(samples)
        guard rms >= FeedbackConstants.silenceGate else {
            DispatchQueue.main.async { [weak self] in self?.detectedHz = nil }
            return
        }

        // YIN is O(n²) — offload it to a background queue so the real-time audio
        // thread is free to keep rendering output without glitches or dropouts.
        // Each buffer is ~85 ms of audio at 48000 Hz, so results arrive ~10× per
        // second regardless of which thread does the math.
        let sr = buffer.format.sampleRate > 0 ? buffer.format.sampleRate : FeedbackConstants.sampleRate
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            let hz = yin(samples: samples, sampleRate: sr, threshold: FeedbackConstants.yinThreshold)
            DispatchQueue.main.async { self?.detectedHz = hz }
        }
    }

    private func buildReport() {
        // Fill in missed notes (any expected note with no collected result).
        var allResults: [NoteResult] = []
        for note in timeline {
            if let result = collected[note.startMs] {
                allResults.append(result)
            } else {
                allResults.append(NoteResult(
                    expected: note,
                    detectedMidi: nil,
                    detectedStartMs: nil,
                    status: .silent
                ))
            }
        }
        report = FeedbackReport(results: allResults)
    }
}

// MARK: - YIN pitch detection algorithm

/// Estimate the fundamental frequency of a monophonic audio buffer using YIN.
///
/// Returns the estimated frequency in Hz, or nil if no clear pitch is found.
///
/// Reference: de Cheveigné & Kawahara (2002), "YIN, a fundamental frequency estimator
/// for speech and music", JASA 111(4).
private func yin(samples: [Float], sampleRate: Double, threshold: Double) -> Double? {
    let n = samples.count
    let halfN = n / 2
    guard halfN > 1 else { return nil }

    // Step 1 & 2: Difference function + cumulative mean normalised difference.
    var d = [Double](repeating: 0, count: halfN)
    var cmndf = [Double](repeating: 0, count: halfN)
    var runningSum = 0.0

    // d[0] = 0 by definition; cmndf[0] = 1.
    cmndf[0] = 1.0

    for tau in 1..<halfN {
        var sum = 0.0
        for j in 0..<halfN {
            let diff = Double(samples[j]) - Double(samples[j + tau])
            sum += diff * diff
        }
        d[tau] = sum
        runningSum += sum
        cmndf[tau] = (runningSum > 0) ? (d[tau] * Double(tau) / runningSum) : 1.0
    }

    // Step 3: Absolute threshold — find the first tau where cmndf < threshold.
    var tau = 2
    while tau < halfN {
        if cmndf[tau] < threshold {
            // Step 4: Parabolic interpolation to refine the estimate.
            let refinedTau = parabolicInterpolation(cmndf, tau: tau)
            guard refinedTau > 0 else { return nil }
            return sampleRate / refinedTau
        }
        tau += 1
    }

    // No clear pitch below threshold — return the global minimum.
    let (minTau, _) = (2..<halfN).map { ($0, cmndf[$0]) }.min(by: { $0.1 < $1.1 }) ?? (0, 1.0)
    guard minTau > 0 else { return nil }
    let refinedTau = parabolicInterpolation(cmndf, tau: minTau)
    guard refinedTau > 0 else { return nil }
    return sampleRate / refinedTau
}

/// Parabolic interpolation around `tau` in the CMNDF to sub-sample refine the minimum.
private func parabolicInterpolation(_ cmndf: [Double], tau: Int) -> Double {
    guard tau > 0 && tau < cmndf.count - 1 else { return Double(tau) }
    let s0 = cmndf[tau - 1]
    let s1 = cmndf[tau]
    let s2 = cmndf[tau + 1]
    let denom = s0 - 2 * s1 + s2
    guard abs(denom) > 1e-9 else { return Double(tau) }
    return Double(tau) + 0.5 * (s0 - s2) / denom
}

// MARK: - Pitch / MIDI utilities

/// Convert a frequency in Hz to the nearest MIDI note number.
private func frequencyToMidi(_ hz: Double) -> Int {
    guard hz > 0 else { return 0 }
    return Int((69.0 + 12.0 * log2(hz / 440.0)).rounded())
}

/// Compute the root-mean-square amplitude of a sample buffer.
private func computeRMS(_ samples: [Float]) -> Float {
    guard !samples.isEmpty else { return 0 }
    let sumSq = samples.reduce(0.0) { $0 + Double($1) * Double($1) }
    return Float((sumSq / Double(samples.count)).squareRoot())
}
