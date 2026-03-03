import SwiftUI
import WebKit

/// Displays rendered sheet music SVG using a WKWebView with playback cursor support.
struct SheetMusicView: View {
    @EnvironmentObject var playbackManager: PlaybackManager
    @EnvironmentObject var midiSettings: MidiSettings
    @EnvironmentObject var feedbackManager: FeedbackManager
    /// Called when a score is loaded with (svgContent, playbackMapJson) for use in the report overlay.
    var onScoreLoaded: (String, String) -> Void = { _, _ in }
    @State private var svgContent: String?
    @State private var playbackMapJson: String?
    @State private var isLoading = true
    @State private var errorMessage: String?
    @State private var lastRenderedWidth: CGFloat = 0
    @State private var lastOptionsJson: String = ""
    /// Monotonically increasing counter to detect stale loadScore results.
    @State private var loadGeneration: Int = 0
    /// Monotonically increasing counter to detect stale regenerateMidi results.
    @State private var midiGeneration: Int = 0

    /// Whether the current file is externally opened (via document picker).
    private var isExternalFile: Bool {
        midiSettings.selectedFileUrl.hasPrefix("external://")
    }

    /// Whether the current file is from an SBF bundle.
    private var isSbfFile: Bool {
        midiSettings.selectedFileUrl.hasPrefix("mbk://")
    }

    /// Extract the filename from the selected file URL.
    private var currentFile: String {
        let url = midiSettings.selectedFileUrl
        if url.hasPrefix("external://") {
            return String(url.dropFirst("external://".count))
        }
        if url.hasPrefix("file://sheetmusic/") {
            return String(url.dropFirst("file://sheetmusic/".count))
        }
        if url.hasPrefix("mbk://") {
            return url.components(separatedBy: "/").last ?? ""
        }
        return url.components(separatedBy: "/").last ?? MidiSettings.defaultLandingFile
    }

    /// Resolve the current selectedFileUrl to (fileData, extension), or nil on failure.
    private func resolveCurrentFileData() -> (Data, String)? {
        let fileUrl = midiSettings.selectedFileUrl

        if fileUrl.hasPrefix("external://") {
            guard let data = midiSettings.externalFileData else { return nil }
            let filename = String(fileUrl.dropFirst("external://".count))
            let ext = (filename as NSString).pathExtension
            return (data, ext)
        }

        if fileUrl.hasPrefix("mbk://") {
            // mbk://<bookId>/music/piece.musicxml
            let withoutScheme = String(fileUrl.dropFirst("mbk://".count))
            guard let slashIdx = withoutScheme.firstIndex(of: "/") else { return nil }
            let bookId = String(withoutScheme[withoutScheme.startIndex..<slashIdx])
            guard let bundle = midiSettings.activeBundles[bookId] else { return nil }
            guard let localURL = bundle.resolveToLocalURL(fileUrl) else { return nil }
            guard let data = try? Data(contentsOf: localURL) else { return nil }
            let ext = localURL.pathExtension
            return (data, ext)
        }

        // Bundled file: file://sheetmusic/<name>
        let filename = currentFile
        let ext  = (filename as NSString).pathExtension
        let name = (filename as NSString).deletingPathExtension
        let url: URL
        if let u = Bundle.main.url(forResource: name, withExtension: ext, subdirectory: "sheetmusic") {
            url = u
        } else if let u = Bundle.main.url(forResource: name, withExtension: ext) {
            url = u
        } else {
            return nil
        }
        guard let data = try? Data(contentsOf: url) else { return nil }
        return (data, ext)
    }

    /// True when the current selection is an mbk bundle that has no pieces.
    private var isInvalidEmptyBundle: Bool {
        guard isSbfFile else { return false }
        let url = midiSettings.selectedFileUrl
        let withoutScheme = String(url.dropFirst("mbk://".count))
        let bookId = withoutScheme.split(separator: "/", maxSplits: 1).first.map(String.init) ?? ""
        guard !bookId.isEmpty, let bundle = midiSettings.activeBundles[bookId] else { return false }
        return bundle.allPieces.isEmpty
    }

