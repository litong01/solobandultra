package com.solobandultra.app.ui.screens

import android.content.ClipboardManager
import android.net.Uri
import android.provider.OpenableColumns
import android.webkit.JavascriptInterface
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.Toast
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.ui.res.painterResource
import com.solobandultra.app.R
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.ChevronRight
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.MusicNote
import androidx.compose.material.icons.filled.Pause
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.filled.Remove
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material.icons.filled.Lock
import androidx.compose.material.icons.automirrored.filled.MenuBook
import androidx.compose.material.icons.filled.SkipNext
import androidx.compose.material.icons.filled.SkipPrevious
import androidx.compose.material.icons.filled.Stop
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import com.solobandultra.app.ui.theme.WenKaiFontFamily
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.ui.text.input.KeyboardType
import android.Manifest
import androidx.compose.material.icons.filled.BarChart
import androidx.compose.material.icons.filled.Close
import com.solobandultra.app.BookBundle
import com.solobandultra.app.BookPiece
import com.solobandultra.app.FeedbackReport
import com.solobandultra.app.FeedbackState
import com.solobandultra.app.MbkExtractor
import com.solobandultra.app.NoteEvent
import com.solobandultra.app.ScoreLib
import com.solobandultra.app.audio.FeedbackManager
import com.solobandultra.app.audio.PlaybackManager
import com.solobandultra.app.ui.theme.SoloBandUltraTheme
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.net.HttpURLConnection
import java.net.URL

// ═══════════════════════════════════════════════════════════════════════
// MIDI Settings state
// ═══════════════════════════════════════════════════════════════════════

// ── Music Source model ───────────────────────────────────────────────

data class MusicItem(val name: String, val url: String)
data class MusicSourceData(val id: String, val name: String, val items: List<MusicItem>)

/** The default music file shown on app launch (landing page). */
const val DEFAULT_LANDING_FILE = "asa-branca.musicxml"

// ── MIDI Settings ───────────────────────────────────────────────────

enum class EnergyLevel(val key: String, val displayName: String) {
    Soft("soft", "Soft"),
    Medium("medium", "Medium"),
    Strong("strong", "Strong")
}

/** Build the JSON string expected by the Rust FFI layer. */
private fun midiOptionsToJson(
    includeMelody: Boolean,
    includePiano: Boolean,
    includeBass: Boolean,
    includeStrings: Boolean,
    includeDrums: Boolean,
    includeMetronome: Boolean,
    energy: EnergyLevel,
    transpose: Int
): String = buildString {
    append("{")
    append("\"include_melody\":$includeMelody,")
    append("\"include_piano\":$includePiano,")
    append("\"include_bass\":$includeBass,")
    append("\"include_strings\":$includeStrings,")
    append("\"include_drums\":$includeDrums,")
    append("\"include_metronome\":$includeMetronome,")
    append("\"energy\":\"${energy.key}\",")
    append("\"transpose\":$transpose")
    append("}")
}

// ═══════════════════════════════════════════════════════════════════════
// Auth-gated action types
// ═══════════════════════════════════════════════════════════════════════

/** Actions that require authentication. */
enum class PendingAuthAction {
    ShowSettings,
    OpenFile,
    PasteLink,
    /** The file URI was already stored in openFileUri by the Activity. */
    LoadExternalUri
}

