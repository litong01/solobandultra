package com.solobandultra.app

import android.content.Context
import java.io.File

/**
 * JNI bridge to the Rust scorelib library for MusicXML rendering,
 * playback map generation, and MIDI generation.
 */
object ScoreLib {

    init {
        System.loadLibrary("scorelib")
    }

    /**
     * Cached SoundFont data — loaded once from assets on first use.
     * Call [loadSoundFont] before using audio rendering functions.
     */
    @Volatile
    private var cachedSoundFont: ByteArray? = null

    /**
     * Load and cache the SoundFont from assets. Safe to call multiple times;
     * only reads from assets on the first invocation.
     */
    fun loadSoundFont(context: Context) {
        if (cachedSoundFont == null) {
            synchronized(this) {
                if (cachedSoundFont == null) {
                    cachedSoundFont = context.assets.open("GeneralUser_GS.sf2").use { it.readBytes() }
                }
            }
        }
    }

    /**
     * Get the cached SoundFont bytes (must call [loadSoundFont] first).
     */
    fun getSoundFont(): ByteArray? = cachedSoundFont

    // ── SVG Rendering ───────────────────────────────────────────────────

    /**
     * Render a MusicXML file at the given path to SVG.
     * @param pageWidth SVG width in user-units (pass 0f for the default 820).
     * @param transpose Semitones to transpose (0 = no change).
     */
    external fun renderFile(path: String, pageWidth: Float, transpose: Int): String?

    /**
     * Render MusicXML bytes to SVG.
     * @param pageWidth SVG width in user-units (pass 0f for the default 820).
     * @param transpose Semitones to transpose (0 = no change).
     */
    external fun renderBytes(data: ByteArray, extension: String?, pageWidth: Float, transpose: Int): String?

    /**
     * Render a MusicXML asset file to SVG.
     * @param pageWidth SVG width in user-units (pass 0f for the default 820).
     * @param transpose Semitones to transpose (0 = no change).
     */
    fun renderAsset(context: Context, assetPath: String, pageWidth: Float = 0f, transpose: Int = 0): String? {
        val extension = assetPath.substringAfterLast('.', "")
        val bytes = context.assets.open(assetPath).use { it.readBytes() }
        return renderBytes(bytes, extension.ifEmpty { null }, pageWidth, transpose)
    }

    /**
     * Render MusicXML bytes to SVG (convenience for pre-loaded data).
     */
    fun renderData(data: ByteArray, ext: String, pageWidth: Float = 0f, transpose: Int = 0): String? {
        return renderBytes(data, ext.ifEmpty { null }, pageWidth, transpose)
    }

    // ── Note Timeline ───────────────────────────────────────────────────

    /**
     * Generate a note timeline JSON array from MusicXML bytes.
     * Returns melody notes (voice 1, part 0) with absolute timestamps:
     *   [{ "start_ms": 0.0, "end_ms": 250.0, "midi": 60, "name": "C4" }, ...]
     * @param transpose Semitones to transpose (0 = no change). Must match render transpose.
     */
    external fun noteTimeline(data: ByteArray, extension: String?, transpose: Int): String?

    /**
     * Generate a note timeline from pre-loaded MusicXML bytes.
     */
    fun noteTimelineFromData(data: ByteArray, ext: String, transpose: Int = 0): String? =
        noteTimeline(data, ext.ifEmpty { null }, transpose)

    // ── Playback Map ────────────────────────────────────────────────────

    /**
     * Generate a playback map JSON string from MusicXML bytes.
     * Contains measure positions, system positions, and timemap.
     * @param pageWidth SVG width in user-units (pass 0f for the default 820).
     * @param transpose Semitones to transpose (0 = no change). Must match render transpose.
     */
    external fun playbackMap(data: ByteArray, extension: String?, pageWidth: Float, transpose: Int): String?

