package com.solobandultra.app.ui.screens

import android.webkit.JavascriptInterface
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.compose.foundation.layout.*
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.MusicNote
import androidx.compose.material.icons.filled.Pause
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.filled.Stop
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import com.solobandultra.app.ScoreLib
import com.solobandultra.app.audio.PlaybackManager
import com.solobandultra.app.ui.theme.SoloBandUltraTheme
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SheetMusicScreen(
    playbackManager: PlaybackManager? = null
) {
    val isPlaying by playbackManager?.isPlaying?.collectAsState()
        ?: remember { mutableStateOf(false) }
    var showMenu by remember { mutableStateOf(false) }

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

    fun loadScore(fileIndex: Int, pageWidth: Float) {
        isLoading = true
        errorMessage = null
        svgContent = null
        playbackMapJson = null
        scope.launch {
            val result = withContext(Dispatchers.IO) {
                try {
                    val svg = ScoreLib.renderAsset(context, availableFiles[fileIndex], pageWidth)
                    val pmap = ScoreLib.playbackMapFromAsset(context, availableFiles[fileIndex], pageWidth)
                    val midi = ScoreLib.generateMidiFromAsset(context, availableFiles[fileIndex])
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
                            onClick = { showMenu = false }
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
}

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

@Composable
private fun PlaybackControlBar(
    isPlaying: Boolean,
    onPlayPause: () -> Unit,
    onStop: () -> Unit
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

            // Metronome placeholder
            IconButton(onClick = { }) {
                Icon(
                    imageVector = Icons.Default.MusicNote,
                    contentDescription = "Metronome",
                    modifier = Modifier.size(28.dp)
                )
            }
        }
    }
}

@Preview(showBackground = true)
@Composable
private fun SheetMusicScreenPreview() {
    SoloBandUltraTheme {
        SheetMusicScreen()
    }
}
