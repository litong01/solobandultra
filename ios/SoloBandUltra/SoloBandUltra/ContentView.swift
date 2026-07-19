import SwiftUI
import UniformTypeIdentifiers

struct ContentView: View {
    @EnvironmentObject var audioSessionManager: AudioSessionManager
    @EnvironmentObject var playbackManager: PlaybackManager
    @EnvironmentObject var midiSettings: MidiSettings
    @EnvironmentObject var authManager: AuthManager
    @EnvironmentObject var feedbackManager: FeedbackManager
    @EnvironmentObject var choirManager: ChoirManager
    @EnvironmentObject var appLanguage: AppLanguage
    @Environment(\.scenePhase) private var scenePhase

    @State private var showSettings = false
    @State private var showFilePicker = false
    @State private var isDownloading = false
    @State private var downloadError: String?
    @State private var clipboardHasUrl = false
    @State private var showPdfViewer = false
    @State private var showReport = false
    @State private var showChoir = false
    @State private var reportSvgContent: String?
    @State private var reportPlaybackMapJson: String?

    // MARK: - Bundle navigation helpers

    /// The currently active bundle, if any.
    private var activeBundle: BookBundle? { midiSettings.activeBundle }

    /// Unlocked pieces in the active bundle.
    private var unlockedPieces: [BookPiece] { activeBundle?.unlockedPieces ?? [] }

    /// Index of the current piece within unlockedPieces.
    private var currentPieceIndex: Int? {
        guard let bundle = activeBundle else { return nil }
        let url = midiSettings.selectedFileUrl
        let prefix = "mbk://\(bundle.bookId)/"
        guard url.hasPrefix(prefix) else { return nil }
        let xml = String(url.dropFirst(prefix.count))
        return unlockedPieces.firstIndex(where: { $0.xml == xml })
    }

    private var canGoPrev: Bool {
        guard let idx = currentPieceIndex else { return false }
        return idx > 0
    }

    private var canGoNext: Bool {
        guard let idx = currentPieceIndex else { return false }
        return idx < unlockedPieces.count - 1
    }

    private func selectPiece(_ piece: BookPiece) {
        guard let bundle = activeBundle else { return }
        playbackManager.stop()
        midiSettings.selectedFileUrl = "mbk://\(bundle.bookId)/\(piece.xml)"
    }

    private func goToPrev() {
        guard let idx = currentPieceIndex, idx > 0 else { return }
        selectPiece(unlockedPieces[idx - 1])
    }

    private func goToNext() {
        guard let idx = currentPieceIndex, idx < unlockedPieces.count - 1 else { return }
        selectPiece(unlockedPieces[idx + 1])
    }

    /// 1-based PDF page for the currently selected piece.
    private var currentPdfPage: Int {
        guard let bundle = activeBundle else { return 1 }
        let url = midiSettings.selectedFileUrl
        let prefix = "mbk://\(bundle.bookId)/"
        guard url.hasPrefix(prefix) else { return 1 }
        let xml = String(url.dropFirst(prefix.count))
        return bundle.pdfPage(forXml: xml)
    }

