package com.solobandultra.app.ui.screens

import android.graphics.Bitmap
import android.graphics.pdf.PdfRenderer
import android.util.Log
import android.os.ParcelFileDescriptor
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.automirrored.filled.List
import androidx.compose.material.icons.filled.Lock
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import com.solobandultra.app.BookBundle
import com.solobandultra.app.BookPiece
import com.solobandultra.app.R
import com.solobandultra.app.ui.stringResourceForLocale
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.File

// ── Public API ────────────────────────────────────────────────────────

/**
 * Full-screen modal PDF viewer backed by Android's built-in [PdfRenderer].
 * Playback continues unaffected while this screen is shown.
 *
 * @param bundle      The active BookBundle whose book.pdf will be displayed.
 * @param startPage   1-based page to open initially.
 * @param onDismiss   Called when the user closes the viewer.
 * @param onPieceSelected Called when the user selects a piece in the jump-to-piece sheet.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun PdfViewerScreen(
    bundle: BookBundle,
    startPage: Int,
    onDismiss: () -> Unit,
    onPieceSelected: (BookPiece) -> Unit = {}
) {
    val pdfFile = bundle.pdfFile
    var showPiecePicker by remember { mutableStateOf(false) }

    // Renderer lifecycle: open once, close on disposal
    val renderer = remember(pdfFile.absolutePath) {
        runCatching { openRenderer(pdfFile) }.getOrNull()
    }
    DisposableEffect(renderer) {
        onDispose { renderer?.close() }
    }

    val pageCount = renderer?.pageCount ?: 0

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(bundle.title, maxLines = 1) },
                navigationIcon = {
                    IconButton(onClick = { showPiecePicker = true }) {
                        Icon(Icons.AutoMirrored.Filled.List, contentDescription = stringResourceForLocale(R.string.content_desc_pieces))
                    }
                },
                actions = {
                    IconButton(onClick = onDismiss) {
                        Icon(Icons.Default.Close, contentDescription = stringResourceForLocale(R.string.content_desc_close))
                    }
                }
            )
        }
    ) { padding ->
        if (renderer == null || pageCount == 0) {
            Box(
                modifier = Modifier.fillMaxSize().padding(padding),
                contentAlignment = Alignment.Center
            ) {
                Text(stringResourceForLocale(R.string.pdf_unable_to_open), style = MaterialTheme.typography.bodyMedium)
            }
        } else {
            PdfPageList(
                renderer = renderer,
                pageCount = pageCount,
                startPage = startPage,
                modifier = Modifier.fillMaxSize().padding(padding)
            )
        }
    }

    if (showPiecePicker) {
        ModalBottomSheet(onDismissRequest = { showPiecePicker = false }) {
            PiecePickerContent(bundle = bundle) { piece ->
                showPiecePicker = false
                onPieceSelected(piece)
            }
        }
    }
}

// ── Scrollable page list ──────────────────────────────────────────────

@Composable
private fun PdfPageList(
    renderer: PdfRenderer,
    pageCount: Int,
    startPage: Int,
    modifier: Modifier = Modifier
) {
    val screenWidthPx = with(LocalContext.current.resources.displayMetrics) { widthPixels }
    val listState = rememberLazyListState(initialFirstVisibleItemIndex = maxOf(0, startPage - 1))

    LazyColumn(
        state = listState,
        modifier = modifier.background(Color.Gray),
        verticalArrangement = Arrangement.spacedBy(4.dp),
        contentPadding = PaddingValues(vertical = 4.dp)
    ) {
        items(pageCount) { pageIndex ->
            PdfPageItem(
                renderer = renderer,
                pageIndex = pageIndex,
                targetWidthPx = screenWidthPx
            )
        }
    }
}

@Composable
private fun PdfPageItem(
    renderer: PdfRenderer,
    pageIndex: Int,
    targetWidthPx: Int
) {
    var bitmap by remember(pageIndex) { mutableStateOf<Bitmap?>(null) }

    LaunchedEffect(pageIndex, targetWidthPx) {
        bitmap = withContext(Dispatchers.IO) {
            renderPage(renderer, pageIndex, targetWidthPx)
        }
    }

    val bmp = bitmap
    if (bmp != null) {
        Image(
            bitmap = bmp.asImageBitmap(),
            contentDescription = "Page ${pageIndex + 1}",
            contentScale = ContentScale.FillWidth,
            modifier = Modifier
                .fillMaxWidth()
                .background(Color.White)
        )
    } else {
        val config = LocalConfiguration.current
        val aspectHint = 1.414f   // A4 aspect ratio
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .height((config.screenWidthDp * aspectHint).dp)
                .background(Color.White),
            contentAlignment = Alignment.Center
        ) {
            CircularProgressIndicator()
        }
    }
}

// ── Piece picker ──────────────────────────────────────────────────────

@Composable
private fun PiecePickerContent(
    bundle: BookBundle,
    onSelect: (BookPiece) -> Unit
) {
    // Flatten pages to a list of (header, piece?) entries for the LazyColumn
    data class Row(val header: String?, val piece: BookPiece?)
    val rows: List<Row> = bundle.pages.flatMap { bookPage ->
        listOf(Row("Page ${bookPage.page}", null)) +
            bookPage.pieces.map { Row(null, it) }
    }

    Column(modifier = Modifier.fillMaxWidth()) {
        Text(
            "Pieces",
            style = MaterialTheme.typography.titleMedium,
            modifier = Modifier.padding(horizontal = 16.dp, vertical = 12.dp)
        )
        HorizontalDivider()
        LazyColumn(
            modifier = Modifier
                .fillMaxWidth()
                .navigationBarsPadding()
        ) {
            items(rows) { row ->
                if (row.header != null) {
                    Text(
                        row.header,
                        style = MaterialTheme.typography.labelMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.padding(start = 16.dp, top = 12.dp, bottom = 4.dp)
                    )
                } else if (row.piece != null) {
                    val piece = row.piece
                    ListItem(
                        headlineContent = {
                            Text(
                                piece.title,
                                color = if (piece.locked) MaterialTheme.colorScheme.onSurfaceVariant
                                        else MaterialTheme.colorScheme.onSurface
                            )
                        },
                        supportingContent = piece.difficulty?.let { diff ->
                            { Text(diff.replaceFirstChar { it.uppercase() }) }
                        },
                        trailingContent = if (piece.locked) {
                            {
                                Icon(
                                    imageVector = Icons.Default.Lock,
                                    contentDescription = "Locked"
                                )
                            }
                        } else null,
                        modifier = Modifier.clickable(enabled = !piece.locked) { onSelect(piece) }
                    )
                }
            }
        }
    }
}

// ── Native rendering helpers ──────────────────────────────────────────

private fun openRenderer(file: File): PdfRenderer {
    val pfd = ParcelFileDescriptor.open(file, ParcelFileDescriptor.MODE_READ_ONLY)
    return PdfRenderer(pfd)
}

private fun renderPage(renderer: PdfRenderer, pageIndex: Int, targetWidthPx: Int): Bitmap? {
    return try {
        renderer.openPage(pageIndex).use { page ->
            val scale  = targetWidthPx.toFloat() / page.width
            val height = (page.height * scale).toInt()
            val bitmap = Bitmap.createBitmap(targetWidthPx, height, Bitmap.Config.ARGB_8888)
            bitmap.eraseColor(android.graphics.Color.WHITE)
            page.render(bitmap, null, null, PdfRenderer.Page.RENDER_MODE_FOR_DISPLAY)
            bitmap
        }
    } catch (e: Exception) {
        Log.w("PdfViewer", "Failed to render page $pageIndex: ${e.message}")
        null
    }
}
