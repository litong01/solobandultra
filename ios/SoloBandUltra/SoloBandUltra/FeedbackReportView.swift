import SwiftUI

/// Post-performance feedback report presented as a sheet.
struct FeedbackReportView: View {
    let report: FeedbackReport
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(spacing: 20) {
                    summarySection
                    Divider()
                    noteListSection
                    if !report.missedNotes.isEmpty {
                        Divider()
                        missedNotesSection
                    }
                }
                .padding()
            }
            .navigationTitle("Performance Report")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                }
            }
        }
    }

    // MARK: - Summary

    private var summarySection: some View {
        VStack(spacing: 12) {
            Text("Summary")
                .font(.headline)
                .frame(maxWidth: .infinity, alignment: .leading)

            HStack(spacing: 0) {
                scoreCard(
                    label: "Pitch",
                    value: String(format: "%.0f%%", report.pitchAccuracy),
                    color: accuracyColor(report.pitchAccuracy)
                )
                scoreCard(
                    label: "Rhythm",
                    value: String(format: "%.0f%%", report.rhythmAccuracy),
                    color: accuracyColor(report.rhythmAccuracy)
                )
                scoreCard(
                    label: "Score",
                    value: String(format: "%.0f%%", report.overallScore),
                    color: accuracyColor(report.overallScore)
                )
            }
            .background(Color(.secondarySystemBackground))
            .clipShape(RoundedRectangle(cornerRadius: 12))

            HStack(spacing: 16) {
                Label("\(report.totalNotes) total", systemImage: "music.note.list")
                Label("\(report.attemptedNotes) played", systemImage: "checkmark.circle")
                Label("\(report.missedNotes.count) missed", systemImage: "xmark.circle")
            }
            .font(.caption)
            .foregroundStyle(.secondary)
        }
    }

    private func scoreCard(label: String, value: String, color: Color) -> some View {
        VStack(spacing: 4) {
            Text(value)
                .font(.system(size: 28, weight: .bold, design: .rounded))
                .foregroundStyle(color)
            Text(label)
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 12)
    }

    // MARK: - Note list

    private var noteListSection: some View {
        VStack(spacing: 8) {
            Text("Notes")
                .font(.headline)
                .frame(maxWidth: .infinity, alignment: .leading)

            // Header row
            HStack {
                Text("Expected").frame(maxWidth: .infinity, alignment: .leading)
                Text("Detected").frame(width: 60)
                Text("Delta").frame(width: 60)
                Text("").frame(width: 12)
            }
            .font(.caption)
            .foregroundStyle(.secondary)

            ForEach(report.results) { result in
                noteRow(result)
            }
        }
    }

    private func noteRow(_ result: NoteResult) -> some View {
        HStack {
            Text(result.expected.name)
                .frame(maxWidth: .infinity, alignment: .leading)
            Text(result.detectedName)
                .frame(width: 60)
                .foregroundStyle(result.detectedMidi == nil ? .secondary : .primary)
            Group {
                if let delta = result.timingDeltaMs {
                    Text(String(format: "%+.0fms", delta))
                        .foregroundStyle(abs(delta) <= 200 ? .green : .orange)
                } else {
                    Text("—").foregroundStyle(.secondary)
                }
            }
            .frame(width: 60)
            .font(.caption.monospacedDigit())

            Circle()
                .fill(statusColor(result.status))
                .frame(width: 10, height: 10)
        }
        .font(.subheadline)
        .padding(.vertical, 2)
    }

    // MARK: - Missed notes

    private var missedNotesSection: some View {
        VStack(spacing: 8) {
            Text("Missed Notes")
                .font(.headline)
                .frame(maxWidth: .infinity, alignment: .leading)
            FlowLayout(spacing: 6) {
                ForEach(report.missedNotes) { result in
                    Text(result.expected.name)
                        .font(.caption)
                        .padding(.horizontal, 8)
                        .padding(.vertical, 4)
                        .background(Color.red.opacity(0.15))
                        .clipShape(Capsule())
                }
            }
        }
    }

    // MARK: - Helpers

    private func accuracyColor(_ pct: Double) -> Color {
        if pct >= 80 { return .green }
        if pct >= 50 { return .orange }
        return .red
    }

    private func statusColor(_ state: FeedbackState) -> Color {
        switch state {
        case .correct:     return .green
        case .wrongTiming: return .yellow
        case .wrongPitch:  return .red
        case .silent:      return Color(.systemGray4)
        }
    }
}

// MARK: - Simple flow layout for missed-note chips

private struct FlowLayout: Layout {
    var spacing: CGFloat = 8

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let containerWidth = proposal.width ?? .infinity
        var x: CGFloat = 0
        var y: CGFloat = 0
        var rowHeight: CGFloat = 0
        for subview in subviews {
            let size = subview.sizeThatFits(.unspecified)
            if x + size.width > containerWidth && x > 0 {
                x = 0
                y += rowHeight + spacing
                rowHeight = 0
            }
            x += size.width + spacing
            rowHeight = max(rowHeight, size.height)
        }
        return CGSize(width: containerWidth, height: y + rowHeight)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        var x = bounds.minX
        var y = bounds.minY
        var rowHeight: CGFloat = 0
        for subview in subviews {
            let size = subview.sizeThatFits(.unspecified)
            if x + size.width > bounds.maxX && x > bounds.minX {
                x = bounds.minX
                y += rowHeight + spacing
                rowHeight = 0
            }
            subview.place(at: CGPoint(x: x, y: y), proposal: .unspecified)
            x += size.width + spacing
            rowHeight = max(rowHeight, size.height)
        }
    }
}

// MARK: - Preview

#Preview {
    let timeline: [NoteEvent] = [
        NoteEvent(startMs: 0,   endMs: 500,  midi: 60, name: "C4"),
        NoteEvent(startMs: 500, endMs: 1000, midi: 62, name: "D4"),
        NoteEvent(startMs: 1000, endMs: 1500, midi: 64, name: "E4"),
        NoteEvent(startMs: 1500, endMs: 2000, midi: 65, name: "F4"),
    ]
    let results: [NoteResult] = [
        NoteResult(expected: timeline[0], detectedMidi: 60, detectedStartMs: 30,   status: .correct),
        NoteResult(expected: timeline[1], detectedMidi: 62, detectedStartMs: 650,  status: .wrongTiming),
        NoteResult(expected: timeline[2], detectedMidi: 63, detectedStartMs: 1010, status: .wrongPitch),
        NoteResult(expected: timeline[3], detectedMidi: nil, detectedStartMs: nil, status: .silent),
    ]
    return FeedbackReportView(report: FeedbackReport(results: results))
}
