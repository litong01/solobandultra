import SwiftUI

struct ContentView: View {
    @EnvironmentObject var audioSessionManager: AudioSessionManager
    @EnvironmentObject var playbackManager: PlaybackManager
    @EnvironmentObject var midiSettings: MidiSettings

    @State private var showSettings = false

    var body: some View {
        NavigationStack {
            VStack(spacing: 0) {
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
                        showSettings = true
                    }
                )
                .padding(.horizontal)
                .padding(.vertical, 12)
                .background(.ultraThinMaterial)
            }
            .navigationTitle("SoloBand Ultra")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .navigationBarTrailing) {
                    Menu {
                        Button(action: {}) {
                            Label("Open File", systemImage: "doc.badge.plus")
                        }
                        Button(action: { showSettings = true }) {
                            Label("Settings", systemImage: "gear")
                        }
                    } label: {
                        Image(systemName: "ellipsis.circle")
                    }
                }
            }
            .sheet(isPresented: $showSettings) {
                SettingsSheet(midiSettings: midiSettings)
                    .presentationDetents([.medium, .large])
                    .presentationDragIndicator(.visible)
            }
        }
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
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(spacing: 24) {
                    // ── 1. Music Source ───────────────────────────
                    SettingsSection("Music Source") {
                        HStack {
                            Image(systemName: "music.note.list")
                                .foregroundStyle(.secondary)
                            Text("Bundled sheet music")
                                .foregroundStyle(.secondary)
                            Spacer()
                            Image(systemName: "chevron.right")
                                .font(.caption)
                                .foregroundStyle(.tertiary)
                        }
                        .padding(.vertical, 8)
                    }

                    // ── 2. Accompaniment ──────────────────────────
                    SettingsSection("Accompaniment") {
                        // Four-column checkbox grid
                        let columns = Array(repeating: GridItem(.flexible(), spacing: 4), count: 4)

                        LazyVGrid(columns: columns, spacing: 6) {
                            CheckboxToggle("Melody", isOn: $midiSettings.includeMelody)
                            CheckboxToggle("Piano", isOn: $midiSettings.includePiano)
                            CheckboxToggle("Bass", isOn: $midiSettings.includeBass)
                            CheckboxToggle("Strings", isOn: $midiSettings.includeStrings)
                            CheckboxToggle("Drums", isOn: $midiSettings.includeDrums)
                            CheckboxToggle("Metronome", isOn: $midiSettings.includeMetronome)
                        }

                        // Energy picker
                        VStack(alignment: .leading, spacing: 8) {
                            Text("Energy")
                                .font(.subheadline)
                                .foregroundStyle(.secondary)

                            Picker("Energy", selection: $midiSettings.energy) {
                                ForEach(MidiSettings.Energy.allCases) { level in
                                    Text(level.displayName).tag(level)
                                }
                            }
                            .pickerStyle(.segmented)
                        }
                        .padding(.top, 8)
                    }

                    // ── 3. Playback ──────────────────────────────
                    SettingsSection("Playback") {
                        HStack(alignment: .center) {
                            // Speed input
                            Text("Speed")
                                .font(.subheadline)
                                .foregroundStyle(.secondary)
                            TextField("1.0", value: $midiSettings.playbackSpeed, format: .number)
                                .textFieldStyle(.roundedBorder)
                                .keyboardType(.decimalPad)
                                .frame(width: 52)
                                .font(.subheadline)
                            Text("×")
                                .font(.subheadline)
                                .foregroundStyle(.secondary)

                            Spacer()

                            // Mute
                            CheckboxToggle("Mute", isOn: $midiSettings.muteMusic)

                            Spacer()

                            // Repeat
                            HStack(spacing: 4) {
                                Text("Repeat")
                                    .font(.subheadline)
                                    .foregroundStyle(.secondary)
                                Button {
                                    if midiSettings.repeatCount > 1 {
                                        midiSettings.repeatCount -= 1
                                    }
                                } label: {
                                    Image(systemName: "minus.circle.fill")
                                        .foregroundStyle(
                                            midiSettings.repeatCount > 1 ? Color.accentColor : Color(.tertiaryLabel)
                                        )
                                }
                                .disabled(midiSettings.repeatCount <= 1)

                                Text("\(midiSettings.repeatCount)×")
                                    .font(.subheadline.monospacedDigit())
                                    .frame(minWidth: 20)

                                Button {
                                    midiSettings.repeatCount += 1
                                } label: {
                                    Image(systemName: "plus.circle.fill")
                                        .foregroundStyle(.tint)
                                }
                            }
                        }
                    }

                    // ── 4. Transpose ─────────────────────────────
                    SettingsSection("Transpose") {
                        HStack(spacing: 16) {
                            Text("Semitones")
                                .foregroundStyle(.secondary)

                            Spacer()

                            Button {
                                midiSettings.transpose -= 1
                            } label: {
                                Image(systemName: "minus.circle.fill")
                                    .font(.title2)
                                    .foregroundStyle(.tint)
                            }

                            Text("\(midiSettings.transpose)")
                                .font(.title3.monospacedDigit())
                                .frame(minWidth: 36)
                                .multilineTextAlignment(.center)

                            Button {
                                midiSettings.transpose += 1
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
                    Button("Done") { dismiss() }
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
}