// ═══════════════════════════════════════════════════════════════════════
// Main screen
// ═══════════════════════════════════════════════════════════════════════

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SheetMusicScreen(
    playbackManager: PlaybackManager? = null,
    openFileUri: Uri? = null,
    onFileUriConsumed: () -> Unit = {},
    isAuthenticated: Boolean = false,
    pendingAuthAction: PendingAuthAction? = null,
    onPendingActionConsumed: () -> Unit = {},
    onLoginRequested: (PendingAuthAction?) -> Unit = {},
    onLogoutRequested: () -> Unit = {}
) {
    val isPlaying by playbackManager?.isPlaying?.collectAsState()
        ?: remember { mutableStateOf(false) }
    var showMenu by remember { mutableStateOf(false) }
    var showSettings by remember { mutableStateOf(false) }

    // ── Feedback Manager ─────────────────────────────────────────────
    val feedbackManager = remember { FeedbackManager() }
    val feedbackState by feedbackManager.state.collectAsState()
    val feedbackReport by feedbackManager.report.collectAsState()
    var showReport by remember { mutableStateOf(false) }

    // Runtime permission launcher for RECORD_AUDIO.
    val audioPermissionLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestPermission()
    ) { granted ->
        if (granted) {
            playbackManager?.setPlayAndRecordMode(true)
            feedbackManager.startListening()
        }
    }

    val context = LocalContext.current
    val scope = rememberCoroutineScope()

    // Persist settings across full app restarts (SharedPreferences is private to this app,
    // requires no extra permissions, and is the platform standard for preference storage).
    val prefs = remember { context.getSharedPreferences("midi_settings", android.content.Context.MODE_PRIVATE) }

    // MIDI settings state — initialised from persisted prefs so they survive process death.
    // rememberSaveable still handles configuration changes (rotation) as before.
    var includeMelody by rememberSaveable { mutableStateOf(prefs.getBoolean("includeMelody", true)) }
    var includePiano by rememberSaveable { mutableStateOf(prefs.getBoolean("includePiano", false)) }
    var includeBass by rememberSaveable { mutableStateOf(prefs.getBoolean("includeBass", false)) }
    var includeStrings by rememberSaveable { mutableStateOf(prefs.getBoolean("includeStrings", false)) }
    var includeDrums by rememberSaveable { mutableStateOf(prefs.getBoolean("includeDrums", true)) }
    var includeMetronome by rememberSaveable { mutableStateOf(prefs.getBoolean("includeMetronome", false)) }
    var includeFeedback  by rememberSaveable { mutableStateOf(prefs.getBoolean("includeFeedback", false)) }
    val energy = EnergyLevel.Strong  // Hardcoded; not user-facing
    var playbackSpeed by rememberSaveable { mutableStateOf(prefs.getFloat("playbackSpeed", 1.0f).toDouble()) }
    var muteMusic by rememberSaveable { mutableStateOf(prefs.getBoolean("muteMusic", false)) }
    var repeatCount by rememberSaveable { mutableIntStateOf(prefs.getInt("repeatCount", 1)) }
    var transpose by rememberSaveable { mutableIntStateOf(prefs.getInt("transpose", 0)) }
    var showCursor by rememberSaveable { mutableStateOf(prefs.getBoolean("showCursor", true)) }

    // Music source selection — external files don't survive restart, so fall back to bundled.
    val savedSourceId = prefs.getString("selectedSourceId", "bundled") ?: "bundled"
    val savedFileUrl  = prefs.getString("selectedFileUrl", "file://sheetmusic/$DEFAULT_LANDING_FILE")
        ?: "file://sheetmusic/$DEFAULT_LANDING_FILE"
    val restoredSourceId = if (savedSourceId == "external") "bundled" else savedSourceId
    val restoredFileUrl  = if (savedSourceId == "external") "file://sheetmusic/$DEFAULT_LANDING_FILE" else savedFileUrl
    var selectedSourceId by rememberSaveable { mutableStateOf(restoredSourceId) }
    var selectedFileUrl by rememberSaveable { mutableStateOf(restoredFileUrl) }

    // External file (opened via document picker or pasted URL)
    var externalFileData by remember { mutableStateOf<ByteArray?>(null) }
    var externalFileName by rememberSaveable { mutableStateOf<String?>(null) }
    /** Monotonically increasing counter to force reload when same file is re-opened. */
    var externalFileVersion by rememberSaveable { mutableIntStateOf(0) }
    var isDownloading by remember { mutableStateOf(false) }

    // ── SBF bundle state ──
    var activeBundles by remember { mutableStateOf<Map<String, BookBundle>>(emptyMap()) }
    var showPdfViewer by remember { mutableStateOf(false) }
    var bundleErrorMessage by remember { mutableStateOf<String?>(null) }

    // ── Bundle navigation helpers ──
    val activeBundle: BookBundle? = run {
        if (!selectedSourceId.startsWith("mbk:")) return@run null
        val bookId = selectedSourceId.removePrefix("mbk:")
        activeBundles[bookId]
    }
    val unlockedPieces: List<BookPiece> = activeBundle?.unlockedPieces ?: emptyList()
    val currentPieceIndex: Int? = run {
        val bundle = activeBundle ?: return@run null
        val prefix = "mbk://${bundle.bookId}/"
        if (!selectedFileUrl.startsWith(prefix)) return@run null
        val xml = selectedFileUrl.removePrefix(prefix)
        unlockedPieces.indexOfFirst { it.xml == xml }.takeIf { it >= 0 }
    }
    val canGoPrev = (currentPieceIndex ?: 0) > 0
    val canGoNext = currentPieceIndex != null && currentPieceIndex < unlockedPieces.size - 1

    fun selectPiece(piece: BookPiece) {
        val bundle = activeBundle ?: return
        playbackManager?.stop()
        selectedFileUrl = "mbk://${bundle.bookId}/${piece.xml}"
    }

    val currentPdfPage: Int = run {
        val bundle = activeBundle ?: return@run 1
        val prefix = "mbk://${bundle.bookId}/"
        if (!selectedFileUrl.startsWith(prefix)) return@run 1
        val xml = selectedFileUrl.removePrefix(prefix)
        bundle.pdfPage(xml)
    }

    // On first composition, discover and load any .mbk files bundled in assets/sheetmusic/.
    LaunchedEffect(Unit) {
        val mbkFiles = withContext(Dispatchers.IO) {
            context.assets.list("sheetmusic")
                ?.filter { it.lowercase().endsWith(".mbk") }
                ?: emptyList()
        }
        for (filename in mbkFiles) {
            val bytes = withContext(Dispatchers.IO) {
                try { context.assets.open("sheetmusic/$filename").use { it.readBytes() } }
                catch (_: Exception) { null }
            } ?: continue
            val bundle = withContext(Dispatchers.IO) {
                try { MbkExtractor.extractAndParse(bytes, MbkExtractor.mbkCacheRoot(context)) }
                catch (_: Exception) { null }
            } ?: continue
            // Only register if not already present (e.g. opened via "Open With").
            if (!activeBundles.containsKey(bundle.bookId)) {
                activeBundles = activeBundles + (bundle.bookId to bundle)
            }
        }
    }

    // Re-load the bundle from cache after configuration change if activeBundles is empty
    LaunchedEffect(selectedSourceId) {
        if (!selectedSourceId.startsWith("mbk:") || activeBundles.isNotEmpty()) return@LaunchedEffect
        val bookId = selectedSourceId.removePrefix("mbk:")
        val cacheDir = java.io.File(MbkExtractor.mbkCacheRoot(context), bookId)
        if (!cacheDir.exists()) return@LaunchedEffect
        val jsonFile = java.io.File(cacheDir, "book.json")
        if (!jsonFile.exists()) return@LaunchedEffect
        try {
            val bundle = BookBundle.parse(jsonFile.readBytes(), cacheDir)
            activeBundles = activeBundles + (bookId to bundle)
        } catch (_: Exception) {}
    }

    /** Read a content URI on IO, validate, and set external file state. */
    fun loadFromUri(uri: Uri) {
        scope.launch {
            val bytes = withContext(Dispatchers.IO) {
                try {
                    context.contentResolver.openInputStream(uri)?.use { it.readBytes() }
                } catch (_: Exception) { null }
            } ?: return@launch

            var displayName = "unknown"
            context.contentResolver.query(uri, null, null, null, null)?.use { cursor ->
                if (cursor.moveToFirst()) {
                    val idx = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME)
                    if (idx >= 0) displayName = cursor.getString(idx) ?: displayName
                }
            }
            val ext = displayName.substringAfterLast('.', "").lowercase()

            when (ext) {
                "mbk" -> {
                    var extractionError: String? = null
                    val bundle = withContext(Dispatchers.IO) {
                        try {
                            MbkExtractor.extractAndParse(bytes, MbkExtractor.mbkCacheRoot(context))
                        } catch (e: Exception) {
                            extractionError = "Could not open \u201c$displayName\u201d: ${e.localizedMessage ?: e.message}"
                            null
                        }
                    }
                    if (bundle == null) {
                        bundleErrorMessage = extractionError ?: "Failed to open \u201c$displayName\u201d."
                        return@launch
                    }
                    activeBundles = activeBundles + (bundle.bookId to bundle)
                    selectedSourceId = "mbk:${bundle.bookId}"
                    val first = bundle.unlockedPieces.firstOrNull() ?: bundle.allPieces.firstOrNull()
                    if (first != null) selectedFileUrl = "mbk://${bundle.bookId}/${first.xml}"
                    // Persist the bundle selection so the same bundle opens on next launch
                    prefs.edit()
                        .putString("selectedSourceId", "mbk:${bundle.bookId}")
                        .putString("selectedFileUrl", if (first != null) "mbk://${bundle.bookId}/${first.xml}" else "file://sheetmusic/$DEFAULT_LANDING_FILE")
                        .apply()
                }
                "musicxml", "mxl", "xml" -> {
                    externalFileData = bytes
                    externalFileName = displayName
                    externalFileVersion++
                    selectedSourceId = "external"
                    selectedFileUrl = "external://$displayName"
                }
            }
        }
    }

    // File picker launcher for opening external MusicXML / MXL files
    val openDocumentLauncher = rememberLauncherForActivityResult(
        contract = ActivityResultContracts.OpenDocument()
    ) { uri -> uri?.let { loadFromUri(it) } }

    // Dynamically discover all .musicxml and .mxl files in the assets/sheetmusic folder
    val availableFiles = remember {
        val files = context.assets.list("sheetmusic") ?: emptyArray()
        files.filter {
                 val lower = it.lowercase()
                 lower.endsWith(".musicxml") || lower.endsWith(".mxl")
             }
             .sorted()
             .map { "sheetmusic/$it" }
    }

    // Build music sources from available files + loaded bundles
    val musicSources = remember(availableFiles, activeBundles) {
        val bundledItems = availableFiles.map { path ->
            val fileName = path.substringAfterLast('/')
            MusicItem(
                name = fileName.substringBeforeLast('.'),
                url = "file://$path"
            )
        }
        val sources = mutableListOf(
            MusicSourceData(id = "bundled", name = "Bundled Sheet Music", items = bundledItems)
        )
        for ((bookId, bundle) in activeBundles.entries.sortedBy { it.key }) {
            val items = bundle.allPieces.map { piece ->
                MusicItem(
                    name = if (piece.locked) "${piece.title} 🔒" else piece.title,
                    url  = "mbk://$bookId/${piece.xml}"
                )
            }
            sources.add(MusicSourceData(id = "mbk:$bookId", name = bundle.title, items = items))
        }
        sources.toList()
    }

    // Auto-select the first file if none is selected
    LaunchedEffect(musicSources) {
        if (selectedFileUrl.isEmpty()) {
            musicSources.firstOrNull()?.items?.firstOrNull()?.let {
                selectedFileUrl = it.url
            }
        }
    }

    // Handle incoming file from "Open With" / file association intent
    // Only process the URI if the user is authenticated (or no auth gate is needed for intent).
    LaunchedEffect(openFileUri, isAuthenticated) {
        val uri = openFileUri ?: return@LaunchedEffect
        if (isAuthenticated) {
            loadFromUri(uri)
            onFileUriConsumed()
        }
        // If not authenticated, the URI stays pending; MainActivity handles the login flow.
    }

    // Execute deferred action after successful login
    LaunchedEffect(isAuthenticated, pendingAuthAction) {
        if (!isAuthenticated || pendingAuthAction == null) return@LaunchedEffect
        when (pendingAuthAction) {
            PendingAuthAction.ShowSettings -> showSettings = true
            PendingAuthAction.OpenFile -> openDocumentLauncher.launch(arrayOf("*/*"))
            PendingAuthAction.PasteLink -> {
                if (!isDownloading) {
                    pasteFromClipboard(
                        context = context,
                        scope = scope,
                        onDownloading = { isDownloading = it },
                        onResult = { bytes, filename ->
                            externalFileData = bytes
                            externalFileName = filename
                            externalFileVersion++
                            selectedSourceId = "external"
                            selectedFileUrl = "external://$filename"
                        }
                    )
                }
            }
            PendingAuthAction.LoadExternalUri -> {
                // The URI is already in openFileUri; the LaunchedEffect above
                // will pick it up now that isAuthenticated is true.
            }
        }
        onPendingActionConsumed()
    }

    var svgContent by remember { mutableStateOf<String?>(null) }
    var playbackMapJson by remember { mutableStateOf<String?>(null) }
    var isLoading by remember { mutableStateOf(true) }
    var errorMessage by remember { mutableStateOf<String?>(null) }
    /** Monotonically increasing counter to detect stale loadScore results. */
    var loadGeneration by remember { mutableIntStateOf(0) }
    /** Counter to detect stale audio-only re-renders (optionsJson changes). */
    var audioGeneration by remember { mutableIntStateOf(0) }

    val screenWidthDp = LocalConfiguration.current.screenWidthDp.toFloat()

    // Derive the options JSON from current settings
    val optionsJson = remember(
        includeMelody, includePiano, includeBass,
        includeStrings, includeDrums, includeMetronome, energy, transpose
    ) {
        midiOptionsToJson(
            includeMelody, includePiano, includeBass,
            includeStrings, includeDrums, includeMetronome, energy, transpose
        )
    }

    fun loadScore(filePath: String, pageWidth: Float) {
        // Bump generation counters so any in-flight load or audio re-render is discarded.
        loadGeneration++
        audioGeneration++
        val thisGeneration = loadGeneration

        isLoading = true
        errorMessage = null
        svgContent = null
        playbackMapJson = null

        // Stop any previous playback immediately so the user never hears the
        // old piece while the new one is loading.
        playbackManager?.stop()

        val isExternal = selectedFileUrl.startsWith("external://")
        val isMbk     = selectedFileUrl.startsWith("mbk://")

        // Resolve mbk:// URL to file bytes on the calling thread (already main)
        val mbkBytes: ByteArray? = if (isMbk) {
            val rest   = selectedFileUrl.removePrefix("mbk://")
            val slash  = rest.indexOf('/')
            if (slash < 0) null else {
                val bookId = rest.substring(0, slash)
                activeBundles[bookId]?.resolveToLocalFile(selectedFileUrl)
                    ?.takeIf { it.exists() }?.readBytes()
            }
        } else null

        val extBytes = if (isExternal) externalFileData else null

        scope.launch {
            val currentOptionsJson = optionsJson
            val currentTranspose = transpose
            val result = withContext(Dispatchers.IO) {
                try {
                    // Ensure SoundFont is cached (no-op after first call)
                    ScoreLib.loadSoundFont(context)

                    val dataBytes  = mbkBytes ?: extBytes
                    if (dataBytes != null) {
                        val ext = filePath.substringAfterLast('.', "")
                        val svg = ScoreLib.renderData(dataBytes, ext, pageWidth, currentTranspose)
                        val pmap = ScoreLib.playbackMapFromData(dataBytes, ext, pageWidth, currentTranspose)
                        val timeline = ScoreLib.noteTimelineFromData(dataBytes, ext, currentTranspose)
                        val audio = ScoreLib.renderAudioFromData(dataBytes, ext, currentOptionsJson)
                        listOf(svg, pmap, timeline, audio)
                    } else {
                        val svg = ScoreLib.renderAsset(context, filePath, pageWidth, currentTranspose)
                        val pmap = ScoreLib.playbackMapFromAsset(context, filePath, pageWidth, currentTranspose)
                        val ext = filePath.substringAfterLast('.', "")
                        val assetBytes = context.assets.open(filePath).use { it.readBytes() }
                        val timeline = ScoreLib.noteTimelineFromData(assetBytes, ext, currentTranspose)
                        val audio = ScoreLib.renderAudioFromAsset(context, filePath, currentOptionsJson)
                        listOf(svg, pmap, timeline, audio)
                    }
                } catch (e: Exception) {
                    listOf(null, null, null, null)
                }
            }

            // Discard this result if a newer loadScore was started while we were working.
            if (thisGeneration != loadGeneration) return@launch

            isLoading = false
            val svg      = result[0] as? String
            val pmap     = result[1] as? String
            val timeline = result[2] as? String
            val audio    = result[3] as? ByteArray
            if (svg != null) {
                svgContent = svg
                playbackMapJson = pmap

                // Load note timeline into FeedbackManager.
                if (timeline != null) {
                    feedbackManager.loadTimeline(NoteEvent.parseList(timeline))
                }

                // Prepare the playback manager with the rendered WAV audio.
                // File write on IO, MediaPlayer setup on Main (needs Looper).
                if (audio != null) {
                    val tempFile = withContext(Dispatchers.IO) {
                        playbackManager?.writeTempWav(audio)
                    }
                    // Re-check generation after IO suspension — a newer loadScore
                    // may have started while we were writing the temp file.
                    if (thisGeneration != loadGeneration) return@launch
                    if (tempFile != null) {
                        playbackManager?.prepareFromFile(tempFile)
                    }
                }
            } else {
                errorMessage = "Failed to render $filePath"
            }
        }
    }

    // Re-render when screen width, selected file, transpose, or bundles change
    LaunchedEffect(screenWidthDp, selectedFileUrl, transpose, externalFileVersion, activeBundles) {
        val filePath = when {
            selectedFileUrl.startsWith("external://") -> selectedFileUrl.removePrefix("external://")
            selectedFileUrl.startsWith("mbk://")      -> selectedFileUrl.substringAfterLast('/')
            else                                       -> selectedFileUrl.removePrefix("file://")
        }
        if (filePath.isNotEmpty()) {
            loadScore(filePath, screenWidthDp)
        }
    }

    // ── Wire playback settings to PlaybackManager (no MIDI regen) ──
    LaunchedEffect(playbackSpeed, muteMusic, repeatCount, showCursor) {
        playbackManager?.speed = playbackSpeed
        playbackManager?.isMuted = muteMusic
        playbackManager?.repeatCount = repeatCount
        playbackManager?.showCursorEnabled = showCursor
    }

    // ── Feedback: start/stop listening with runtime permission ───────
    // Enable play-and-record mode on Android so mic capture works while playback is active.
    LaunchedEffect(isPlaying, includeFeedback) {
        if (!includeFeedback) {
            feedbackManager.stopListening()
            playbackManager?.setPlayAndRecordMode(false)
            return@LaunchedEffect
        }
        if (isPlaying) {
            playbackManager?.setPlayAndRecordMode(true)
            val hasPermission = context.checkSelfPermission(Manifest.permission.RECORD_AUDIO) ==
                android.content.pm.PackageManager.PERMISSION_GRANTED
            if (hasPermission) {
                feedbackManager.startListening()
            } else {
                audioPermissionLauncher.launch(Manifest.permission.RECORD_AUDIO)
            }
        } else {
            feedbackManager.stopListening()
            playbackManager?.setPlayAndRecordMode(false)
        }
    }

    // ── Feedback: drive update() at ~10 Hz while playing ─────────────
    LaunchedEffect(isPlaying, includeFeedback) {
        if (!isPlaying || !includeFeedback) return@LaunchedEffect
        while (isActive) {
            playbackManager?.currentTimeMs?.value?.let { ms ->
                feedbackManager.update(ms)
            }
            delay(100)
        }
    }

    // ── Feedback: update cursor color when state changes ─────────────
    LaunchedEffect(feedbackState, includeFeedback) {
        if (includeFeedback) {
            playbackManager?.setCursorColor(feedbackState.cursorColor)
        } else {
            playbackManager?.setCursorColor(FeedbackState.Silent.cursorColor)
        }
    }

    // Regenerate audio when settings change (no need to re-render SVG)
    LaunchedEffect(optionsJson) {
        // Skip the initial launch (already handled by the loadScore above)
        if (svgContent == null) return@LaunchedEffect

        // Bump audio generation so any in-flight audio re-render is discarded.
        audioGeneration++
        val thisAudioGen = audioGeneration

        val isExternal = selectedFileUrl.startsWith("external://")
        val isMbk      = selectedFileUrl.startsWith("mbk://")
        val filePath = when {
            isExternal -> selectedFileUrl.removePrefix("external://")
            isMbk      -> selectedFileUrl.substringAfterLast('/')
            else       -> selectedFileUrl.removePrefix("file://")
        }
        if (filePath.isEmpty()) return@LaunchedEffect

        val currentOptionsJson = optionsJson
        val dataBytes: ByteArray? = when {
            isMbk -> {
                val rest  = selectedFileUrl.removePrefix("mbk://")
                val slash = rest.indexOf('/')
                if (slash < 0) null else activeBundles[rest.substring(0, slash)]
                    ?.resolveToLocalFile(selectedFileUrl)?.takeIf { it.exists() }?.readBytes()
            }
            isExternal -> externalFileData
            else -> null
        }
        val audio = withContext(Dispatchers.IO) {
            try {
                ScoreLib.loadSoundFont(context)
                if (dataBytes != null) {
                    val ext = filePath.substringAfterLast('.', "")
                    ScoreLib.renderAudioFromData(dataBytes, ext, currentOptionsJson)
                } else {
                    ScoreLib.renderAudioFromAsset(context, filePath, currentOptionsJson)
                }
            } catch (_: Exception) {
                null
            }
        }
        // Discard if a newer audio re-render or loadScore was started.
        if (thisAudioGen != audioGeneration) return@LaunchedEffect
        if (audio != null) {
            val tempFile = withContext(Dispatchers.IO) {
                playbackManager?.writeTempWav(audio)
            }
            // Re-check after IO suspension.
            if (thisAudioGen != audioGeneration) return@LaunchedEffect
            if (tempFile != null) {
                playbackManager?.prepareFromFile(tempFile)
            }
        }
    }

    // Bottom sheet state
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = false)

    Scaffold(
        topBar = {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .statusBarsPadding()
                    .padding(horizontal = 12.dp, vertical = 6.dp),
                verticalAlignment = Alignment.CenterVertically
            ) {
                Box(
                    modifier = Modifier
                        .size(32.dp)
                        .clip(RoundedCornerShape(6.dp))
                        .background(Color(0xFF1A1040))
                ) {
                    Image(
                        painter = painterResource(id = R.drawable.ic_launcher_foreground),
                        contentDescription = "Mysoloband",
                        modifier = Modifier.fillMaxSize()
                    )
                }
                Spacer(modifier = Modifier.weight(1f))
                Box {
                    IconButton(onClick = { showMenu = !showMenu }) {
                        Icon(Icons.Default.MoreVert, contentDescription = "Menu")
                    }
                    DropdownMenu(
                        expanded = showMenu,
                        onDismissRequest = { showMenu = false }
                    ) {
                        // Check clipboard for a valid MusicXML URL each time the menu opens
                        val pasteEnabled = remember(showMenu) {
                            if (!showMenu) return@remember false
                            clipboardHasMusicXmlUrl(context)
                        }

                        // ── Gated actions ──
                        DropdownMenuItem(
                            text = { Text("Open File") },
                            onClick = {
                                showMenu = false
                                if (isAuthenticated) {
                                    openDocumentLauncher.launch(arrayOf("*/*"))
                                } else {
                                    onLoginRequested(PendingAuthAction.OpenFile)
                                }
                            }
                        )
                        DropdownMenuItem(
                            text = { Text("Paste Link") },
                            enabled = pasteEnabled,
                            onClick = {
                                showMenu = false
                                if (isAuthenticated) {
                                    if (!isDownloading) {
                                        pasteFromClipboard(
                                            context = context,
                                            scope = scope,
                                            onDownloading = { isDownloading = it },
                                            onResult = { bytes, filename ->
                                                externalFileData = bytes
                                                externalFileName = filename
                                                externalFileVersion++
                                                selectedSourceId = "external"
                                                selectedFileUrl = "external://$filename"
                                            }
                                        )
                                    }
                                } else {
                                    onLoginRequested(PendingAuthAction.PasteLink)
                                }
                            }
                        )
                        DropdownMenuItem(
                            text = { Text("Settings") },
                            onClick = {
                                showMenu = false
                                if (isAuthenticated) {
                                    showSettings = true
                                } else {
                                    onLoginRequested(PendingAuthAction.ShowSettings)
                                }
                            }
                        )

                        HorizontalDivider()

                        // ── Login / Logout ──
                        if (isAuthenticated) {
                            DropdownMenuItem(
                                text = { Text("Sign Out") },
                                onClick = {
                                    showMenu = false
                                    onLogoutRequested()
                                }
                            )
                        } else {
                            DropdownMenuItem(
                                text = { Text("Sign In") },
                                onClick = {
                                    showMenu = false
                                    onLoginRequested(null)
                                }
                            )
                        }
                    }
                }
            }
        },
        bottomBar = {
            PlaybackControlBar(
                isPlaying     = isPlaying,
                bundleActive  = activeBundle != null,
                canGoPrev     = canGoPrev,
                canGoNext     = canGoNext,
                onPrev        = { val idx = currentPieceIndex; if (idx != null && idx > 0) selectPiece(unlockedPieces[idx - 1]) },
                onPlayPause   = { playbackManager?.togglePlayPause() },
                onStop        = { playbackManager?.stop() },
                onNext        = { val idx = currentPieceIndex; if (idx != null && idx < unlockedPieces.size - 1) selectPiece(unlockedPieces[idx + 1]) },
                onSettings    = {
                    if (isAuthenticated) showSettings = true
                    else onLoginRequested(PendingAuthAction.ShowSettings)
                },
                onBook           = { showPdfViewer = true },
                feedbackEnabled  = includeFeedback,
                reportAvailable  = feedbackReport != null,
                onReport         = { showReport = true }
            )
        }
    ) { paddingValues ->
        Box(modifier = Modifier.fillMaxSize().padding(paddingValues)) {
        Column(
            modifier = Modifier
                .fillMaxSize()
        ) {
            // Score content
            Box(
                modifier = Modifier
                    .fillMaxSize()
                    .weight(1f),
                contentAlignment = Alignment.Center
            ) {
                when {
                    isLoading -> {
                        CircularProgressIndicator()
                    }
                    errorMessage != null -> {
                        Text(
                            text = errorMessage ?: "Unknown error",
                            color = MaterialTheme.colorScheme.error
                        )
                    }
                    svgContent != null -> {
                        SvgWebView(
                            svg = svgContent!!,
                            playbackMapJson = playbackMapJson,
                            playbackManager = playbackManager,
                            cursorBarVisible = showCursor
                        )
                    }
                }
            }
        }
        }

        // Download overlay
        if (isDownloading) {
            Box(
                modifier = Modifier.fillMaxSize(),
                contentAlignment = Alignment.Center
            ) {
                Surface(
                    shape = RoundedCornerShape(12.dp),
                    tonalElevation = 6.dp
                ) {
                    Column(
                        modifier = Modifier.padding(24.dp),
                        horizontalAlignment = Alignment.CenterHorizontally,
                        verticalArrangement = Arrangement.spacedBy(12.dp)
                    ) {
                        CircularProgressIndicator()
                        Text("Downloading…", style = MaterialTheme.typography.bodyMedium)
                    }
                }
            }
        }
    }

    // ── PDF Viewer (full-screen) ─────────────────────────────────────
    if (showPdfViewer && activeBundle != null) {
        val bundle = activeBundle
        androidx.compose.ui.window.Dialog(
            onDismissRequest = { showPdfViewer = false },
            properties = androidx.compose.ui.window.DialogProperties(usePlatformDefaultWidth = false)
        ) {
            Surface(modifier = Modifier.fillMaxSize()) {
                PdfViewerScreen(
                    bundle      = bundle,
                    startPage   = currentPdfPage,
                    onDismiss   = { showPdfViewer = false },
                    onPieceSelected = { piece ->
                        showPdfViewer = false
                        selectPiece(piece)
                    }
                )
            }
        }
    }

    // ── Feedback report sheet ────────────────────────────────────────
    if (showReport && feedbackReport != null) {
        FeedbackReportScreen(
            report = feedbackReport!!,
            onDismiss = { showReport = false }
        )
    }

    // ── Bundle error dialog ──────────────────────────────────────────
    bundleErrorMessage?.let { msg ->
        AlertDialog(
            onDismissRequest = { bundleErrorMessage = null },
            confirmButton = {
                TextButton(onClick = { bundleErrorMessage = null }) { Text("OK") }
            },
            title = { Text("Bundle Error") },
            text  = { Text(msg) }
        )
    }

    // ── Settings Bottom Sheet ────────────────────────────────────────
    if (showSettings) {
        ModalBottomSheet(
            onDismissRequest = { showSettings = false },
            sheetState = sheetState
        ) {
            // Scope smaller font sizes to the settings sheet only.
            val settingsTypography = MaterialTheme.typography.copy(
                headlineSmall = MaterialTheme.typography.headlineSmall.copy(fontSize = 20.sp),
                titleMedium   = MaterialTheme.typography.titleMedium.copy(fontSize = 14.sp),
                bodyMedium    = MaterialTheme.typography.bodyMedium.copy(fontSize = 12.sp),
                bodySmall     = MaterialTheme.typography.bodySmall.copy(fontSize = 11.sp),
            )
            MaterialTheme(typography = settingsTypography) {
            SettingsSheetContent(
                musicSources = musicSources,
                initialSelectedSourceId = selectedSourceId,
                initialSelectedFileUrl = selectedFileUrl,
                initialIncludeMelody = includeMelody,
                initialIncludePiano = includePiano,
                initialIncludeBass = includeBass,
                initialIncludeStrings = includeStrings,
                initialIncludeDrums = includeDrums,
                initialIncludeMetronome = includeMetronome,
                initialIncludeFeedback = includeFeedback,
                initialPlaybackSpeed = playbackSpeed,
                initialMuteMusic = muteMusic,
                initialRepeatCount = repeatCount,
                initialTranspose = transpose,
                initialShowCursor = showCursor,
                onDone = { src, file, mel, pia, bas, str, drm, met, fbk, spd, mute, rep, trans, cursor ->
                    selectedSourceId = src
                    selectedFileUrl = file
                    includeMelody = mel
                    includePiano = pia
                    includeBass = bas
                    includeStrings = str
                    includeDrums = drm
                    includeMetronome = met
                    includeFeedback = fbk
                    playbackSpeed = spd
                    muteMusic = mute
                    repeatCount = rep
                    transpose = trans
                    showCursor = cursor
                    showSettings = false
                    // Persist so settings survive full app restart
                    prefs.edit()
                        .putBoolean("includeMelody", mel)
                        .putBoolean("includePiano", pia)
                        .putBoolean("includeBass", bas)
                        .putBoolean("includeStrings", str)
                        .putBoolean("includeDrums", drm)
                        .putBoolean("includeMetronome", met)
                        .putBoolean("includeFeedback", fbk)
                        .putFloat("playbackSpeed", spd.toFloat())
                        .putBoolean("muteMusic", mute)
                        .putInt("repeatCount", rep)
                        .putInt("transpose", trans)
                        .putBoolean("showCursor", cursor)
                        // External files don't survive restart — save bundled default instead
                        .putString("selectedSourceId", if (src == "external") "bundled" else src)
                        .putString("selectedFileUrl", if (src == "external") "file://sheetmusic/$DEFAULT_LANDING_FILE" else file)
                        .apply()
                }
            )
            } // end MaterialTheme(typography = settingsTypography)
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Paste Link helper
// ═══════════════════════════════════════════════════════════════════════

/** Check if the clipboard contains an HTTP(S) URL pointing to a MusicXML file. */
private fun clipboardHasMusicXmlUrl(context: android.content.Context): Boolean {
    val clipboard = context.getSystemService(android.content.Context.CLIPBOARD_SERVICE) as ClipboardManager
    val text = clipboard.primaryClip?.getItemAt(0)?.text?.toString()?.trim() ?: return false
    val url = try { URL(text) } catch (_: Exception) { return false }
    val scheme = url.protocol?.lowercase()
    if (scheme != "http" && scheme != "https") return false
    val ext = url.path.substringAfterLast('.', "").lowercase()
    return ext == "musicxml" || ext == "mxl" || ext == "xml"
}

/** Read clipboard, validate as a MusicXML URL, download, and deliver the bytes. */
private fun pasteFromClipboard(
    context: android.content.Context,
    scope: kotlinx.coroutines.CoroutineScope,
    onDownloading: (Boolean) -> Unit,
    onResult: (ByteArray, String) -> Unit
) {
    val clipboard = context.getSystemService(android.content.Context.CLIPBOARD_SERVICE) as ClipboardManager
    val text = clipboard.primaryClip?.getItemAt(0)?.text?.toString()?.trim()

    val url = try { text?.let { URL(it) } } catch (_: Exception) { null }
    val scheme = url?.protocol?.lowercase()
    if (url == null || (scheme != "http" && scheme != "https")) return

    val filename = url.path.substringAfterLast('/')
    val ext = filename.substringAfterLast('.', "").lowercase()
    if (ext != "musicxml" && ext != "mxl" && ext != "xml") return

    onDownloading(true)
    scope.launch {
        val bytes = withContext(Dispatchers.IO) {
            try {
                val connection = url.openConnection() as HttpURLConnection
                connection.connectTimeout = 15_000
                connection.readTimeout = 15_000
                connection.requestMethod = "GET"
                if (connection.responseCode in 200..299) {
                    connection.inputStream.use { it.readBytes() }
                } else {
                    null
                }
            } catch (_: Exception) {
                null
            }
        }
        onDownloading(false)
        if (bytes != null && bytes.isNotEmpty()) {
            onResult(bytes, filename)
        } else {
            Toast.makeText(context, "Download failed.", Toast.LENGTH_SHORT).show()
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Settings sheet content
// ═══════════════════════════════════════════════════════════════════════

// ── Settings label style tokens ──────────────────────────────────────────
// Single source of truth for all option labels in the settings screen.
// The actual size is controlled by the settingsTypography MaterialTheme
// override at the call site. Change bodySmall there to restyle every label.
@Composable private fun settingsLabelStyle() =
    MaterialTheme.typography.bodySmall.copy(color = MaterialTheme.colorScheme.onSurface)
@Composable private fun settingsLabelChineseStyle() =
    MaterialTheme.typography.bodySmall.copy(fontFamily = WenKaiFontFamily, color = MaterialTheme.colorScheme.onSurface)

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun SettingsSheetContent(
    musicSources: List<MusicSourceData>,
    initialSelectedSourceId: String,
    initialSelectedFileUrl: String,
    initialIncludeMelody: Boolean,
    initialIncludePiano: Boolean,
    initialIncludeBass: Boolean,
    initialIncludeStrings: Boolean,
    initialIncludeDrums: Boolean,
    initialIncludeMetronome: Boolean,
    initialIncludeFeedback: Boolean,
    initialPlaybackSpeed: Double,
    initialMuteMusic: Boolean,
    initialRepeatCount: Int,
    initialTranspose: Int,
    initialShowCursor: Boolean,
    onDone: (String, String, Boolean, Boolean, Boolean, Boolean, Boolean, Boolean, Boolean, Double, Boolean, Int, Int, Boolean) -> Unit
) {
    // Local working copies (only applied when Apply is tapped)
    var selectedSourceId by remember { mutableStateOf(initialSelectedSourceId) }
    var selectedFileUrl by remember { mutableStateOf(initialSelectedFileUrl) }
    var includeMelody by remember { mutableStateOf(initialIncludeMelody) }
    var includePiano by remember { mutableStateOf(initialIncludePiano) }
    var includeBass by remember { mutableStateOf(initialIncludeBass) }
    var includeStrings by remember { mutableStateOf(initialIncludeStrings) }
    var includeDrums by remember { mutableStateOf(initialIncludeDrums) }
    var includeMetronome by remember { mutableStateOf(initialIncludeMetronome) }
    var includeFeedback by remember { mutableStateOf(initialIncludeFeedback) }
    var playbackSpeed by remember { mutableStateOf(initialPlaybackSpeed) }
    var muteMusic by remember { mutableStateOf(initialMuteMusic) }
    var repeatCount by remember { mutableIntStateOf(initialRepeatCount) }
    var transpose by remember { mutableIntStateOf(initialTranspose) }
    var showCursor by remember { mutableStateOf(initialShowCursor) }
    var showLockedDialog by remember { mutableStateOf(false) }

    if (showLockedDialog) {
        AlertDialog(
            onDismissRequest = { showLockedDialog = false },
            confirmButton = {
                TextButton(onClick = { showLockedDialog = false }) { Text("OK") }
            },
            title = { Text("Piece Locked") },
            text = { Text("This piece is not yet available. Purchase the full bundle to unlock it.") }
        )
    }

    Column(
        modifier = Modifier
            .fillMaxWidth()
            .verticalScroll(rememberScrollState())
            .padding(horizontal = 20.dp)
            .padding(bottom = 32.dp),
        verticalArrangement = Arrangement.spacedBy(20.dp)
    ) {
        // Title row with Apply button
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically
        ) {
            Text(
                text = "Settings",
                style = MaterialTheme.typography.headlineSmall
            )
            TextButton(onClick = {
                onDone(
                    selectedSourceId, selectedFileUrl,
                    includeMelody, includePiano, includeBass, includeStrings,
                    includeDrums, includeMetronome, includeFeedback, playbackSpeed,
                    muteMusic, repeatCount, transpose, showCursor
                )
            }) {
                Text("Apply", style = MaterialTheme.typography.bodySmall)
            }
        }

        // ── 1. Music Source ──────────────────────────────────────
        SettingsCard("Music Source") {
            // Playlist row: label on left, dropdown on right (matches iOS layout)
            var sourceExpanded by remember { mutableStateOf(false) }
            val selectedSource = musicSources.firstOrNull { it.id == selectedSourceId }

            Row(
                verticalAlignment = Alignment.CenterVertically,
                modifier = Modifier.fillMaxWidth()
            ) {
                Text(
                    "Playlist",
                    style = settingsLabelStyle(),
                    modifier = Modifier.width(72.dp)
                )
                ExposedDropdownMenuBox(
                    expanded = sourceExpanded,
                    onExpandedChange = { sourceExpanded = it },
                    modifier = Modifier.weight(1f)
                ) {
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        modifier = Modifier
                            .menuAnchor()
                            .fillMaxWidth()
                            .border(
                                1.dp,
                                MaterialTheme.colorScheme.outline,
                                RoundedCornerShape(8.dp)
                            )
                            .clip(RoundedCornerShape(8.dp))
                            .padding(horizontal = 12.dp, vertical = 8.dp)
                    ) {
                        Text(
                            selectedSource?.name ?: "",
                            style = settingsLabelStyle(),
                            modifier = Modifier.weight(1f)
                        )
                        ExposedDropdownMenuDefaults.TrailingIcon(expanded = sourceExpanded)
                    }
                    ExposedDropdownMenu(
                        expanded = sourceExpanded,
                        onDismissRequest = { sourceExpanded = false }
                    ) {
                        musicSources.forEach { source ->
                            DropdownMenuItem(
                                text = { Text(source.name, style = settingsLabelStyle()) },
                                onClick = {
                                    selectedSourceId = source.id
                                    // Auto-select the first non-locked item of the new source.
                                    val first = source.items.firstOrNull { !it.url.startsWith("mbk://") || !it.name.endsWith("🔒") }
                                        ?: source.items.firstOrNull()
                                    if (first != null) selectedFileUrl = first.url
                                    sourceExpanded = false
                                },
                                modifier = Modifier.defaultMinSize(minHeight = 36.dp),
                                contentPadding = PaddingValues(horizontal = 12.dp, vertical = 4.dp)
                            )
                        }
                    }
                }
            }

            // Music row: label on left, dropdown on right (matches iOS layout)
            if (selectedSource != null && selectedSource.items.isNotEmpty()) {
                Spacer(modifier = Modifier.height(8.dp))

                var fileExpanded by remember { mutableStateOf(false) }
                val selectedFile = selectedSource.items.firstOrNull { it.url == selectedFileUrl }

                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    modifier = Modifier.fillMaxWidth()
                ) {
                    Text(
                        "Music",
                        style = settingsLabelStyle(),
                        modifier = Modifier.width(72.dp)
                    )
                    ExposedDropdownMenuBox(
                        expanded = fileExpanded,
                        onExpandedChange = { fileExpanded = it },
                        modifier = Modifier.weight(1f)
                    ) {
                        Row(
                            verticalAlignment = Alignment.CenterVertically,
                            modifier = Modifier
                                .menuAnchor()
                                .fillMaxWidth()
                                .border(
                                    1.dp,
                                    MaterialTheme.colorScheme.outline,
                                    RoundedCornerShape(8.dp)
                                )
                                .clip(RoundedCornerShape(8.dp))
                                .padding(horizontal = 12.dp, vertical = 8.dp)
                        ) {
                            Text(
                                selectedFile?.name ?: "",
                                style = settingsLabelChineseStyle(),
                                modifier = Modifier.weight(1f)
                            )
                            ExposedDropdownMenuDefaults.TrailingIcon(expanded = fileExpanded)
                        }
                        ExposedDropdownMenu(
                            expanded = fileExpanded,
                            onDismissRequest = { fileExpanded = false }
                        ) {
                            selectedSource.items.forEach { item ->
                                DropdownMenuItem(
                                    text = { Text(item.name, style = settingsLabelChineseStyle()) },
                                    onClick = {
                                        if (item.url.startsWith("mbk://") && item.name.endsWith("🔒")) {
                                            showLockedDialog = true
                                        } else {
                                            selectedFileUrl = item.url
                                        }
                                        fileExpanded = false
                                    },
                                    modifier = Modifier.defaultMinSize(minHeight = 36.dp),
                                    contentPadding = PaddingValues(horizontal = 12.dp, vertical = 4.dp)
                                )
                            }
                        }
                    }
                }
            }
        }

        // ── 2. Accompaniment ─────────────────────────────────────
        SettingsCard("Accompaniment") {
            // Four-column checkbox grid
            Column(
                modifier = Modifier.padding(vertical = 6.dp),
                verticalArrangement = Arrangement.spacedBy(12.dp)
            ) {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(4.dp)
                ) {
                    CompactCheckbox("Melody", includeMelody, { includeMelody = it }, Modifier.weight(1f))
                    CompactCheckbox("Piano", includePiano, { includePiano = it }, Modifier.weight(1f))
                    CompactCheckbox("Bass", includeBass, { includeBass = it }, Modifier.weight(1f))
                    CompactCheckbox("Strings", includeStrings, { includeStrings = it }, Modifier.weight(1f))
                }
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(4.dp)
                ) {
                    CompactCheckbox("Drums", includeDrums, { includeDrums = it }, Modifier.weight(1f))
                    CompactCheckbox("Metronome", includeMetronome, { includeMetronome = it }, Modifier.weight(1f))
                    CompactCheckbox("Feedback", includeFeedback, { includeFeedback = it }, Modifier.weight(1f))
                    Spacer(modifier = Modifier.weight(1f))
                }
            }
        }

        // ── 3. Playback ─────────────────────────────────────────
        // Adaptive: single row on tablets (>= 600dp), two rows on phones.
        SettingsCard("Playback") {
            val isNarrow = LocalConfiguration.current.screenWidthDp < 600
            var speedText by remember(playbackSpeed) {
                mutableStateOf(
                    playbackSpeed.toBigDecimal().stripTrailingZeros().toPlainString()
                )
            }

            // ── Reusable pieces ──
            // Each control is wrapped in its own Row so it emits a single
            // child node into any parent layout, making space distribution
            // predictable regardless of sheet or screen width.

            @Composable
            fun SpeedInput() {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        text = "Speed",
                        style = settingsLabelStyle()
                    )
                    Spacer(modifier = Modifier.width(4.dp))
                    BasicTextField(
                        value = speedText,
                        onValueChange = { newText ->
                            speedText = newText
                            newText.toDoubleOrNull()?.let { playbackSpeed = it }
                        },
                        modifier = Modifier
                            .width(48.dp)
                            .border(1.dp, MaterialTheme.colorScheme.outline, RoundedCornerShape(6.dp))
                            .padding(horizontal = 6.dp, vertical = 6.dp),
                        singleLine = true,
                        textStyle = settingsLabelStyle().copy(
                            textAlign = TextAlign.Center,
                            color = MaterialTheme.colorScheme.onSurface
                        ),
                        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Decimal)
                    )
                    Text(
                        text = "×",
                        style = settingsLabelStyle()
                    )
                }
            }

            @Composable
            fun MuteCheckbox() {
                Row(
                    modifier = Modifier
                        .clip(RoundedCornerShape(4.dp))
                        .clickable { muteMusic = !muteMusic }
                        .padding(vertical = 2.dp),
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Text(text = "Mute", style = settingsLabelStyle())
                    Spacer(modifier = Modifier.width(4.dp))
                    Checkbox(checked = muteMusic, onCheckedChange = null, modifier = Modifier.size(20.dp))
                }
            }

            @Composable
            fun CursorCheckbox() {
                Row(
                    modifier = Modifier
                        .clip(RoundedCornerShape(4.dp))
                        .clickable { showCursor = !showCursor }
                        .padding(vertical = 2.dp),
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Text(text = "Cursor", style = settingsLabelStyle())
                    Spacer(modifier = Modifier.width(4.dp))
                    Checkbox(checked = showCursor, onCheckedChange = null, modifier = Modifier.size(20.dp))
                }
            }

            @Composable
            fun RepeatStepper() {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        text = "Repeat",
                        style = settingsLabelStyle()
                    )
                    Spacer(modifier = Modifier.width(4.dp))
                    FilledTonalIconButton(
                        onClick = { if (repeatCount > 1) repeatCount -= 1 },
                        enabled = repeatCount > 1,
                        modifier = Modifier.size(28.dp)
                    ) {
                        Icon(Icons.Default.Remove, "Decrease", Modifier.size(14.dp))
                    }
                    Text(
                        text = "${repeatCount}×",
                        style = MaterialTheme.typography.bodyMedium,
                        textAlign = TextAlign.Center,
                        modifier = Modifier.width(28.dp)
                    )
                    FilledTonalIconButton(
                        onClick = { repeatCount += 1 },
                        modifier = Modifier.size(28.dp)
                    ) {
                        Icon(Icons.Default.Add, "Increase", Modifier.size(14.dp))
                    }
                }
            }

            // ── Layout ──

            if (isNarrow) {
                // Phone portrait: two rows — Speed | Mute, then Cursor | Repeat
                Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        SpeedInput()
                        MuteCheckbox()
                    }
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        CursorCheckbox()
                        RepeatStepper()
                    }
                }
            } else {
                // Tablet / wide: all four in one row, evenly distributed.
                // SpaceBetween on 4 single-node children is reliable regardless
                // of the sheet's actual container width.
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    SpeedInput()
                    MuteCheckbox()
                    CursorCheckbox()
                    RepeatStepper()
                }
            }
        }

        // ── 4. Transpose ────────────────────────────────────────
        SettingsCard("Transpose") {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(vertical = 4.dp),
                verticalAlignment = Alignment.CenterVertically
            ) {
                Text(
                    text = "Semitones",
                    style = settingsLabelStyle()
                )

                Spacer(modifier = Modifier.weight(1f))

                FilledTonalIconButton(
                    onClick = { transpose -= 1 },
                    modifier = Modifier.size(36.dp)
                ) {
                    Icon(
                        imageVector = Icons.Default.Remove,
                        contentDescription = "Decrease",
                        modifier = Modifier.size(18.dp)
                    )
                }

                Text(
                    text = "$transpose",
                    style = MaterialTheme.typography.titleMedium,
                    textAlign = TextAlign.Center,
                    modifier = Modifier.width(44.dp)
                )

                FilledTonalIconButton(
                    onClick = { transpose += 1 },
                    modifier = Modifier.size(36.dp)
                ) {
                    Icon(
                        imageVector = Icons.Default.Add,
                        contentDescription = "Increase",
                        modifier = Modifier.size(18.dp)
                    )
                }
            }
        }
    }
}

