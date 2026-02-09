package com.solobandultra.app

import android.content.Context
import java.io.File

/**
 * JNI bridge to the Rust scorelib library for MusicXML rendering.
 */
object ScoreLib {

    init {
        System.loadLibrary("scorelib")
    }

    /**
     * Render a MusicXML file at the given path to SVG.
     */
    external fun renderFile(path: String): String?

    /**
     * Render MusicXML bytes to SVG.
     */
    external fun renderBytes(data: ByteArray, extension: String?): String?

    /**
     * Render a MusicXML asset file to SVG.
     * Copies the asset to a temp file and renders it.
     */
    fun renderAsset(context: Context, assetPath: String): String? {
        val extension = assetPath.substringAfterLast('.', "")
        val bytes = context.assets.open(assetPath).use { it.readBytes() }
        return renderBytes(bytes, extension.ifEmpty { null })
    }
}
