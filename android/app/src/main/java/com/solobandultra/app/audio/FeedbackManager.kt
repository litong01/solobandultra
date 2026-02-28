package com.solobandultra.app.audio

import android.annotation.SuppressLint
import android.media.AudioFormat
import android.media.AudioRecord
import android.media.MediaRecorder
import android.media.audiofx.AcousticEchoCanceler
import android.media.audiofx.NoiseSuppressor
import android.util.Log
import com.solobandultra.app.FeedbackReport
import com.solobandultra.app.FeedbackState
import com.solobandultra.app.NoteEvent
import com.solobandultra.app.NoteResult
import com.solobandultra.app.isPitchMatch
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlin.math.abs
import kotlin.math.log2

// ── Constants ─────────────────────────────────────────────────────────────────

private const val TAG = "SBU.Feedback"
private const val SAMPLE_RATE = 48000
private const val BUFFER_SIZE = 4096
// Slightly permissive threshold to handle the ambient mic environment.
private const val YIN_THRESHOLD = 0.20
private const val TIMING_WINDOW_MS = 200.0
// Plausible instrument range: C2 (~65 Hz) – C7 (~2093 Hz)
private const val MIN_FREQ_HZ = 60.0
private const val MAX_FREQ_HZ = 2200.0

// ── FeedbackManager ────────────────────────────────────────────────────────────

/**
 * Manages real-time pitch detection and post-performance reporting on Android.
 *
 * Usage:
 * 1. Call [loadTimeline] when a new score is loaded.
 * 2. Call [startListening] when playback begins with Feedback on.
 * 3. Call [update] on every playback frame (~10 Hz).
 * 4. Call [stopListening] when playback ends; [report] is then populated.
 */
class FeedbackManager {

    // ── Public state flows ────────────────────────────────────────────────────

    private val _state = MutableStateFlow(FeedbackState.Silent)
    val state: StateFlow<FeedbackState> = _state.asStateFlow()

    private val _report = MutableStateFlow<FeedbackReport?>(null)
    val report: StateFlow<FeedbackReport?> = _report.asStateFlow()

    // ── Score data ────────────────────────────────────────────────────────────

    private var timeline: List<NoteEvent> = emptyList()
    private var expectedNoteIndex = 0

    // ── Capture state ─────────────────────────────────────────────────────────

    private var audioRecord: AudioRecord? = null
    private var aecEffect: AcousticEchoCanceler? = null
    private var nsEffect: NoiseSuppressor? = null
    private var captureJob: Job? = null
    @Volatile private var detectedHz: Double? = null
    private var isListening = false

    // ── Accumulation for report ───────────────────────────────────────────────

    private val collected = mutableMapOf<Double, NoteResult>()

    // ── Public API ────────────────────────────────────────────────────────────

    /** Load the note timeline for the current score. */
    fun loadTimeline(events: List<NoteEvent>) {
        timeline = events
        reset()
        Log.d(TAG, "loadTimeline: ${events.size} notes")
    }

    /**
     * Begin microphone capture.
     * The caller must have already obtained [android.Manifest.permission.RECORD_AUDIO].
     */
    @SuppressLint("MissingPermission")
    fun startListening() {
        if (isListening) return
        Log.d(TAG, "startListening: timeline=${timeline.size} notes")

        val record = createAudioRecord() ?: run {
            Log.e(TAG, "startListening: createAudioRecord() returned null")
            return
        }

        // Reset per-play counters so replaying the same score starts fresh.
        expectedNoteIndex = 0
        collected.clear()
        detectedHz = null

        audioRecord = record
        record.startRecording()
        isListening = true
        Log.d(TAG, "startListening: recording started (isListening=true)")

        captureJob = CoroutineScope(Dispatchers.IO).launch {
            // Non-blocking reads accumulated into a sliding window for YIN.
            val accumBuf  = FloatArray(BUFFER_SIZE * 2)
            val shortBuf  = ShortArray(BUFFER_SIZE)
            val floatChunk = FloatArray(BUFFER_SIZE)
            var accumulated = 0
            var bufCount = 0
            var zeroStreak = 0
            var firstData = false

            while (isListening) {
                val read = record.read(shortBuf, 0, BUFFER_SIZE, AudioRecord.READ_NON_BLOCKING)
                when {
                    read > 0 -> {
                        if (!firstData) {
                            Log.d(TAG, "first audio data: $read samples")
                            firstData = true
                        }
                        zeroStreak = 0
                        for (i in 0 until read) floatChunk[i] = shortBuf[i] / 32768f

                        // Append to accumulator
                        val toCopy = minOf(read, accumBuf.size - accumulated)
                        floatChunk.copyInto(accumBuf, accumulated, 0, toCopy)
                        accumulated += toCopy

                        if (accumulated >= BUFFER_SIZE) {
                            bufCount++
                            val hz = yinFiltered(accumBuf, 0, BUFFER_SIZE, SAMPLE_RATE.toDouble(), YIN_THRESHOLD)
                            detectedHz = hz
                            if (bufCount % 20 == 0) {
                                val maxAbs = (0 until BUFFER_SIZE).maxOfOrNull { abs(accumBuf[it]) } ?: 0f
                                val r = rms(accumBuf, BUFFER_SIZE)
                                Log.d(TAG, "buf #$bufCount  rms=${"%.5f".format(r)}  max=${"%.5f".format(maxAbs)}  hz=${hz?.let { "%.1f".format(it) } ?: "null"}")
                            }
                            // Slide: keep second half as overlap
                            val half = BUFFER_SIZE / 2
                            accumBuf.copyInto(accumBuf, 0, half, BUFFER_SIZE)
                            accumulated = half
                        }
                    }
                    read == 0 -> {
                        zeroStreak++
                        if (zeroStreak % 200 == 0)
                            Log.w(TAG, "no audio data for ~${zeroStreak * 5}ms")
                        Thread.sleep(5)
                    }
                    else -> {
                        Log.e(TAG, "AudioRecord.read error: $read — stopping capture")
                        break
                    }
                }
            }
            Log.d(TAG, "capture loop exited  bufCount=$bufCount")
        }
    }

