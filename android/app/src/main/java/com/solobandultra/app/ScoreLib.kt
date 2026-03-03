package com.solobandultra.app

import android.content.Context
import java.io.File

/**
 * JNI bridge to the Rust scorelib library for MusicXML rendering,
 * playback map generation, and MIDI generation.
 *
 * Native methods return null on failure (e.g. parse error, invalid input).
 * Callers should check for null and surface an appropriate error to the user.
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
     * On failure (e.g. missing asset), leaves cache null; audio rendering will return null.
     */
    fun loadSoundFont(context: Context) {
        if (cachedSoundFont == null) {
            synchronized(this) {
                if (cachedSoundFont == null) {
                    try {
                        cachedSoundFont = context.assets.open("GeneralUser_GS.sf2").use { it.readBytes() }
                    } catch (e: Exception) {
                        android.util.Log.e("ScoreLib", "Failed to load SoundFont: ${e.message}")
                    }
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
     * @param partsFilter Optional comma-separated 1-based part indices (e.g. "1,3,5"). Pass null for all parts.
     */
    external fun renderFile(path: String, pageWidth: Float, transpose: Int, partsFilter: String?): String?

    /**
     * Render MusicXML bytes to SVG.
     * @param pageWidth SVG width in user-units (pass 0f for the default 820).
     * @param transpose Semitones to transpose (0 = no change).
     * @param partsFilter Optional comma-separated 1-based part indices (e.g. "1,3,5"). Pass null for all parts.
     */
    external fun renderBytes(data: ByteArray, extension: String?, pageWidth: Float, transpose: Int, partsFilter: String?): String?

    /**
     * Render a MusicXML asset file to SVG.
     * @param pageWidth SVG width in user-units (pass 0f for the default 820).
     * @param transpose Semitones to transpose (0 = no change).
     * @param partsFilter Optional comma-separated 1-based part indices (e.g. "1,3,5"). Pass null for all parts.
     */
    fun renderAsset(context: Context, assetPath: String, pageWidth: Float = 0f, transpose: Int = 0, partsFilter: String? = null): String? {
        val extension = assetPath.substringAfterLast('.', "")
        val bytes = context.assets.open(assetPath).use { it.readBytes() }
        return renderBytes(bytes, extension.ifEmpty { null }, pageWidth, transpose, partsFilter)
    }

    /**
     * Render MusicXML bytes to SVG (convenience for pre-loaded data).
     * @param partsFilter Optional comma-separated 1-based part indices (e.g. "1,3,5"). Pass null for all parts.
     */
    fun renderData(data: ByteArray, ext: String, pageWidth: Float = 0f, transpose: Int = 0, partsFilter: String? = null): String? {
        return renderBytes(data, ext.ifEmpty { null }, pageWidth, transpose, partsFilter)
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
     * @param partsFilter Same as used for SVG rendering (e.g. "1,3"). Pass null for all staves.
     */
    external fun playbackMap(data: ByteArray, extension: String?, pageWidth: Float, transpose: Int, partsFilter: String?): String?

    /**
     * Generate a playback map from a MusicXML asset file.
     * @param transpose Semitones to transpose (0 = no change). Must match render transpose.
     * @param partsFilter Same as used for SVG rendering. Pass null for all staves.
     */
    fun playbackMapFromAsset(context: Context, assetPath: String, pageWidth: Float = 0f, transpose: Int = 0, partsFilter: String? = null): String? {
        val extension = assetPath.substringAfterLast('.', "")
        val bytes = context.assets.open(assetPath).use { it.readBytes() }
        return playbackMap(bytes, extension.ifEmpty { null }, pageWidth, transpose, partsFilter)
    }

    /**
     * Generate a playback map from pre-loaded MusicXML bytes.
     * @param partsFilter Same as used for SVG rendering. Pass null for all staves.
     */
    fun playbackMapFromData(data: ByteArray, ext: String, pageWidth: Float = 0f, transpose: Int = 0, partsFilter: String? = null): String? {
        return playbackMap(data, ext.ifEmpty { null }, pageWidth, transpose, partsFilter)
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