    var body: some View {
        VStack(spacing: 0) {
            // Compact top bar: icon left, menu right
            HStack {
                Image("AppIconImage")
                    .resizable()
                    .scaledToFit()
                    .frame(width: 28, height: 28)
                    .clipShape(RoundedRectangle(cornerRadius: 6, style: .continuous))
                    .help(L10n.string("content_desc_mysoloband", language: appLanguage.preferredCode))

                Spacer()

                Menu {
                    // ── Gated actions ──
                    Button(action: { requireAuth(for: .openFile) }) {
                        Label(L10n.string("menu_open_file", language: appLanguage.preferredCode), systemImage: "doc.badge.plus")
                    }
                    Button(action: { requireAuth(for: .pasteLink) }) {
                        Label(L10n.string("menu_paste_link", language: appLanguage.preferredCode), systemImage: "doc.on.clipboard")
                    }
                    .disabled(!clipboardHasUrl)
                    Button(action: { requireAuth(for: .showChoir) }) {
                        Label(L10n.string("menu_choir", language: appLanguage.preferredCode), systemImage: "person.3")
                    }
                    Button(action: { requireAuth(for: .showSettings) }) {
                        Label(L10n.string("menu_settings", language: appLanguage.preferredCode), systemImage: "gear")
                    }

                    Divider()

                    // ── Login / Logout ──
                    if authManager.isAuthenticated {
                        Button(action: { authManager.logout() }) {
                            Label(L10n.string("menu_sign_out", language: appLanguage.preferredCode), systemImage: "rectangle.portrait.and.arrow.right")
                        }
                    } else {
                        Button(action: { authManager.login() }) {
                            Label(L10n.string("menu_sign_in", language: appLanguage.preferredCode), systemImage: "person.crop.circle.badge.plus")
                        }
                    }
                } label: {
                    Image(systemName: "ellipsis.circle")
                        .font(.title3)
                }
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 6)

            // Sheet music display area
            SheetMusicView(onScoreLoaded: { svg, pmap in
                reportSvgContent = svg
                reportPlaybackMapJson = pmap
            })
            .frame(maxWidth: .infinity, maxHeight: .infinity)

            Divider()

            // Playback controls (when in choir, bar sends commands to all; otherwise local only)
            PlaybackControlBar(
                isPlaying: $playbackManager.isPlaying,
                bundleActive: activeBundle != nil,
                showPrevNext: activeBundle != nil && (activeBundle?.unlockedPieces.count ?? 0) > 1,
                canGoPrev: canGoPrev,
                canGoNext: canGoNext,
                onPrev:     choirManager.isJoined ? { choirManager.sendCommand("prev") } : goToPrev,
                onPlayPause: choirManager.isJoined ? { choirManager.sendCommand(playbackManager.isPlaying ? "pause" : "play") } : { playbackManager.togglePlayPause() },
                onStop:      choirManager.isJoined ? { choirManager.sendCommand("stop") } : { playbackManager.stop() },
                onNext:     choirManager.isJoined ? { choirManager.sendCommand("next") } : goToNext,
                onSettings:  { requireAuth(for: .showSettings) },
                showBook: activeBundle?.hasPdfFile == true,
                onBook:      { showPdfViewer = true },
                feedbackEnabled: midiSettings.includeFeedback,
                reportAvailable: feedbackManager.report != nil,
                onReport:    { showReport = true }
            )
            .padding(.horizontal)
            .padding(.vertical, 12)
            .background(.ultraThinMaterial)
        }
        .fullScreenCover(isPresented: $showPdfViewer) {
            if let bundle = activeBundle, bundle.hasPdfFile {
                PdfViewerView(
                    bundle: bundle,
                    startPage: currentPdfPage,
                    isPresented: $showPdfViewer
                ) { piece in
                    selectPiece(piece)
                }
            }
        }
        .sheet(isPresented: $showReport) {
            if let report = feedbackManager.report {
                FeedbackReportView(
                    report: report,
                    svgContent: reportSvgContent,
                    playbackMapJson: reportPlaybackMapJson
                )
            }
        }
        .onAppear {
            // Choir: when a scheduled command is received, run the action at execute_at
            choirManager.onScheduledCommand = { [weak playbackManager, self] cmd, executeAtMs in
                guard let pm = playbackManager else { return }
                let nowMs = Int64(Date().timeIntervalSince1970 * 1000)
                let delaySec = max(0, Double(executeAtMs - nowMs) / 1000.0)
                print("[Choir] onScheduledCommand cmd=\(cmd) executeAtMs=\(executeAtMs) delaySec=\(delaySec)")
                DispatchQueue.main.asyncAfter(deadline: .now() + delaySec) {
                    print("[Choir] executing cmd=\(cmd)")
                    switch cmd {
                    case "play": pm.play()
                    case "pause": pm.pause()
                    case "stop": pm.stop()
                    case "prev": self.goToPrev()
                    case "next": self.goToNext()
                    default: break
                    }
                }
            }
            // Wire FeedbackManager time updates to PlaybackManager's poll tick.
            playbackManager.onTimeUpdate = { [weak feedbackManager] ms in
                feedbackManager?.update(musicMs: ms)
            }
            // Wire mic tap through PlaybackManager's single AVAudioEngine so both
            // mic capture and audio output share one engine (avoids session conflicts).
            feedbackManager.tapInstaller = { [weak playbackManager] handler in
                playbackManager?.installMicrophoneTap(handler: handler)
            }
            feedbackManager.tapRemover = { [weak playbackManager] in
                playbackManager?.removeMicrophoneTap()
            }
            playbackManager.beforePlaybackStarts = { [weak feedbackManager] in
                guard midiSettings.includeFeedback else { return }
                feedbackManager?.installTapIfReady()
            }
            playbackManager.useDuplexForFeedback = midiSettings.includeFeedback
            if midiSettings.includeFeedback {
                feedbackManager.startListening()
            }
        }
        .onChange(of: midiSettings.includeFeedback) { includeFeedback in
            playbackManager.useDuplexForFeedback = includeFeedback
            if includeFeedback {
                // Request permission and install tap handler now so duplex path is ready when user presses Play.
                feedbackManager.startListening()
            } else {
                feedbackManager.stopListening()
            }
        }
        .onChange(of: playbackManager.isPlaying) { isNowPlaying in
            guard midiSettings.includeFeedback else { return }
            if isNowPlaying {
                feedbackManager.startListening()
            } else {
                feedbackManager.stopListening()
            }
        }
        .overlay {
            if isDownloading {
                ZStack {
                    Color.black.opacity(0.3).ignoresSafeArea()
                    VStack(spacing: 12) {
                        ProgressView()
                            .scaleEffect(1.5)
                        Text("Downloading…")
                            .font(.callout)
                            .foregroundStyle(.white)
                    }
                    .padding(24)
                    .background(.ultraThinMaterial)
                    .clipShape(RoundedRectangle(cornerRadius: 12))
                }
            }
        }
        .alert("Paste Error", isPresented: .init(
            get: { downloadError != nil },
            set: { if !$0 { downloadError = nil } }
        )) {
            Button("OK") { downloadError = nil }
        } message: {
            Text(downloadError ?? "")
        }
        .alert("Bundle Error", isPresented: .init(
            get: { midiSettings.errorMessage != nil },
            set: { if !$0 { midiSettings.errorMessage = nil } }
        )) {
            Button("OK") { midiSettings.errorMessage = nil }
        } message: {
            Text(midiSettings.errorMessage ?? "")
        }
        .fileImporter(
            isPresented: $showFilePicker,
            allowedContentTypes: [.xml, .data, .zip],
            allowsMultipleSelection: false
        ) { result in
            switch result {
            case .success(let urls):
                guard let url = urls.first else { return }
                guard url.startAccessingSecurityScopedResource() else { return }
                defer { url.stopAccessingSecurityScopedResource() }

                guard let data = try? Data(contentsOf: url) else { return }
                let filename = url.lastPathComponent
                let ext = (filename as NSString).pathExtension.lowercased()

                if ext == "mbk" {
                    Task.detached(priority: .userInitiated) {
                        do {
                            let bundle = try SoloBandUltraApp.extractAndParseMbk(data: data)
                            await MainActor.run {
                                midiSettings.activeBundles[bundle.bookId] = bundle
                                midiSettings.selectedSourceId = "mbk:\(bundle.bookId)"
                                if bundle.allPieces.isEmpty {
                                    midiSettings.selectedFileUrl = "mbk://\(bundle.bookId)/"
                                    midiSettings.errorMessage = "This bundle contains no music."
                                } else if let first = bundle.unlockedPieces.first ?? bundle.allPieces.first {
                                    midiSettings.selectedFileUrl = "mbk://\(bundle.bookId)/\(first.xml)"
                                }
                                midiSettings.saveToDisk()
                            }
                        } catch {
                            let msg = "Could not open \"\(filename)\": \(error.localizedDescription)"
                            await MainActor.run { midiSettings.errorMessage = msg }
                        }
                    }
                } else if ext == "musicxml" || ext == "mxl" || ext == "xml" {
                    midiSettings.externalFileData = data
                    midiSettings.externalFileName = filename
                    midiSettings.externalFileVersion += 1
                    midiSettings.selectedSourceId = "external"
                    midiSettings.selectedFileUrl = "external://\(filename)"
                }
            case .failure:
                break
            }
        }
        .overlay {
            if showSettings {
                BottomSheetOverlay(isPresented: $showSettings) {
                    SettingsSheet(midiSettings: midiSettings, isPresented: $showSettings, appLanguage: appLanguage)
                }
            }
        }
        .overlay {
            if showChoir {
                BottomSheetOverlay(isPresented: $showChoir) {
                    ChoirSheet(isPresented: $showChoir, choirManager: choirManager)
                }
            }
        }
        .animation(.spring(response: 0.35, dampingFraction: 0.86), value: showSettings)
        .animation(.spring(response: 0.35, dampingFraction: 0.86), value: showChoir)
        .onAppear { checkClipboardForUrl() }
        .onChange(of: scenePhase) { phase in
            if phase == .active { checkClipboardForUrl() }
        }
        // ── Execute deferred action after successful login ──
        .onChange(of: authManager.isAuthenticated) { authenticated in
            guard authenticated, let action = authManager.pendingAction else { return }
            authManager.pendingAction = nil
            executePendingAction(action)
        }
    }

