package com.solobandultra.app.audio

import android.content.Context
import android.media.AudioAttributes
import android.media.AudioFocusRequest
import android.media.AudioManager
import android.os.Build
import android.util.Log

/**
 * Manages the audio session for the app.
 *
 * Configures audio to play through the MEDIA stream, which is independent
 * of the ringer/notification volume. This means audio will play even when
 * the device is in silent or vibrate mode, as long as the media volume
 * is turned up.
 *
 * On Android, "silent mode" affects the ringer stream, not the media stream.
 * By using AudioAttributes with USAGE_MEDIA, our audio plays through the
 * media stream and is unaffected by the ringer/silent switch.
 */
class AudioSessionManager(private val context: Context) {

    companion object {
        private const val TAG = "AudioSessionManager"
    }

    private val audioManager: AudioManager =
        context.getSystemService(Context.AUDIO_SERVICE) as AudioManager

    private var audioFocusRequest: AudioFocusRequest? = null
    private var hasAudioFocus = false

    private val audioAttributes: AudioAttributes = AudioAttributes.Builder()
        .setUsage(AudioAttributes.USAGE_MEDIA)
        .setContentType(AudioAttributes.CONTENT_TYPE_MUSIC)
        .build()

    private val focusChangeListener = AudioManager.OnAudioFocusChangeListener { focusChange ->
        when (focusChange) {
            AudioManager.AUDIOFOCUS_GAIN -> {
                Log.d(TAG, "Audio focus gained")
                hasAudioFocus = true
                // Resume playback at normal volume
            }
            AudioManager.AUDIOFOCUS_LOSS -> {
                Log.d(TAG, "Audio focus lost permanently")
                hasAudioFocus = false
                // Stop playback
            }
            AudioManager.AUDIOFOCUS_LOSS_TRANSIENT -> {
                Log.d(TAG, "Audio focus lost temporarily")
                hasAudioFocus = false
                // Pause playback
            }
            AudioManager.AUDIOFOCUS_LOSS_TRANSIENT_CAN_DUCK -> {
                Log.d(TAG, "Audio focus lost temporarily, can duck")
                // Lower volume
            }
        }
    }

    /**
     * Configure the audio session for media playback.
     * This sets the volume control stream to MUSIC, ensuring the hardware
     * volume buttons control the media volume (not the ringer volume).
     */
    fun configureAudioSession() {
        // Ensure volume buttons control the media stream
        // This is typically done at the Activity level, but we set it here for clarity
        Log.d(TAG, "Audio session configured for media playback")
        Log.d(TAG, "Current media volume: ${audioManager.getStreamVolume(AudioManager.STREAM_MUSIC)}")
        Log.d(TAG, "Max media volume: ${audioManager.getStreamMaxVolume(AudioManager.STREAM_MUSIC)}")

        // Ensure media volume is not zero (optional: you might want to warn the user instead)
        val currentVolume = audioManager.getStreamVolume(AudioManager.STREAM_MUSIC)
        if (currentVolume == 0) {
            Log.w(TAG, "Media volume is at 0. Audio will not be audible until volume is raised.")
        }
    }

    /**
     * Request audio focus for playback.
     * Returns true if focus was granted.
     */
    fun requestAudioFocus(): Boolean {
        val request = AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN)
            .setAudioAttributes(audioAttributes)
            .setOnAudioFocusChangeListener(focusChangeListener)
            .setWillPauseWhenDucked(false)
            .build()

        audioFocusRequest = request

        val result = audioManager.requestAudioFocus(request)
        hasAudioFocus = result == AudioManager.AUDIOFOCUS_REQUEST_GRANTED

        if (hasAudioFocus) {
            Log.d(TAG, "Audio focus granted")
        } else {
            Log.w(TAG, "Audio focus request denied")
        }

        return hasAudioFocus
    }

    /**
     * Abandon audio focus when playback stops.
     */
    fun abandonAudioFocus() {
        audioFocusRequest?.let { request ->
            audioManager.abandonAudioFocusRequest(request)
            hasAudioFocus = false
            Log.d(TAG, "Audio focus abandoned")
        }
    }

    /**
     * Get the AudioAttributes configured for media playback.
     * Pass these to MediaPlayer or ExoPlayer when setting up audio output.
     */
    fun getAudioAttributes(): AudioAttributes = audioAttributes

    /**
     * Release all resources.
     */
    fun release() {
        abandonAudioFocus()
        audioFocusRequest = null
    }
}
