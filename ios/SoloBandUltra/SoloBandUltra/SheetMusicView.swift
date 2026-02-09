import SwiftUI

/// View that will display rendered sheet music.
/// Currently shows a placeholder; will be replaced with Rust-rendered content.
struct SheetMusicView: View {
    @State private var zoomScale: CGFloat = 1.0
    @State private var offset: CGSize = .zero

    var body: some View {
        GeometryReader { geometry in
            ScrollView([.horizontal, .vertical], showsIndicators: true) {
                VStack(spacing: 24) {
                    // Placeholder header
                    VStack(spacing: 8) {
                        Text("Asa Branca")
                            .font(.largeTitle)
                            .fontWeight(.bold)

                        Text("White Wing")
                            .font(.title3)
                            .foregroundStyle(.secondary)

                        Text("Luiz Gonzaga • Arr. Karim Ratib")
                            .font(.subheadline)
                            .foregroundStyle(.tertiary)
                    }
                    .padding(.top, 32)

                    // Placeholder staff lines
                    VStack(spacing: 40) {
                        ForEach(0..<4, id: \.self) { systemIndex in
                            StaffPlaceholder(systemIndex: systemIndex)
                        }
                    }
                    .padding(.horizontal, 20)

                    // Integration note
                    VStack(spacing: 12) {
                        Image(systemName: "music.note.list")
                            .font(.system(size: 48))
                            .foregroundStyle(.quaternary)

                        Text("Sheet music rendering will be powered by Rust")
                            .font(.callout)
                            .foregroundStyle(.quaternary)
                            .multilineTextAlignment(.center)
                    }
                    .padding(.vertical, 40)
                }
                .frame(minWidth: geometry.size.width)
            }
        }
        .background(Color(.systemBackground))
    }
}

// MARK: - Staff Placeholder

struct StaffPlaceholder: View {
    let systemIndex: Int

    var body: some View {
        VStack(spacing: 0) {
            // Measure indicator
            HStack {
                Text("System \(systemIndex + 1)")
                    .font(.caption2)
                    .foregroundStyle(.quaternary)
                Spacer()
            }

            // Five staff lines
            ZStack {
                VStack(spacing: 8) {
                    ForEach(0..<5, id: \.self) { _ in
                        Rectangle()
                            .fill(Color.gray.opacity(0.3))
                            .frame(height: 1)
                    }
                }

                // Placeholder note symbols
                HStack(spacing: 24) {
                    ForEach(0..<8, id: \.self) { noteIndex in
                        Circle()
                            .fill(Color.primary.opacity(0.15))
                            .frame(width: 12, height: 12)
                            .offset(y: CGFloat((noteIndex + systemIndex) % 5) * 9 - 18)
                    }
                }
            }
            .frame(height: 40)
            .padding(.vertical, 8)
        }
    }
}

#Preview {
    SheetMusicView()
}
