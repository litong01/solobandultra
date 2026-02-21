package com.solobandultra.app

import android.content.Context
import java.io.File
import java.io.InputStream
import java.util.zip.ZipInputStream

/**
 * Extracts .mbk bundle files (ZIP archives) and parses their book.json index.
 */
object MbkExtractor {

    /**
     * Extract [zipStream] into [destDir], creating directories as needed.
     * Supports both Store and Deflate entries (handled transparently by [ZipInputStream]).
     */
    fun extract(zipStream: InputStream, destDir: File) {
        destDir.mkdirs()
        ZipInputStream(zipStream.buffered()).use { zis ->
            var entry = zis.nextEntry
            while (entry != null) {
                if (!entry.isDirectory) {
                    val target = File(destDir, entry.name)
                    target.parentFile?.mkdirs()
                    target.outputStream().use { out ->
                        zis.copyTo(out)
                    }
                }
                zis.closeEntry()
                entry = zis.nextEntry
            }
        }
    }

    /**
     * Extract a .mbk bundle from [zipBytes], parse its book.json, and return a [BookBundle].
     * The bundle is cached under `<cacheDir>/mbk/<bookId>/`.
     *
     * @throws Exception if extraction or parsing fails.
     */
    fun extractAndParse(zipBytes: ByteArray, cacheRoot: File): BookBundle {
        // First pass: extract to a temp directory and read book.json to get the bookId.
        val tmpDir = File(cacheRoot, "_tmp_${System.currentTimeMillis()}")
        try {
            extract(zipBytes.inputStream(), tmpDir)

            val bookJsonFile = File(tmpDir, "book.json")
            require(bookJsonFile.exists()) { "book.json not found in bundle" }

            val jsonBytes = bookJsonFile.readBytes()
            // Quick parse to get bookId before we move files
            val tempBundle = BookBundle.parse(jsonBytes, tmpDir)
            val bookId = tempBundle.bookId

            // Move to permanent cache location
            val cacheDir = File(cacheRoot, bookId)
            if (cacheDir.exists()) cacheDir.deleteRecursively()
            tmpDir.renameTo(cacheDir)

            // Re-parse from the permanent location
            return BookBundle.parse(File(cacheDir, "book.json").readBytes(), cacheDir)

        } catch (e: Exception) {
            tmpDir.deleteRecursively()
            throw e
        }
    }

    /** Convenience: get the mbk cache root directory for this app. */
    fun mbkCacheRoot(context: Context): File =
        File(context.cacheDir, "mbk")
}