    // MARK: - Auth gating

    /// If authenticated, execute the action immediately; otherwise, trigger login
    /// and defer the action until authentication succeeds.
    private func requireAuth(for action: PendingAuthAction) {
        if authManager.isAuthenticated {
            executePendingAction(action)
        } else {
            authManager.login(then: action)
        }
    }

    /// Execute a previously deferred action (called after successful login or immediately).
    private func executePendingAction(_ action: PendingAuthAction) {
        switch action {
        case .showSettings:
            showSettings = true
        case .showChoir:
            showChoir = true
        case .openFile:
            showFilePicker = true
        case .pasteLink:
            pasteFromClipboard()
        case .loadExternal(let data, let filename):
            midiSettings.externalFileData = data
            midiSettings.externalFileName = filename
            midiSettings.externalFileVersion += 1
            midiSettings.selectedSourceId = "external"
            midiSettings.selectedFileUrl = "external://\(filename)"
        }
    }

    // MARK: - Clipboard detection

    /// Check if the clipboard probably contains a web URL (without triggering the paste prompt).
    private func checkClipboardForUrl() {
        Task {
            do {
                let patterns = try await UIPasteboard.general.detectedPatterns(for: [\.probableWebURL])
                await MainActor.run {
                    clipboardHasUrl = patterns.contains(\.probableWebURL)
                }
            } catch {
                await MainActor.run {
                    clipboardHasUrl = false
                }
            }
        }
    }

    // MARK: - Paste Link

    /// Read clipboard, validate as a MusicXML URL, download, and load.
    private func pasteFromClipboard() {
        // Prevent overlapping downloads
        guard !isDownloading else { return }

        guard let clipString = UIPasteboard.general.string?.trimmingCharacters(in: .whitespacesAndNewlines),
              !clipString.isEmpty,
              clipString.count <= 2048,
              let url = URL(string: clipString),
              let scheme = url.scheme?.lowercased(),
              scheme == "http" || scheme == "https" else {
            return // Button should be disabled; silent guard only
        }

        let pathExt = (url.lastPathComponent as NSString).pathExtension.lowercased()
        guard pathExt == "musicxml" || pathExt == "mxl" || pathExt == "xml" else {
            downloadError = "URL doesn't point to a MusicXML file (.musicxml, .mxl, or .xml)."
            return
        }

        isDownloading = true
        downloadError = nil
        let filename = url.lastPathComponent

        URLSession.shared.dataTask(with: url) { data, response, error in
            DispatchQueue.main.async {
                isDownloading = false

                if let error = error {
                    downloadError = "Download failed: \(error.localizedDescription)"
                    return
                }

                guard let httpResponse = response as? HTTPURLResponse,
                      (200...299).contains(httpResponse.statusCode) else {
                    downloadError = "Download failed: server returned an error."
                    return
                }

                guard let data = data, !data.isEmpty else {
                    downloadError = "Downloaded file is empty."
                    return
                }

                midiSettings.externalFileData = data
                midiSettings.externalFileName = filename
                midiSettings.externalFileVersion += 1
                midiSettings.selectedSourceId = "external"
                midiSettings.selectedFileUrl = "external://\(filename)"
            }
        }.resume()
    }
}

// MARK: - Playback Control Bar

struct PlaybackControlBar: View {
    @Binding var isPlaying: Bool
    /// True when a bundle is loaded.
    var bundleActive: Bool = false
    /// Show prev/next when bundle has more than one piece.
    var showPrevNext: Bool = false
    var canGoPrev: Bool = false
    var canGoNext: Bool = false
    var onPrev: (() -> Void)?
    let onPlayPause: () -> Void
    let onStop: () -> Void
    var onNext: (() -> Void)?
    let onSettings: () -> Void
    /// Show book button only when bundle has a PDF file.
    var showBook: Bool = false
    var onBook: (() -> Void)?
    /// Whether the Feedback toggle is on (gates report button visibility).
    var feedbackEnabled: Bool = false
    /// Whether a completed performance report is available to show.
    var reportAvailable: Bool = false
    var onReport: (() -> Void)?

    var body: some View {
        HStack(spacing: 0) {
            Spacer()

            // ── Bundle navigation (prev/next only when more than one piece) ──
            if showPrevNext {
                Button(action: { onPrev?() }) {
                    Image(systemName: "backward.end.fill")
                        .font(.title3)
                        .foregroundStyle(canGoPrev ? .primary : .tertiary)
                }
                .disabled(!canGoPrev)
                .frame(minWidth: 40)
            }

            // Stop
            Button(action: onStop) {
                Image(systemName: "stop.fill")
                    .font(.title2)
                    .foregroundStyle(.primary)
            }
            .frame(minWidth: 44)

            // Play / Pause (large)
            Button(action: onPlayPause) {
                Image(systemName: isPlaying ? "pause.circle.fill" : "play.circle.fill")
                    .font(.system(size: 52))
                    .foregroundStyle(.tint)
            }
            .padding(.horizontal, 16)

            // Next (bundle; only when more than one piece)
            if showPrevNext {
                Button(action: { onNext?() }) {
                    Image(systemName: "forward.end.fill")
                        .font(.title3)
                        .foregroundStyle(canGoNext ? .primary : .tertiary)
                }
                .disabled(!canGoNext)
                .frame(minWidth: 40)
            }

            // Separator
            if bundleActive {
                Divider()
                    .frame(height: 24)
                    .padding(.horizontal, 6)
            }

            // Settings
            Button(action: onSettings) {
                Image(systemName: "gear")
                    .font(.title2)
                    .foregroundStyle(.primary)
            }
            .frame(minWidth: 44)

            // Book (bundle PDF viewer; only when bundle has a PDF file)
            if showBook {
                Button(action: { onBook?() }) {
                    Image(systemName: "book.fill")
                        .font(.title2)
                        .foregroundStyle(.tint)
                }
                .frame(minWidth: 44)
            }

            // Report (post-performance feedback summary)
            if feedbackEnabled && reportAvailable {
                Button(action: { onReport?() }) {
                    Image(systemName: "chart.bar.fill")
                        .font(.title2)
                        .foregroundStyle(.tint)
                }
                .frame(minWidth: 44)
            }

            Spacer()
        }
    }
}

