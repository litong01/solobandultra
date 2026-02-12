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
import androidx.compose.material.icons.filled.Stop
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.ui.text.input.KeyboardType
import com.solobandultra.app.ScoreLib
import com.solobandultra.app.audio.PlaybackManager
import com.solobandultra.app.ui.theme.SoloBandUltraTheme
import kotlinx.coroutines.Dispatchers
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
// Main screen
// ═══════════════════════════════════════════════════════════════════════

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SheetMusicScreen(
    playbackManager: PlaybackManager? = null,
    openFileUri: Uri? = null,
    onFileUriConsumed: () -> Unit = {}
) {
    val isPlaying by playbackManager?.isPlaying?.collectAsState()
        ?: remember { mutableStateOf(false) }
    var showMenu by remember { mutableStateOf(false) }
    var showSettings by remember { mutableStateOf(false) }

    // MIDI settings state
    var includeMelody by remember { mutableStateOf(true) }
    var includePiano by remember { mutableStateOf(false) }
    var includeBass by remember { mutableStateOf(false) }
    var includeStrings by remember { mutableStateOf(false) }
    var includeDrums by remember { mutableStateOf(false) }
    var includeMetronome by remember { mutableStateOf(true) }
    val energy = EnergyLevel.Strong  // Hardcoded; not user-facing
    var playbackSpeed by remember { mutableStateOf(1.0) }
    var muteMusic by remember { mutableStateOf(false) }
    var repeatCount by remember { mutableIntStateOf(1) }
    var transpose by remember { mutableIntStateOf(0) }

    // Music source selection
    var selectedSourceId by remember { mutableStateOf("bundled") }
    var selectedFileUrl by remember { mutableStateOf("file://sheetmusic/$DEFAULT_LANDING_FILE") }

    // External file (opened via document picker or pasted URL)
    var externalFileData by remember { mutableStateOf<ByteArray?>(null) }
    var externalFileName by remember { mutableStateOf<String?>(null) }
    /** Monotonically increasing counter to force reload when same file is re-opened. */
    var externalFileVersion by remember { mutableIntStateOf(0) }
    var isDownloading by remember { mutableStateOf(false) }

    val context = LocalContext.current
    val scope = rememberCoroutineScope()

    /** Read a content URI on IO, validate, and set external file state. */
    fun loadFromUri(uri: Uri) {
        scope.launch {
            val result = withContext(Dispatchers.IO) {
                try {
                    val bytes = context.contentResolver.openInputStream(uri)?.use { it.readBytes() }
                        ?: return@withContext null
                    var displayName = "unknown.musicxml"
                    context.contentResolver.query(uri, null, null, null, null)?.use { cursor ->
                        if (cursor.moveToFirst()) {
                            val idx = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME)
                            if (idx >= 0) displayName = cursor.getString(idx) ?: displayName
                        }
                    }
                    val ext = displayName.substringAfterLast('.', "").lowercase()
                    if (ext == "musicxml" || ext == "mxl" || ext == "xml") {
                        Pair(bytes, displayName)
                    } else null
                } catch (_: Exception) { null }
            } ?: return@launch
            externalFileData = result.first
            externalFileName = result.second
            externalFileVersion++
            selectedSourceId = "external"
            selectedFileUrl = "external://${result.second}"
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

    // Build music sources from available files
    val musicSources = remember(availableFiles) {
        val items = availableFiles.map { path ->
            val fileName = path.substringAfterLast('/')
            MusicItem(
                name = fileName.substringBeforeLast('.'),
                url = "file://$path"
            )
        }
        listOf(MusicSourceData(id = "bundled", name = "Bundled Sheet Music", items = items))
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
    LaunchedEffect(openFileUri) {
        val uri = openFileUri ?: return@LaunchedEffect
        loadFromUri(uri)
        onFileUriConsumed()
    }

    var svgContent by remember { mutableStateOf<String?>(null) }
    var playbackMapJson by remember { mutableStateOf<String?>(null) }
    var isLoading by remember { mutableStateOf(true) }
    var errorMessage by remember { mutableStateOf<String?>(null) }
    /** Monotonically increasing counter to detect stale loadScore results. */
    var loadGeneration by remember { mutableIntStateOf(0) }

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
        // Bump the generation counter so any in-flight load is discarded.
        loadGeneration++
        val thisGeneration = loadGeneration

        isLoading = true
        errorMessage = null
        svgContent = null
        playbackMapJson = null

        // Stop any previous playback immediately so the user never hears the
        // old piece while the new one is loading.
        playbackManager?.stop()

        val isExternal = selectedFileUrl.startsWith("external://")
        val extBytes = if (isExternal) externalFileData else null
        scope.launch {
            val currentOptionsJson = optionsJson
            val currentTranspose = transpose
            val result = withContext(Dispatchers.IO) {
                try {
                    if (isExternal && extBytes != null) {
                        val ext = filePath.substringAfterLast('.', "")
                        val svg = ScoreLib.renderData(extBytes, ext, pageWidth, currentTranspose)
                        val pmap = ScoreLib.playbackMapFromData(extBytes, ext, pageWidth, currentTranspose)
                        val midi = ScoreLib.generateMidiFromData(extBytes, ext, currentOptionsJson)
                        Triple(svg, pmap, midi)
                    } else {
                        val svg = ScoreLib.renderAsset(context, filePath, pageWidth, currentTranspose)
                        val pmap = ScoreLib.playbackMapFromAsset(context, filePath, pageWidth, currentTranspose)
                        val midi = ScoreLib.generateMidiFromAsset(
                            context, filePath, currentOptionsJson
                        )
                        Triple(svg, pmap, midi)
                    }
                } catch (e: Exception) {
                    Triple(null, null, null)
                }
            }

            // Discard this result if a newer loadScore was started while we were working.
            if (thisGeneration != loadGeneration) return@launch

            isLoading = false
            val (svg, pmap, midi) = result
            if (svg != null) {
                svgContent = svg
                playbackMapJson = pmap

                // Prepare the playback manager with the MIDI data
                if (midi != null) {
                    playbackManager?.prepareMidi(midi)
                }
            } else {
                errorMessage = "Failed to render $filePath"
            }
        }
    }

    // Re-render when screen width, selected file, or transpose changes
    LaunchedEffect(screenWidthDp, selectedFileUrl, transpose, externalFileVersion) {
        val filePath = if (selectedFileUrl.startsWith("external://")) {
            selectedFileUrl.removePrefix("external://")
        } else {
            selectedFileUrl.removePrefix("file://")
        }
        if (filePath.isNotEmpty()) {
            loadScore(filePath, screenWidthDp)
        }
    }

    // ── Wire playback settings to PlaybackManager (no MIDI regen) ──
    LaunchedEffect(playbackSpeed, muteMusic, repeatCount) {
        playbackManager?.speed = playbackSpeed
        playbackManager?.isMuted = muteMusic
        playbackManager?.repeatCount = repeatCount
    }

    // Regenerate MIDI when settings change (no need to re-render SVG)
    LaunchedEffect(optionsJson) {
        // Skip the initial launch (already handled by the loadScore above)
        if (svgContent == null) return@LaunchedEffect

        val isExternal = selectedFileUrl.startsWith("external://")
        val filePath = if (isExternal) {
            selectedFileUrl.removePrefix("external://")
        } else {
            selectedFileUrl.removePrefix("file://")
        }
        if (filePath.isEmpty()) return@LaunchedEffect

        val currentOptionsJson = optionsJson
        val extBytes = if (isExternal) externalFileData else null
        val midi = withContext(Dispatchers.IO) {
            try {
                if (isExternal && extBytes != null) {
                    val ext = filePath.substringAfterLast('.', "")
                    ScoreLib.generateMidiFromData(extBytes, ext, currentOptionsJson)
                } else {
                    ScoreLib.generateMidiFromAsset(context, filePath, currentOptionsJson)
                }
            } catch (_: Exception) {
                null
            }
        }
        if (midi != null) {
            playbackManager?.prepareMidi(midi)
        }
    }

    // Bottom sheet state
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = false)

    Scaffold(
        topBar = {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 12.dp, vertical = 6.dp),
                verticalAlignment = Alignment.CenterVertically
            ) {
                Image(
                    painter = painterResource(id = R.mipmap.ic_launcher),
                    contentDescription = "Mysoloband",
                    modifier = Modifier
                        .size(32.dp)
                        .clip(RoundedCornerShape(6.dp))
                )
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

                        DropdownMenuItem(
                            text = { Text("Open File") },
                            onClick = {
                                showMenu = false
                                openDocumentLauncher.launch(arrayOf("*/*"))
                            }
                        )
                        DropdownMenuItem(
                            text = { Text("Paste Link") },
                            enabled = pasteEnabled,
                            onClick = {
                                showMenu = false
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
                        )
                        DropdownMenuItem(
                            text = { Text("Settings") },
                            onClick = {
                                showMenu = false
                                showSettings = true
                            }
                        )
                    }
                }
            }
        },
        bottomBar = {
            PlaybackControlBar(
                isPlaying = isPlaying,
                onPlayPause = {
                    playbackManager?.togglePlayPause()
                },
                onStop = {
                    playbackManager?.stop()
                },
                onSettings = {
                    showSettings = true
                }
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
                            playbackManager = playbackManager
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

    // ── Settings Bottom Sheet ────────────────────────────────────────
    if (showSettings) {
        ModalBottomSheet(
            onDismissRequest = { showSettings = false },
            sheetState = sheetState
        ) {
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
                initialPlaybackSpeed = playbackSpeed,
                initialMuteMusic = muteMusic,
                initialRepeatCount = repeatCount,
                initialTranspose = transpose,
                onDone = { src, file, mel, pia, bas, str, drm, met, spd, mute, rep, trans ->
                    selectedSourceId = src
                    selectedFileUrl = file
                    includeMelody = mel
                    includePiano = pia
                    includeBass = bas
                    includeStrings = str
                    includeDrums = drm
                    includeMetronome = met
                    playbackSpeed = spd
                    muteMusic = mute
                    repeatCount = rep
                    transpose = trans
                    showSettings = false
                }
            )
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
    initialPlaybackSpeed: Double,
    initialMuteMusic: Boolean,
    initialRepeatCount: Int,
    initialTranspose: Int,
    onDone: (String, String, Boolean, Boolean, Boolean, Boolean, Boolean, Boolean, Double, Boolean, Int, Int) -> Unit
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
    var playbackSpeed by remember { mutableStateOf(initialPlaybackSpeed) }
    var muteMusic by remember { mutableStateOf(initialMuteMusic) }
    var repeatCount by remember { mutableIntStateOf(initialRepeatCount) }
    var transpose by remember { mutableIntStateOf(initialTranspose) }

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
                    includeDrums, includeMetronome, playbackSpeed,
                    muteMusic, repeatCount, transpose
                )
            }) {
                Text("Apply", style = MaterialTheme.typography.bodySmall)
            }
        }

        // ── 1. Music Source ──────────────────────────────────────
        SettingsCard("Music Source") {
            // Source dropdown
            var sourceExpanded by remember { mutableStateOf(false) }
            val selectedSource = musicSources.firstOrNull { it.id == selectedSourceId }

            ExposedDropdownMenuBox(
                expanded = sourceExpanded,
                onExpandedChange = { sourceExpanded = it }
            ) {
                OutlinedTextField(
                    value = selectedSource?.name ?: "",
                    onValueChange = {},
                    readOnly = true,
                    label = { Text("Playlist") },
                    trailingIcon = { ExposedDropdownMenuDefaults.TrailingIcon(expanded = sourceExpanded) },
                    modifier = Modifier
                        .menuAnchor()
                        .fillMaxWidth(),
                    textStyle = MaterialTheme.typography.bodyMedium
                )
                ExposedDropdownMenu(
                    expanded = sourceExpanded,
                    onDismissRequest = { sourceExpanded = false }
                ) {
                    musicSources.forEach { source ->
                        DropdownMenuItem(
                            text = { Text(source.name) },
                            onClick = {
                                selectedSourceId = source.id
                                sourceExpanded = false
                            }
                        )
                    }
                }
            }

            // File picker (shown when a source with items is selected)
            if (selectedSource != null && selectedSource.items.isNotEmpty()) {
                Spacer(modifier = Modifier.height(4.dp))

                var fileExpanded by remember { mutableStateOf(false) }
                val selectedFile = selectedSource.items.firstOrNull { it.url == selectedFileUrl }

                ExposedDropdownMenuBox(
                    expanded = fileExpanded,
                    onExpandedChange = { fileExpanded = it }
                ) {
                    OutlinedTextField(
                        value = selectedFile?.name ?: "",
                        onValueChange = {},
                        readOnly = true,
                        label = { Text("Music") },
                        trailingIcon = { ExposedDropdownMenuDefaults.TrailingIcon(expanded = fileExpanded) },
                        modifier = Modifier
                            .menuAnchor()
                            .fillMaxWidth(),
                        textStyle = MaterialTheme.typography.bodyMedium
                    )
                    ExposedDropdownMenu(
                        expanded = fileExpanded,
                        onDismissRequest = { fileExpanded = false }
                    ) {
                        selectedSource.items.forEach { item ->
                            DropdownMenuItem(
                                text = { Text(item.name) },
                                onClick = {
                                    selectedFileUrl = item.url
                                    fileExpanded = false
                                }
                            )
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
                    Spacer(modifier = Modifier.weight(1f))
                    Spacer(modifier = Modifier.weight(1f))
                }
            }
        }

        // ── 3. Playback ─────────────────────────────────────────
        SettingsCard("Playback") {
            var speedText by remember(playbackSpeed) {
                mutableStateOf(
                    playbackSpeed.toBigDecimal().stripTrailingZeros().toPlainString()
                )
            }

            Row(
                modifier = Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically
            ) {
                // Speed input
                Text(
                    text = "Speed",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant
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
                    textStyle = MaterialTheme.typography.bodySmall.copy(
                        textAlign = TextAlign.Center,
                        color = MaterialTheme.colorScheme.onSurface
                    ),
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Decimal)
                )
                Text(
                    text = "×",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant
                )

                Spacer(modifier = Modifier.weight(1f))

                // Mute checkbox (text before checkbox)
                Row(
                    modifier = Modifier
                        .clip(RoundedCornerShape(4.dp))
                        .clickable { muteMusic = !muteMusic }
                        .padding(vertical = 2.dp),
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Text(
                        text = "Mute",
                        style = MaterialTheme.typography.bodySmall
                    )
                    Spacer(modifier = Modifier.width(4.dp))
                    Checkbox(
                        checked = muteMusic,
                        onCheckedChange = null,
                        modifier = Modifier.size(20.dp)
                    )
                }

                Spacer(modifier = Modifier.weight(1f))

                // Repeat stepper
                Text(
                    text = "Repeat",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant
                )
                Spacer(modifier = Modifier.width(4.dp))
                FilledTonalIconButton(
                    onClick = { if (repeatCount > 1) repeatCount -= 1 },
                    enabled = repeatCount > 1,
                    modifier = Modifier.size(28.dp)
                ) {
                    Icon(
                        imageVector = Icons.Default.Remove,
                        contentDescription = "Decrease",
                        modifier = Modifier.size(14.dp)
                    )
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
                    Icon(
                        imageVector = Icons.Default.Add,
                        contentDescription = "Increase",
                        modifier = Modifier.size(14.dp)
                    )
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
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant
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
            style = MaterialTheme.typography.bodySmall,
            maxLines = 1,
            modifier = Modifier.weight(1f)
        )
    }
}

// ═══════════════════════════════════════════════════════════════════════
// SVG WebView
// ═══════════════════════════════════════════════════════════════════════

@Composable
private fun SvgWebView(
    svg: String,
    playbackMapJson: String?,
    playbackManager: PlaybackManager?
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

            val html = buildHtml(svg, playbackMapJson)
            webView.loadDataWithBaseURL(null, html, "text/html", "UTF-8", null)
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
        playbackManager?.seekTo(timeMs)
    }
}

