package com.solobandultra.app.ui.screens

import android.webkit.JavascriptInterface
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
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

// ═══════════════════════════════════════════════════════════════════════
// MIDI Settings state
// ═══════════════════════════════════════════════════════════════════════

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
    energy: EnergyLevel
): String = buildString {
    append("{")
    append("\"include_melody\":$includeMelody,")
    append("\"include_piano\":$includePiano,")
    append("\"include_bass\":$includeBass,")
    append("\"include_strings\":$includeStrings,")
    append("\"include_drums\":$includeDrums,")
    append("\"include_metronome\":$includeMetronome,")
    append("\"energy\":\"${energy.key}\"")
    append("}")
}

// ═══════════════════════════════════════════════════════════════════════
// Main screen
// ═══════════════════════════════════════════════════════════════════════

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SheetMusicScreen(
    playbackManager: PlaybackManager? = null
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
    var energy by remember { mutableStateOf(EnergyLevel.Medium) }
    var playbackSpeed by remember { mutableStateOf(1.0) }
    var muteMusic by remember { mutableStateOf(false) }
    var repeatCount by remember { mutableIntStateOf(1) }
    var transpose by remember { mutableIntStateOf(0) }

    val context = LocalContext.current

    // Dynamically discover all .musicxml and .mxl files in the assets/sheetmusic folder
    val availableFiles = remember {
        val files = context.assets.list("sheetmusic") ?: emptyArray()
        files.filter { it.endsWith(".musicxml") || it.endsWith(".mxl") }
             .sorted()
             .map { "sheetmusic/$it" }
    }
    var selectedIndex by remember { mutableIntStateOf(0) }
    var svgContent by remember { mutableStateOf<String?>(null) }
    var playbackMapJson by remember { mutableStateOf<String?>(null) }
    var isLoading by remember { mutableStateOf(true) }
    var errorMessage by remember { mutableStateOf<String?>(null) }

    val scope = rememberCoroutineScope()
    val screenWidthDp = LocalConfiguration.current.screenWidthDp.toFloat()

    // Derive the options JSON from current settings
    val optionsJson = remember(
        includeMelody, includePiano, includeBass,
        includeStrings, includeDrums, includeMetronome, energy
    ) {
        midiOptionsToJson(
            includeMelody, includePiano, includeBass,
            includeStrings, includeDrums, includeMetronome, energy
        )
    }

    fun loadScore(fileIndex: Int, pageWidth: Float) {
        isLoading = true
        errorMessage = null
        svgContent = null
        playbackMapJson = null
        scope.launch {
            val currentOptionsJson = optionsJson
            val result = withContext(Dispatchers.IO) {
                try {
                    val svg = ScoreLib.renderAsset(context, availableFiles[fileIndex], pageWidth)
                    val pmap = ScoreLib.playbackMapFromAsset(context, availableFiles[fileIndex], pageWidth)
                    val midi = ScoreLib.generateMidiFromAsset(
                        context, availableFiles[fileIndex], currentOptionsJson
                    )
                    Triple(svg, pmap, midi)
                } catch (e: Exception) {
                    Triple(null, null, null)
                }
            }
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
                errorMessage = "Failed to render ${availableFiles[fileIndex]}"
            }
        }
    }

    // Re-render when screen width changes (e.g. device rotation)
    LaunchedEffect(screenWidthDp, selectedIndex) {
        loadScore(selectedIndex, screenWidthDp)
    }

    // Regenerate MIDI when settings change (no need to re-render SVG)
    LaunchedEffect(optionsJson) {
        // Skip the initial launch (already handled by the loadScore above)
        if (svgContent == null) return@LaunchedEffect

        val fileIndex = selectedIndex
        if (fileIndex < 0 || fileIndex >= availableFiles.size) return@LaunchedEffect

        val currentOptionsJson = optionsJson
        val midi = withContext(Dispatchers.IO) {
            try {
                ScoreLib.generateMidiFromAsset(
                    context, availableFiles[fileIndex], currentOptionsJson
                )
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
            TopAppBar(
                title = { Text("SoloBand Ultra") },
                actions = {
                    IconButton(onClick = { showMenu = !showMenu }) {
                        Icon(Icons.Default.MoreVert, contentDescription = "Menu")
                    }
                    DropdownMenu(
                        expanded = showMenu,
                        onDismissRequest = { showMenu = false }
                    ) {
                        DropdownMenuItem(
                            text = { Text("Open File") },
                            onClick = { showMenu = false }
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
            )
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
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(paddingValues)
        ) {
            // Score file selector
            TabRow(selectedTabIndex = selectedIndex) {
                availableFiles.forEachIndexed { index, file ->
                    Tab(
                        selected = selectedIndex == index,
                        onClick = {
                            selectedIndex = index
                        },
                        text = {
                            Text(file.substringAfterLast('/'))
                        }
                    )
                }
            }

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

    // ── Settings Bottom Sheet ────────────────────────────────────────
    if (showSettings) {
        ModalBottomSheet(
            onDismissRequest = { showSettings = false },
            sheetState = sheetState
        ) {
            SettingsSheetContent(
                includeMelody = includeMelody,
                onMelodyChange = { includeMelody = it },
                includePiano = includePiano,
                onPianoChange = { includePiano = it },
                includeBass = includeBass,
                onBassChange = { includeBass = it },
                includeStrings = includeStrings,
                onStringsChange = { includeStrings = it },
                includeDrums = includeDrums,
                onDrumsChange = { includeDrums = it },
                includeMetronome = includeMetronome,
                onMetronomeChange = { includeMetronome = it },
                energy = energy,
                onEnergyChange = { energy = it },
                playbackSpeed = playbackSpeed,
                onSpeedChange = { playbackSpeed = it },
                muteMusic = muteMusic,
                onMuteMusicChange = { muteMusic = it },
                repeatCount = repeatCount,
                onRepeatCountChange = { repeatCount = it },
                transpose = transpose,
                onTransposeChange = { transpose = it }
            )
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Settings sheet content
// ═══════════════════════════════════════════════════════════════════════

@Composable
private fun SettingsSheetContent(
    includeMelody: Boolean,
    onMelodyChange: (Boolean) -> Unit,
    includePiano: Boolean,
    onPianoChange: (Boolean) -> Unit,
    includeBass: Boolean,
    onBassChange: (Boolean) -> Unit,
    includeStrings: Boolean,
    onStringsChange: (Boolean) -> Unit,
    includeDrums: Boolean,
    onDrumsChange: (Boolean) -> Unit,
    includeMetronome: Boolean,
    onMetronomeChange: (Boolean) -> Unit,
    energy: EnergyLevel,
    onEnergyChange: (EnergyLevel) -> Unit,
    playbackSpeed: Double,
    onSpeedChange: (Double) -> Unit,
    muteMusic: Boolean,
    onMuteMusicChange: (Boolean) -> Unit,
    repeatCount: Int,
    onRepeatCountChange: (Int) -> Unit,
    transpose: Int,
    onTransposeChange: (Int) -> Unit
) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .verticalScroll(rememberScrollState())
            .padding(horizontal = 20.dp)
            .padding(bottom = 32.dp),
        verticalArrangement = Arrangement.spacedBy(20.dp)
    ) {
        Text(
            text = "Settings",
            style = MaterialTheme.typography.headlineSmall
        )

        // ── 1. Music Source ──────────────────────────────────────
        SettingsCard("Music Source") {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(vertical = 8.dp),
                verticalAlignment = Alignment.CenterVertically
            ) {
                Icon(
                    imageVector = Icons.Default.MusicNote,
                    contentDescription = null,
                    tint = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.size(20.dp)
                )
                Spacer(modifier = Modifier.width(8.dp))
                Text(
                    text = "Bundled sheet music",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.weight(1f)
                )
                Icon(
                    imageVector = Icons.Default.ChevronRight,
                    contentDescription = null,
                    tint = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.5f),
                    modifier = Modifier.size(18.dp)
                )
            }
        }

        // ── 2. Accompaniment ─────────────────────────────────────
        SettingsCard("Accompaniment") {
            // Four-column checkbox grid
            Column(verticalArrangement = Arrangement.spacedBy(0.dp)) {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(4.dp)
                ) {
                    CompactCheckbox("Melody", includeMelody, onMelodyChange, Modifier.weight(1f))
                    CompactCheckbox("Piano", includePiano, onPianoChange, Modifier.weight(1f))
                    CompactCheckbox("Bass", includeBass, onBassChange, Modifier.weight(1f))
                    CompactCheckbox("Strings", includeStrings, onStringsChange, Modifier.weight(1f))
                }
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(4.dp)
                ) {
                    CompactCheckbox("Drums", includeDrums, onDrumsChange, Modifier.weight(1f))
                    CompactCheckbox("Metronome", includeMetronome, onMetronomeChange, Modifier.weight(1f))
                    Spacer(modifier = Modifier.weight(1f))
                    Spacer(modifier = Modifier.weight(1f))
                }
            }

            Spacer(modifier = Modifier.height(12.dp))

            // Energy picker
            Text(
                text = "Energy",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant
            )

            Spacer(modifier = Modifier.height(6.dp))

            SingleChoiceSegmentedButtonRow(modifier = Modifier.fillMaxWidth()) {
                EnergyLevel.entries.forEachIndexed { index, level ->
                    SegmentedButton(
                        shape = SegmentedButtonDefaults.itemShape(
                            index = index,
                            count = EnergyLevel.entries.size
                        ),
                        onClick = { onEnergyChange(level) },
                        selected = energy == level,
                        label = { Text(level.displayName) }
                    )
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
                        newText.toDoubleOrNull()?.let { onSpeedChange(it) }
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

                // Mute checkbox
                CompactCheckbox("Mute", muteMusic, onMuteMusicChange)

                Spacer(modifier = Modifier.weight(1f))

                // Repeat stepper
                Text(
                    text = "Repeat",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant
                )
                Spacer(modifier = Modifier.width(4.dp))
                FilledTonalIconButton(
                    onClick = { if (repeatCount > 1) onRepeatCountChange(repeatCount - 1) },
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
                    onClick = { onRepeatCountChange(repeatCount + 1) },
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
                    onClick = { onTransposeChange(transpose - 1) },
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
                    onClick = { onTransposeChange(transpose + 1) },
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

    var offset = timeMs - entry.timestamp_ms;
    var ratio = entry.duration_ms > 0 ? offset / entry.duration_ms : 0;
    if (ratio < 0) ratio = 0;
    if (ratio > 1) ratio = 1;

    var cursorX_svg = mPos.x + ratio * mPos.width;

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