// ── Settings helper composables ──────────────────────────────────────

@Composable
private fun SettingsCard(
    title: String,
    content: @Composable ColumnScope.() -> Unit
) {
    Column {
        Text(
            text = title,
            style = MaterialTheme.typography.titleMedium,
            fontWeight = FontWeight.SemiBold,
            modifier = Modifier.padding(bottom = 8.dp)
        )

        Surface(
            shape = RoundedCornerShape(12.dp),
            tonalElevation = 1.dp,
            modifier = Modifier.fillMaxWidth()
        ) {
            Column(
                modifier = Modifier.padding(14.dp),
                content = content
            )
        }
    }
}

@Composable
private fun CompactCheckbox(
    label: String,
    checked: Boolean,
    onCheckedChange: (Boolean) -> Unit,
    modifier: Modifier = Modifier
) {
    Row(
        modifier = modifier
            .clip(RoundedCornerShape(4.dp))
            .clickable { onCheckedChange(!checked) }
            .padding(vertical = 2.dp),
        verticalAlignment = Alignment.CenterVertically
    ) {
        Checkbox(
            checked = checked,
            onCheckedChange = null,
            modifier = Modifier.size(20.dp)
        )
        Spacer(modifier = Modifier.width(4.dp))
        Text(
            text = label,
            style = settingsLabelStyle(),
            maxLines = 1,
            modifier = Modifier.weight(1f)
        )
    }
}

