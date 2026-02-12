import Foundation

// MARK: - Music Source Model

/// A single music file with a display name and source URL.
struct MusicItem: Identifiable, Hashable {
    let name: String
    let url: String

    var id: String { url }
}

/// A collection of music files from a single source.
struct MusicSource: Identifiable {
    let id: String
    let name: String
    let items: [MusicItem]
}

// MARK: - MIDI Settings

/// Observable model for MIDI generation options and playback settings.
///
/// Mirrors the Rust `MidiOptions` struct plus additional UI settings.
/// Changes are published to SwiftUI views so playback can be regenerated.
class MidiSettings: ObservableObject {
    // ── Accompaniment track toggles ──
    @Published var includeMelody: Bool = true
    @Published var includePiano: Bool = false
    @Published var includeBass: Bool = false
    @Published var includeStrings: Bool = false
    @Published var includeDrums: Bool = false
    @Published var includeMetronome: Bool = true

    // ── Energy level (hardcoded to strong; not user-facing) ──
    @Published var energy: Energy = .strong

    // ── Playback ──
    @Published var playbackSpeed: Double = 1.0
    @Published var muteMusic: Bool = false
    @Published var repeatCount: Int = 1

    // ── Transpose (semitones) ──
    @Published var transpose: Int = 0

    // ── Music source selection ──
    /// The default music file shown on app launch (landing page).
    static let defaultLandingFile = "asa-branca.musicxml"
    static let defaultLandingFileUrl = "file://SheetMusic/asa-branca.musicxml"

    @Published var selectedSourceId: String = "bundled"
    @Published var selectedFileUrl: String = defaultLandingFileUrl

    // ── External file (opened via document picker) ──
    /// Raw bytes of an externally opened file (from Files, iCloud, Google Drive, etc.)
    @Published var externalFileData: Data? = nil
    /// Display name of the externally opened file.
    @Published var externalFileName: String? = nil
    /// Monotonically increasing counter bumped every time an external file is set.
    /// Used to force a reload even when the filename is identical.
    @Published var externalFileVersion: Int = 0

    enum Energy: String, CaseIterable, Identifiable {
        case soft = "soft"
        case medium = "medium"
        case strong = "strong"

        var id: String { rawValue }

        var displayName: String {
            switch self {
            case .soft: return "Soft"
            case .medium: return "Medium"
            case .strong: return "Strong"
            }
        }
    }

    /// Preset speed values for the picker.
    static let speedOptions: [Double] = [0.5, 0.75, 1.0, 1.25, 1.5, 2.0]

    /// Human-readable label for a speed value.
    static func speedLabel(_ speed: Double) -> String {
        if speed == 1.0 { return "1×" }
        // Trim trailing zeros: 0.5× not 0.50×
        let formatted = speed.truncatingRemainder(dividingBy: 1) == 0
            ? String(format: "%.0f", speed)
            : String(format: "%.2g", speed)
        return "\(formatted)×"
    }

    /// Serialize to the JSON format expected by the Rust FFI layer.
    func toJson() -> String {
        let parts = [
            "\"include_melody\":\(includeMelody)",
            "\"include_piano\":\(includePiano)",
            "\"include_bass\":\(includeBass)",
            "\"include_strings\":\(includeStrings)",
            "\"include_drums\":\(includeDrums)",
            "\"include_metronome\":\(includeMetronome)",
            "\"energy\":\"\(energy.rawValue)\"",
            "\"transpose\":\(transpose)"
        ]
        return "{\(parts.joined(separator: ","))}"
    }
}
