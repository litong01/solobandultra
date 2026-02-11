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

    // ── Internal state ──────────────────────────────────────────────────

    private var mediaPlayer: MediaPlayer? = null
    private var midiTempFile: File? = null
    var webView: WebView? = null

    private var choreographerCallback: Choreographer.FrameCallback? = null

    // ── Public API ──────────────────────────────────────────────────────

    /**
     * Prepare MIDI data for playback (does not start playing).
     */
    fun prepareMidi(midiBytes: ByteArray) {
        stop()

        try {
            // MediaPlayer requires a file, so write MIDI data to a temp file
            val tempFile = File.createTempFile("playback", ".mid", context.cacheDir)
            tempFile.writeBytes(midiBytes)
            midiTempFile = tempFile

            val player = MediaPlayer()
            player.setAudioAttributes(audioSessionManager.getAudioAttributes())
            player.setDataSource(tempFile.absolutePath)
            player.prepare()

            player.setOnCompletionListener {
                playbackDidFinish()
            }

            mediaPlayer = player
            _durationMs.value = player.duration.toDouble()

            // Show cursor at the beginning
            updateCursor(0.0)

            Log.d(TAG, "MIDI prepared: ${player.duration / 1000.0}s")
        } catch (e: Exception) {
            Log.e(TAG, "Failed to prepare MIDI: ${e.message}")
            mediaPlayer = null
            _durationMs.value = 0.0
        }
    }

    /**
     * Start or resume playback.
     */
    fun play() {
        val player = mediaPlayer ?: run {
            Log.w(TAG, "No MIDI data loaded")
            return
        }

        audioSessionManager.requestAudioFocus()
        player.start()
        _isPlaying.value = true
        startChoreographer()

        Log.d(TAG, "Playing")
    }

    /**
     * Pause playback.
     */
    fun pause() {
        mediaPlayer?.pause()
        _isPlaying.value = false
        _currentTimeMs.value = mediaPlayer?.currentPosition?.toDouble() ?: _currentTimeMs.value
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
     * Seek to a specific time in milliseconds.
     */
    fun seekTo(timeMs: Double) {
        val player = mediaPlayer ?: return

        val clampedMs = timeMs.coerceIn(0.0, player.duration.toDouble())
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
                    val posMs = player.currentPosition.toDouble()
                    _currentTimeMs.value = posMs
                    updateCursor(posMs)
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

    // ── Private ─────────────────────────────────────────────────────────

    private fun playbackDidFinish() {
        _isPlaying.value = false
        stopChoreographer()
        _currentTimeMs.value = 0.0
        audioSessionManager.abandonAudioFocus()

        // Reset cursor to the beginning (keep it visible)
        updateCursor(0.0)

        Log.d(TAG, "Playback finished")
    }
}
