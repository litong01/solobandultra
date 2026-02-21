package com.solobandultra.app

import org.json.JSONArray
import org.json.JSONObject
import java.io.File

// ── Bundle model types ────────────────────────────────────────────────

/** A single piece (MusicXML file) inside a .mbk bundle. */
data class BookPiece(
    val xml: String,          // path relative to bundle root, e.g. "music/asa-branca.musicxml"
    val title: String,        // display name shown in piece picker
    val difficulty: String?,
    val locked: Boolean,
    val tags: List<String>
)

/** One page in the PDF book and its associated pieces. */
data class BookPage(
    val page: Int,            // 1-based PDF page number
    val pieces: List<BookPiece>
)

/** A fully-parsed .mbk bundle ready for use. */
data class BookBundle(
    val bookId: String,
    val version: Int,
    val title: String,
    val pages: List<BookPage>,
    val cacheDir: File         // <cache>/mbk/<bookId>/
) {
    /** All pieces in page order (locked and unlocked). */
    val allPieces: List<BookPiece> get() = pages.flatMap { it.pieces }

    /** All unlocked pieces in page order. */
    val unlockedPieces: List<BookPiece> get() = allPieces.filter { !it.locked }

    /** File path for the cached book.pdf. */
    val pdfFile: File get() = File(cacheDir, "book.pdf")

    /** Resolve an `mbk://<bookId>/…` URL to a local File. */
    fun resolveToLocalFile(mbkUrl: String): File? {
        val prefix = "mbk://$bookId/"
        if (!mbkUrl.startsWith(prefix)) return null
        val rel = mbkUrl.removePrefix(prefix)
        return File(cacheDir, rel)
    }

    /** 1-based PDF page for the piece whose xml path matches. */
    fun pdfPage(xml: String): Int {
        for (page in pages) {
            if (page.pieces.any { it.xml == xml }) return page.page
        }
        return 1
    }

    companion object {
        /** Parse the bytes of a `book.json` file into a [BookBundle]. */
        fun parse(jsonBytes: ByteArray, cacheDir: File): BookBundle {
            val top = JSONObject(String(jsonBytes, Charsets.UTF_8))
            val bookId  = top.getString("bookId")
            val version = top.getInt("version")
            val title   = top.optString("title", bookId)

            val rawPages = top.getJSONArray("pages")
            val pages = (0 until rawPages.length()).map { i ->
                val pageObj  = rawPages.getJSONObject(i)
                val pageNum  = pageObj.getInt("page")
                // Accept both "pieces" (canonical) and "music" (legacy)
                val rawPieces: JSONArray = pageObj.optJSONArray("pieces")
                    ?: pageObj.optJSONArray("music")
                    ?: JSONArray()

                val pieces = (0 until rawPieces.length()).mapNotNull { j ->
                    val d = rawPieces.getJSONObject(j)
                    val xml   = d.optString("xml",   "").takeIf { it.isNotEmpty() } ?: return@mapNotNull null
                    val ptitle = d.optString("title", "").takeIf { it.isNotEmpty() } ?: return@mapNotNull null
                    BookPiece(
                        xml        = xml,
                        title      = ptitle,
                        difficulty = d.optString("difficulty", "").takeIf { it.isNotEmpty() },
                        locked     = d.optBoolean("locked", false),
                        tags       = d.optJSONArray("tags")?.let { arr ->
                            (0 until arr.length()).map { arr.getString(it) }
                        } ?: emptyList()
                    )
                }
                BookPage(page = pageNum, pieces = pieces)
            }

            return BookBundle(bookId = bookId, version = version, title = title,
                              pages = pages, cacheDir = cacheDir)
        }
    }
}
