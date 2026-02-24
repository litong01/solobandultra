package com.solobandultra.app.ui.screens

import android.webkit.WebView
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import com.solobandultra.app.FeedbackReport
import com.solobandultra.app.FeedbackState
import com.solobandultra.app.NoteResult
import com.solobandultra.app.ScoreLib
import kotlin.math.abs
import org.json.JSONArray
import org.json.JSONObject

// ── Colors ────────────────────────────────────────────────────────────────────

private val ColorCorrect     = Color(0xFF4CAF50)
private val ColorWrongTiming = Color(0xFFFFC107)
private val ColorWrongPitch  = Color(0xFFF44336)
private val ColorMissed      = Color(0xFF9E9E9E)

// ── Main Screen ───────────────────────────────────────────────────────────────

/**
 * Post-performance feedback report presented as a [ModalBottomSheet].
 * When [svgContent] and [playbackMapJson] are provided, shows the score SVG with
 * colored dots overlaid below each note (replacing the long note table).
 *
 * @param report  The [FeedbackReport] to display.
 * @param svgContent Optional score SVG HTML fragment for overlay view.
 * @param playbackMapJson Optional playback map JSON for dot positioning.
 * @param onDismiss Called when the user closes the sheet.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun FeedbackReportScreen(
    report: FeedbackReport,
    svgContent: String? = null,
    playbackMapJson: String? = null,
    onDismiss: () -> Unit
) {
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    val showSvgOverlay = svgContent != null && playbackMapJson != null
    val overlayDotsJson = if (showSvgOverlay) buildOverlayDotsJson(report, playbackMapJson!!) else null

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = sheetState,
        dragHandle = { BottomSheetDefaults.DragHandle() }
    ) {
        Column(modifier = Modifier.fillMaxWidth()) {
            // Header
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 16.dp, vertical = 8.dp),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically
            ) {
                Text(
                    text = "Performance Report",
                    style = MaterialTheme.typography.titleLarge,
                    fontWeight = FontWeight.SemiBold
                )
                IconButton(onClick = onDismiss) {
                    Icon(Icons.Filled.Close, contentDescription = "Close")
                }
            }

            if (showSvgOverlay && overlayDotsJson != null) {
                val svgWithOverlay = ScoreLib.addFeedbackOverlay(svgContent!!, overlayDotsJson)
                if (svgWithOverlay != null) {
                    // Summary + SVG with overlay (Rust-generated; no script)
                    Column(
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(horizontal = 16.dp),
                        verticalArrangement = Arrangement.spacedBy(16.dp)
                    ) {
                        SummarySection(report)
                        HorizontalDivider()
                        Text(
                            text = "Note accuracy on score",
                            style = MaterialTheme.typography.titleMedium,
                            fontWeight = FontWeight.SemiBold
                        )
                        Text(
                            text = "Green = on time, Yellow = wrong timing, Red = wrong pitch, Gray = missed",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant
                        )
                        Box(
                            modifier = Modifier
                                .fillMaxWidth()
                                .heightIn(min = 200.dp)
                        ) {
                            AndroidView(
                                factory = { ctx ->
                                    WebView(ctx).apply {
                                        settings.javaScriptEnabled = false
                                    }
                                },
                                update = { webView ->
                                    val html = buildReportSvgHtml(svgWithOverlay)
                                    webView.loadDataWithBaseURL(
                                        "file:///android_asset/",
                                        html,
                                        "text/html",
                                        "UTF-8",
                                        null
                                    )
                                }
                            )
                        }
                    }
                } else {
                    LazyColumn(
                        modifier = Modifier.fillMaxWidth(),
                        contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 0.dp, bottom = 32.dp),
                        verticalArrangement = Arrangement.spacedBy(16.dp)
                    ) {
                        item { SummarySection(report) }
                        item { HorizontalDivider() }
                        item {
                            Text(
                                text = "Notes",
                                style = MaterialTheme.typography.titleMedium,
                                fontWeight = FontWeight.SemiBold
                            )
                        }
                        item { NoteListHeader() }
                        items(report.results) { result ->
                            NoteRow(result)
                        }
                    }
                }
            } else {
                // Fallback: table of notes (when no SVG/pmap available)
                LazyColumn(
                    modifier = Modifier.fillMaxWidth(),
                    contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 0.dp, bottom = 32.dp),
                    verticalArrangement = Arrangement.spacedBy(16.dp)
                ) {
                    item { SummarySection(report) }
                    item { HorizontalDivider() }
                    item {
                        Text(
                            text = "Notes",
                            style = MaterialTheme.typography.titleMedium,
                            fontWeight = FontWeight.SemiBold
                        )
                    }
                    item { NoteListHeader() }
                    items(report.results) { result ->
                        NoteRow(result)
                    }
                }
            }
        }
    }
}

/**
 * Build JSON array of overlay dots from report and playback map.
 * Each entry: { x, y, colors: string[] } in SVG coordinates; multiple colors = stacked dots.
 */
