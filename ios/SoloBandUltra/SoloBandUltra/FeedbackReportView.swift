import SwiftUI
import WebKit

private let feedbackDotRowHeight = 14.0

/// Post-performance feedback report presented as a sheet.
/// When svgContent and playbackMapJson are provided, shows the score SVG with colored dots overlaid (no note table).
struct FeedbackReportView: View {
    let report: FeedbackReport
    var svgContent: String? = nil
    var playbackMapJson: String? = nil
    @Environment(\.dismiss) private var dismiss

    private var showSvgOverlay: Bool { svgContent != nil && playbackMapJson != nil }
    private var overlayDotsJson: String? {
        guard let pmap = playbackMapJson else { return nil }
        return Self.buildOverlayDotsJson(report: report, playbackMapJson: pmap)
    }

    var body: some View {
        NavigationStack {
            if showSvgOverlay, let overlayJson = overlayDotsJson, let svg = svgContent {
                ScrollView {
                    VStack(spacing: 20) {
                        summarySection
                        Divider()
                        Text("Note accuracy on score")
                            .font(.headline)
                            .frame(maxWidth: .infinity, alignment: .leading)
                        Text("Green = on time, Yellow = wrong timing, Red = wrong pitch, Gray = missed")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .frame(maxWidth: .infinity, alignment: .leading)
                        ReportOverlayWebView(html: Self.buildReportOverlayHtml(svg: svg, overlayDotsJson: overlayJson))
                            .frame(height: 280)
                    }
                    .padding()
                }
            } else {
                ScrollView {
                    VStack(spacing: 20) {
                        summarySection
                        Divider()
                        noteListSection
                    }
                    .padding()
                }
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

    // MARK: - Overlay helpers

    static func buildOverlayDotsJson(report: FeedbackReport, playbackMapJson: String) -> String? {
        guard let data = playbackMapJson.data(using: .utf8),
              let pmap = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let measures = pmap["measures"] as? [[String: Any]],
              let systems = pmap["systems"] as? [[String: Any]] else { return nil }

        let grouped = Dictionary(grouping: report.results.filter { $0.expected.measureIdx >= 0 && $0.expected.noteIdx >= 0 }) { (result: NoteResult) in
            (result.expected.measureIdx, result.expected.noteIdx)
        }

        var dots: [[String: Any]] = []
        for (key, results) in grouped {
            let (measureIdx, noteIdx) = key
            guard let measure = measures.first(where: { ($0["measure_idx"] as? Int) == measureIdx }),
                  let systemIdx = measure["system_idx"] as? Int,
                  systemIdx < systems.count,
                  let notePositions = measure["note_positions"] as? [[Double]],
                  noteIdx < notePositions.count,
                  notePositions[noteIdx].count >= 2 else { continue }
            let x = notePositions[noteIdx][1]
            let sys = systems[systemIdx] as? [String: Any]
            let baseY: Double
            if let dy = sys?["dots_base_y"] as? Double {
                baseY = dy
            } else if let sy = sys?["y"] as? Double, let sh = sys?["height"] as? Double {
                baseY = sy + sh + 16
            } else {
                baseY = 0
            }
            let colors = results.map { r in r.status == .silent ? "#9E9E9E" : r.status.cursorColor }
            dots.append(["x": x, "y": baseY, "colors": colors])
        }
        guard let out = try? JSONSerialization.data(withJSONObject: dots),
              let str = String(data: out, encoding: .utf8) else { return nil }
        return str
    }

    static func buildReportOverlayHtml(svg: String, overlayDotsJson: String) -> String {
        let safeSvg: String = {
            guard let regex = try? NSRegularExpression(pattern: "<script[^>]*>[\\s\\S]*?</script>", options: .caseInsensitive) else { return svg }
            let range = NSRange(svg.startIndex..., in: svg)
            return regex.stringByReplacingMatches(in: svg, range: range, withTemplate: "")
        }()
        let dotsJs = overlayDotsJson.replacingOccurrences(of: "</", with: "<\\/")
        return """
        <!DOCTYPE html>
        <html>
        <head>
        <meta name="viewport" content="width=device-width, initial-scale=1.0, maximum-scale=3.0, user-scalable=yes">
        <style>
            @font-face { font-family: 'Lora'; src: url('Fonts/Lora-Regular.ttf') format('truetype'); font-weight: 100 900; font-style: normal; }
            @font-face { font-family: 'Lora'; src: url('Fonts/Lora-Italic.ttf') format('truetype'); font-weight: 100 900; font-style: italic; }
            @font-face { font-family: 'LXGW WenKai'; src: url('Fonts/LXGWWenKai-Regular.ttf') format('truetype'); font-weight: normal; font-style: normal; }
            * { margin: 0; padding: 0; box-sizing: border-box; }
            body { background: white; display: flex; justify-content: center; padding: 8px; }
            #score-container { position: relative; display: inline-block; width: 100%; }
            #score-container svg { width: 100%; height: auto; max-width: 100%; display: block; }
            #overlay { position: absolute; left: 0; top: 0; width: 100%; height: 100%; pointer-events: none; }
            #overlay svg { width: 100%; height: 100%; display: block; }
        </style>
        </head>
        <body>
        <div id="score-container">
            \(safeSvg)
            <div id="overlay"></div>
        </div>
        <script>
        (function() {
            var dots = \(dotsJs);
            var scoreSvg = document.querySelector('#score-container > svg');
            if (!scoreSvg || !dots || dots.length === 0) return;
            var vb = scoreSvg.getAttribute('viewBox');
            if (!vb) return;
            var rowHeight = Number(\(feedbackDotRowHeight));
            var radius = 4;
            var ns = 'http://www.w3.org/2000/svg';
            var g = document.createElementNS(ns, 'g');
            for (var i = 0; i < dots.length; i++) {
                var d = dots[i];
                var x = Number(d.x);
                var baseY = Number(d.y);
                var colors = d.colors || [];
                for (var j = 0; j < colors.length; j++) {
                    var circle = document.createElementNS(ns, 'circle');
                    circle.setAttribute('cx', String(x));
                    circle.setAttribute('cy', String(baseY + j * rowHeight));
                    circle.setAttribute('r', radius);
                    circle.setAttribute('fill', colors[j]);
                    g.appendChild(circle);
                }
            }
            var overlaySvg = document.createElementNS(ns, 'svg');
            overlaySvg.setAttribute('viewBox', vb);
            overlaySvg.setAttribute('preserveAspectRatio', scoreSvg.getAttribute('preserveAspectRatio') || 'xMidYMid meet');
            overlaySvg.appendChild(g);
            document.getElementById('overlay').appendChild(overlaySvg);
        })();
        </script>
        </body>
        </html>
        """
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

// MARK: - WebView for report overlay (SVG + dots)

private struct ReportOverlayWebView: UIViewRepresentable {
    let html: String

    func makeUIView(context: Context) -> WKWebView {
        let webView = WKWebView()
        webView.isOpaque = false
        webView.backgroundColor = .clear
        webView.scrollView.backgroundColor = .clear
        return webView
    }

    func updateUIView(_ webView: WKWebView, context: Context) {
        webView.loadHTMLString(html, baseURL: Bundle.main.bundleURL)
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