// ═══════════════════════════════════════════════════════════════════════
// SVG WebView
// ═══════════════════════════════════════════════════════════════════════

/**
 * Holder for tracking what content is currently loaded in the WebView,
 * so we can skip redundant HTML reloads during recomposition.
 */
private data class WebViewContentTag(val svgHash: Int, val pmapHash: Int)

@Composable
private fun SvgWebView(
    svg: String,
    playbackMapJson: String?,
    playbackManager: PlaybackManager?,
    cursorBarVisible: Boolean = true
) {
    AndroidView(
        factory = { context ->
            WebView(context).apply {
                webViewClient = WebViewClient()
                settings.apply {
                    @Suppress("SetJavaScriptEnabled")
                    javaScriptEnabled = true
                    builtInZoomControls = true
                    displayZoomControls = false
                    useWideViewPort = true
                    loadWithOverviewMode = true
                    setSupportZoom(true)
                }
                setBackgroundColor(android.graphics.Color.WHITE)

                // Add JavaScript interface for receiving seek events from the cursor
                addJavascriptInterface(
                    PlaybackJsInterface(playbackManager),
                    "Android"
                )

                // Give the playback manager a reference to this WebView
                playbackManager?.webView = this
            }
        },
        update = { webView ->
            // Update the playback manager's WebView reference
            playbackManager?.webView = webView

            // Skip redundant HTML reloads — the update block fires on every
            // recomposition (e.g. play/pause state change), but we only need
            // to reload when the SVG or playback map actually changed.
            val newTag = WebViewContentTag(svg.hashCode(), (playbackMapJson ?: "").hashCode())
            val currentTag = webView.tag as? WebViewContentTag
            if (newTag == currentTag) return@AndroidView

            webView.tag = newTag
            val html = buildHtml(svg, playbackMapJson, cursorBarVisible)
            // Use Android assets as base URL so @font-face can resolve bundled font files.
            webView.loadDataWithBaseURL("file:///android_asset/", html, "text/html", "UTF-8", null)
        },
        onRelease = {
            // Clear the stale reference when the WebView leaves composition,
            // so PlaybackManager doesn't post to a destroyed WebView.
            playbackManager?.webView = null
        },
        modifier = Modifier.fillMaxSize()
    )
}