// MARK: - Settings Bottom Sheet

struct SettingsSheet: View {
    @ObservedObject var midiSettings: MidiSettings
    @Binding var isPresented: Bool
    @ObservedObject var appLanguage: AppLanguage

    // ── Working copies of settings (only applied when Apply is tapped) ──
    @State private var selectedSourceId: String = "bundled"
    @State private var selectedFileUrl: String = MidiSettings.defaultLandingFileUrl
    @State private var includeMelody: Bool = true
    @State private var melodyTracksOption: String = "all"
    @State private var melodyTracksList: String = ""
    @State private var includePiano: Bool = false
    @State private var includeBass: Bool = false
    @State private var includeStrings: Bool = false
    @State private var includeDrums: Bool = true
    @State private var includeMetronome: Bool = false
    @State private var includeFeedback: Bool = false
    @State private var playbackSpeed: Double = 1.0
    @State private var muteMusic: Bool = false
    @State private var repeatCount: Int = 1
    @State private var transpose: Int = 0
    @State private var showCursor: Bool = true
    @State private var scoreRenderingMode: String = "staff"
    @State private var staffStavesOption: String = "all"
    @State private var staffStavesList: String = ""
    @State private var jianpuStaffNumber: String = "1"
    @State private var selectedLanguageCode: String = ""
    @State private var showLockedAlert: Bool = false

    /// Available music sources: built-in bundle + any loaded .mbk bundles.
    private var musicSources: [MusicSource] {
        var sources = [MusicSource(id: "bundled", name: L10n.string("source_bundled", language: selectedLanguageCode), items: Self.discoverBundledFiles())]
        for (_, bundle) in midiSettings.activeBundles.sorted(by: { $0.key < $1.key }) {
            let items = bundle.allPieces.map { piece in
                MusicItem(
                    name: piece.locked ? "\(piece.title) 🔒" : piece.title,
                    url: "mbk://\(bundle.bookId)/\(piece.xml)"
                )
            }
            sources.append(MusicSource(id: "mbk:\(bundle.bookId)", name: bundle.title, items: items))
        }
        return sources
    }

    private var selectedSource: MusicSource? {
        musicSources.first { $0.id == selectedSourceId }
    }

    /// Returns `true` when the given `mbk://` URL corresponds to a locked piece.
    private func isLockedMbkUrl(_ url: String) -> Bool {
        guard url.hasPrefix("mbk://") else { return false }
        let path = String(url.dropFirst("mbk://".count)) // "<bookId>/<xml>"
        let parts = path.split(separator: "/", maxSplits: 1)
        guard parts.count == 2 else { return false }
        let bookId = String(parts[0])
        let xml    = String(parts[1])
        return midiSettings.activeBundles[bookId]?.allPieces.first(where: { $0.xml == xml })?.locked == true
    }

