package com.solobandultra.app.ui.screens

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
import com.solobandultra.app.ui.theme.SoloBandUltraTheme
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SheetMusicScreen(
    onPlayPause: (Boolean) -> Unit = {},
    onStop: () -> Unit = {}
) {
    var isPlaying by remember { mutableStateOf(false) }
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
    var isLoading by remember { mutableStateOf(true) }
    var errorMessage by remember { mutableStateOf<String?>(null) }

    val scope = rememberCoroutineScope()
    val screenWidthDp = LocalConfiguration.current.screenWidthDp.toFloat()

    fun loadScore(fileIndex: Int, pageWidth: Float) {
        isLoading = true
        errorMessage = null
        svgContent = null
        scope.launch {
            val svg = withContext(Dispatchers.IO) {
                try {
                    ScoreLib.renderAsset(context, availableFiles[fileIndex], pageWidth)
                } catch (e: Exception) {
                    null
                }
            }
            isLoading = false
            if (svg != null) {
                svgContent = svg
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
                    isPlaying = !isPlaying
                    onPlayPause(isPlaying)
                },
                onStop = {
                    isPlaying = false
                    onStop()
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
                        SvgWebView(svg = svgContent!!)
                    }
                }
            }
        }
    }
}

@Composable
private fun SvgWebView(svg: String) {
    AndroidView(
        factory = { context ->
            WebView(context).apply {
                webViewClient = WebViewClient()
                settings.apply {
                    builtInZoomControls = true
                    displayZoomControls = false
                    useWideViewPort = true
                    loadWithOverviewMode = true
                    setSupportZoom(true)
                }
                setBackgroundColor(android.graphics.Color.WHITE)
            }
        },
        update = { webView ->
            val html = """
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
                    svg {
                        width: 100%;
                        height: auto;
                        max-width: 100%;
                    }
                </style>
                </head>
                <body>
                $svg
                </body>
                </html>
            """.trimIndent()
            webView.loadDataWithBaseURL(null, html, "text/html", "UTF-8", null)
        },
        modifier = Modifier.fillMaxSize()
    )
}

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