    /** Stop capture and build the final report. */
    fun stopListening() {
        if (!isListening) {
            Log.d(TAG, "stopListening: not listening — report NOT built")
            return
        }
        Log.d(TAG, "stopListening: collected=${collected.size}/${timeline.size}")
        isListening = false
        captureJob?.cancel()
        captureJob = null
        audioRecord?.stop()
        audioRecord?.release()
        audioRecord = null
        aecEffect?.release(); aecEffect = null
        nsEffect?.release();  nsEffect = null
        _state.value = FeedbackState.Silent
        buildReport()
    }

    /**
     * Try to create an [AudioRecord] using the best available audio source.
     *
     * Source order:
     *  1. UNPROCESSED — avoids aggressive OEM processing (AEC/NS/AGC),
     *     best when you want to pick up real acoustic instruments / room audio.
     *  2. MIC — common default.
     *  3. CAMCORDER — sometimes has different gain/processing on devices that
     *     behave poorly with MIC while media is playing.
     *  4. VOICE_RECOGNITION — tends to apply voice-y processing; last resort.
     *  5. VOICE_COMMUNICATION — may strongly echo-cancel device playback; keep as
     *     a final fallback for devices that otherwise deliver no input at all.
     *
     * Encoding: PCM_16BIT only (universally supported; PCM_FLOAT may block on
     * some HALs even after STATE_INITIALIZED succeeds).
     */
    @SuppressLint("MissingPermission")
    private fun createAudioRecord(): AudioRecord? {
        val sources = listOf(
            MediaRecorder.AudioSource.UNPROCESSED,
            MediaRecorder.AudioSource.MIC,
            MediaRecorder.AudioSource.CAMCORDER,
            MediaRecorder.AudioSource.VOICE_RECOGNITION,
            MediaRecorder.AudioSource.VOICE_COMMUNICATION
        )

        val minBuf = maxOf(
            AudioRecord.getMinBufferSize(SAMPLE_RATE, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT),
            BUFFER_SIZE * 2  // keep the buffer generous to avoid overruns
        )

        for (source in sources) {
            val record = try {
                AudioRecord(
                    source, SAMPLE_RATE,
                    AudioFormat.CHANNEL_IN_MONO,
                    AudioFormat.ENCODING_PCM_16BIT,
                    minBuf
                )
            } catch (e: Exception) {
                Log.w(TAG, "Failed to create AudioRecord for source $source: ${e.message}")
                null
            } ?: continue

            if (record.state != AudioRecord.STATE_INITIALIZED) {
                record.release()
                Log.w(TAG, "AudioRecord not initialized for source=$source")
                continue
            }

            val sid = record.audioSessionId
            // Disable NoiseSuppressor: instrument transients can be attenuated.
            if (NoiseSuppressor.isAvailable()) {
                nsEffect = NoiseSuppressor.create(sid)
                nsEffect?.enabled = false
            }
            // Disable AEC: we want to capture the instrument even if the phone
            // speaker is audible; we rely on the user being closer to the mic.
            if (AcousticEchoCanceler.isAvailable()) {
                aecEffect = AcousticEchoCanceler.create(sid)
                aecEffect?.enabled = false
            }

            val sourceName = when (source) {
                MediaRecorder.AudioSource.MIC -> "MIC"
                MediaRecorder.AudioSource.CAMCORDER -> "CAMCORDER"
                MediaRecorder.AudioSource.VOICE_RECOGNITION -> "VOICE_RECOG"
                MediaRecorder.AudioSource.VOICE_COMMUNICATION -> "VOICE_COMM"
                MediaRecorder.AudioSource.UNPROCESSED -> "UNPROCESSED"
                else -> "SRC_$source"
            }
            Log.d(TAG, "AudioRecord created: $sourceName / PCM_16BIT  bufBytes=$minBuf")
            return record
        }
        Log.e(TAG, "createAudioRecord: all sources failed")
        return null
    }

