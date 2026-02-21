# SoloBand Bundle (.mbk) Feature Specification

## Overview

This feature introduces the **SoloBand Bundle** (`.mbk`) file format — a self-contained
music book that pairs a PDF score book with its extracted MusicXML practice files and a
structured index.  It also adds three new bottom-bar buttons (**Previous**, **Next**,
**Book**) that are active only when a bundle is loaded.

---

## 1. Bundle Format (.mbk)

An `.mbk` file is a standard ZIP archive with a renamed extension.

### Internal layout

```
my-book.mbk  (ZIP)
├── book.pdf          # Full PDF of the score book
├── book.json         # Page-to-piece index (see §2)
└── music/            # Extracted MusicXML files
    ├── piece-a.musicxml
    ├── piece-b.mxl
    └── …
```

### Rules
- All three top-level entries (`book.pdf`, `book.json`, `music/`) are required.
- MusicXML filenames inside `music/` may use `.musicxml` or `.mxl` extensions.
- The `xml` field in `book.json` must match a real file inside the archive
  (path relative to the archive root, e.g. `"music/piece-a.musicxml"`).

---

## 2. book.json Schema

The index file conforms to `pdfiumlib/schema/music-book-map.schema.json`.

### Top-level fields

| Field    | Type    | Required | Description                                      |
|----------|---------|----------|--------------------------------------------------|
| bookId   | string  | ✓        | Unique identifier (UUID or slug)                 |
| version  | integer | ✓        | Schema version; currently `1`                    |
| title    | string  |          | Human-readable book title shown in the playlist  |
| pages    | array   | ✓        | Ordered list of `PageEntry` objects              |

### PageEntry

| Field  | Type    | Required | Description                              |
|--------|---------|----------|------------------------------------------|
| page   | integer | ✓        | 1-based PDF page where the piece starts  |
| pieces | array   | ✓        | One or more `PieceEntry` objects         |

> **Note:** The sample bundle uses the key `"music"` instead of `"pieces"`.
> The parser will accept both keys; `"pieces"` is canonical going forward.

### PieceEntry

| Field      | Type    | Required | Description                                                  |
|------------|---------|----------|--------------------------------------------------------------|
| xml        | string  | ✓        | Path to MusicXML file relative to bundle root                |
| title      | string  | ✓        | Display name shown in the piece picker and toolbar           |
| difficulty | string  |          | `"beginner"`, `"intermediate"`, or `"advanced"`              |
| locked     | boolean |          | If `true`, piece requires purchase/unlock; defaults `false`  |
| tags       | string[]|          | Freeform tags for future filtering (genre, instrument, etc.) |

### Example

```json
{
  "bookId": "mysoloband-2026",
  "version": 1,
  "title": "Mysoloband Practice Book",
  "pages": [
    {
      "page": 1,
      "pieces": [
        { "xml": "music/asa-branca.musicxml",  "title": "Asa Branca",  "difficulty": "intermediate" },
        { "xml": "music/blue-bag-folly.musicxml", "title": "Blue Bag Folly", "locked": true }
      ]
    },
    {
      "page": 2,
      "pieces": [
        { "xml": "music/chopin-trois-valses.mxl", "title": "Chopin Three Valses", "difficulty": "advanced" }
      ]
    }
  ]
}
```

---

## 3. Playlist Integration

### Music sources model (existing → extended)