/**
 * Build the complete HTML document with SVG, cursor div, and playback JavaScript.
 */
private fun buildHtml(svg: String, playbackMapJson: String?): String {
    val pmapJS = playbackMapJson ?: "null"
    return """
        <!DOCTYPE html>
        <html>
        <head>
        <meta name="viewport" content="width=device-width, initial-scale=1.0, maximum-scale=3.0, user-scalable=yes">
        <style>
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
            $svg
            <div id="cursor"></div>
        </div>
        <script>
        ${CURSOR_JAVASCRIPT}
        // Initialize playback map and show cursor at the beginning
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
    onPlayPause: () -> Unit,
    onStop: () -> Unit,
    onSettings: () -> Unit
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

            // Settings button
            IconButton(onClick = onSettings) {
                Icon(
                    imageVector = Icons.Default.Settings,
                    contentDescription = "Settings",
                    modifier = Modifier.size(28.dp)
                )
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

function showCursor() {
    if (_cursorEl) _cursorEl.style.display = 'block';
}

function hideCursor() {
    if (_cursorEl) _cursorEl.style.display = 'none';
    _currentSystemIdx = -1;
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
            var segRatio = (ratio - np[lo][0]) / (np[hi][0] - np[lo][0]);
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

        var offsetRatio = (svgX - clickedMeasure.x) / clickedMeasure.width;
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