private fun buildOverlayDotsJson(report: FeedbackReport, playbackMapJson: String): String? {
    return try {
        val pmap = JSONObject(playbackMapJson)
        val measures = pmap.optJSONArray("measures") ?: return null
        val systems = pmap.optJSONArray("systems") ?: return null

        // Group results by (measureIdx, noteIdx) to handle multiple passes
        val group = report.results
            .filter { it.expected.measureIdx >= 0 && it.expected.noteIdx >= 0 }
            .groupBy { Pair(it.expected.measureIdx, it.expected.noteIdx) }

        val dotsArray = JSONArray()
        for ((key, results) in group) {
            val (measureIdx, noteIdx) = key
            val measure = (0 until measures.length()).firstOrNull { i ->
                measures.getJSONObject(i).optInt("measure_idx", -1) == measureIdx
            } ?: continue
            val m = measures.getJSONObject(measure)
            val systemIdx = m.optInt("system_idx", 0)
            val notePositions = m.optJSONArray("note_positions") ?: continue
            if (noteIdx >= notePositions.length()) continue
            val fracX = notePositions.getJSONArray(noteIdx)
            val x = if (fracX.length() >= 2) fracX.getDouble(1) else m.optDouble("x", 0.0)
            if (systemIdx >= systems.length()) continue
            val sys = systems.getJSONObject(systemIdx)
            val baseY = sys.optDouble("dots_base_y", sys.optDouble("y", 0.0) + sys.optDouble("height", 40.0) + 16.0)
            val colors = results.map { r ->
                if (r.status == FeedbackState.Silent) "#9E9E9E" else r.status.cursorColor
            }
            val dotEntry = JSONObject().apply {
                put("x", x)
                put("y", baseY)
                put("colors", JSONArray(colors))
            }
            dotsArray.put(dotEntry)
        }
        dotsArray.toString()
    } catch (_: Exception) {
        null
    }
}

/**
 * Minimal HTML to display the score SVG (with overlay already embedded by Rust).
 * No script — overlay is generated in Rust when the user opens the report.
 */
private fun buildReportSvgHtml(svgWithOverlay: String): String {
    return """
        <!DOCTYPE html>
        <html>
        <head>
        <meta name="viewport" content="width=device-width, initial-scale=1.0, maximum-scale=3.0, user-scalable=yes">
        <style>
            @font-face { font-family: 'Lora'; src: url('fonts/Lora-Regular.ttf') format('truetype'); font-weight: 100 900; font-style: normal; }
            @font-face { font-family: 'Lora'; src: url('fonts/Lora-Italic.ttf') format('truetype'); font-weight: 100 900; font-style: italic; }
            @font-face { font-family: 'LXGW WenKai'; src: url('fonts/LXGWWenKai-Regular.ttf') format('truetype'); font-weight: normal; font-style: normal; }
            * { margin: 0; padding: 0; box-sizing: border-box; }
            body { background: white; display: flex; justify-content: center; padding: 8px; }
            svg { width: 100%; height: auto; max-width: 100%; display: block; }
        </style>
        </head>
        <body>
        $svgWithOverlay
        </body>
        </html>
    """.trimIndent()
}

// ── Summary ───────────────────────────────────────────────────────────────────

