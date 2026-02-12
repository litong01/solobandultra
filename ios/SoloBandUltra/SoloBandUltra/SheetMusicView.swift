import SwiftUI
import WebKit

/// Displays rendered sheet music SVG using a WKWebView with playback cursor support.
struct SheetMusicView: View {
    @EnvironmentObject var playbackManager: PlaybackManager
    @EnvironmentObject var midiSettings: MidiSettings
    @State private var svgContent: String?
    @State private var playbackMapJson: String?
    @State private var isLoading = true
    @State private var errorMessage: String?
    @State private var lastRenderedWidth: CGFloat = 0
    @State private var lastOptionsJson: String = ""

    /// Extract the filename from the selected file URL.
    private var currentFile: String {
        let url = midiSettings.selectedFileUrl
        if url.hasPrefix("file://SheetMusic/") {
            return String(url.dropFirst("file://SheetMusic/".count))
        }
        return url.components(separatedBy: "/").last ?? MidiSettings.defaultLandingFile
    }

    var body: some View {
        GeometryReader { geometry in
            VStack(spacing: 0) {
                // Score display
                if isLoading {
                    ProgressView("Rendering score...")
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                } else if let error = errorMessage {
                    VStack(spacing: 12) {
                        Image(systemName: "exclamationmark.triangle")
                            .font(.system(size: 40))
                            .foregroundStyle(.secondary)
                        Text(error)
                            .font(.callout)
                            .foregroundStyle(.secondary)
                            .multilineTextAlignment(.center)
                    }
                    .padding()
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                } else if let svg = svgContent {
                    SVGWebView(
                        svgString: svg,
                        playbackMapJson: playbackMapJson,
                        playbackManager: playbackManager
                    )
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                } else {
                    Text("No score loaded")
                        .foregroundStyle(.secondary)
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                }
            }
            .background(Color(.systemBackground))
            .onAppear {
                // Sync playback settings to PlaybackManager on first appear
                playbackManager.speed = midiSettings.playbackSpeed
                playbackManager.isMuted = midiSettings.muteMusic
                playbackManager.repeatCount = midiSettings.repeatCount
                loadScore(width: geometry.size.width)
            }
            .onChange(of: midiSettings.selectedFileUrl) { _ in
                loadScore(width: geometry.size.width)
            }
            .onChange(of: midiSettings.transpose) { _ in
                loadScore(width: geometry.size.width)
            }
            .onChange(of: geometry.size.width) { newWidth in
                // Re-render when width changes (e.g. device rotation)
                if abs(newWidth - lastRenderedWidth) > 10 {
                    loadScore(width: newWidth)
                }
            }
            .onReceive(midiSettings.objectWillChange) { _ in
                // When MIDI settings change, regenerate MIDI with new options.
                // Use a small delay so the published value has landed.
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.1) {
                    regenerateMidi()
                }
            }
            // ── Playback settings → PlaybackManager (no MIDI regen) ──
            .onChange(of: midiSettings.playbackSpeed) { newSpeed in
                playbackManager.speed = newSpeed
            }
            .onChange(of: midiSettings.muteMusic) { newMute in
                playbackManager.isMuted = newMute
            }
            .onChange(of: midiSettings.repeatCount) { newRepeat in
                playbackManager.repeatCount = newRepeat
            }
        }
    }

    private func loadScore(width: CGFloat) {
        isLoading = true
        errorMessage = nil
        svgContent = nil
        playbackMapJson = nil
        lastRenderedWidth = width

        let pageWidth = Double(width)
        let optionsJson = midiSettings.toJson()
        lastOptionsJson = optionsJson
        let transposeVal = Int32(midiSettings.transpose)

        DispatchQueue.global(qos: .userInitiated).async {
            // Find the file in the app bundle
        let filename = currentFile
        guard !filename.isEmpty else {
            DispatchQueue.main.async {
                isLoading = false
                errorMessage = "No music file selected"
            }
            return
        }
        let ext = (filename as NSString).pathExtension
        let name = (filename as NSString).deletingPathExtension

        // With a folder reference, files are inside a "SheetMusic" subdirectory
        // of the app bundle. Try there first, then fall back to the bundle root.
        let url: URL
        if let folderURL = Bundle.main.url(forResource: name, withExtension: ext, subdirectory: "SheetMusic") {
            url = folderURL
        } else if let rootURL = Bundle.main.url(forResource: name, withExtension: ext) {
            url = rootURL
        } else {
            DispatchQueue.main.async {
                isLoading = false
                errorMessage = "File '\(filename)' not found in app bundle"
                }
                return
            }

            // Read file data for byte-based APIs
            guard let data = try? Data(contentsOf: url) else {
                DispatchQueue.main.async {
                    isLoading = false
                    errorMessage = "Failed to read '\(filename)'"
                }
                return
            }

            // Render SVG using the Rust library (with transpose)
            let svg = ScoreLib.renderFile(at: url.path, pageWidth: pageWidth, transpose: transposeVal)

            // Generate playback map (with transpose for consistent positions)
            let pmap = ScoreLib.playbackMap(data, extension: ext, pageWidth: pageWidth, transpose: transposeVal)

            // Generate MIDI data for playback with current settings (transpose is in optionsJson)
            let midi = ScoreLib.generateMidi(data, extension: ext, optionsJson: optionsJson)

            DispatchQueue.main.async {
                isLoading = false
                if let svg = svg {
                    svgContent = svg
                    playbackMapJson = pmap

                    // Prepare the playback manager with the MIDI data
                    if let midiData = midi {
                        playbackManager.prepareMidi(midiData)
                    }
                } else {
                    errorMessage = "Failed to render '\(filename)'"
                }
            }
        }
    }

    /// Regenerate only the MIDI data when settings change (no need to re-render SVG).
    private func regenerateMidi() {
        let optionsJson = midiSettings.toJson()
        guard optionsJson != lastOptionsJson else { return }
        lastOptionsJson = optionsJson

        let filename = currentFile
        guard !filename.isEmpty else { return }

        let ext = (filename as NSString).pathExtension
        let name = (filename as NSString).deletingPathExtension

        DispatchQueue.global(qos: .userInitiated).async {
            let url: URL
            if let folderURL = Bundle.main.url(forResource: name, withExtension: ext, subdirectory: "SheetMusic") {
                url = folderURL
            } else if let rootURL = Bundle.main.url(forResource: name, withExtension: ext) {
                url = rootURL
            } else {
                return
            }

            guard let data = try? Data(contentsOf: url) else { return }
            let midi = ScoreLib.generateMidi(data, extension: ext, optionsJson: optionsJson)

            DispatchQueue.main.async {
                if let midiData = midi {
                    playbackManager.prepareMidi(midiData)
                }
            }
        }
    }
}

