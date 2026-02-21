import SwiftUI
import PDFKit

// MARK: - Full-screen PDF Viewer

/// Full-screen modal PDF viewer backed by Apple's built-in PDFKit.
/// Playback continues unaffected while the viewer is open.
struct PdfViewerView: View {
    let bundle: BookBundle
    /// 1-based PDF page to open at initially.
    let startPage: Int
    @Binding var isPresented: Bool
    /// Invoked when the user selects a piece from the jump-to-piece overlay.
    var onPieceSelected: ((BookPiece) -> Void)?

    @State private var showPiecePicker = false

    var body: some View {
        NavigationStack {
            ZStack {
                PDFViewRepresentable(pdfURL: bundle.pdfURL, startPage: startPage)
                    .ignoresSafeArea(edges: .bottom)

                // Floating piece-list button (top-left corner)
                VStack {
                    HStack {
                        Button {
                            showPiecePicker = true
                        } label: {
                            Image(systemName: "list.bullet")
                                .padding(10)
                                .background(.ultraThinMaterial)
                                .clipShape(Circle())
                        }
                        .padding(.leading, 12)
                        .padding(.top, 8)
                        Spacer()
                    }
                    Spacer()
                }
            }
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .principal) {
                    Text(bundle.title)
                        .font(.headline)
                        .lineLimit(1)
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Close") { isPresented = false }
                }
            }
        }
        .sheet(isPresented: $showPiecePicker) {
            PiecePickerSheet(bundle: bundle) { piece in
                showPiecePicker = false
                onPieceSelected?(piece)
            }
        }
    }
}

// MARK: - PDFView UIViewRepresentable

private struct PDFViewRepresentable: UIViewRepresentable {
    let pdfURL: URL
    let startPage: Int  // 1-based

    func makeCoordinator() -> Coordinator { Coordinator() }

    func makeUIView(context: Context) -> PDFView {
        let view = PDFView()
        view.autoScales = true
        view.displayMode = .singlePageContinuous
        view.displayDirection = .vertical
        view.backgroundColor = .white
        return view
    }

    func updateUIView(_ pdfView: PDFView, context: Context) {
        guard pdfView.document?.documentURL != pdfURL else { return }
        guard let doc = PDFDocument(url: pdfURL) else { return }
        pdfView.document = doc
        // Jump to start page only on first load of this document
        if !context.coordinator.didScrollToStart {
            let idx = max(0, startPage - 1)
            if let page = doc.page(at: idx) {
                pdfView.go(to: page)
            }
            context.coordinator.didScrollToStart = true
        }
    }

    class Coordinator {
        var didScrollToStart = false
    }
}

// MARK: - Piece Picker Sheet

private struct PiecePickerSheet: View {
    let bundle: BookBundle
    let onSelect: (BookPiece) -> Void

    var body: some View {
        NavigationStack {
            List {
                ForEach(bundle.pages, id: \.page) { bookPage in
                    Section("Page \(bookPage.page)") {
                        ForEach(bookPage.pieces) { piece in
                            Button {
                                onSelect(piece)
                            } label: {
                                HStack {
                                    VStack(alignment: .leading, spacing: 2) {
                                        Text(piece.title)
                                            .font(.body)
                                            .foregroundStyle(piece.locked ? .secondary : .primary)
                                        if let diff = piece.difficulty {
                                            Text(diff.capitalized)
                                                .font(.caption)
                                                .foregroundStyle(.secondary)
                                        }
                                    }
                                    Spacer()
                                    if piece.locked {
                                        Image(systemName: "lock.fill")
                                            .foregroundStyle(.secondary)
                                    }
                                }
                            }
                            .foregroundStyle(piece.locked ? .secondary : .primary)
                        }
                    }
                }
            }
            .navigationTitle("Pieces")
            .navigationBarTitleDisplayMode(.inline)
        }
    }
}
