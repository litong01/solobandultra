import SwiftUI

struct ContentView: View {
    @EnvironmentObject var audioSessionManager: AudioSessionManager
    @EnvironmentObject var playbackManager: PlaybackManager

    var body: some View {
        NavigationStack {
            VStack(spacing: 0) {
                // Sheet music display area
                SheetMusicView()
                    .frame(maxWidth: .infinity, maxHeight: .infinity)

                Divider()

                // Playback controls
                PlaybackControlBar(
                    isPlaying: $playbackManager.isPlaying
                ) {
                    playbackManager.togglePlayPause()
                } onStop: {
                    playbackManager.stop()
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
        .environmentObject(PlaybackManager(audioSessionManager: AudioSessionManager()))
}