/**
 * JavaScript interface for receiving playback events from the WebView.
 */
private class PlaybackJsInterface(private val playbackManager: PlaybackManager?) {
    @JavascriptInterface
    fun seekTo(timeMs: Double) {
        // JavascriptInterface methods run on a WebView background thread.
        // MediaPlayer requires main-thread access, so post to the main looper.
        android.os.Handler(android.os.Looper.getMainLooper()).post {
            playbackManager?.seekTo(timeMs)
        }
    }
}

/**
 * Build the complete HTML document with SVG, cursor div, and playback JavaScript.
 */
private fun buildHtml(svg: String, playbackMapJson: String?, cursorBarVisible: Boolean = true): String {
    // Escape "</script>" sequences so they don't prematurely close the
    // <script> block when the JSON contains that literal string.
    val pmapJS = (playbackMapJson ?: "null").replace("</", "<\\/")
    // Strip any <script> tags from SVG to prevent XSS from external MusicXML files.
    val safeSvg = svg.replace(Regex("<script[^>]*>[\\s\\S]*?</script>", RegexOption.IGNORE_CASE), "")
    return """
        <!DOCTYPE html>
        <html>
        <head>
        <meta name="viewport" content="width=device-width, initial-scale=1.0, maximum-scale=3.0, user-scalable=yes">
        <style>
            @font-face {
                font-family: 'Lora';
                src: url('fonts/Lora-Regular.ttf') format('truetype');
                font-weight: 100 900;
                font-style: normal;
            }
            @font-face {
                font-family: 'Lora';
                src: url('fonts/Lora-Italic.ttf') format('truetype');
                font-weight: 100 900;
                font-style: italic;
            }
            @font-face {
                font-family: 'LXGW WenKai';
                src: url('fonts/LXGWWenKai-Regular.ttf') format('truetype');
                font-weight: normal;
                font-style: normal;
            }
            * { margin: 0; padding: 0; box-sizing: border-box; }
            body {
                background: white;
                display: flex;
                justify-content: center;
                padding: 8px;
            }
            #score-container {
                position: relative;
                display: inline-block;
                width: 100%;
            }
            svg {
                width: 100%;
                height: auto;
                max-width: 100%;
                display: block;
            }
            #cursor {
                position: absolute;
                top: 0;
                left: 0;
                width: 3px;
                background: rgb(234, 107, 36);
                opacity: 0.85;
                will-change: transform;
                z-index: 2;
                display: none;
                pointer-events: none;
                border-radius: 1px;
            }
        </style>
        </head>
        <body>
        <div id="score-container">
            $safeSvg
            <div id="cursor"></div>
        </div>
        <script>
        ${CURSOR_JAVASCRIPT}
        // Apply cursor bar visibility from the native setting before init
        _cursorBarVisible = $cursorBarVisible;
        // Initialize playback map and position cursor at the beginning
        var _pmapData = $pmapJS;
        if (_pmapData) { initPlayback(_pmapData); showCursor(); moveCursor(0); }
        </script>
        </body>
        </html>
    """.trimIndent()
}