// MARK: - SVGWebView with playback cursor support

/// WKWebView wrapper for displaying SVG content with an animated playback cursor.
struct SVGWebView: UIViewRepresentable {
    let svgString: String
    let playbackMapJson: String?
    let playbackManager: PlaybackManager

    func makeCoordinator() -> Coordinator {
        Coordinator(playbackManager: playbackManager)
    }

    func makeUIView(context: Context) -> WKWebView {
        let config = WKWebViewConfiguration()

        // Register message handler for receiving seek events from JavaScript
        config.userContentController.add(context.coordinator, name: "playback")

        let webView = WKWebView(frame: .zero, configuration: config)
        webView.isOpaque = false
        webView.backgroundColor = .clear
        webView.scrollView.backgroundColor = .clear
        webView.scrollView.showsVerticalScrollIndicator = true
        webView.scrollView.showsHorizontalScrollIndicator = false
        webView.scrollView.bounces = true

        // Give the playback manager a reference to the web view for cursor updates
        playbackManager.webView = webView

        return webView
    }

    func updateUIView(_ webView: WKWebView, context: Context) {
        // Update coordinator's reference to playback manager
        context.coordinator.playbackManager = playbackManager

        // Ensure the webView reference is current
        playbackManager.webView = webView

        let html = Self.buildHTML(svg: svgString, playbackMapJson: playbackMapJson)
        webView.loadHTMLString(html, baseURL: nil)
    }