    /// Scan the app bundle's SheetMusic folder for .musicxml and .mxl files.
    private static func discoverBundledFiles() -> [MusicItem] {
        guard let resourcesURL = Bundle.main.url(forResource: "sheetmusic", withExtension: nil) else {
            return []
        }
        let contents = (try? FileManager.default.contentsOfDirectory(at: resourcesURL,
                            includingPropertiesForKeys: nil)) ?? []
        return contents
            .map { $0.lastPathComponent }
            .filter {
                let lower = $0.lowercased()
                return lower.hasSuffix(".musicxml") || lower.hasSuffix(".mxl")
            }
            .sorted()
            .map { file in
                MusicItem(
                    name: (file as NSString).deletingPathExtension,
                    url: "file://sheetmusic/\(file)"
                )
            }
    }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(spacing: 24) {
                    // ── 1. Music Source ───────────────────────────
                    SettingsSection(L10n.string("settings_music_source", language: selectedLanguageCode)) {
                        // Playlist dropdown — use Menu so the collapsed label
                        // respects .settingsLabel font (Picker ignores custom
                        // fonts on its closed-state button label).
                        HStack {
                            Text(L10n.string("settings_playlist", language: selectedLanguageCode))
                                .font(.settingsLabel)
                                .fixedSize()
                            Spacer()
                            Menu {
                                ForEach(musicSources) { source in
                                    Button {
                                        selectedSourceId = source.id
                                        // Auto-select the first non-locked item of the new source.
                                        if let first = source.items.first(where: { !isLockedMbkUrl($0.url) })
                                            ?? source.items.first {
                                            selectedFileUrl = first.url
                                        }
                                    } label: {
                                        Text(source.name).font(.settingsLabel)
                                    }
                                }
                            } label: {
                                HStack(spacing: 4) {
                                    Text(selectedSource?.name ?? "")
                                        .font(.settingsLabel)
                                    Image(systemName: "chevron.up.chevron.down")
                                        .font(.settingsLabel)
                                        .foregroundStyle(.secondary)
                                }
                            }
                            .tint(.primary)
                        }

                        // Music file dropdown
                        if let source = selectedSource, !source.items.isEmpty {
                            HStack {
                                Text(L10n.string("settings_music", language: selectedLanguageCode))
                                    .font(.settingsLabel)
                                    .fixedSize()
                                Spacer()
                                Menu {
                                    ForEach(source.items) { item in
                                        Button {
                                            if isLockedMbkUrl(item.url) {
                                                showLockedAlert = true
                                            } else {
                                                selectedFileUrl = item.url
                                            }
                                        } label: {
                                            Text(item.name).font(.settingsLabelChinese)
                                        }
                                    }
                                } label: {
                                    HStack(spacing: 4) {
                                        Text(source.items.first(where: { $0.url == selectedFileUrl })?.name ?? "")
                                            .font(.settingsLabelChinese)
                                        Image(systemName: "chevron.up.chevron.down")
                                            .font(.settingsLabel)
                                            .foregroundStyle(.secondary)
                                    }
                                }
                                .tint(.primary)
                            }
                        }
                    }

                    // ── 2. Accompaniment ──────────────────────────
                    SettingsSection(L10n.string("settings_accompaniment", language: selectedLanguageCode)) {
                        // Four-column checkbox grid
                        let columns = Array(repeating: GridItem(.flexible(), spacing: 4), count: 4)

                        VStack(alignment: .leading, spacing: 12) {
                            LazyVGrid(columns: columns, spacing: 16) {
                                CheckboxToggle(L10n.string("settings_melody", language: selectedLanguageCode), isOn: $includeMelody)
                                CheckboxToggle(L10n.string("settings_piano", language: selectedLanguageCode), isOn: $includePiano)
                                CheckboxToggle(L10n.string("settings_bass", language: selectedLanguageCode), isOn: $includeBass)
                                CheckboxToggle(L10n.string("settings_strings", language: selectedLanguageCode), isOn: $includeStrings)
                                CheckboxToggle(L10n.string("settings_drums", language: selectedLanguageCode), isOn: $includeDrums)
                                CheckboxToggle(L10n.string("settings_metronome", language: selectedLanguageCode), isOn: $includeMetronome)
                                CheckboxToggle(L10n.string("settings_feedback", language: selectedLanguageCode), isOn: $includeFeedback)
                            }

                            if includeMelody {
                                HStack(spacing: 8) {
                                    Text(L10n.string("settings_track", language: selectedLanguageCode))
                                        .font(.settingsLabel)
                                    Button {
                                        melodyTracksOption = "all"
                                        melodyTracksList = ""
                                    } label: {
                                        HStack(spacing: 4) {
                                            Image(systemName: melodyTracksOption == "all" ? "largecircle.fill.circle" : "circle")
                                                .font(.caption)
                                            Text(L10n.string("settings_all", language: selectedLanguageCode))
                                                .font(.caption)
                                        }
                                        .foregroundStyle(.primary)
                                    }
                                    .buttonStyle(.plain)
                                    Button {
                                        melodyTracksOption = "custom"
                                    } label: {
                                        HStack(spacing: 4) {
                                            Image(systemName: melodyTracksOption == "custom" ? "largecircle.fill.circle" : "circle")
                                                .font(.caption)
                                            Text(L10n.string("settings_specific_tracks", language: selectedLanguageCode))
                                                .font(.caption)
                                        }
                                        .foregroundStyle(.primary)
                                    }
                                    .buttonStyle(.plain)
                                    if melodyTracksOption == "custom" {
                                        TextField("1,2", text: $melodyTracksList)
                                            .textFieldStyle(.roundedBorder)
                                            .keyboardType(.numbersAndPunctuation)
                                            .frame(width: 64)
                                            .onChange(of: melodyTracksList) { newValue in
                                                melodyTracksList = newValue.filter { $0.isNumber || $0 == "," }
                                            }
                                    }
                                }
                            }
                        }
                        .padding(.vertical, 4)
                    }

                    // ── 3. Playback ──────────────────────────────
                    SettingsSection(L10n.string("settings_playback", language: selectedLanguageCode)) {
                        PlaybackSettingsContent(
                            playbackSpeed: $playbackSpeed,
                            muteMusic: $muteMusic,
                            showCursor: $showCursor,
                            repeatCount: $repeatCount,
                            languageCode: selectedLanguageCode
                        )
                    }

                    // ── 4. Transpose ─────────────────────────────
                    SettingsSection(L10n.string("settings_transpose", language: selectedLanguageCode)) {
                        HStack(spacing: 16) {
                            Text(L10n.string("settings_semitones", language: selectedLanguageCode))
                                .font(.settingsLabel)

                            Spacer()

                            Button {
                                transpose -= 1
                            } label: {
                                Image(systemName: "minus.circle.fill")
                                    .font(.title2)
                                    .foregroundStyle(.tint)
                            }

                            Text("\(transpose)")
                                .font(.edwin(.title3).monospacedDigit())
                                .frame(minWidth: 36)
                                .multilineTextAlignment(.center)

                            Button {
                                transpose += 1
                            } label: {
                                Image(systemName: "plus.circle.fill")
                                    .font(.title2)
                                    .foregroundStyle(.tint)
                            }
                        }
                        .padding(.vertical, 4)
                    }

                    // ── 5. Score Rendering ─────────────────────────────
                    SettingsSection(L10n.string("settings_score_rendering", language: selectedLanguageCode)) {
                        VStack(alignment: .leading, spacing: 10) {
                            // Row 1: Staff + All + Specific staves + narrow entry when Staff & custom
                            HStack(alignment: .center, spacing: 8) {
                                Button { scoreRenderingMode = "staff" } label: {
                                    HStack(spacing: 6) {
                                        Image(systemName: scoreRenderingMode == "staff" ? "largecircle.fill.circle" : "circle")
                                            .font(.body)
                                        Text(L10n.string("settings_staff", language: selectedLanguageCode))
                                            .font(.settingsLabel)
                                    }
                                    .foregroundStyle(.primary)
                                }
                                .buttonStyle(.plain)
                                Button {
                                    scoreRenderingMode = "staff"
                                    staffStavesOption = "all"
                                    staffStavesList = ""
                                } label: {
                                    HStack(spacing: 4) {
                                        Image(systemName: staffStavesOption == "all" ? "largecircle.fill.circle" : "circle")
                                            .font(.caption)
                                        Text(L10n.string("settings_all", language: selectedLanguageCode))
                                            .font(.caption)
                                    }
                                    .foregroundStyle(.primary)
                                }
                                .buttonStyle(.plain)
                                Button {
                                    scoreRenderingMode = "staff"
                                    staffStavesOption = "custom"
                                } label: {
                                    HStack(spacing: 4) {
                                        Image(systemName: staffStavesOption == "custom" ? "largecircle.fill.circle" : "circle")
                                            .font(.caption)
                                        Text(L10n.string("settings_specific_staves", language: selectedLanguageCode))
                                            .font(.caption)
                                    }
                                    .foregroundStyle(.primary)
                                }
                                .buttonStyle(.plain)
                                if scoreRenderingMode == "staff" && staffStavesOption == "custom" {
                                    TextField("1,3,4,6", text: $staffStavesList)
                                        .textFieldStyle(.roundedBorder)
                                        .keyboardType(.numbersAndPunctuation)
                                        .frame(width: 80)
                                        .onChange(of: staffStavesList) { newValue in
                                            staffStavesList = newValue.filter { $0.isNumber || $0 == "," }
                                        }
                                }
                            }
                            .frame(maxWidth: .infinity, alignment: .leading)

                            // Row 2: Jianpu + Specific part + narrow entry
                            HStack(alignment: .center, spacing: 8) {
                                Button { scoreRenderingMode = "jianpu" } label: {
                                    HStack(spacing: 6) {
                                        Image(systemName: scoreRenderingMode == "jianpu" ? "largecircle.fill.circle" : "circle")
                                            .font(.body)
                                        Text(L10n.string("settings_jianpu", language: selectedLanguageCode))
                                            .font(.settingsLabel)
                                    }
                                    .foregroundStyle(.primary)
                                }
                                .buttonStyle(.plain)
                                if scoreRenderingMode == "jianpu" {
                                    Text(L10n.string("settings_specific_staff", language: selectedLanguageCode))
                                        .font(.settingsLabel)
                                    TextField("1", text: $jianpuStaffNumber)
                                        .textFieldStyle(.roundedBorder)
                                        .keyboardType(.numberPad)
                                        .frame(width: 40)
                                        .onChange(of: jianpuStaffNumber) { newValue in
                                            let firstSegment = newValue.split(separator: ",").first.map(String.init) ?? newValue
                                            let digitsOnly = firstSegment.filter { $0.isNumber }
                                            jianpuStaffNumber = String(digitsOnly.prefix(4))
                                        }
                                }
                            }
                            .frame(maxWidth: .infinity, alignment: .leading)
                        }
                        .padding(.vertical, 4)
                    }

                    // ── 6. Language (at bottom; applied only when Apply is tapped) ──
                    SettingsSection(L10n.string("settings_language", language: selectedLanguageCode)) {
                        HStack {
                            Text(L10n.string("settings_language", language: selectedLanguageCode))
                                .font(.settingsLabel)
                                .fixedSize()
                            Spacer()
                            Menu {
                                Button { selectedLanguageCode = "" } label: {
                                    Text(L10n.string("language_system", language: nil)).font(.settingsLabel)
                                }
                                Button { selectedLanguageCode = "en" } label: {
                                    Text(L10n.string("language_english", language: nil)).font(.settingsLabel)
                                }
                                Button { selectedLanguageCode = "zh-Hans" } label: {
                                    Text(L10n.string("language_chinese", language: nil)).font(.settingsLabel)
                                }
                                Button { selectedLanguageCode = "ja" } label: {
                                    Text(L10n.string("language_japanese", language: nil)).font(.settingsLabel)
                                }
                                Button { selectedLanguageCode = "ko" } label: {
                                    Text(L10n.string("language_korean", language: nil)).font(.settingsLabel)
                                }
                                Button { selectedLanguageCode = "de" } label: {
                                    Text(L10n.string("language_german", language: nil)).font(.settingsLabel)
                                }
                                Button { selectedLanguageCode = "es" } label: {
                                    Text(L10n.string("language_spanish", language: nil)).font(.settingsLabel)
                                }
                                Button { selectedLanguageCode = "fr" } label: {
                                    Text(L10n.string("language_french", language: nil)).font(.settingsLabel)
                                }
                            } label: {
                                HStack(spacing: 4) {
                                    Text(selectedLanguageCode.isEmpty ? L10n.string("language_system", language: nil) :
                                        selectedLanguageCode == "en" ? L10n.string("language_english", language: nil) :
                                        selectedLanguageCode == "zh-Hans" ? L10n.string("language_chinese", language: nil) :
                                        selectedLanguageCode == "ja" ? L10n.string("language_japanese", language: nil) :
                                        selectedLanguageCode == "ko" ? L10n.string("language_korean", language: nil) :
                                        selectedLanguageCode == "de" ? L10n.string("language_german", language: nil) :
                                        selectedLanguageCode == "es" ? L10n.string("language_spanish", language: nil) :
                                        selectedLanguageCode == "fr" ? L10n.string("language_french", language: nil) :
                                        L10n.string("language_system", language: nil))
                                        .font(.settingsLabel)
                                    Image(systemName: "chevron.up.chevron.down")
                                        .font(.settingsLabel)
                                        .foregroundStyle(.secondary)
                                }
                            }
                            .tint(.primary)
                        }
                        .padding(.vertical, 4)
                    }
                }
                .padding(.horizontal, 20)
                .padding(.bottom, 32)
            }
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .principal) {
                    Text(L10n.string("settings_title", language: selectedLanguageCode))
                        .font(.edwin(.headline))
                        .fontWeight(.semibold)
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button(L10n.string("settings_apply", language: selectedLanguageCode)) { applySettings() }
                        .font(.settingsLabel)
                }
            }
            .onAppear { loadFromSettings() }
            .alert(L10n.string("piece_locked_title", language: selectedLanguageCode), isPresented: $showLockedAlert) {
                Button(L10n.string("ok", language: selectedLanguageCode), role: .cancel) {}
            } message: {
                Text(L10n.string("piece_locked_message", language: selectedLanguageCode))
            }
        }
    }

    /// Copy current midiSettings into local working copies.
    private func loadFromSettings() {
        selectedSourceId = midiSettings.selectedSourceId
        selectedFileUrl = midiSettings.selectedFileUrl
        includeMelody = midiSettings.includeMelody
        melodyTracksOption = midiSettings.melodyTracksOption
        melodyTracksList = midiSettings.melodyTracksList
        includePiano = midiSettings.includePiano
        includeBass = midiSettings.includeBass
        includeStrings = midiSettings.includeStrings
        includeDrums = midiSettings.includeDrums
        includeMetronome = midiSettings.includeMetronome
        includeFeedback = midiSettings.includeFeedback
        playbackSpeed = midiSettings.playbackSpeed
        muteMusic = midiSettings.muteMusic
        repeatCount = midiSettings.repeatCount
        transpose = midiSettings.transpose
        showCursor = midiSettings.showCursor
        scoreRenderingMode = midiSettings.scoreRenderingMode
        staffStavesOption = midiSettings.staffStavesOption
        staffStavesList = midiSettings.staffStavesList
        jianpuStaffNumber = midiSettings.jianpuStaffNumber
        selectedLanguageCode = appLanguage.preferredCode
    }

    /// Write local working copies back to midiSettings and dismiss.
    private func applySettings() {
        midiSettings.selectedSourceId = selectedSourceId
        midiSettings.selectedFileUrl = selectedFileUrl
        midiSettings.includeMelody = includeMelody
        midiSettings.melodyTracksOption = melodyTracksOption
        midiSettings.melodyTracksList = melodyTracksList
        midiSettings.includePiano = includePiano
        midiSettings.includeBass = includeBass
        midiSettings.includeStrings = includeStrings
        midiSettings.includeDrums = includeDrums
        midiSettings.includeMetronome = includeMetronome
        midiSettings.includeFeedback = includeFeedback
        midiSettings.playbackSpeed = playbackSpeed
        midiSettings.muteMusic = muteMusic
        midiSettings.repeatCount = repeatCount
        midiSettings.transpose = transpose
        midiSettings.showCursor = showCursor
        midiSettings.scoreRenderingMode = scoreRenderingMode
        midiSettings.staffStavesOption = staffStavesOption
        midiSettings.staffStavesList = staffStavesList
        midiSettings.jianpuStaffNumber = jianpuStaffNumber
        appLanguage.preferredCode = selectedLanguageCode
        midiSettings.saveToDisk()
        isPresented = false
    }
}