    /**
     * Called every ~100 ms with the current music position.
     * Updates expected note and emits real-time feedback state.
     */
    fun update(musicMs: Double) {
        if (!isListening || timeline.isEmpty()) return

        advanceExpectedNote(musicMs)
        if (expectedNoteIndex >= timeline.size) return
        val expected = timeline[expectedNoteIndex]

        val hz = detectedHz
        val newState: FeedbackState = if (hz != null) {
            val detMidi = frequencyToMidi(hz)
            val pitchOk = isPitchMatch(detMidi, expected.midi)
            if (pitchOk) {
                if (abs(musicMs - expected.startMs) <= TIMING_WINDOW_MS)
                    FeedbackState.Correct
                else
                    FeedbackState.WrongTiming
            } else {
                FeedbackState.WrongPitch
            }
        } else {
            FeedbackState.Silent
        }

        // Record first detection within this note's window.
        if (hz != null && !collected.containsKey(expected.startMs)) {
            collected[expected.startMs] = NoteResult(
                expected        = expected,
                detectedMidi    = frequencyToMidi(hz),
                detectedStartMs = musicMs,
                status          = newState
            )
        }

        if (_state.value != newState) _state.value = newState
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    private fun reset() {
        expectedNoteIndex = 0
        collected.clear()
        detectedHz = null
        _report.value = null
        _state.value = FeedbackState.Silent
    }

    private fun advanceExpectedNote(musicMs: Double) {
        while (expectedNoteIndex + 1 < timeline.size &&
               timeline[expectedNoteIndex].endMs <= musicMs) {
            expectedNoteIndex++
        }
    }

    private fun buildReport() {
        val allResults = timeline.map { note ->
            collected[note.startMs] ?: NoteResult(
                expected        = note,
                detectedMidi    = null,
                detectedStartMs = null,
                status          = FeedbackState.Silent
            )
        }
        _report.value = FeedbackReport(allResults)
    }
}

// ── YIN pitch detection ────────────────────────────────────────────────────────

private fun yinFiltered(buf: FloatArray, offset: Int, length: Int, sampleRate: Double, threshold: Double): Double? {
    val hz = yin(buf, offset, length, sampleRate, threshold) ?: return null
    return if (hz in MIN_FREQ_HZ..MAX_FREQ_HZ) hz else null
}

/**
 * YIN pitch estimator (de Cheveigné & Kawahara, JASA 2002).
 * Returns the fundamental frequency in Hz, or null if no clear pitch is found.
 */
private fun yin(buf: FloatArray, offset: Int, length: Int, sampleRate: Double, threshold: Double): Double? {
    val halfN = length / 2
    if (halfN < 2) return null

    val d     = DoubleArray(halfN)
    val cmndf = DoubleArray(halfN)
    var runningSum = 0.0
    cmndf[0] = 1.0

    for (tau in 1 until halfN) {
        var sum = 0.0
        for (j in 0 until halfN) {
            val diff = buf[offset + j].toDouble() - buf[offset + j + tau].toDouble()
            sum += diff * diff
        }
        d[tau] = sum
        runningSum += sum
        cmndf[tau] = if (runningSum > 0.0) d[tau] * tau / runningSum else 1.0
    }

    var tau = 2
    while (tau < halfN) {
        if (cmndf[tau] < threshold) {
            val refined = parabolicInterpolation(cmndf, tau)
            if (refined > 0.0) return sampleRate / refined
        }
        tau++
    }
    return null
}

private fun parabolicInterpolation(cmndf: DoubleArray, tau: Int): Double {
    if (tau <= 0 || tau >= cmndf.size - 1) return tau.toDouble()
    val s0 = cmndf[tau - 1]; val s1 = cmndf[tau]; val s2 = cmndf[tau + 1]
    val denom = s0 - 2.0 * s1 + s2
    if (abs(denom) < 1e-9) return tau.toDouble()
    return tau + 0.5 * (s0 - s2) / denom
}

// ── Pitch / MIDI helpers ───────────────────────────────────────────────────────

private fun frequencyToMidi(hz: Double): Int {
    if (hz <= 0.0) return 0
    return kotlin.math.round(69.0 + 12.0 * log2(hz / 440.0)).toInt()
}

private fun rms(buf: FloatArray, length: Int): Float {
    if (length <= 0) return 0f
    var sum = 0.0
    for (i in 0 until length) sum += buf[i].toDouble() * buf[i].toDouble()
    return kotlin.math.sqrt(sum / length).toFloat()
}
