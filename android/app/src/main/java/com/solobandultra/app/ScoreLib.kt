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
     */
    external fun renderFile(path: String, pageWidth: Float): String?

    /**
     * Render MusicXML bytes to SVG.
     * @param pageWidth SVG width in user-units (pass 0f for the default 820).
     */
    external fun renderBytes(data: ByteArray, extension: String?, pageWidth: Float): String?

    /**
     * Render a MusicXML asset file to SVG.
     * @param pageWidth SVG width in user-units (pass 0f for the default 820).
     */
    fun renderAsset(context: Context, assetPath: String, pageWidth: Float = 0f): String? {
        val extension = assetPath.substringAfterLast('.', "")
        val bytes = context.assets.open(assetPath).use { it.readBytes() }
        return renderBytes(bytes, extension.ifEmpty { null }, pageWidth)
    }

    // ── Playback Map ────────────────────────────────────────────────────

    /**
     * Generate a playback map JSON string from MusicXML bytes.
     * Contains measure positions, system positions, and timemap.
     * @param pageWidth SVG width in user-units (pass 0f for the default 820).
     */
    external fun playbackMap(data: ByteArray, extension: String?, pageWidth: Float): String?

    /**
     * Generate a playback map from a MusicXML asset file.
     */
    fun playbackMapFromAsset(context: Context, assetPath: String, pageWidth: Float = 0f): String? {
        val extension = assetPath.substringAfterLast('.', "")
        val bytes = context.assets.open(assetPath).use { it.readBytes() }
        return playbackMap(bytes, extension.ifEmpty { null }, pageWidth)
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
}