// MARK: - App Fonts

private extension Font {
    static func edwin(_ style: Font.TextStyle) -> Font {
        .custom("Edwin-Bold", size: style.basePointSize, relativeTo: style)
    }

    /// Noto Sans CJK for Chinese/Japanese/Korean (settings, menus). Uses SC face from bundled TTC.
    static let wenKaiSubheadline = Font.custom("Noto Sans CJK SC", size: 13, relativeTo: .subheadline)
    static let wenKaiBody = Font.custom("Noto Sans CJK SC", size: 15, relativeTo: .body)

    // ── Settings label tokens ────────────────────────────────────────
    // Single source of truth for all option labels in the settings screen.
    // Change these two lines to restyle every label at once.
    static let settingsLabel: Font = .edwin(.subheadline)
    static let settingsLabelChinese: Font = .wenKaiSubheadline
}

private extension Font.TextStyle {
    /// Apple's default (non-scaled) base sizes for each text style.
    /// Using these as the anchor lets `relativeTo:` handle Dynamic Type
    /// scaling correctly without inflating on iPad or large-text settings.
    var basePointSize: CGFloat {
        switch self {
        case .largeTitle:  return 34
        case .title:       return 28
        case .title2:      return 22
        case .title3:      return 18
        case .headline:    return 15
        case .subheadline: return 13
        case .body:        return 15
        case .callout:     return 14
        case .footnote:    return 12
        case .caption:     return 11
        case .caption2:    return 10
        @unknown default:  return 15
        }
    }
}

