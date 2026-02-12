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
}