@Composable
private fun SummarySection(report: FeedbackReport) {
    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Text("Summary", style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.SemiBold)

        Row(
            modifier = Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(12.dp))
                .background(MaterialTheme.colorScheme.surfaceVariant)
        ) {
            ScoreCard(
                label = "Pitch",
                value = "%.0f%%".format(report.pitchAccuracy),
                color = accuracyColor(report.pitchAccuracy),
                modifier = Modifier.weight(1f)
            )
            ScoreCard(
                label = "Rhythm",
                value = "%.0f%%".format(report.rhythmAccuracy),
                color = accuracyColor(report.rhythmAccuracy),
                modifier = Modifier.weight(1f)
            )
            ScoreCard(
                label = "Score",
                value = "%.0f%%".format(report.overallScore),
                color = accuracyColor(report.overallScore),
                modifier = Modifier.weight(1f)
            )
        }

        Row(
            horizontalArrangement = Arrangement.spacedBy(16.dp),
            modifier = Modifier.fillMaxWidth()
        ) {
            StatChip("${report.totalNotes} total")
            StatChip("${report.attemptedNotes} played")
            StatChip("${report.missedNotes.size} missed")
        }
    }
}

@Composable
private fun ScoreCard(label: String, value: String, color: Color, modifier: Modifier = Modifier) {
    Column(
        modifier = modifier.padding(vertical = 12.dp),
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        Text(
            text = value,
            fontSize = 28.sp,
            fontWeight = FontWeight.Bold,
            color = color
        )
        Text(
            text = label,
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant
        )
    }
}

@Composable
private fun StatChip(text: String) {
    Surface(
        shape = RoundedCornerShape(50),
        color = MaterialTheme.colorScheme.surfaceVariant
    ) {
        Text(
            text = text,
            style = MaterialTheme.typography.labelSmall,
            modifier = Modifier.padding(horizontal = 10.dp, vertical = 4.dp)
        )
    }
}

// ── Note list ─────────────────────────────────────────────────────────────────

@Composable
private fun NoteListHeader() {
    Row(
        modifier = Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically
    ) {
        Text("Expected", style = MaterialTheme.typography.labelSmall,
             color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.weight(1f))
        Text("Detected", style = MaterialTheme.typography.labelSmall,
             color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.width(64.dp), textAlign = TextAlign.Center)
        Text("Delta", style = MaterialTheme.typography.labelSmall,
             color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.width(64.dp), textAlign = TextAlign.End)
        Spacer(modifier = Modifier.width(20.dp))
    }
}

@Composable
private fun NoteRow(result: NoteResult) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 2.dp),
        verticalAlignment = Alignment.CenterVertically
    ) {
        Text(result.expected.name, modifier = Modifier.weight(1f))
        Text(
            result.detectedName,
            modifier = Modifier.width(64.dp),
            textAlign = TextAlign.Center,
            color = if (result.detectedMidi == null) MaterialTheme.colorScheme.onSurfaceVariant
                    else MaterialTheme.colorScheme.onSurface
        )
        val deltaText = result.timingDeltaMs?.let { "%+.0fms".format(it) } ?: "—"
        val deltaColor = result.timingDeltaMs?.let {
            if (abs(it) <= 200) ColorCorrect else ColorWrongTiming
        } ?: MaterialTheme.colorScheme.onSurfaceVariant
        Text(
            deltaText,
            modifier = Modifier.width(64.dp),
            textAlign = TextAlign.End,
            style = MaterialTheme.typography.bodySmall,
            color = deltaColor
        )
        Spacer(modifier = Modifier.width(8.dp))
        Box(
            modifier = Modifier
                .size(10.dp)
                .clip(CircleShape)
                .background(statusColor(result.status))
        )
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

private fun accuracyColor(pct: Double): Color = when {
    pct >= 80 -> ColorCorrect
    pct >= 50 -> ColorWrongTiming
    else      -> ColorWrongPitch
}

private fun statusColor(state: FeedbackState): Color = when (state) {
    FeedbackState.Correct     -> ColorCorrect
    FeedbackState.WrongTiming -> ColorWrongTiming
    FeedbackState.WrongPitch  -> ColorWrongPitch
    FeedbackState.Silent      -> ColorMissed
}