// MARK: - Choir Bottom Sheet

struct ChoirSheet: View {
    @Binding var isPresented: Bool
    @ObservedObject var choirManager: ChoirManager
    @EnvironmentObject var authManager: AuthManager
    @EnvironmentObject var appLanguage: AppLanguage

    @State private var choirName = ""
    @State private var joinPassword = ""

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(spacing: 24) {
                    SettingsSection(L10n.string("choir_title", language: appLanguage.preferredCode)) {
                        VStack(alignment: .leading, spacing: 12) {
                            Text(L10n.string("choir_name", language: appLanguage.preferredCode))
                                .font(.settingsLabel)
                            TextField(L10n.string("choir_name", language: appLanguage.preferredCode), text: $choirName)
                                .textFieldStyle(.roundedBorder)
                                .textInputAutocapitalization(.words)
                                .autocorrectionDisabled()
                            Text(L10n.string("choir_password", language: appLanguage.preferredCode))
                                .font(.settingsLabel)
                            SecureField(L10n.string("choir_password", language: appLanguage.preferredCode), text: $joinPassword)
                                .textFieldStyle(.roundedBorder)
                            if let err = choirManager.joinError {
                                Text(err)
                                    .font(.settingsLabel)
                                    .foregroundStyle(.red)
                            }
                            if choirManager.isReconnecting {
                                Text(L10n.string("choir_reconnecting", language: appLanguage.preferredCode))
                                    .font(.settingsLabel)
                                    .foregroundStyle(.secondary)
                            }
                            Button(action: {
                                if choirManager.isJoined || choirManager.isReconnecting {
                                    choirManager.leave()
                                } else {
                                    let room = choirName.trimmingCharacters(in: .whitespacesAndNewlines)
                                    if !room.isEmpty {
                                        UserDefaults.standard.set(room, forKey: "lastChoirName")
                                        UserDefaults.standard.set(joinPassword, forKey: "lastChoirPassword")
                                    }
                                    choirManager.join(
                                        choirName: choirName,
                                        password: joinPassword,
                                        tokenProvider: { try await authManager.getAccessToken() }
                                    )
                                }
                            }) {
                                Text((choirManager.isJoined || choirManager.isReconnecting) ? L10n.string("choir_leave", language: appLanguage.preferredCode) : L10n.string("choir_join", language: appLanguage.preferredCode))
                                    .frame(maxWidth: .infinity)
                                    .padding(.vertical, 10)
                            }
                            .buttonStyle(.borderedProminent)
                            .disabled(!choirManager.isJoined && !choirManager.isReconnecting && choirName.trimmingCharacters(in: .whitespaces).isEmpty)
                        }
                    }
                }
                .padding()
            }
            .navigationTitle(L10n.string("choir_title", language: appLanguage.preferredCode))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button(L10n.string("choir_done", language: appLanguage.preferredCode)) { isPresented = false }
                }
            }
            .onAppear {
                if choirName.isEmpty {
                    choirName = UserDefaults.standard.string(forKey: "lastChoirName") ?? ""
                    joinPassword = UserDefaults.standard.string(forKey: "lastChoirPassword") ?? ""
                }
            }
        }
    }
}

// MARK: - Settings Helpers

/// A titled settings section with a rounded card background.
private struct SettingsSection<Content: View>: View {
    let title: String
    let content: Content

    init(_ title: String, @ViewBuilder content: () -> Content) {
        self.title = title
        self.content = content()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(title)
                .font(.edwin(.headline))

            VStack(alignment: .leading, spacing: 4) {
                content
            }
            .padding(14)
            .background(Color(.secondarySystemGroupedBackground))
            .clipShape(RoundedRectangle(cornerRadius: 12))
        }
    }
}

/// A compact checkbox toggle for use in settings grids and rows.
private struct CheckboxToggle: View {
    let label: String
    @Binding var isOn: Bool

    init(_ label: String, isOn: Binding<Bool>) {
        self.label = label
        self._isOn = isOn
    }

    var body: some View {
        Button {
            isOn.toggle()
        } label: {
            HStack(spacing: 3) {
                Image(systemName: isOn ? "checkmark.square.fill" : "square")
                    .foregroundStyle(isOn ? Color.accentColor : .secondary)
                    .font(.edwin(.callout))
                Text(label)
                    .font(.settingsLabel)
                    .lineLimit(1)
                Spacer(minLength: 0)
            }
        }
        .buttonStyle(.plain)
    }
}

