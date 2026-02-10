import SwiftUI
import WebKit

/// Displays rendered sheet music SVG using a WKWebView.
struct SheetMusicView: View {
    @State private var svgContent: String?
    @State private var isLoading = true
    @State private var errorMessage: String?
    @State private var selectedFile: String = "asa-branca.musicxml"

    private let availableFiles = [
        "asa-branca.musicxml",
        "童年.mxl",
        "chopin-trois-valses.mxl"
    ]

    var body: some View {
        VStack(spacing: 0) {
            // File picker
            Picker("Score", selection: $selectedFile) {
                ForEach(availableFiles, id: \.self) { file in
                    Text(file).tag(file)
                }
            }
            .pickerStyle(.segmented)
            .padding(.horizontal, 16)
            .padding(.vertical, 8)
            .onChange(of: selectedFile) { _ in
                loadScore()
            }

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
                SVGWebView(svgString: svg)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                Text("No score loaded")
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .background(Color(.systemBackground))
        .onAppear {
            loadScore()
        }
    }

    private func loadScore() {
        isLoading = true
        errorMessage = nil
        svgContent = nil

        DispatchQueue.global(qos: .userInitiated).async {
            // Find the file in the app bundle
            let filename = selectedFile
            let ext = (filename as NSString).pathExtension
            let name = (filename as NSString).deletingPathExtension

            guard let url = Bundle.main.url(forResource: name, withExtension: ext) else {
                DispatchQueue.main.async {
                    isLoading = false
                    errorMessage = "File '\(filename)' not found in app bundle"
                }
                return
            }

            // Render using the Rust library
            let svg = ScoreLib.renderFile(at: url.path)

            DispatchQueue.main.async {
                isLoading = false
                if let svg = svg {
                    svgContent = svg
                } else {
                    errorMessage = "Failed to render '\(filename)'"
                }
            }
        }
    }
}

/// WKWebView wrapper for displaying SVG content.
struct SVGWebView: UIViewRepresentable {
    let svgString: String

    func makeUIView(context: Context) -> WKWebView {
        let config = WKWebViewConfiguration()
        let webView = WKWebView(frame: .zero, configuration: config)
        webView.isOpaque = false
        webView.backgroundColor = .clear
        webView.scrollView.backgroundColor = .clear
        webView.scrollView.showsVerticalScrollIndicator = true
        webView.scrollView.showsHorizontalScrollIndicator = false
        webView.scrollView.bounces = true
        return webView
    }

    func updateUIView(_ webView: WKWebView, context: Context) {
        let html = """
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
            svg {
                width: 100%;
                height: auto;
                max-width: 100%;
            }
        </style>
        </head>
        <body>
        \(svgString)
        </body>
        </html>
        """
        webView.loadHTMLString(html, baseURL: nil)
    }
}

#Preview {
    SheetMusicView()
}
