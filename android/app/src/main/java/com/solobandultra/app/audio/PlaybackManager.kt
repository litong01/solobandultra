package com.solobandultra.app.audio

import android.content.Context
import android.media.MediaPlayer
import android.util.Log
import android.view.Choreographer
import android.webkit.WebView
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import java.io.File

/**
 * Manages MIDI playback and cursor synchronization on Android.
 *
 * Uses MediaPlayer for native MIDI playback (no audio through WebView)
 * and Choreographer for frame-accurate cursor position updates.
 *
 * Supports:
 * - **Speed** — scales MIDI tempo events so the player runs faster/slower.
 * - **Mute** — sets MediaPlayer volume to zero; player still runs for cursor sync.
 * - **Repeat** — replays the piece N times automatically.
 */
class PlaybackManager(
    private val context: Context,
    private val audioSessionManager: AudioSessionManager
) {
    companion object {
        private const val TAG = "PlaybackManager"

        /**
         * Scale every tempo meta-event (`FF 51 03 tt tt tt`) in the MIDI data
         * by dividing the microseconds-per-quarter value by [speed].
         */
        fun scaleMidiTempo(data: ByteArray, speed: Double): ByteArray {
            if (speed <= 0.0 || speed == 1.0) return data
            val bytes = data.copyOf()
            var i = 0
            while (i < bytes.size - 5) {
                if (bytes[i] == 0xFF.toByte() &&
                    bytes[i + 1] == 0x51.toByte() &&
                    bytes[i + 2] == 0x03.toByte()
                ) {
                    val uspq = ((bytes[i + 3].toInt() and 0xFF) shl 16) or
                               ((bytes[i + 4].toInt() and 0xFF) shl 8) or
                               (bytes[i + 5].toInt() and 0xFF)
                    val newUspq = maxOf(1, (uspq.toDouble() / speed).toInt())
                    bytes[i + 3] = ((newUspq shr 16) and 0xFF).toByte()
                    bytes[i + 4] = ((newUspq shr 8) and 0xFF).toByte()
                    bytes[i + 5] = (newUspq and 0xFF).toByte()
                    i += 6
                } else {
                    i++
                }
            }
            return bytes
        }
    }

    // ── Observable state ────────────────────────────────────────────────

    private val _isPlaying = MutableStateFlow(false)
    val isPlaying: StateFlow<Boolean> = _isPlaying.asStateFlow()

    private val _currentTimeMs = MutableStateFlow(0.0)
    val currentTimeMs: StateFlow<Double> = _currentTimeMs.asStateFlow()

    private val _durationMs = MutableStateFlow(0.0)
    val durationMs: StateFlow<Double> = _durationMs.asStateFlow()

    // ── Playback settings ──────────────────────────────────────────────

    /** Playback speed multiplier. Clamped to [0.1, 5.0]. */
    var speed: Double = 1.0
        set(value) {
            val clamped = value.coerceIn(0.1, 5.0)
            if (field != clamped) {
                field = clamped
                applySpeedChange()
            }
        }

    /** When `true`, volume is zero but playback & cursor still run. */
    var isMuted: Boolean = false
        set(value) {
            field = value
            applyMuteChange()
        }

    /** Total number of plays (1 = play once, 2 = play twice, …). */
    var repeatCount: Int = 1

    // ── Internal state ──────────────────────────────────────────────────

    private var mediaPlayer: MediaPlayer? = null
    private var midiTempFile: File? = null
    var webView: WebView? = null

    private var choreographerCallback: Choreographer.FrameCallback? = null

    /** Original (un-scaled) MIDI bytes. Kept for re-scaling when speed changes. */
    private var originalMidiData: ByteArray? = null

    /** Remaining repeats (decremented on each finish). */
    private var remainingRepeats: Int = 0

    // ── Public API ──────────────────────────────────────────────────────

    /**
     * Prepare MIDI data for playback (does not start playing).
     */
    fun prepareMidi(midiBytes: ByteArray) {
        stop()
        originalMidiData = midiBytes
        rebuildPlayer()
    }

    /**
     * Start or resume playback.
     */
    fun play() {
        val player = mediaPlayer ?: run {
            Log.w(TAG, "No MIDI data loaded")
            return
        }

        // Set remaining repeats at the start of a fresh play (position near 0).
        if (_currentTimeMs.value < 1.0) {
            remainingRepeats = repeatCount
        }

        audioSessionManager.requestAudioFocus()
        // Apply current mute state
        applyMuteVolume(player)
        player.start()
        _isPlaying.value = true
        startChoreographer()

        Log.d(TAG, "Playing (speed=$speed, muted=$isMuted)")
    }

    /**
     * Pause playback.
     */
    fun pause() {
        val player = mediaPlayer ?: return
        player.pause()
        _isPlaying.value = false
        val playerMs = player.currentPosition.toDouble()
        _currentTimeMs.value = playerMs * speed
        stopChoreographer()

        // Keep cursor at the paused position
        updateCursor(_currentTimeMs.value)

        Log.d(TAG, "Paused at ${_currentTimeMs.value / 1000.0}s (music time)")
    }

    /**
     * Stop playback and reset to the beginning.
     */
    fun stop() {
        mediaPlayer?.let { player ->
            if (player.isPlaying) player.stop()
            player.release()
        }
        mediaPlayer = null
        _isPlaying.value = false
        _currentTimeMs.value = 0.0
        remainingRepeats = 0
        stopChoreographer()

        // Reset cursor to the beginning (keep it visible)
        updateCursor(0.0)

        // Clean up temp file
        midiTempFile?.delete()
        midiTempFile = null

        Log.d(TAG, "Stopped")
    }

    /**
     * Toggle play/pause.
     */
    fun togglePlayPause() {
        if (_isPlaying.value) {
            pause()
        } else {
            play()
        }
    }

    /**
     * Seek to a specific *music* time in milliseconds.
     */
    fun seekTo(musicTimeMs: Double) {
        val player = mediaPlayer ?: return

        val clampedMs = musicTimeMs.coerceIn(0.0, _durationMs.value)
        // Convert music time to player time
        val playerMs = (clampedMs / speed).toInt()
        player.seekTo(playerMs)
        _currentTimeMs.value = clampedMs

        // Update cursor immediately at the seek position
        updateCursor(clampedMs)

        Log.d(TAG, "Seeked to ${clampedMs / 1000.0}s (music time)")
    }

    /**
     * Release all resources. Call when the activity/composable is destroyed.
     */
    fun release() {
        stop()
        originalMidiData = null
        audioSessionManager.release()
    }

    // ── Choreographer (frame-accurate cursor updates) ───────────────────

    private fun startChoreographer() {
        stopChoreographer()
        val callback = object : Choreographer.FrameCallback {
            override fun doFrame(frameTimeNanos: Long) {
                if (_isPlaying.value) {
                    val player = mediaPlayer ?: return
                    // Player position is in wall-clock time; multiply by speed to get music time.
                    val musicMs = player.currentPosition.toDouble() * speed
                    _currentTimeMs.value = musicMs
                    updateCursor(musicMs)
                    Choreographer.getInstance().postFrameCallback(this)
                }
            }
        }
        choreographerCallback = callback
        Choreographer.getInstance().postFrameCallback(callback)
    }

    private fun stopChoreographer() {
        choreographerCallback?.let {
            Choreographer.getInstance().removeFrameCallback(it)
        }
        choreographerCallback = null
    }

    // ── WebView cursor communication ────────────────────────────────────

    private fun updateCursor(timeMs: Double) {
        webView?.post {
            webView?.evaluateJavascript(
                "if (typeof moveCursor === 'function') { showCursor(); moveCursor($timeMs); }",
                null
            )
        }
    }

    private fun hideCursor() {
        webView?.post {
            webView?.evaluateJavascript(
                "if (typeof hideCursor === 'function') { hideCursor(); }",
                null
            )
        }
    }

    // ── Private helpers ─────────────────────────────────────────────────

    /**
     * Re-create the MediaPlayer from [originalMidiData] using the current [speed].
     */
    private fun rebuildPlayer() {
        val original = originalMidiData ?: return
        val scaled = scaleMidiTempo(original, speed)

        try {
            // MediaPlayer requires a file, so write MIDI data to a temp file
            val tempFile = File.createTempFile("playback", ".mid", context.cacheDir)
            tempFile.writeBytes(scaled)
            midiTempFile = tempFile

            val player = MediaPlayer()
            player.setAudioAttributes(audioSessionManager.getAudioAttributes())
            player.setDataSource(tempFile.absolutePath)
            player.prepare()

            player.setOnCompletionListener {
                playbackDidFinish()
            }

            mediaPlayer = player
            // Duration in music time = player wall-clock duration * speed
            _durationMs.value = player.duration.toDouble() * speed

            // Show cursor at the beginning
            updateCursor(0.0)

            Log.d(TAG, "MIDI prepared: ${_durationMs.value / 1000.0}s (music time), speed=$speed")
        } catch (e: Exception) {
            Log.e(TAG, "Failed to prepare MIDI: ${e.message}")
            mediaPlayer = null
            _durationMs.value = 0.0
        }
    }

    /** Called when speed changes at runtime. */
    private fun applySpeedChange() {
        val original = originalMidiData ?: return
        val wasPlaying = _isPlaying.value
        val savedMusicTimeMs = _currentTimeMs.value

        if (wasPlaying) pause()
        rebuildPlayer()

        if (savedMusicTimeMs > 0.0) {
            seekTo(savedMusicTimeMs)
        }
        if (wasPlaying) play()
    }

    /** Apply mute volume to the current MediaPlayer. */
    private fun applyMuteVolume(player: MediaPlayer) {
        if (isMuted) {
            player.setVolume(0f, 0f)
        } else {
            player.setVolume(1f, 1f)
        }
    }

    /** Called when mute changes at runtime. */
    private fun applyMuteChange() {
        mediaPlayer?.let { applyMuteVolume(it) }
    }

    /** Called when playback reaches the end naturally. */
    private fun playbackDidFinish() {
        remainingRepeats--
        if (remainingRepeats > 0) {
            // ── More repeats to go — restart from the beginning ──
            Log.d(TAG, "Repeat ${repeatCount - remainingRepeats}/$repeatCount")
            mediaPlayer?.seekTo(0)
            _currentTimeMs.value = 0.0
            mediaPlayer?.start()
            return
        }

        // ── All repeats done ──
        _isPlaying.value = false
        stopChoreographer()
        _currentTimeMs.value = 0.0
        audioSessionManager.abandonAudioFocus()

        // Reset cursor to the beginning (keep it visible)
        updateCursor(0.0)

        Log.d(TAG, "Playback finished (all repeats done)")
    }
}
