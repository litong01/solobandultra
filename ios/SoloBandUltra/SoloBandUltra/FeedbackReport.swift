import Foundation

// MARK: - Note Event (decoded from Rust JSON)

/// A single melody note from the score, with absolute timing.
struct NoteEvent: Codable, Identifiable {
    let startMs: Double
    let endMs: Double
    let midi: Int
    let name: String
    /// Original measure index for overlay positioning.
    let measureIdx: Int
    /// Index of this note among melody notes in that measure.
    let noteIdx: Int

    var id: Double { startMs }

    enum CodingKeys: String, CodingKey {
        case startMs = "start_ms"
        case endMs   = "end_ms"
        case midi
        case name
        case measureIdx = "measure_idx"
        case noteIdx = "note_idx"
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        startMs = try c.decode(Double.self, forKey: .startMs)
        endMs = try c.decode(Double.self, forKey: .endMs)
        midi = try c.decode(Int.self, forKey: .midi)
        name = try c.decode(String.self, forKey: .name)
        measureIdx = try c.decodeIfPresent(Int.self, forKey: .measureIdx) ?? 0
        noteIdx = try c.decodeIfPresent(Int.self, forKey: .noteIdx) ?? 0
    }

    init(startMs: Double, endMs: Double, midi: Int, name: String, measureIdx: Int = 0, noteIdx: Int = 0) {
        self.startMs = startMs
        self.endMs = endMs
        self.midi = midi
        self.name = name
        self.measureIdx = measureIdx
        self.noteIdx = noteIdx
    }

    /// Duration in milliseconds.
    var durationMs: Double { endMs - startMs }
}

// MARK: - Feedback State

/// The real-time feedback state for the current note.
enum FeedbackState: Equatable {
    /// No signal detected (user not playing / silence).
    case silent
    /// Correct pitch and within the timing window.
    case correct
    /// Correct pitch, but note onset was outside the timing window.
    case wrongTiming
    /// Wrong pitch detected.
    case wrongPitch

    /// Hex color string for the cursor.
    var cursorColor: String {
        switch self {
        case .silent:      return "rgb(234,107,36)"   // default orange
        case .correct:     return "#4CAF50"            // green
        case .wrongTiming: return "#FFC107"            // yellow
        case .wrongPitch:  return "#F44336"            // red
        }
    }
}

// MARK: - Per-note result

/// The outcome for a single expected note in the score.
struct NoteResult: Identifiable {
    /// The expected note from the score.
    let expected: NoteEvent
    /// The MIDI note number the user actually played (nil = missed / no signal).
    let detectedMidi: Int?
    /// The timestamp (music ms) when the user's onset was detected (nil = missed).
    let detectedStartMs: Double?
    /// How far in ms the detected onset was from the expected onset (positive = late).
    var timingDeltaMs: Double? {
        guard let det = detectedStartMs else { return nil }
        return det - expected.startMs
    }
    /// Whether the detected pitch matches the expected pitch (within 50 cents ≈ 1 semitone).
    var pitchCorrect: Bool {
        guard let det = detectedMidi else { return false }
        return abs(det - expected.midi) <= 1
    }
    /// The final status of this note.
    let status: FeedbackState

    var id: Double { expected.startMs }

    /// Human-readable detected note name, or "—" if missed.
    var detectedName: String {
        guard let midi = detectedMidi else { return "—" }
        return midiToName(midi)
    }
}

/// Convert a MIDI note number to a readable name like "C4", "F#3".
func midiToName(_ midi: Int) -> String {
    let names = ["C","C#","D","Eb","E","F","F#","G","Ab","A","Bb","B"]
    let octave = (midi / 12) - 1
    let pc     = midi % 12
    return "\(names[pc])\(octave)"
}

// MARK: - Post-performance report

/// Aggregated performance report built after a piece finishes.
struct FeedbackReport {
    let results: [NoteResult]

    /// Percentage of notes where the detected pitch was correct (0–100).
    var pitchAccuracy: Double {
        let pitched = results.filter { $0.detectedMidi != nil }
        guard !pitched.isEmpty else { return 0 }
        let correct = pitched.filter { $0.pitchCorrect }.count
        return Double(correct) / Double(pitched.count) * 100
    }

    /// Percentage of detected notes where timing was within ±200 ms (0–100).
    var rhythmAccuracy: Double {
        let detected = results.filter { $0.detectedStartMs != nil }
        guard !detected.isEmpty else { return 0 }
        let onTime = detected.filter { abs($0.timingDeltaMs ?? 9999) <= 200 }.count
        return Double(onTime) / Double(detected.count) * 100
    }

    /// Weighted overall score: 60 % pitch + 40 % rhythm.
    var overallScore: Double {
        pitchAccuracy * 0.6 + rhythmAccuracy * 0.4
    }

    /// Notes where no detection was recorded.
    var missedNotes: [NoteResult] {
        results.filter { $0.detectedMidi == nil }
    }

    /// Timing delta (ms) for each detected note, in score order.
    var timingDeviations: [Double] {
        results.compactMap { $0.timingDeltaMs }
    }

    /// Total notes in the score.
    var totalNotes: Int { results.count }

    /// Notes the user attempted (non-missed).
    var attemptedNotes: Int { results.filter { $0.detectedMidi != nil }.count }
}
