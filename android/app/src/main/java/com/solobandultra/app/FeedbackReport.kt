package com.solobandultra.app

import org.json.JSONArray
import kotlin.math.abs

// ── Note Event (decoded from Rust JSON) ──────────────────────────────────────

/** A single melody note from the score with absolute timing. */
data class NoteEvent(
    val startMs: Double,
    val endMs: Double,
    val midi: Int,
    val name: String
) {
    val durationMs: Double get() = endMs - startMs

    companion object {
        /** Parse a JSON array string (from [ScoreLib.noteTimeline]) into a list. */
        fun parseList(json: String): List<NoteEvent> {
            return try {
                val arr = JSONArray(json)
                List(arr.length()) { i ->
                    val obj = arr.getJSONObject(i)
                    NoteEvent(
                        startMs = obj.getDouble("start_ms"),
                        endMs   = obj.getDouble("end_ms"),
                        midi    = obj.getInt("midi"),
                        name    = obj.getString("name")
                    )
                }
            } catch (_: Exception) {
                emptyList()
            }
        }
    }
}

// ── Feedback State ────────────────────────────────────────────────────────────

/** Real-time feedback state for the current note. */
enum class FeedbackState(val cursorColor: String) {
    /** No signal / user not playing. */
    Silent("rgb(234,107,36)"),
    /** Correct pitch and within the timing window (±200 ms). */
    Correct("#4CAF50"),
    /** Correct pitch but note arrived outside the timing window. */
    WrongTiming("#FFC107"),
    /** Wrong pitch detected. */
    WrongPitch("#F44336")
}

// ── Per-note result ───────────────────────────────────────────────────────────

/** Outcome for a single expected note in the score. */
data class NoteResult(
    val expected: NoteEvent,
    val detectedMidi: Int?,
    val detectedStartMs: Double?,
    val status: FeedbackState
) {
    val timingDeltaMs: Double? get() =
        if (detectedStartMs != null) detectedStartMs - expected.startMs else null

    /** True if detected pitch matches expected: within 2 semitones, or same pitch class but wrong octave (YIN harmonic). */
    val pitchCorrect: Boolean get() = isPitchMatch(detectedMidi, expected.midi)

    val detectedName: String get() =
        if (detectedMidi != null) midiToName(detectedMidi) else "—"
}

/**
 * Whether detected pitch counts as correct for the expected note.
 * Allows within 2 semitones (tuning/YIN drift) or same pitch class one octave off (YIN harmonic).
 */
fun isPitchMatch(detectedMidi: Int?, expectedMidi: Int): Boolean {
    if (detectedMidi == null) return false
    val diff = abs(detectedMidi - expectedMidi)
    return diff <= 2 || (diff == 12 && (detectedMidi % 12) == (expectedMidi % 12))
}

/** Convert a MIDI note number to a readable name like "C4", "F#3". */
fun midiToName(midi: Int): String {
    val names = arrayOf("C","C#","D","Eb","E","F","F#","G","Ab","A","Bb","B")
    val octave = (midi / 12) - 1
    val pc = midi % 12
    return "${names[pc]}$octave"
}

// ── Post-performance report ───────────────────────────────────────────────────

/** Aggregated performance report built after a piece finishes. */
data class FeedbackReport(val results: List<NoteResult>) {

    /** Percentage of notes where detected pitch was correct (0–100). */
    val pitchAccuracy: Double get() {
        val pitched = results.filter { it.detectedMidi != null }
        if (pitched.isEmpty()) return 0.0
        val correct = pitched.count { it.pitchCorrect }
        return correct.toDouble() / pitched.size * 100.0
    }

    /** Percentage of detected notes where timing was within ±200 ms (0–100). */
    val rhythmAccuracy: Double get() {
        val detected = results.filter { it.detectedStartMs != null }
        if (detected.isEmpty()) return 0.0
        val onTime = detected.count { abs(it.timingDeltaMs ?: 9999.0) <= 200.0 }
        return onTime.toDouble() / detected.size * 100.0
    }

    /** Weighted overall score: 60 % pitch + 40 % rhythm. */
    val overallScore: Double get() = pitchAccuracy * 0.6 + rhythmAccuracy * 0.4

    val missedNotes: List<NoteResult> get() = results.filter { it.detectedMidi == null }

    val timingDeviations: List<Double> get() = results.mapNotNull { it.timingDeltaMs }

    val totalNotes: Int get() = results.size

    val attemptedNotes: Int get() = results.count { it.detectedMidi != null }
}
