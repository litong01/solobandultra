import SwiftUI

struct ContentView: View {
    @EnvironmentObject var audioSessionManager: AudioSessionManager
    @State private var isPlaying = false

    var body: some View {
        NavigationStack {
            VStack(spacing: 0) {
                // Sheet music display area
                SheetMusicView()
                    .frame(maxWidth: .infinity, maxHeight: .infinity)

                Divider()

                // Playback controls
                PlaybackControlBar(isPlaying: $isPlaying) {
                    togglePlayback()
                } onStop: {
                    stopPlayback()
                }
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
                        Button(action: {}) {
                            Label("Settings", systemImage: "gear")
                        }
                    } label: {
                        Image(systemName: "ellipsis.circle")
                    }
                }
            }
        }
    }

    private func togglePlayback() {
        isPlaying.toggle()
        if isPlaying {
            audioSessionManager.ensureSessionActive()
            // Playback will be implemented with Rust audio engine
            print("[Playback] Started")
        } else {
            print("[Playback] Paused")
        }
    }

    private func stopPlayback() {
        isPlaying = false
        print("[Playback] Stopped")
    }
}

// MARK: - Playback Control Bar

struct PlaybackControlBar: View {
    @Binding var isPlaying: Bool
    let onPlayPause: () -> Void
    let onStop: () -> Void

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

            // Placeholder for tempo/metronome
            Button(action: {}) {
                Image(systemName: "metronome")
                    .font(.title2)
                    .foregroundStyle(.primary)
            }

            Spacer()
        }
    }
}

#Preview {
    ContentView()
        .environmentObject(AudioSessionManager())
}
