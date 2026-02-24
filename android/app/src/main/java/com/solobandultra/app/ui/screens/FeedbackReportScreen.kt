package com.solobandultra.app.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.layout.ExperimentalLayoutApi
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
import com.solobandultra.app.FeedbackReport
import com.solobandultra.app.FeedbackState
import com.solobandultra.app.NoteResult
import kotlin.math.abs

// ── Colors ────────────────────────────────────────────────────────────────────

private val ColorCorrect     = Color(0xFF4CAF50)
private val ColorWrongTiming = Color(0xFFFFC107)
private val ColorWrongPitch  = Color(0xFFF44336)
private val ColorMissed      = Color(0xFF9E9E9E)

// ── Main Screen ───────────────────────────────────────────────────────────────

/**
 * Post-performance feedback report presented as a [ModalBottomSheet].
 *
 * @param report  The [FeedbackReport] to display.
 * @param onDismiss Called when the user closes the sheet.
 */
@OptIn(ExperimentalMaterial3Api::class, ExperimentalLayoutApi::class)
@Composable
fun FeedbackReportScreen(
    report: FeedbackReport,
    onDismiss: () -> Unit
) {
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)

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
                if (report.missedNotes.isNotEmpty()) {
                    item { HorizontalDivider() }
                    item { MissedNotesSection(report.missedNotes) }
                }
            }
        }
    }
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

// ── Missed notes ──────────────────────────────────────────────────────────────

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun MissedNotesSection(missed: List<NoteResult>) {
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text("Missed Notes", style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.SemiBold)
        FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), modifier = Modifier.fillMaxWidth()) {
            missed.forEach { result ->
                Surface(
                    shape = RoundedCornerShape(50),
                    color = Color(0xFFF44336).copy(alpha = 0.15f)
                ) {
                    Text(
                        text = result.expected.name,
                        style = MaterialTheme.typography.labelSmall,
                        modifier = Modifier.padding(horizontal = 10.dp, vertical = 4.dp)
                    )
                }
            }
        }
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
