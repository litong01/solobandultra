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
    /// Melody voice filter: "all" or "custom" with comma-separated MusicXML voices (e.g. "1,3").
    @Published var melodyTracksOption: String = "all"
    @Published var melodyTracksList: String = ""
    @Published var includePiano: Bool = false
    @Published var includeBass: Bool = false
    @Published var includeStrings: Bool = false
    @Published var includeDrums: Bool = true
    @Published var includeMetronome: Bool = false
    @Published var includeFeedback: Bool = false

    // ── Energy level (hardcoded to strong; not user-facing) ──
    @Published var energy: Energy = .strong

    // ── Playback ──
    @Published var playbackSpeed: Double = 1.0
    @Published var muteMusic: Bool = false
    @Published var repeatCount: Int = 1

    // ── Cursor ──
    /// Whether to show the playback cursor overlay on the sheet music.
    /// This is a pure UI toggle — no MIDI or SVG regeneration required.
    @Published var showCursor: Bool = true

    // ── Transpose (semitones) ──
    @Published var transpose: Int = 0

    // ── Score rendering: staff vs jianpu; staff "all" or custom list (e.g. 1,3,4,6); jianpu single part number ──
    @Published var scoreRenderingMode: String = "staff"
    @Published var staffStavesOption: String = "all"
    @Published var staffStavesList: String = ""
    @Published var jianpuStaffNumber: String = "1"

    // ── Music source selection ──
    /// The default music file shown on app launch (landing page).
    static let defaultLandingFile = "asa-branca.musicxml"
    static let defaultLandingFileUrl = "file://sheetmusic/asa-branca.musicxml"

    @Published var selectedSourceId: String = "bundled"
    @Published var selectedFileUrl: String = defaultLandingFileUrl

    // ── Persistence ──

    init() {
        loadFromDisk()
    }

    private enum Key {
        static let includeMelody       = "includeMelody"
        static let melodyTracksOption  = "melodyTracksOption"
        static let melodyTracksList    = "melodyTracksList"
        /// Legacy single-track key (migrated on load).
        static let melodyTrack         = "melodyTrack"
        static let includePiano     = "includePiano"
        static let includeBass      = "includeBass"
        static let includeStrings   = "includeStrings"
        static let includeDrums     = "includeDrums"
        static let includeMetronome = "includeMetronome"
        static let includeFeedback  = "includeFeedback"
        static let playbackSpeed    = "playbackSpeed"
        static let muteMusic        = "muteMusic"
        static let repeatCount      = "repeatCount"
        static let showCursor          = "showCursor"
        static let transpose           = "transpose"
        static let scoreRenderingMode  = "scoreRenderingMode"
        static let staffStavesOption   = "staffStavesOption"
        static let staffStavesList     = "staffStavesList"
        static let jianpuStaffNumber   = "jianpuStaffNumber"
        static let selectedSourceId    = "selectedSourceId"
        static let selectedFileUrl     = "selectedFileUrl"
    }

    func saveToDisk() {
        let d = UserDefaults.standard
        d.set(includeMelody,       forKey: Key.includeMelody)
        d.set(melodyTracksOption,  forKey: Key.melodyTracksOption)
        d.set(melodyTracksList,    forKey: Key.melodyTracksList)
        d.set(includePiano,     forKey: Key.includePiano)
        d.set(includeBass,      forKey: Key.includeBass)
        d.set(includeStrings,   forKey: Key.includeStrings)
        d.set(includeDrums,     forKey: Key.includeDrums)
        d.set(includeMetronome, forKey: Key.includeMetronome)
        d.set(includeFeedback,  forKey: Key.includeFeedback)
        d.set(playbackSpeed,    forKey: Key.playbackSpeed)
        d.set(muteMusic,        forKey: Key.muteMusic)
        d.set(repeatCount,      forKey: Key.repeatCount)
        d.set(showCursor,          forKey: Key.showCursor)
        d.set(transpose,           forKey: Key.transpose)
        d.set(scoreRenderingMode,  forKey: Key.scoreRenderingMode)
        d.set(staffStavesOption,   forKey: Key.staffStavesOption)
        d.set(staffStavesList,    forKey: Key.staffStavesList)
        d.set(jianpuStaffNumber,   forKey: Key.jianpuStaffNumber)
        // External files don't survive restart — fall back to bundled default.
        let srcToSave = selectedSourceId == "external" ? "bundled" : selectedSourceId
        let urlToSave = selectedSourceId == "external" ? MidiSettings.defaultLandingFileUrl : selectedFileUrl
        d.set(srcToSave, forKey: Key.selectedSourceId)
        d.set(urlToSave, forKey: Key.selectedFileUrl)
    }

    private func loadFromDisk() {
        let d = UserDefaults.standard
        // If no value has ever been saved, keep the compiled-in defaults.
        guard d.object(forKey: Key.includeMelody) != nil else { return }
        includeMelody    = d.bool(forKey: Key.includeMelody)
        if let s = d.string(forKey: Key.melodyTracksOption) { melodyTracksOption = s }
        if let s = d.string(forKey: Key.melodyTracksList) { melodyTracksList = s }
        // Migrate legacy single-track Int (1–4) into option + list.
        if d.object(forKey: Key.melodyTracksOption) == nil,
           d.object(forKey: Key.melodyTrack) != nil {
            let legacy = d.integer(forKey: Key.melodyTrack)
            if (1...4).contains(legacy) {
                melodyTracksOption = "custom"
                melodyTracksList = "\(legacy)"
            }
        }
        includePiano     = d.bool(forKey: Key.includePiano)
        includeBass      = d.bool(forKey: Key.includeBass)
        includeStrings   = d.bool(forKey: Key.includeStrings)
        includeDrums     = d.bool(forKey: Key.includeDrums)
        includeMetronome = d.bool(forKey: Key.includeMetronome)
        includeFeedback  = d.bool(forKey: Key.includeFeedback)
        playbackSpeed    = d.double(forKey: Key.playbackSpeed)
        muteMusic        = d.bool(forKey: Key.muteMusic)
        repeatCount      = d.integer(forKey: Key.repeatCount)
        showCursor          = d.bool(forKey: Key.showCursor)
        transpose           = d.integer(forKey: Key.transpose)
        if let s = d.string(forKey: Key.scoreRenderingMode)  { scoreRenderingMode  = s }
        if let s = d.string(forKey: Key.staffStavesOption)    { staffStavesOption   = s }
        if let s = d.string(forKey: Key.staffStavesList)      { staffStavesList     = s }
        if let s = d.string(forKey: Key.jianpuStaffNumber)    { jianpuStaffNumber   = s }
        if let src = d.string(forKey: Key.selectedSourceId) { selectedSourceId = src }
        if let url = d.string(forKey: Key.selectedFileUrl)  { selectedFileUrl  = url }
    }

    // ── External file (opened via document picker) ──
    /// Raw bytes of an externally opened file (from Files, iCloud, Google Drive, etc.)
    @Published var externalFileData: Data? = nil
    /// Display name of the externally opened file.
    @Published var externalFileName: String? = nil
    /// Monotonically increasing counter bumped every time an external file is set.
    /// Used to force a reload even when the filename is identical.
    @Published var externalFileVersion: Int = 0

    // ── SBF bundles ──
    /// Loaded bundles keyed by bookId.  Populated when a .mbk file is opened.
    @Published var activeBundles: [String: BookBundle] = [:]

    /// Non-nil when an error should be surfaced to the user (e.g., corrupt .mbk).
    @Published var errorMessage: String? = nil

    /// The bundle corresponding to the currently selected source, if any.
    var activeBundle: BookBundle? {
        guard selectedSourceId.hasPrefix("mbk:") else { return nil }
        let bookId = String(selectedSourceId.dropFirst("mbk:".count))
        return activeBundles[bookId]
    }

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
    ///
    /// Until multi-track MIDI filtering lands, only the first selected track
    /// number is sent as `melody_track` (0 = all voices).
    func toJson() -> String {
        let track: Int = {
            guard melodyTracksOption == "custom" else { return 0 }
            let first = melodyTracksList
                .split(separator: ",")
                .map { $0.trimmingCharacters(in: .whitespaces) }
                .compactMap { Int($0) }
                .first { (1...4).contains($0) }
            return first ?? 0
        }()
        let parts = [
            "\"include_melody\":\(includeMelody)",
            "\"include_piano\":\(includePiano)",
            "\"include_bass\":\(includeBass)",
            "\"include_strings\":\(includeStrings)",
            "\"include_drums\":\(includeDrums)",
            "\"include_metronome\":\(includeMetronome)",
            "\"include_feedback\":\(includeFeedback)",
            "\"energy\":\"\(energy.rawValue)\"",
            "\"transpose\":\(transpose)",
            "\"melody_track\":\(track)"
        ]
        return "{\(parts.joined(separator: ","))}"
    }
}