// ═══════════════════════════════════════════════════════════════════════
// Playback control bar
// ═══════════════════════════════════════════════════════════════════════

@Composable
private fun PlaybackControlBar(
    isPlaying: Boolean,
    bundleActive: Boolean = false,
    canGoPrev: Boolean = false,
    canGoNext: Boolean = false,
    onPrev: () -> Unit = {},
    onPlayPause: () -> Unit,
    onStop: () -> Unit,
    onNext: () -> Unit = {},
    onSettings: () -> Unit,
    onBook: () -> Unit = {},
    feedbackEnabled: Boolean = false,
    reportAvailable: Boolean = false,
    onReport: () -> Unit = {}
) {
    Surface(
        tonalElevation = 3.dp,
        shadowElevation = 4.dp
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 16.dp, vertical = 8.dp)
                .navigationBarsPadding(),
            horizontalArrangement = Arrangement.Center,
            verticalAlignment = Alignment.CenterVertically
        ) {
            // ── Bundle: Previous ──
            if (bundleActive) {
                IconButton(onClick = onPrev, enabled = canGoPrev) {
                    Icon(
                        imageVector = Icons.Default.SkipPrevious,
                        contentDescription = "Previous",
                        modifier = Modifier.size(26.dp),
                        tint = if (canGoPrev) MaterialTheme.colorScheme.onSurface
                               else MaterialTheme.colorScheme.onSurface.copy(alpha = 0.3f)
                    )
                }
            }

            // Stop button
            IconButton(onClick = onStop) {
                Icon(
                    imageVector = Icons.Default.Stop,
                    contentDescription = "Stop",
                    modifier = Modifier.size(28.dp)
                )
            }

            Spacer(modifier = Modifier.width(24.dp))

            // Play/Pause button (larger)
            FilledIconButton(
                onClick = onPlayPause,
                modifier = Modifier.size(56.dp)
            ) {
                Icon(
                    imageVector = if (isPlaying) Icons.Default.Pause else Icons.Default.PlayArrow,
                    contentDescription = if (isPlaying) "Pause" else "Play",
                    modifier = Modifier.size(32.dp)
                )
            }

            Spacer(modifier = Modifier.width(24.dp))

            // ── Bundle: Next ──
            if (bundleActive) {
                IconButton(onClick = onNext, enabled = canGoNext) {
                    Icon(
                        imageVector = Icons.Default.SkipNext,
                        contentDescription = "Next",
                        modifier = Modifier.size(26.dp),
                        tint = if (canGoNext) MaterialTheme.colorScheme.onSurface
                               else MaterialTheme.colorScheme.onSurface.copy(alpha = 0.3f)
                    )
                }
                // Separator
                Spacer(modifier = Modifier.width(4.dp))
                VerticalDivider(modifier = Modifier.height(24.dp))
                Spacer(modifier = Modifier.width(4.dp))
            }

            // Settings button
            IconButton(onClick = onSettings) {
                Icon(
                    imageVector = Icons.Default.Settings,
                    contentDescription = "Settings",
                    modifier = Modifier.size(28.dp)
                )
            }

            // ── Bundle: Book (PDF viewer) ──
            if (bundleActive) {
                IconButton(onClick = onBook) {
                    Icon(
                        imageVector = Icons.AutoMirrored.Filled.MenuBook,
                        contentDescription = "Book",
                        modifier = Modifier.size(28.dp),
                        tint = MaterialTheme.colorScheme.primary
                    )
                }
            }

            // ── Feedback Report ──
            if (feedbackEnabled && reportAvailable) {
                IconButton(onClick = onReport) {
                    Icon(
                        imageVector = Icons.Default.BarChart,
                        contentDescription = "Performance Report",
                        modifier = Modifier.size(28.dp),
                        tint = MaterialTheme.colorScheme.primary
                    )
                }
            }
        }
    }
}

