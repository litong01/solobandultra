import SwiftUI
import UniformTypeIdentifiers

struct ContentView: View {
    @EnvironmentObject var audioSessionManager: AudioSessionManager
    @EnvironmentObject var playbackManager: PlaybackManager
    @EnvironmentObject var midiSettings: MidiSettings
    @EnvironmentObject var authManager: AuthManager
    @Environment(\.scenePhase) private var scenePhase

    @State private var showSettings = false
    @State private var showFilePicker = false
    @State private var isDownloading = false
    @State private var downloadError: String?
    @State private var clipboardHasUrl = false

    var body: some View {
        VStack(spacing: 0) {
            // Compact top bar: icon left, menu right
            HStack {
                Image("AppIconImage")
                    .resizable()
                    .scaledToFit()
                    .frame(width: 28, height: 28)
                    .clipShape(RoundedRectangle(cornerRadius: 6, style: .continuous))
                    .help("Mysoloband")

                Spacer()

                Menu {
                    // ── Gated actions ──
                    Button(action: { requireAuth(for: .openFile) }) {
                        Label("Open File", systemImage: "doc.badge.plus")
                    }
                    Button(action: { requireAuth(for: .pasteLink) }) {
                        Label("Paste Link", systemImage: "doc.on.clipboard")
                    }
                    .disabled(!clipboardHasUrl)
                    Button(action: { requireAuth(for: .showSettings) }) {
                        Label("Settings", systemImage: "gear")
                    }

                    Divider()

                    // ── Login / Logout ──
                    if authManager.isAuthenticated {
                        Button(action: { authManager.logout() }) {
                            Label("Sign Out", systemImage: "rectangle.portrait.and.arrow.right")
                        }
                    } else {
                        Button(action: { authManager.login() }) {
                            Label("Sign In", systemImage: "person.crop.circle.badge.plus")
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
            SheetMusicView()
                .frame(maxWidth: .infinity, maxHeight: .infinity)

            Divider()

            // Playback controls
            PlaybackControlBar(
                isPlaying: $playbackManager.isPlaying,
                onPlayPause: {
                    playbackManager.togglePlayPause()
                },
                onStop: {
                    playbackManager.stop()
                },
                onSettings: {
                    requireAuth(for: .showSettings)
                }
            )
            .padding(.horizontal)
            .padding(.vertical, 12)
            .background(.ultraThinMaterial)
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
        .fileImporter(
            isPresented: $showFilePicker,
            allowedContentTypes: [.xml, .data],
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

                guard ext == "musicxml" || ext == "mxl" || ext == "xml" else { return }

                midiSettings.externalFileData = data
                midiSettings.externalFileName = filename
                midiSettings.externalFileVersion += 1
                midiSettings.selectedSourceId = "external"
                midiSettings.selectedFileUrl = "external://\(filename)"
            case .failure:
                break
            }
        }
        .overlay {
            if showSettings {
                ZStack(alignment: .bottom) {
                    // Dimming scrim
                    Color.black.opacity(0.3)
                        .ignoresSafeArea()
                        .onTapGesture { showSettings = false }

                    // Bottom-anchored settings card
                    SettingsSheet(midiSettings: midiSettings, isPresented: $showSettings)
                        .frame(maxHeight: UIScreen.main.bounds.height * 0.55)
                        .background(Color(.systemBackground))
                        .clipShape(RoundedRectangle(cornerRadius: 16, style: .continuous))
                        .shadow(color: .black.opacity(0.15), radius: 20, y: -5)
                        .padding(.horizontal, 8)
                        .padding(.bottom, 8)
                }
                .transition(.opacity.combined(with: .move(edge: .bottom)))
            }
        }
        .animation(.spring(response: 0.35, dampingFraction: 0.86), value: showSettings)
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
        UIPasteboard.general.detectPatterns(for: [.probableWebURL]) { result in
            DispatchQueue.main.async {
                if case .success(let patterns) = result {
                    clipboardHasUrl = patterns.contains(.probableWebURL)
                } else {
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
    let onPlayPause: () -> Void
    let onStop: () -> Void
    let onSettings: () -> Void

    var body: some View {
        HStack(spacing: 32) {
            Spacer()

            // Stop button
            Button(action: onStop) {
                Image(systemName: "stop.fill")
                    .font(.title2)
                    .foregroundStyle(.primary)
            }

            // Play/Pause button
            Button(action: onPlayPause) {
                Image(systemName: isPlaying ? "pause.circle.fill" : "play.circle.fill")
                    .font(.system(size: 52))
                    .foregroundStyle(.tint)
            }

            // Settings button
            Button(action: onSettings) {
                Image(systemName: "gear")
                    .font(.title2)
                    .foregroundStyle(.primary)
            }

            Spacer()
        }
    }
}

// MARK: - Settings Bottom Sheet

struct SettingsSheet: View {
    @ObservedObject var midiSettings: MidiSettings
    @Binding var isPresented: Bool

    // ── Working copies of settings (only applied when Apply is tapped) ──
    @State private var selectedSourceId: String = "bundled"
    @State private var selectedFileUrl: String = MidiSettings.defaultLandingFileUrl
    @State private var includeMelody: Bool = true
    @State private var includePiano: Bool = false
    @State private var includeBass: Bool = false
    @State private var includeStrings: Bool = false
    @State private var includeDrums: Bool = false
    @State private var includeMetronome: Bool = true
    @State private var playbackSpeed: Double = 1.0
    @State private var muteMusic: Bool = false
    @State private var repeatCount: Int = 1
    @State private var transpose: Int = 0

    /// Available music sources (currently just bundled files).
    private var musicSources: [MusicSource] {
        [MusicSource(id: "bundled", name: "Bundled Sheet Music", items: Self.discoverBundledFiles())]
    }

    private var selectedSource: MusicSource? {
        musicSources.first { $0.id == selectedSourceId }
    }

    /// Scan the app bundle's SheetMusic folder for .musicxml and .mxl files.
    private static func discoverBundledFiles() -> [MusicItem] {
        guard let resourcesURL = Bundle.main.url(forResource: "SheetMusic", withExtension: nil) else {
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
                    url: "file://SheetMusic/\(file)"
                )
            }
    }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(spacing: 24) {
                    // ── 1. Music Source ───────────────────────────
                    SettingsSection("Music Source") {
                        // Playlist dropdown
                        HStack {
                            Text("Playlist")
                                .font(.subheadline)
                                .foregroundStyle(.secondary)
                                .frame(width: 60, alignment: .leading)
                            Spacer()
                            Picker("", selection: $selectedSourceId) {
                                ForEach(musicSources) { source in
                                    Text(source.name).tag(source.id)
                                }
                            }
                            .pickerStyle(.menu)
                            .tint(.primary)
                        }

                        // File picker (shown when a source is selected)
                        if let source = selectedSource, !source.items.isEmpty {
                            HStack {
                                Text("Music")
                                    .font(.subheadline)
                                    .foregroundStyle(.secondary)
                                    .frame(width: 60, alignment: .leading)
                                Spacer()
                                Picker("", selection: $selectedFileUrl) {
                                    ForEach(source.items) { item in
                                        Text(item.name).tag(item.url)
                                    }
                                }
                                .pickerStyle(.menu)
                                .tint(.primary)
                            }
                        }
                    }

                    // ── 2. Accompaniment ──────────────────────────
                    SettingsSection("Accompaniment") {
                        // Four-column checkbox grid
                        let columns = Array(repeating: GridItem(.flexible(), spacing: 4), count: 4)

                        LazyVGrid(columns: columns, spacing: 16) {
                            CheckboxToggle("Melody", isOn: $includeMelody)
                            CheckboxToggle("Piano", isOn: $includePiano)
                            CheckboxToggle("Bass", isOn: $includeBass)
                            CheckboxToggle("Strings", isOn: $includeStrings)
                            CheckboxToggle("Drums", isOn: $includeDrums)
                            CheckboxToggle("Metronome", isOn: $includeMetronome)
                        }
                        .padding(.vertical, 4)
                    }

                    // ── 3. Playback ──────────────────────────────
                    SettingsSection("Playback") {
                        HStack(alignment: .center) {
                            // Speed input
                            Text("Speed")
                                .font(.subheadline)
                                .foregroundStyle(.secondary)
                            TextField("1.0", value: $playbackSpeed, format: .number)
                                .textFieldStyle(.roundedBorder)
                                .keyboardType(.decimalPad)
                                .frame(width: 40)
                                .font(.subheadline)

                            Spacer()

                            // Mute
                            Button {
                                muteMusic.toggle()
                            } label: {
                                HStack(spacing: 3) {
                                    Text("Mute")
                                        .font(.subheadline)
                                        .foregroundStyle(.primary)
                                    Image(systemName: muteMusic ? "checkmark.square.fill" : "square")
                                        .foregroundStyle(muteMusic ? Color.accentColor : .secondary)
                                        .font(.callout)
                                }
                            }
                            .buttonStyle(.plain)

                            Spacer()

                            // Repeat input
                            Text("Repeat")
                                .font(.subheadline)
                                .foregroundStyle(.secondary)
                            TextField("1", value: $repeatCount, format: .number)
                                .textFieldStyle(.roundedBorder)
                                .keyboardType(.numberPad)
                                .frame(width: 40)
                                .font(.subheadline)
                        }
                    }

                    // ── 4. Transpose ─────────────────────────────
                    SettingsSection("Transpose") {
                        HStack(spacing: 16) {
                            Text("Semitones")
                                .foregroundStyle(.secondary)

                            Spacer()

                            Button {
                                transpose -= 1
                            } label: {
                                Image(systemName: "minus.circle.fill")
                                    .font(.title2)
                                    .foregroundStyle(.tint)
                            }

                            Text("\(transpose)")
                                .font(.title3.monospacedDigit())
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
                }
                .padding(.horizontal, 20)
                .padding(.bottom, 32)
            }
            .navigationTitle("Settings")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Apply") { applySettings() }
                        .font(.subheadline)
                }
            }
            .onAppear { loadFromSettings() }
        }
    }

    /// Copy current midiSettings into local working copies.
    private func loadFromSettings() {
        selectedSourceId = midiSettings.selectedSourceId
        selectedFileUrl = midiSettings.selectedFileUrl
        includeMelody = midiSettings.includeMelody
        includePiano = midiSettings.includePiano
        includeBass = midiSettings.includeBass
        includeStrings = midiSettings.includeStrings
        includeDrums = midiSettings.includeDrums
        includeMetronome = midiSettings.includeMetronome
        playbackSpeed = midiSettings.playbackSpeed
        muteMusic = midiSettings.muteMusic
        repeatCount = midiSettings.repeatCount
        transpose = midiSettings.transpose
    }

    /// Write local working copies back to midiSettings and dismiss.
    private func applySettings() {
        midiSettings.selectedSourceId = selectedSourceId
        midiSettings.selectedFileUrl = selectedFileUrl
        midiSettings.includeMelody = includeMelody
        midiSettings.includePiano = includePiano
        midiSettings.includeBass = includeBass
        midiSettings.includeStrings = includeStrings
        midiSettings.includeDrums = includeDrums
        midiSettings.includeMetronome = includeMetronome
        midiSettings.playbackSpeed = playbackSpeed
        midiSettings.muteMusic = muteMusic
        midiSettings.repeatCount = repeatCount
        midiSettings.transpose = transpose
        isPresented = false
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
                .font(.headline)

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
                    .font(.callout)
                Text(label)
                    .font(.subheadline)
                    .foregroundStyle(.primary)
                    .lineLimit(1)
                Spacer(minLength: 0)
            }
        }
        .buttonStyle(.plain)
    }
}

#Preview {
    ContentView()
        .environmentObject(AudioSessionManager())
        .environmentObject(PlaybackManager(audioSessionManager: AudioSessionManager()))
        .environmentObject(MidiSettings())
        .environmentObject(AuthManager())
}
