import Foundation

// MARK: - Bundle model types

/// A single piece (MusicXML file) inside a .mbk bundle.
struct BookPiece: Identifiable, Hashable, Equatable {
    let xml: String         // path relative to bundle root, e.g. "music/asa-branca.musicxml"
    let title: String       // display name shown in piece picker
    let difficulty: String?
    let locked: Bool
    let tags: [String]

    var id: String { xml }
}

/// One page in the PDF book and its associated pieces.
struct BookPage: Equatable {
    let page: Int           // 1-based PDF page number where pieces on this page start
    let pieces: [BookPiece]
}

/// A fully-parsed .mbk bundle ready for use.
struct BookBundle: Equatable {
    let bookId: String
    let version: Int
    let title: String
    let pages: [BookPage]
    let cacheDir: URL       // <cache>/mbk/<bookId>/

    // MARK: - Derived helpers

    /// All pieces in page order (locked and unlocked).
    var allPieces: [BookPiece] {
        pages.flatMap(\.pieces)
    }

    /// All unlocked pieces in page order.
    var unlockedPieces: [BookPiece] {
        allPieces.filter { !$0.locked }
    }

    /// File URL for the cached book.pdf.
    var pdfURL: URL {
        cacheDir.appendingPathComponent("book.pdf")
    }

    /// Resolve an `mbk://<bookId>/…` URL to a local file URL.
    func resolveToLocalURL(_ mbkURL: String) -> URL? {
        let prefix = "mbk://\(bookId)/"
        guard mbkURL.hasPrefix(prefix) else { return nil }
        let rel = String(mbkURL.dropFirst(prefix.count))
        return cacheDir.appendingPathComponent(rel)
    }

    /// 1-based PDF page for the piece whose `xml` path matches.
    func pdfPage(forXml xml: String) -> Int {
        for page in pages {
            if page.pieces.contains(where: { $0.xml == xml }) {
                return page.page
            }
        }
        return 1
    }

    // MARK: - Parsing

    enum ParseError: Error {
        case invalidJSON
        case missingField(String)
    }

    /// Parse the bytes of a `book.json` file into a `BookBundle`.
    static func parse(jsonData: Data, cacheDir: URL) throws -> BookBundle {
        guard let top = try? JSONSerialization.jsonObject(with: jsonData) as? [String: Any] else {
            throw ParseError.invalidJSON
        }

        guard let bookId = top["bookId"] as? String else {
            throw ParseError.missingField("bookId")
        }
        guard let version = top["version"] as? Int else {
            throw ParseError.missingField("version")
        }
        let title = top["title"] as? String ?? bookId

        guard let rawPages = top["pages"] as? [[String: Any]] else {
            throw ParseError.missingField("pages")
        }

        let pages: [BookPage] = try rawPages.map { pageDict in
            guard let pageNum = pageDict["page"] as? Int else {
                throw ParseError.missingField("page")
            }
            // Accept both "pieces" (canonical) and "music" (legacy) keys.
            let rawPieces = (pageDict["pieces"] as? [[String: Any]])
                ?? (pageDict["music"]  as? [[String: Any]])
                ?? []

            let pieces: [BookPiece] = rawPieces.compactMap { d in
                guard let xml   = d["xml"]   as? String,
                      let title = d["title"] as? String else { return nil }
                return BookPiece(
                    xml:        xml,
                    title:      title,
                    difficulty: d["difficulty"] as? String,
                    locked:     d["locked"]     as? Bool   ?? false,
                    tags:       d["tags"]        as? [String] ?? []
                )
            }
            return BookPage(page: pageNum, pieces: pieces)
        }

        return BookBundle(
            bookId:   bookId,
            version:  version,
            title:    title,
            pages:    pages,
            cacheDir: cacheDir
        )
    }
}