/**
 * Shared cursor JavaScript (ported from mysoloband).
 * Identical to the iOS version — kept as a string constant.
 */
private const val CURSOR_JAVASCRIPT = """
// --- Playback cursor synchronization ---
// Ported from mysoloband's VerovioRendererBase._move() and Player.play()

var _measures = [];
var _systems = [];
var _timemap = [];
var _measureByIdx = {};
var _cursorEl = null;
var _currentSystemIdx = -1;
var _isInitialized = false;
var _svgEl = null;
var _containerEl = null;
var _totalDurationMs = 0;

// --- Feedback cursor color ---
var _cursorColor = 'rgb(234,107,36)';

// Set the cursor bar color for real-time feedback.
function setCursorColor(color) {
    _cursorColor = color;
    if (_cursorEl) {
        _cursorEl.style.backgroundColor = color;
    }
}

function initPlayback(playbackMap) {
    _measures = playbackMap.measures || [];
    _systems = playbackMap.systems || [];
    _timemap = playbackMap.timemap || [];
    _cursorEl = document.getElementById('cursor');
    _svgEl = document.querySelector('svg');
    _containerEl = document.getElementById('score-container');

    _measureByIdx = {};
    for (var i = 0; i < _measures.length; i++) {
        _measureByIdx[_measures[i].measure_idx] = _measures[i];
    }

    if (_timemap.length > 0) {
        var last = _timemap[_timemap.length - 1];
        _totalDurationMs = last.timestamp_ms + last.duration_ms;
    }

    _isInitialized = true;
}

var _cursorBarVisible = true;  // whether the orange bar is drawn

function showCursor() {
    if (_cursorEl) {
        _cursorEl.style.display = 'block';
        _cursorEl.style.opacity = _cursorBarVisible ? '0.85' : '0';
    }
}

function hideCursor() {
    if (_cursorEl) _cursorEl.style.display = 'none';
    _currentSystemIdx = -1;
}

// Toggle the orange cursor bar on/off without affecting position
// tracking or auto-scroll.  When hidden, moveCursor() still runs
// (so the score scrolls with the music) but the bar is invisible.
function setCursorBarVisible(visible) {
    _cursorBarVisible = visible;
    if (_cursorEl) {
        _cursorEl.style.opacity = visible ? '0.85' : '0';
    }
}

function findTimemapEntry(timeMs) {
    if (_timemap.length === 0) return null;
    var lo = 0, hi = _timemap.length - 1;
    while (lo < hi) {
        var mid = (lo + hi + 1) >> 1;
        if (_timemap[mid].timestamp_ms <= timeMs) {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    return _timemap[lo];
}

function getScaleFactor() {
    if (!_svgEl || !_containerEl) return 1;
    var svgWidth = _svgEl.viewBox.baseVal.width;
    if (svgWidth <= 0) {
        svgWidth = parseFloat(_svgEl.getAttribute('width')) || 820;
    }
    var containerWidth = _containerEl.clientWidth;
    return containerWidth / svgWidth;
}

function moveCursor(timeMs) {
    if (!_isInitialized || !_cursorEl) return;

    if (timeMs < 0) timeMs = 0;
    if (timeMs > _totalDurationMs) timeMs = _totalDurationMs;

    var entry = findTimemapEntry(timeMs);
    if (!entry) return;

    var mPos = _measureByIdx[entry.original_index];
    if (!mPos) return;

    // Compute time ratio within the measure (0.0 – 1.0)
    var offset = timeMs - entry.timestamp_ms;
    var ratio = entry.duration_ms > 0 ? offset / entry.duration_ms : 0;
    if (ratio < 0) ratio = 0;
    if (ratio > 1) ratio = 1;

    // Piecewise-linear interpolation using per-note positions
    var cursorX_svg;
    var np = mPos.note_positions;
    if (np && np.length > 1) {
        // Find the segment that brackets the current ratio
        var lo = 0;
        for (var i = 1; i < np.length; i++) {
            if (np[i][0] <= ratio) lo = i;
            else break;
        }
        var hi = Math.min(lo + 1, np.length - 1);
        if (lo === hi) {
            cursorX_svg = np[lo][1];
        } else {
            var denom = np[hi][0] - np[lo][0];
            var segRatio = denom > 0 ? (ratio - np[lo][0]) / denom : 0;
            cursorX_svg = np[lo][1] + segRatio * (np[hi][1] - np[lo][1]);
        }
    } else {
        // Fallback: linear interpolation across the whole measure
        cursorX_svg = mPos.x + ratio * mPos.width;
    }

    var sys = _systems[mPos.system_idx];
    if (!sys) return;

    // Extend cursor 2 staff-line-spacings (20 SVG units) above and below the staff
    var EXTEND = 20;
    var scale = getScaleFactor();
    var cursorX = cursorX_svg * scale;
    var cursorY = (sys.y - EXTEND) * scale;
    var cursorHeight = (sys.height + EXTEND * 2) * scale;

    _cursorEl.style.transform = 'translate(' + cursorX + 'px, ' + cursorY + 'px)';
    _cursorEl.style.height = cursorHeight + 'px';

    if (mPos.system_idx !== _currentSystemIdx) {
        _currentSystemIdx = mPos.system_idx;
        setTimeout(function() {
            _cursorEl.scrollIntoView({ behavior: 'smooth', block: 'center' });
        }, 50);
    }
}

// --- Click-to-seek ---
document.addEventListener('DOMContentLoaded', function() {
    var container = document.getElementById('score-container');
    if (!container) return;

    container.addEventListener('click', function(e) {
        if (!_isInitialized || _measures.length === 0) return;

        var rect = container.getBoundingClientRect();
        var clickX = e.clientX - rect.left;
        var clickY = e.clientY - rect.top;

        var scale = getScaleFactor();
        var svgX = clickX / scale;
        var svgY = clickY / scale;

        var clickedSystemIdx = -1;
        for (var s = 0; s < _systems.length; s++) {
            var sys = _systems[s];
            if (svgY >= sys.y - 10 && svgY <= sys.y + sys.height + 30) {
                clickedSystemIdx = s;
                break;
            }
        }
        if (clickedSystemIdx < 0) return;

        var clickedMeasure = null;
        for (var m = 0; m < _measures.length; m++) {
            var meas = _measures[m];
            if (meas.system_idx === clickedSystemIdx &&
                svgX >= meas.x && svgX <= meas.x + meas.width) {
                clickedMeasure = meas;
                break;
            }
        }
        if (!clickedMeasure) return;

        var tmEntry = null;
        for (var t = 0; t < _timemap.length; t++) {
            if (_timemap[t].original_index === clickedMeasure.measure_idx) {
                tmEntry = _timemap[t];
                break;
            }
        }
        if (!tmEntry) return;

        var offsetRatio = clickedMeasure.width > 0
            ? (svgX - clickedMeasure.x) / clickedMeasure.width : 0;
        if (offsetRatio < 0) offsetRatio = 0;
        if (offsetRatio > 1) offsetRatio = 1;

        var seekTimeMs = tmEntry.timestamp_ms + offsetRatio * tmEntry.duration_ms;

        if (window.webkit && window.webkit.messageHandlers && window.webkit.messageHandlers.playback) {
            window.webkit.messageHandlers.playback.postMessage({
                action: 'seek',
                timeMs: seekTimeMs
            });
        }
        if (window.Android) {
            window.Android.seekTo(seekTimeMs);
        }
    });
});
"""

@Preview(showBackground = true)
@Composable
private fun SheetMusicScreenPreview() {
    SoloBandUltraTheme {
        SheetMusicScreen()
    }
}
