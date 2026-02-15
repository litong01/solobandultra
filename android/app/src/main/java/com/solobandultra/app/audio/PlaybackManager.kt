package com.solobandultra.app.audio

import android.content.Context
import android.media.MediaPlayer
import android.media.PlaybackParams
import android.util.Log
import android.view.Choreographer
import android.webkit.WebView
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import java.io.File

/**
 * Manages offline-rendered WAV audio playback and cursor synchronization on Android.
 *
 * Uses MediaPlayer with PlaybackParams for native WAV playback with speed control
 * (time-stretch without pitch change) and Choreographer for frame-accurate cursor
 * position updates.
 *
 * Supports:
 * - **Speed** — uses PlaybackParams for time-stretch speed control.
 * - **Mute** — sets MediaPlayer volume to zero; player still runs for cursor sync.
 * - **Repeat** — replays the piece N times automatically.
 */
class PlaybackManager(
    private val context: Context,
    private val audioSessionManager: AudioSessionManager
) {
    companion object {
        private const val TAG = "PlaybackManager"
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
    private var wavTempFile: File? = null
    var webView: WebView? = null

    private var choreographerCallback: Choreographer.FrameCallback? = null

    /** Remaining repeats (decremented on each finish). */
    private var remainingRepeats: Int = 0

    // ── Public API ──────────────────────────────────────────────────────

    /**
     * Write WAV data to a temp file on a background thread.
     * Returns the temp file, or null on error.
     * This is the only part that should run on Dispatchers.IO.
     */
    fun writeTempWav(wavBytes: ByteArray): File? {
        return try {
            val tempFile = File.createTempFile("playback", ".wav", context.cacheDir)
            tempFile.writeBytes(wavBytes)
            tempFile
        } catch (e: Exception) {
            Log.e(TAG, "Failed to write WAV temp file: ${e.message}")
            null
        }
    }

    /**
     * Prepare a previously written WAV temp file for playback.
     * MUST be called on the main thread (MediaPlayer needs a Looper).
     */
    fun prepareFromFile(tempFile: File) {
        stop()
        loadWavFromFile(tempFile)
    }

    /**
     * Start or resume playback.
     */
    fun play() {
        val player = mediaPlayer ?: run {
            Log.w(TAG, "No audio data loaded")
            return
        }

        // Set remaining repeats at the start of a fresh play (position near 0).
        if (_currentTimeMs.value < 1.0) {
            remainingRepeats = repeatCount
        }

        audioSessionManager.requestAudioFocus()
        applyMuteVolume(player)
        applyPlaybackSpeed(player)
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
        // With PlaybackParams, currentPosition is in media time (music time).
        _currentTimeMs.value = player.currentPosition.toDouble()
        stopChoreographer()

        // Keep cursor at the paused position
        updateCursor(_currentTimeMs.value)

        Log.d(TAG, "Paused at ${_currentTimeMs.value / 1000.0}s")
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
        wavTempFile?.delete()
        wavTempFile = null

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
        player.seekTo(clampedMs.toInt())
        _currentTimeMs.value = clampedMs

        // Update cursor immediately at the seek position
        updateCursor(clampedMs)

        Log.d(TAG, "Seeked to ${clampedMs / 1000.0}s")
    }

    /**
     * Release all resources. Call when the activity/composable is destroyed.
     */
    fun release() {
        stop()
        audioSessionManager.release()
    }

    // ── Choreographer (frame-accurate cursor updates) ───────────────────

    private fun startChoreographer() {
        stopChoreographer()
        val callback = object : Choreographer.FrameCallback {
            override fun doFrame(frameTimeNanos: Long) {
                if (_isPlaying.value) {
                    val player = mediaPlayer ?: return
                    // With PlaybackParams speed control, currentPosition is in media time
                    // (i.e. music time) — it advances through the original timeline.
                    val musicMs = player.currentPosition.toDouble()
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
     * Load a WAV temp file into a MediaPlayer.
     * Must be called on the main thread (Looper required for MediaPlayer).
     */
    private fun loadWavFromFile(tempFile: File) {
        try {
            wavTempFile = tempFile

            val player = MediaPlayer()
            player.setAudioAttributes(audioSessionManager.getAudioAttributes())
            player.setDataSource(tempFile.absolutePath)
            player.prepare()

            player.setOnCompletionListener {
                playbackDidFinish()
            }

            mediaPlayer = player
            // Duration is the full length of the WAV (music time)
            _durationMs.value = player.duration.toDouble()

            // Show cursor at the beginning
            updateCursor(0.0)

            Log.d(TAG, "Audio prepared: ${_durationMs.value / 1000.0}s, speed=$speed")
        } catch (e: Exception) {
            Log.e(TAG, "Failed to prepare audio: ${e.message}")
            mediaPlayer = null
            _durationMs.value = 0.0
        }
    }

    /** Apply playback speed via PlaybackParams (time-stretch, no pitch change). */
    private fun applyPlaybackSpeed(player: MediaPlayer) {
        try {
            val params = PlaybackParams()
                .setSpeed(speed.toFloat())
                .setPitch(1.0f) // Keep original pitch
            player.playbackParams = params
        } catch (e: Exception) {
            Log.w(TAG, "Failed to set playback speed: ${e.message}")
        }
    }

    /** Called when speed changes at runtime. */
    private fun applySpeedChange() {
        val player = mediaPlayer ?: return
        if (_isPlaying.value) {
            applyPlaybackSpeed(player)
        }
        // If paused, speed will be applied on next play()
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
            mediaPlayer?.let { applyPlaybackSpeed(it) }
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