    var body: some View {
        GeometryReader { geometry in
            VStack(spacing: 0) {
                // Score display
                if isInvalidEmptyBundle {
                    VStack(spacing: 12) {
                        Image(systemName: "exclamationmark.triangle")
                            .font(.system(size: 40))
                            .foregroundStyle(.secondary)
                        Text("This bundle contains no music.")
                            .font(.callout)
                            .foregroundStyle(.secondary)
                            .multilineTextAlignment(.center)
                    }
                    .padding()
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                } else if isLoading {
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
                playbackManager.showCursorEnabled = midiSettings.showCursor
                loadScore(width: geometry.size.width)
            }
            .onChange(of: midiSettings.selectedFileUrl) { _ in
                loadScore(width: geometry.size.width)
            }
            .onChange(of: midiSettings.externalFileVersion) { _ in
                // Force reload even when the same external filename is reopened.
                if isExternalFile {
                    loadScore(width: geometry.size.width)
                }
            }
            .onChange(of: midiSettings.activeBundles) { _ in
                if isSbfFile { loadScore(width: geometry.size.width) }
            }
            .onChange(of: midiSettings.transpose) { _ in
                loadScore(width: geometry.size.width)
            }
            .onChange(of: midiSettings.scoreRenderingMode) { _ in loadScore(width: geometry.size.width) }
            .onChange(of: midiSettings.staffStavesOption) { _ in loadScore(width: geometry.size.width) }
            .onChange(of: midiSettings.staffStavesList) { _ in loadScore(width: geometry.size.width) }
            .onChange(of: midiSettings.jianpuStaffNumber) { _ in loadScore(width: geometry.size.width) }
            .onChange(of: geometry.size.width) { newWidth in
                // Re-render when width changes (e.g. device rotation)
                if abs(newWidth - lastRenderedWidth) > 10 {
                    loadScore(width: newWidth)
                }
            }
            // ── Accompaniment toggles → regenerate MIDI (track selection changed) ──
            .onChange(of: midiSettings.includeMelody) { _ in regenerateMidi() }
            .onChange(of: midiSettings.includePiano) { _ in regenerateMidi() }
            .onChange(of: midiSettings.includeBass) { _ in regenerateMidi() }
            .onChange(of: midiSettings.includeStrings) { _ in regenerateMidi() }
            .onChange(of: midiSettings.includeDrums) { _ in regenerateMidi() }
            .onChange(of: midiSettings.includeMetronome) { _ in regenerateMidi() }
            .onChange(of: midiSettings.energy) { _ in regenerateMidi() }
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
            .onChange(of: midiSettings.showCursor) { newShowCursor in
                playbackManager.showCursorEnabled = newShowCursor
            }
            // ── Feedback cursor color ────────────────────────────────────────
            .onChange(of: feedbackManager.state) { newState in
                let hex = newState.cursorColor
                playbackManager.webView?.evaluateJavaScript(
                    "if (typeof setCursorColor === 'function') { setCursorColor('\(hex)'); }",
                    completionHandler: nil
                )
            }
        }
    }

    private func loadScore(width: CGFloat) {
        // Bump the generation counter so any in-flight load is discarded.
        loadGeneration += 1
        let thisGeneration = loadGeneration

        isLoading = true
        errorMessage = nil
        svgContent = nil
        playbackMapJson = nil
        lastRenderedWidth = width

        // Stop any previous playback immediately so the user never hears the
        // old piece while the new one is loading.
        playbackManager.stop()

        let pageWidth = Double(width)
        let optionsJson = midiSettings.toJson()
        lastOptionsJson = optionsJson
        let transposeVal = Int32(midiSettings.transpose)

        let partsFilter: String?
        if midiSettings.scoreRenderingMode == "staff" {
            if midiSettings.staffStavesOption == "all" {
                partsFilter = nil
            } else {
                let list = midiSettings.staffStavesList.split(separator: ",")
                    .compactMap { seg -> Int? in
                        let s = seg.filter { $0.isNumber }
                        return s.isEmpty ? nil : Int(s)
                    }
                let sorted = Array(Set(list)).sorted()
                partsFilter = sorted.isEmpty ? nil : sorted.map(String.init).joined(separator: ",")
            }
        } else {
            partsFilter = midiSettings.jianpuStaffNumber.isEmpty ? nil : midiSettings.jianpuStaffNumber
        }

        // Capture resolved file data on the main thread before going to background.
        guard let (resolvedData, ext) = resolveCurrentFileData() else {
            isLoading = false
            errorMessage = "No music file selected"
            return
        }
        let data = resolvedData
        let filename = currentFile

        DispatchQueue.global(qos: .userInitiated).async {
            guard !filename.isEmpty else {
                DispatchQueue.main.async {
                    guard thisGeneration == loadGeneration else { return }
                    isLoading = false
                    errorMessage = "No music file selected"
                }
                return
            }

            // Render SVG from bytes (works for external, bundled, and .mbk files)
            let svg = ScoreLib.renderData(data, extension: ext, pageWidth: pageWidth, transpose: transposeVal, partsFilter: partsFilter)

            // Generate playback map (same partsFilter as SVG so cursor height/position match)
            let pmap = ScoreLib.playbackMap(data, extension: ext, pageWidth: pageWidth, transpose: transposeVal, partsFilter: partsFilter)

            // Generate note timeline for real-time feedback (voice 1, part 0)
            let timelineJson = ScoreLib.noteTimeline(data, extension: ext, transpose: transposeVal)

            // Render audio (MIDI→WAV) for playback with current settings
            let audio = ScoreLib.renderAudio(data, extension: ext, optionsJson: optionsJson)

            DispatchQueue.main.async {
                // Discard this result if a newer loadScore was started while we were working.
                guard thisGeneration == loadGeneration else { return }

                isLoading = false
                if let svg = svg {
                    svgContent = svg
                    playbackMapJson = pmap
                    onScoreLoaded(svg, pmap ?? "")

                    // Load the note timeline into FeedbackManager.
                    if let json = timelineJson,
                       let data = json.data(using: .utf8),
                       let events = try? JSONDecoder().decode([NoteEvent].self, from: data) {
                        feedbackManager.loadTimeline(events)
                    }

                    // Prepare the playback manager with the rendered audio
                    if let wavData = audio {
                        playbackManager.prepareAudio(wavData)
                    }
                } else {
                    errorMessage = "Failed to render '\(filename)'"
                }
            }
        }
    }

    /// Regenerate only the audio when accompaniment/energy settings change
    /// (no need to re-render SVG).
    private func regenerateMidi() {
        let optionsJson = midiSettings.toJson()
        guard optionsJson != lastOptionsJson else { return }
        lastOptionsJson = optionsJson

        // Bump the generation counter so any in-flight regen is discarded.
        midiGeneration += 1
        let thisMidiGen = midiGeneration

        guard let (data, ext) = resolveCurrentFileData(), !currentFile.isEmpty else { return }

        DispatchQueue.global(qos: .userInitiated).async {
            let audio = ScoreLib.renderAudio(data, extension: ext, optionsJson: optionsJson)

            DispatchQueue.main.async {
                // Discard if a newer regeneration was started while we were working.
                guard thisMidiGen == midiGeneration else { return }
                if let wavData = audio {
                    playbackManager.prepareAudio(wavData)
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

    static func dismantleUIView(_ webView: WKWebView, coordinator: Coordinator) {
        // Remove the script message handler to break the retain cycle:
        // WebView -> UserContentController -> Coordinator -> PlaybackManager.
        // Without this, the WKWebView and Coordinator are never deallocated.
        webView.configuration.userContentController.removeScriptMessageHandler(forName: "playback")
        coordinator.playbackManager.webView = nil
    }

    func updateUIView(_ webView: WKWebView, context: Context) {
        // Update coordinator's reference to playback manager
        context.coordinator.playbackManager = playbackManager

        // Ensure the webView reference is current
        playbackManager.webView = webView

        // Only reload the WebView when the SVG or playback map actually changed.
        // Without this guard, SwiftUI calls updateUIView on every body re-evaluation
        // (e.g. 60fps during playback due to @Published changes), which would
        // rebuild + reload the entire HTML document each time — destroying the
        // JavaScript state (cursor, playback map) and wasting CPU/memory.
        let svgHash = svgString.hashValue
        let pmapHash = (playbackMapJson ?? "").hashValue
        guard svgHash != context.coordinator.lastLoadedSvgHash
           || pmapHash != context.coordinator.lastLoadedPmapHash else {
            return
        }
        context.coordinator.lastLoadedSvgHash = svgHash
        context.coordinator.lastLoadedPmapHash = pmapHash

        let html = Self.buildHTML(
            svg: svgString,
            playbackMapJson: playbackMapJson,
            cursorBarVisible: playbackManager.showCursorEnabled
        )
        // Use the bundle resource URL as base so @font-face relative paths
        // (e.g. "Fonts/Lora-Regular.ttf") resolve to the bundled font files.
        webView.loadHTMLString(html, baseURL: Bundle.main.resourceURL)
    }

    /// Build the complete HTML document with SVG, cursor div, and playback JavaScript.
    static func buildHTML(svg: String, playbackMapJson: String?, cursorBarVisible: Bool = true) -> String {
        // Escape "</script>" sequences so they don't prematurely close the
        // <script> block when the JSON or SVG contains that literal string.
        let pmapJS = (playbackMapJson ?? "null").replacingOccurrences(of: "</", with: "<\\/")
        // Strip any <script> tags from SVG to prevent XSS from external MusicXML files.
        let safeSvg = svg.replacingOccurrences(
            of: "<script[^>]*>[\\s\\S]*?</script>",
            with: "",
            options: .regularExpression
        )
        return """
        <!DOCTYPE html>
        <html>
        <head>
        <meta name="viewport" content="width=device-width, initial-scale=1.0, maximum-scale=3.0, user-scalable=yes">
        <style>
            @font-face {
                font-family: 'Lora';
                src: url('Fonts/Lora-Regular.ttf') format('truetype');
                font-weight: 100 900;
                font-style: normal;
            }
            @font-face {
                font-family: 'Lora';
                src: url('Fonts/Lora-Italic.ttf') format('truetype');
                font-weight: 100 900;
                font-style: italic;
            }
            @font-face {
                font-family: 'LXGW WenKai';
                src: url('Fonts/LXGWWenKai-Regular.ttf') format('truetype');
                font-weight: normal;
                font-style: normal;
            }
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
            \(safeSvg)
            <div id="cursor"></div>
        </div>
        <script>
        \(Self.cursorJavaScript)
        // Apply cursor bar visibility from the native setting before init
        _cursorBarVisible = \(cursorBarVisible);
        // Initialize playback map and position cursor at the beginning
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
    // Cursor animation runs entirely inside the WebView via
    // requestAnimationFrame — NO cross-process IPC during playback.
    // Swift sends one-shot commands: startCursorAnimation / stopCursorAnimation.

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

    // ─── Feedback cursor color ─────────────────────────────────────────
    var _cursorColor = 'rgb(234,107,36)';  // default orange

    /// Set the cursor bar color for real-time feedback.
    /// Pass a CSS color string: '#4CAF50', '#FFC107', '#F44336', or 'rgb(234,107,36)'.
    function setCursorColor(color) {
        _cursorColor = color;
        if (_cursorEl) {
            _cursorEl.style.backgroundColor = color;
        }
    }

    // ─── Self-driven animation state ──────────────────────────────────
    var _animating = false;       // true while requestAnimationFrame loop is active
    var _animStartWallMs = 0;     // performance.now() at which animation started
    var _animStartMusicMs = 0;    // music-time (ms) at which animation started
    var _animSpeed = 1.0;         // playback speed multiplier
    var _animFrameId = null;      // requestAnimationFrame handle

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

    /// Called once by Swift when playback starts (or after seek/speed change).
    /// The WebView then drives the cursor via requestAnimationFrame — zero IPC.
    function startCursorAnimation(musicMs, speed) {
        _animStartMusicMs = musicMs;
        _animStartWallMs = performance.now();
        _animSpeed = speed;
        showCursor();
        if (!_animating) {
            _animating = true;
            _animFrameId = requestAnimationFrame(_animLoop);
        }
    }

    /// Called by Swift on pause/stop.  Positions the cursor and halts animation.
    function stopCursorAnimation(musicMs) {
        _animating = false;
        if (_animFrameId) {
            cancelAnimationFrame(_animFrameId);
            _animFrameId = null;
        }
        if (musicMs <= 0) {
            moveCursor(0);
        } else {
            moveCursor(musicMs);
        }
    }

    /// Internal animation loop — runs via requestAnimationFrame, no IPC.
    function _animLoop() {
        if (!_animating) return;
        var elapsed = performance.now() - _animStartWallMs;
        var musicMs = _animStartMusicMs + elapsed * _animSpeed;
        moveCursor(musicMs);
        _animFrameId = requestAnimationFrame(_animLoop);
    }

    var _cursorBarVisible = true;  // whether the orange bar is drawn

    function showCursor() {
        if (_cursorEl) {
            _cursorEl.style.display = 'block';
            _cursorEl.style.opacity = _cursorBarVisible ? '0.85' : '0';
        }
    }

    function hideCursor() {
        if (_cursorEl) _cursorEl.style.display = 'none';
        _currentSystemIdx = -1;
    }

    /// Toggle the orange cursor bar on/off without affecting position
    /// tracking or auto-scroll.  When hidden, moveCursor() still runs
    /// (so the score scrolls with the music) but the bar is invisible.
    function setCursorBarVisible(visible) {
        _cursorBarVisible = visible;
        if (_cursorEl) {
            _cursorEl.style.opacity = visible ? '0.85' : '0';
        }
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
                var denom = np[hi][0] - np[lo][0];
                var segRatio = denom > 0 ? (ratio - np[lo][0]) / denom : 0;
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
            var offsetRatio = clickedMeasure.width > 0
                ? (svgX - clickedMeasure.x) / clickedMeasure.width : 0;
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
        /// Track the last loaded content to avoid redundant WebView reloads.
        var lastLoadedSvgHash: Int = 0
        var lastLoadedPmapHash: Int = 0

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
        .environmentObject(FeedbackManager())
}