    /**
     * Generate a playback map from a MusicXML asset file.
     * @param transpose Semitones to transpose (0 = no change). Must match render transpose.
     */
    fun playbackMapFromAsset(context: Context, assetPath: String, pageWidth: Float = 0f, transpose: Int = 0): String? {
        val extension = assetPath.substringAfterLast('.', "")
        val bytes = context.assets.open(assetPath).use { it.readBytes() }
        return playbackMap(bytes, extension.ifEmpty { null }, pageWidth, transpose)
    }

    /**
     * Generate a playback map from pre-loaded MusicXML bytes.
     */
    fun playbackMapFromData(data: ByteArray, ext: String, pageWidth: Float = 0f, transpose: Int = 0): String? {
        return playbackMap(data, ext.ifEmpty { null }, pageWidth, transpose)
    }

    /**
     * Add the feedback overlay layer (colored dots) to a score SVG for the performance report.
     * @param svg The score SVG string.
     * @param overlayDotsJson JSON array of { "x", "y", "colors": string[] } in SVG coordinates.
     * @return New SVG string with overlay inserted, or null on error.
     */
    external fun addFeedbackOverlay(svg: String, overlayDotsJson: String): String?

    // ── MIDI Generation ─────────────────────────────────────────────────

    /**
     * Generate MIDI (SMF Type 1) bytes from MusicXML bytes.
     * @param optionsJson JSON string with MIDI options, or null for defaults.
     */
    external fun generateMidi(data: ByteArray, extension: String?, optionsJson: String?): ByteArray?

    /**
     * Generate MIDI bytes from a MusicXML asset file.
     */
    fun generateMidiFromAsset(
        context: Context,
        assetPath: String,
        optionsJson: String? = null
    ): ByteArray? {
        val extension = assetPath.substringAfterLast('.', "")
        val bytes = context.assets.open(assetPath).use { it.readBytes() }
        return generateMidi(bytes, extension.ifEmpty { null }, optionsJson)
    }

    /**
     * Generate MIDI bytes from pre-loaded MusicXML bytes.
     */
    fun generateMidiFromData(
        data: ByteArray,
        ext: String,
        optionsJson: String? = null
    ): ByteArray? {
        return generateMidi(data, ext.ifEmpty { null }, optionsJson)
    }

    // ── Audio Rendering (offline MIDI→WAV) ──────────────────────────────

    /**
     * Render MusicXML bytes to WAV audio using a SoundFont.
     * Returns a complete WAV file as a byte array, or null on error.
     * @param data MusicXML file bytes.
     * @param extension File extension hint (e.g. "musicxml", "mxl"), or null.
     * @param optionsJson JSON string with MIDI options, or null for defaults.
     * @param soundfontData SoundFont (.sf2) file bytes.
     */
    external fun renderAudio(
        data: ByteArray,
        extension: String?,
        optionsJson: String?,
        soundfontData: ByteArray
    ): ByteArray?

    /**
     * Render a MusicXML asset to WAV audio using the cached SoundFont.
     * Call [loadSoundFont] before using this method.
     */
    fun renderAudioFromAsset(
        context: Context,
        assetPath: String,
        optionsJson: String? = null
    ): ByteArray? {
        val sfBytes = cachedSoundFont ?: return null
        val extension = assetPath.substringAfterLast('.', "")
        val bytes = context.assets.open(assetPath).use { it.readBytes() }
        return renderAudio(bytes, extension.ifEmpty { null }, optionsJson, sfBytes)
    }

    /**
     * Render pre-loaded MusicXML bytes to WAV audio using the cached SoundFont.
     * Call [loadSoundFont] before using this method.
     */
    fun renderAudioFromData(
        data: ByteArray,
        ext: String,
        optionsJson: String? = null
    ): ByteArray? {
        val sfBytes = cachedSoundFont ?: return null
        return renderAudio(data, ext.ifEmpty { null }, optionsJson, sfBytes)
    }
}