/// Adaptive playback settings layout:
/// - **Portrait phone** (compact horizontal + regular vertical): two rows,
///   Speed | Mute on the first row, Cursor | Repeat on the second.
/// - **Landscape phone** (compact vertical) or **iPad** (regular horizontal):
///   all four controls in a single row.
private struct PlaybackSettingsContent: View {
    @Binding var playbackSpeed: Double
    @Binding var muteMusic: Bool
    @Binding var showCursor: Bool
    @Binding var repeatCount: Int
    var languageCode: String = ""

    @Environment(\.horizontalSizeClass) private var hSizeClass
    @Environment(\.verticalSizeClass) private var vSizeClass

    /// Single-row when landscape (compact vertical) or on iPad (regular horizontal).
    private var useSingleRow: Bool {
        hSizeClass == .regular || vSizeClass == .compact
    }

    var body: some View {
        if useSingleRow {
            // Landscape phone or iPad — all four in one row
            HStack(alignment: .center) {
                speedControl
                Spacer()
                muteToggle
                Spacer()
                cursorToggle
                Spacer()
                repeatControl
            }
        } else {
            // Portrait phone — two rows: Speed | Mute, then Cursor | Repeat
            VStack(spacing: 12) {
                HStack(alignment: .center) {
                    speedControl
                    Spacer()
                    muteToggle
                }
                HStack(alignment: .center) {
                    cursorToggle
                    Spacer()
                    repeatControl
                }
            }
        }
    }

    // MARK: - Subviews

    private var speedControl: some View {
        HStack(spacing: 4) {
            Text(L10n.string("settings_speed", language: languageCode))
                .font(.settingsLabel)
            TextField("1.0", value: $playbackSpeed, format: .number)
                .textFieldStyle(.roundedBorder)
                .keyboardType(.decimalPad)
                .frame(width: 40)
                .font(.settingsLabel)
        }
    }

    private var muteToggle: some View {
        Button { muteMusic.toggle() } label: {
            HStack(spacing: 3) {
                Text(L10n.string("settings_mute", language: languageCode))
                    .font(.settingsLabel)
                Image(systemName: muteMusic ? "checkmark.square.fill" : "square")
                    .foregroundStyle(muteMusic ? Color.accentColor : .secondary)
                    .font(.edwin(.callout))
            }
        }
        .buttonStyle(.plain)
    }

    private var cursorToggle: some View {
        Button { showCursor.toggle() } label: {
            HStack(spacing: 3) {
                Text(L10n.string("settings_cursor", language: languageCode))
                    .font(.settingsLabel)
                Image(systemName: showCursor ? "checkmark.square.fill" : "square")
                    .foregroundStyle(showCursor ? Color.accentColor : .secondary)
                    .font(.edwin(.callout))
            }
        }
        .buttonStyle(.plain)
    }

    private var repeatControl: some View {
        HStack(spacing: 4) {
            Text(L10n.string("settings_repeat", language: languageCode))
                .font(.settingsLabel)
            TextField("1", value: $repeatCount, format: .number)
                .textFieldStyle(.roundedBorder)
                .keyboardType(.numberPad)
                .frame(width: 40)
                .font(.settingsLabel)
        }
    }
}

// MARK: - Draggable Bottom Sheet Overlay

/// A custom bottom-sheet overlay that anchors to the bottom of the screen,
/// spans full width, and can be dragged between a collapsed (~50%) and
/// expanded (~90%) height.  Mimics the Android `ModalBottomSheet` behaviour.
private struct BottomSheetOverlay<Content: View>: View {
    @Binding var isPresented: Bool
    @ViewBuilder let content: Content

    /// Fraction of screen height for the collapsed (initial) stop.
    private let collapsedFraction: CGFloat = 0.65
    /// Fraction of screen height for the expanded stop.
    private let expandedFraction: CGFloat = 0.92

    @State private var currentFraction: CGFloat = 0.65
    @GestureState private var dragOffset: CGFloat = 0

    var body: some View {
        GeometryReader { geo in
            let screenHeight = geo.size.height
            let sheetHeight = screenHeight * currentFraction + dragOffset

            ZStack(alignment: .bottom) {
                // Dimming scrim
                Color.black.opacity(0.3)
                    .ignoresSafeArea()
                    .onTapGesture { isPresented = false }

                // Sheet
                VStack(spacing: 0) {
                    // Drag handle
                    Capsule()
                        .fill(Color(.systemGray3))
                        .frame(width: 36, height: 5)
                        .padding(.top, 8)
                        .padding(.bottom, 4)

                    content
                        .frame(maxHeight: .infinity)
                }
                .frame(width: geo.size.width, height: max(sheetHeight, 0))
                .background(Color(.systemBackground))
                .clipShape(
                    UnevenRoundedRectangle(
                        topLeadingRadius: 16,
                        bottomLeadingRadius: 0,
                        bottomTrailingRadius: 0,
                        topTrailingRadius: 16,
                        style: .continuous
                    )
                )
                .shadow(color: .black.opacity(0.15), radius: 20, y: -5)
                .gesture(
                    DragGesture()
                        .updating($dragOffset) { value, state, _ in
                            state = -value.translation.height
                        }
                        .onEnded { value in
                            let projected = -value.predictedEndTranslation.height
                            let midpoint = screenHeight * (collapsedFraction + expandedFraction) / 2
                            let targetHeight = screenHeight * currentFraction + projected

                            withAnimation(.spring(response: 0.35, dampingFraction: 0.86)) {
                                if targetHeight > midpoint {
                                    currentFraction = expandedFraction
                                } else if targetHeight < screenHeight * collapsedFraction * 0.5 {
                                    // Dragged far enough down to dismiss
                                    isPresented = false
                                } else {
                                    currentFraction = collapsedFraction
                                }
                            }
                        }
                )
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottom)
        }
        .ignoresSafeArea()
        .transition(.opacity.combined(with: .move(edge: .bottom)))
        .onAppear { currentFraction = collapsedFraction }
    }
}

#Preview {
    let asm = AudioSessionManager()
    return ContentView()
        .environmentObject(asm)
        .environmentObject(PlaybackManager(audioSessionManager: asm))
        .environmentObject(MidiSettings())
        .environmentObject(AuthManager())
        .environmentObject(AppLanguage())
}
