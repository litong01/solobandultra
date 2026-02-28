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

    var speed: Double = 1.0
        set(value) {
            val clamped = value.coerceIn(0.1, 5.0)
            if (field != clamped) {
                field = clamped
                resetPlayback()
            }
        }

    var isMuted: Boolean = false
        set(value) {
            if (field != value) {
                field = value
                resetPlayback()
            }
        }

    var repeatCount: Int = 1
        set(value) {
            if (field != value) {
                field = value
                resetPlayback()
            }
        }

    var showCursorEnabled: Boolean = true
        set(value) {
            if (field != value) {
                field = value
                resetPlayback()
                setCursorBarVisible(value)
            }
        }

    private fun resetPlayback() {
        val player = mediaPlayer
        if (player != null && _isPlaying.value) {
            try {
                if (player.isPlaying) {
                    player.pause()
                }
                player.seekTo(0)
            } catch (e: IllegalStateException) {
                Log.d(TAG, "MediaPlayer IllegalStateException: ${e.message}")
            }
        } else if (player != null && _currentTimeMs.value > 0.0) {
            try {
                player.seekTo(0)
            } catch (e: IllegalStateException) {
                Log.d(TAG, "MediaPlayer IllegalStateException: ${e.message}")
            }
        } else {
            return
        }
        _isPlaying.value = false
        _currentTimeMs.value = 0.0
        remainingRepeats = 0
        stopChoreographer()
        audioSessionManager.abandonAudioFocus()
        updateCursor(0.0)
    }

    // ── Internal state ──────────────────────────────────────────────────

    private var mediaPlayer: MediaPlayer? = null
    private var wavTempFile: File? = null
    var webView: WebView? = null

    private var choreographerCallback: Choreographer.FrameCallback? = null
    private var remainingRepeats: Int = 0

    // ── Public API ──────────────────────────────────────────────────────

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

    fun prepareFromFile(tempFile: File) {
        stop()
        loadWavFromFile(tempFile)
    }

    fun play() {
        if (mediaPlayer == null && wavTempFile != null) {
            loadWavFromFile(wavTempFile!!)
        }
        val player = mediaPlayer ?: run {
            Log.w(TAG, "No audio data loaded")
            return
        }

        if (!_isPlaying.value && _currentTimeMs.value == 0.0) {
            remainingRepeats = repeatCount
        }

        if (!audioSessionManager.requestAudioFocus()) {
            Log.w(TAG, "Audio focus denied")
            return
        }

        applyMuteVolume(player)
        applyPlaybackSpeed(player)
        try {
            player.start()
        } catch (e: IllegalStateException) {
            Log.e(TAG, "Failed to start playback: ${e.message}")
            audioSessionManager.abandonAudioFocus()
            return
        }
        _isPlaying.value = true
        startChoreographer()
    }

    fun pause() {
        val player = mediaPlayer ?: return
        try {
            if (player.isPlaying) {
                player.pause()
            }
        } catch (e: IllegalStateException) {
            Log.d(TAG, "MediaPlayer IllegalStateException: ${e.message}")
        }
        _isPlaying.value = false
        try {
            _currentTimeMs.value = player.currentPosition.toDouble()
        } catch (e: IllegalStateException) {
            Log.d(TAG, "MediaPlayer IllegalStateException: ${e.message}")
        }
        stopChoreographer()
        updateCursor(_currentTimeMs.value)
    }

    fun stop() {
        val player = mediaPlayer
        if (player != null) {
            try {
                if (player.isPlaying) {
                    player.stop()
                }
            } catch (e: IllegalStateException) {
                Log.d(TAG, "MediaPlayer IllegalStateException: ${e.message}")
            } finally {
                player.release()
            }
        }
        mediaPlayer = null
        _isPlaying.value = false
        _currentTimeMs.value = 0.0
        remainingRepeats = 0
        stopChoreographer()
        audioSessionManager.abandonAudioFocus()
        updateCursor(0.0)
    }

    fun togglePlayPause() {
        if (_isPlaying.value) pause() else play()
    }

    fun setPlayAndRecordMode(enabled: Boolean) {
        audioSessionManager.setPlayAndRecordMode(enabled)
    }

    fun seekTo(musicTimeMs: Double) {
        val player = mediaPlayer ?: return
        val clampedMs = musicTimeMs.coerceIn(0.0, durationMs.value)
        try {
            player.seekTo(clampedMs.toInt())
        } catch (e: IllegalStateException) {
            Log.d(TAG, "MediaPlayer IllegalStateException: ${e.message}")
            return
        }
        _currentTimeMs.value = clampedMs
        updateCursor(clampedMs)
    }

    fun release() {
        stop()
        webView = null
        audioSessionManager.release()
    }

    // ── Choreographer ───────────────────────────────────────────────────

    private fun startChoreographer() {
        stopChoreographer()
        val callback = object : Choreographer.FrameCallback {
            override fun doFrame(frameTimeNanos: Long) {
                if (_isPlaying.value) {
                    val player = mediaPlayer
                    if (player != null) {
                        try {
                            val musicMs = player.currentPosition.toDouble()
                            _currentTimeMs.value = musicMs
                            updateCursor(musicMs)
                        } catch (e: IllegalStateException) {
                            Log.d(TAG, "MediaPlayer IllegalStateException: ${e.message}")
                        }
                    }
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
        val wv = webView ?: return
        wv.post {
            wv.evaluateJavascript(
                "if (typeof moveCursor === 'function') { moveCursor($timeMs); }",
                null
            )
        }
    }

    private fun setCursorBarVisible(visible: Boolean) {
        val wv = webView ?: return
        wv.post {
            wv.evaluateJavascript(
                "if (typeof setCursorBarVisible === 'function') { setCursorBarVisible($visible); }",
                null
            )
        }
    }

    fun setCursorColor(color: String) {
        val wv = webView ?: return
        wv.post {
            wv.evaluateJavascript(
                "if (typeof setCursorColor === 'function') { setCursorColor('${color.replace("'", "\\'")}'); }",
                null
            )
        }
    }

    // ── Private helpers ─────────────────────────────────────────────────

    private fun loadWavFromFile(tempFile: File) {
        try {
            wavTempFile?.takeIf { it != tempFile }?.delete()
            wavTempFile = tempFile

            val player = MediaPlayer()
            player.setAudioAttributes(audioSessionManager.getAudioAttributes())
            player.setDataSource(tempFile.absolutePath)
            player.prepare()

            player.setOnCompletionListener {
                playbackDidFinish()
            }

            player.setOnErrorListener { _, what, extra ->
                Log.e(TAG, "MediaPlayer error: what=$what extra=$extra")
                stop()
                true
            }

            mediaPlayer = player
            _durationMs.value = player.duration.toDouble()
            updateCursor(0.0)
        } catch (e: Exception) {
            Log.e(TAG, "Failed to prepare audio: ${e.message}")
            mediaPlayer = null
            _durationMs.value = 0.0
        }
    }

    private fun applyPlaybackSpeed(player: MediaPlayer) {
        try {
            val params = PlaybackParams()
                .setSpeed(speed.toFloat())
                .setPitch(1.0f)
            player.playbackParams = params
        } catch (e: Exception) {
            Log.w(TAG, "Failed to set playback speed: ${e.message}")
        }
    }

    private fun applyMuteVolume(player: MediaPlayer) {
        if (isMuted) player.setVolume(0f, 0f) else player.setVolume(1f, 1f)
    }

    private fun playbackDidFinish() {
        remainingRepeats--
        if (remainingRepeats > 0) {
            try {
                mediaPlayer?.seekTo(0)
                _currentTimeMs.value = 0.0
                mediaPlayer?.let { applyPlaybackSpeed(it) }
                mediaPlayer?.start()
            } catch (e: IllegalStateException) {
                Log.d(TAG, "MediaPlayer IllegalStateException: ${e.message}")
                stop()
            }
            return
        }

        _isPlaying.value = false
        stopChoreographer()
        _currentTimeMs.value = 0.0
        audioSessionManager.abandonAudioFocus()
        updateCursor(0.0)
    }
}