Today the app has a single hardcoded source `"bundled"` (the app's built-in sheet music)
plus an optional `"external"` source for files opened via "Open With".  After this feature:

| Source type  | ID pattern      | Items                               |
|--------------|-----------------|-------------------------------------|
| Built-in     | `"bundled"`     | Files in the app's `sheetmusic/`    |
| MBK bundle   | `"mbk:<bookId>"`| Pieces from `book.json`, in page order |
| External file| `"external"`    | Single file opened via "Open With"  |

### Opening an .mbk file

**"Open With" path (now):**
1. User taps `.mbk` in Files app / Mail / Safari → OS hands it to the app.
2. App extracts the ZIP into the platform cache directory under
   `<cache>/mbk/<bookId>/` (overwriting any previous version of that bookId).
3. A new `MusicSource` is created with `id = "mbk:<bookId>"`, `name = book.json title`,
   and items built from the `pages` array (in page order, locked pieces included but
   visually distinguished).
4. This source is added to the playlist and selected automatically.
5. The first unlocked piece in the bundle is selected as the active file.

**Backend/purchased bundles path (future):**
- Bundles fetched from a server are stored in app-permanent storage rather than cache.
- Multiple purchased bundles appear simultaneously in the playlist source picker.
- The rest of the UX is identical.

### Piece order
Pieces are listed in the order they appear in `pages`, then in the order they appear
within each `PageEntry.pieces` array.  This matches reading order through the book.

### Locked pieces
- Shown in the piece picker with a lock icon.
- Tapping a locked piece shows a "not yet available" or "purchase to unlock" message.
- Locked pieces are skipped by the **Previous** / **Next** buttons.

---

## 4. New Bottom-Bar Buttons

Three buttons are added to the bottom toolbar alongside the existing
Stop / Play-Pause / Settings buttons.

### Layout

```
[ ◀ Prev ] [ ■ Stop ]  [ ▶ Play ] [ ▶ Next ] | [ ⚙ Settings ] [ 📖 Book ] 
```

The exact visual arrangement (order, grouping, separator) will be finalized during
implementation to fit both phone and tablet layouts.

### Button states

| Button   | Enabled when                                         | Disabled when                                   |
|----------|------------------------------------------------------|-------------------------------------------------|
| Previous | Bundle active AND current piece is not the first unlocked piece | No bundle, OR already on first piece  |
| Next     | Bundle active AND current piece is not the last unlocked piece  | No bundle, OR already on last piece   |
| Book     | Bundle active (any piece selected)                   | No bundle loaded (standalone MusicXML or built-in) |

All three buttons are **hidden** (not just greyed out) when no bundle is active, so the
toolbar does not feel cluttered for users who never use bundles.

### Previous / Next behavior
- Navigates to the immediately adjacent unlocked piece in page order.
- Loading the new piece is identical to the user manually selecting it from the playlist:
  the MusicXML is loaded, the sheet music re-renders, MIDI is regenerated.
- Does **not** wrap around (no loop from last piece back to first).
- If playback is in progress, it is stopped before switching pieces (because the piece
  itself is changing).

### Book button behavior
- Opens the **PDF Viewer** (§5) as a full-screen modal.
- The viewer opens at the PDF page corresponding to the currently active piece
  (looked up from `book.json`).
- Each subsequent tap always jumps to the page of the piece that is active at that moment.
- **Playback is not interrupted** when the PDF viewer opens or closes.  If the user is
  listening to a piece while opening the book to follow along, playback continues
  unaffected.  The user remains in full control — they can stop from the main screen
  after dismissing the viewer, or simply let it play while reading.

---

## 5. PDF Viewer

### Presentation
- Full-screen modal sheet, dismissible by swipe-down or a small close button (×) in
  the top-right corner.
- Background is white (matches typical printed score pages).

### Page rendering
- Pages are rendered by the **pdfiumlib** native library to RGBA bitmaps, then
  displayed as images inside a vertical scroll view.
- Render width = device physical pixel width (for crisp text on Retina / high-DPI
  screens).
- Pages are rendered lazily: the visible page and ±1 page ahead are rendered; distant
  pages are rendered on demand as the user scrolls.
- Previously rendered page bitmaps are cached in memory (LRU, max ~10 pages) to avoid
  re-rendering on scroll-back.

### Navigation controls
Navigation UI is intentionally minimal so it does not obscure score content:

- **Page indicator strip** — a thin, semi-transparent bar at the very bottom of the
  screen showing `Page N of M`.  Tapping it reveals a compact jump-to-page input.
- **Swipe** — vertical swipe scrolls within a page; horizontal swipe moves to the
  previous/next page (if multi-page layout is used).
- **Tap zones** — tapping the left ~20% of the screen goes to the previous page; tapping
  the right ~20% goes to the next page.  The center area is reserved for content
  interaction (future: text selection, annotation).
- No full-screen toolbar is shown to maximise score reading area.

### Jump-to-piece
- A small floating button (list icon, top-left) opens a piece-picker overlay listing
  all pieces in the bundle.  Selecting a piece jumps the PDF to that piece's start page
  and also changes the active piece in the main screen.

---

## 6. pdfiumlib Integration

### Library layout

```
pdfiumlib/
├── third_party/pdfium/
│   ├── android-arm/lib/libpdfium.so
│   ├── android-arm64/lib/libpdfium.so
│   ├── android-x64/lib/libpdfium.so
│   ├── android-x86/lib/libpdfium.so
│   ├── ios-arm64/lib/libpdfium.dylib
│   └── ios-x64/lib/libpdfium.dylib
└── wrapper/
    ├── pdfium_wrapper.h    # Public C API
    └── pdfium_wrapper.c    # Thin wrapper implementation
```

### C API summary

```c
void  pdfium_init(void);
void  pdfium_destroy(void);
void* pdfium_load_document(const char* path);   // returns opaque handle or NULL
void  pdfium_close_document(void* doc);
void* pdfium_render_page(void* doc, int page_index, int target_width); // returns PdfiumBitmap*
void  pdfium_free_bitmap(void* bitmap);
```

`pdfium_init()` is called once at app start.  A single document handle is kept open
while its bundle is the active source; it is closed when the source changes.

### iOS integration
- `libpdfium.dylib` (arm64 + x86_64) linked into the Xcode project.
- `pdfium_wrapper.h` exposed to Swift via the existing bridging header.
- Page rendering happens on a background `DispatchQueue`; the resulting `UIImage` is
  published to the SwiftUI view on the main actor.

### Android integration
- `libpdfium.so` files copied to `android/app/src/main/jniLibs/<abi>/`.
- A new `PdfiumBridge.kt` JNI wrapper class calls the C functions via `System.loadLibrary`.
- Page rendering runs on `Dispatchers.IO`; bitmaps are converted to `android.graphics.Bitmap`
  and published to Compose state on the main thread.

---

## 7. Data Flow Diagram

```
User opens .mbk
        │
        ▼
  Unzip to cache/<bookId>/
        │
        ├─── book.json ──► Parse pieces list ──► MusicSource added to playlist
        │
        ├─── music/*.xml ─► Loaded on demand when piece is selected
        │
        └─── book.pdf ───► Opened by pdfiumlib when Book button is tapped
                                │
                                ▼
                       PdfiumBitmap (RGBA)
                                │
                                ▼
                    UIImage / android.graphics.Bitmap
                                │
                                ▼
                      Full-screen PDF Viewer modal
```

---

## 8. File URL Conventions

| Source               | URL prefix         | Example                                        |
|----------------------|--------------------|------------------------------------------------|
| Built-in file        | `file://sheetmusic/` | `file://sheetmusic/asa-branca.musicxml`      |
| Externally opened    | `external://`      | `external://MyScore.musicxml`                  |
| MBK bundle piece     | `mbk://<bookId>/`  | `mbk://mysoloband-2026/music/asa-branca.musicxml` |

The `mbk://` prefix lets all existing file-loading paths distinguish bundle pieces
without a separate code branch.  The app resolves the physical path by looking up
`<cache>/mbk/<bookId>/music/asa-branca.musicxml`.

---

## 9. Persistence

| Item                         | Storage                  | Notes                                          |
|------------------------------|--------------------------|------------------------------------------------|
| Selected source ID           | UserDefaults / SharedPrefs | Restored on next launch                      |
| Selected piece URL           | UserDefaults / SharedPrefs | Restored on next launch                      |
| Extracted bundle files       | Cache directory          | OS may evict; app re-extracts on next open    |
| Purchased bundle files (future) | App Documents / permanent | Never evicted                              |

---

## 10. Implementation Phases

### Phase 1 — Bundle parsing & playlist
- [ ] Define `BookBundle`, `BookPage`, `BookPiece` model types (both platforms)
- [ ] SBF extraction logic (unzip `book.json` + `music/` to cache)
- [ ] `book.json` parser (handle both `"pieces"` and `"music"` keys)
- [ ] Extend `MusicSource` / `MusicSourceData` to carry bundle metadata
- [ ] `mbk://` URL resolution in existing file-loading paths
- [ ] Piece picker shows piece titles (from `book.json`) instead of raw filenames

### Phase 2 — Previous / Next / Book buttons
- [ ] iOS: three new toolbar buttons, visibility/enable logic
- [ ] Android: three new toolbar buttons, visibility/enable logic
- [ ] Previous / Next navigation (skip locked pieces)
- [ ] Stop playback on piece switch

### Phase 3 — PDF Viewer (iOS)
- [ ] Link `libpdfium.dylib` into Xcode project
- [ ] Swift bridging for `pdfium_wrapper.h`
- [ ] `PdfRenderer` class (load doc, render page on background queue, LRU cache)
- [ ] `PdfViewerView` SwiftUI full-screen modal
- [ ] Page indicator strip + tap-zone navigation
- [ ] Jump-to-piece overlay

### Phase 4 — PDF Viewer (Android)
- [ ] Copy `.so` files to `jniLibs`
- [ ] `PdfiumBridge.kt` JNI wrapper
- [ ] `PdfRenderer` coroutine-based renderer (IO dispatcher, LRU cache)
- [ ] `PdfViewerScreen` Compose full-screen modal
- [ ] Page indicator strip + tap-zone navigation
- [ ] Jump-to-piece overlay

### Phase 5 — Polish & edge cases
- [ ] Locked piece UX (lock icon, tap message)
- [ ] Bundle title in playlist source picker
- [ ] Error handling: corrupt ZIP, missing `book.pdf`, invalid `book.json`
- [ ] Cache eviction recovery (re-extract on missing files)
- [ ] CI: add sample `.mbk` to test assets; smoke-test extraction

---

## 11. Open Questions

1. **Locked pieces IAP** — placeholder UI is enough for now; actual purchase flow is
   out of scope for this feature.
2. **Backend bundle delivery** — API contract and authentication are out of scope;
   the `MusicSource` model is designed to accommodate server-fetched sources.
3. **PDF annotation / highlighting** — not in scope; tap-zone center reserved for future.
4. **Multi-page pieces** — the schema supports a piece starting on one page but
   spanning several.  The viewer opens at `page` (the start page); no automatic
   end-page tracking for now.