    /// Build the complete HTML document with SVG, cursor div, and playback JavaScript.
    static func buildHTML(svg: String, playbackMapJson: String?) -> String {
        let pmapJS = playbackMapJson ?? "null"
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
            \(svg)
            <div id="cursor"></div>
        </div>
        <script>
        \(Self.cursorJavaScript)
        // Initialize playback map and show cursor at the beginning
        var _pmapData = \(pmapJS);
        if (_pmapData) { initPlayback(_pmapData); showCursor(); moveCursor(0); }
        </script>
        </body>
        </html>
        """
    }

    /// The shared cursor JavaScript (ported from mysoloband).
    static let cursorJavaScript: String = """
    // ─── Playback cursor synchronization ───────────────────────────────
    // Ported from mysoloband's VerovioRendererBase._move() and Player.play()

    var _measures = [];      // {measure_idx, x, width, system_idx}
    var _systems = [];       // {y, height}
    var _timemap = [];       // {index, original_index, timestamp_ms, duration_ms, tempo_bpm}
    var _measureByIdx = {};  // original_index -> {x, width, system_idx}
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

        // Build a lookup from original measure index to visual position
        _measureByIdx = {};
        for (var i = 0; i < _measures.length; i++) {
            _measureByIdx[_measures[i].measure_idx] = _measures[i];
        }

        // Compute total duration
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

    // Binary search: find the timemap entry for a given time in ms
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

    // Scale factor: SVG user units -> CSS pixels in the container
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

        // Clamp to valid range
        if (timeMs < 0) timeMs = 0;
        if (timeMs > _totalDurationMs) timeMs = _totalDurationMs;

        var entry = findTimemapEntry(timeMs);
        if (!entry) return;

        // Find the visual position for this measure
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

        // Get the system for vertical positioning
        var sys = _systems[mPos.system_idx];
        if (!sys) return;

        // Extend cursor 2 staff-line-spacings (20 SVG units) above and below the staff
        var EXTEND = 20;
        var scale = getScaleFactor();
        var cursorX = cursorX_svg * scale;
        var cursorY = (sys.y - EXTEND) * scale;
        var cursorHeight = (sys.height + EXTEND * 2) * scale;

        // Position the cursor
        _cursorEl.style.transform = 'translate(' + cursorX + 'px, ' + cursorY + 'px)';
        _cursorEl.style.height = cursorHeight + 'px';

        // Auto-scroll when the system changes
        if (mPos.system_idx !== _currentSystemIdx) {
            _currentSystemIdx = mPos.system_idx;
            // Scroll the cursor into view with smooth animation
            // Use a small timeout to let the cursor position update first
            setTimeout(function() {
                _cursorEl.scrollIntoView({ behavior: 'smooth', block: 'center' });
            }, 50);
        }
    }

    // ─── Click-to-seek ─────────────────────────────────────────────────

    document.addEventListener('DOMContentLoaded', function() {
        var container = document.getElementById('score-container');
        if (!container) return;

        container.addEventListener('click', function(e) {
            if (!_isInitialized || _measures.length === 0) return;

            // Get click position relative to the container
            var rect = container.getBoundingClientRect();
            var clickX = e.clientX - rect.left;
            var clickY = e.clientY - rect.top;

            // Convert from CSS pixels to SVG user units
            var scale = getScaleFactor();
            var svgX = clickX / scale;
            var svgY = clickY / scale;

            // Find which system was clicked (by Y coordinate)
            var clickedSystemIdx = -1;
            for (var s = 0; s < _systems.length; s++) {
                var sys = _systems[s];
                if (svgY >= sys.y - 10 && svgY <= sys.y + sys.height + 30) {
                    clickedSystemIdx = s;
                    break;
                }
            }
            if (clickedSystemIdx < 0) return;

            // Find which measure in that system was clicked (by X coordinate)
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

            // Find the timemap entry for this measure
            var tmEntry = null;
            for (var t = 0; t < _timemap.length; t++) {
                if (_timemap[t].original_index === clickedMeasure.measure_idx) {
                    tmEntry = _timemap[t];
                    break;
                }
            }
            if (!tmEntry) return;

            // Compute proportional offset within the measure
            var offsetRatio = (svgX - clickedMeasure.x) / clickedMeasure.width;
            if (offsetRatio < 0) offsetRatio = 0;
            if (offsetRatio > 1) offsetRatio = 1;

            var seekTimeMs = tmEntry.timestamp_ms + offsetRatio * tmEntry.duration_ms;

            // Report to native
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

    // MARK: - Coordinator for WKScriptMessageHandler

    class Coordinator: NSObject, WKScriptMessageHandler {
        var playbackManager: PlaybackManager

        init(playbackManager: PlaybackManager) {
            self.playbackManager = playbackManager
        }

        func userContentController(_ userContentController: WKUserContentController,
                                   didReceive message: WKScriptMessage) {
            guard message.name == "playback",
                  let body = message.body as? [String: Any],
                  let action = body["action"] as? String else {
                return
            }

            if action == "seek", let timeMs = body["timeMs"] as? Double {
                playbackManager.seek(to: timeMs)
            }
        }
    }
}

#Preview {
    SheetMusicView()
        .environmentObject(PlaybackManager(audioSessionManager: AudioSessionManager()))
        .environmentObject(MidiSettings())
}
