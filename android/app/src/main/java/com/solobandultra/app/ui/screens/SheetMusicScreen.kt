package com.solobandultra.app.ui.screens

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
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
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.solobandultra.app.ui.theme.SoloBandUltraTheme

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SheetMusicScreen(
    onPlayPause: (Boolean) -> Unit = {},
    onStop: () -> Unit = {}
) {
    var isPlaying by remember { mutableStateOf(false) }
    var showMenu by remember { mutableStateOf(false) }

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
        SheetMusicContent(
            modifier = Modifier
                .fillMaxSize()
                .padding(paddingValues)
        )
    }
}

@Composable
private fun SheetMusicContent(modifier: Modifier = Modifier) {
    val scrollState = rememberScrollState()

    Column(
        modifier = modifier
            .verticalScroll(scrollState)
            .padding(horizontal = 16.dp),
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        Spacer(modifier = Modifier.height(24.dp))

        // Title section
        Text(
            text = "Asa Branca",
            fontSize = 28.sp,
            fontWeight = FontWeight.Bold,
            textAlign = TextAlign.Center
        )
        Text(
            text = "White Wing",
            fontSize = 18.sp,
            fontStyle = FontStyle.Italic,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.Center
        )
        Text(
            text = "Luiz Gonzaga • Arr. Karim Ratib",
            fontSize = 14.sp,
            color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.7f),
            textAlign = TextAlign.Center
        )

        Spacer(modifier = Modifier.height(32.dp))

        // Placeholder staff systems
        for (systemIndex in 0..3) {
            StaffPlaceholder(systemIndex = systemIndex)
            Spacer(modifier = Modifier.height(32.dp))
        }

        Spacer(modifier = Modifier.height(24.dp))

        // Integration note
        Icon(
            imageVector = Icons.Default.MusicNote,
            contentDescription = null,
            modifier = Modifier.size(48.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.2f)
        )
        Spacer(modifier = Modifier.height(8.dp))
        Text(
            text = "Sheet music rendering will be powered by Rust",
            fontSize = 13.sp,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.3f),
            textAlign = TextAlign.Center
        )

        Spacer(modifier = Modifier.height(48.dp))
    }
}

@Composable
private fun StaffPlaceholder(systemIndex: Int) {
    val lineColor = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.2f)
    val noteColor = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.12f)

    Column {
        Text(
            text = "System ${systemIndex + 1}",
            fontSize = 10.sp,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.25f)
        )
        Spacer(modifier = Modifier.height(4.dp))

        Canvas(
            modifier = Modifier
                .fillMaxWidth()
                .height(50.dp)
        ) {
            val lineSpacing = size.height / 6f

            // Draw 5 staff lines
            for (i in 1..5) {
                val y = i * lineSpacing
                drawLine(
                    color = lineColor,
                    start = Offset(0f, y),
                    end = Offset(size.width, y),
                    strokeWidth = 1f
                )
            }

            // Draw placeholder note circles
            val noteCount = 8
            val noteSpacing = size.width / (noteCount + 1)
            for (i in 1..noteCount) {
                val x = i * noteSpacing
                val lineIndex = ((i + systemIndex) % 5) + 1
                val y = lineIndex * lineSpacing
                drawCircle(
                    color = noteColor,
                    radius = 6f,
                    center = Offset(x, y)
                )
            }
        }
    }
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
