import Foundation

/// Swift wrapper around the Rust scorelib C FFI.
enum ScoreLib {

    // MARK: - SVG Rendering

    /// Render a MusicXML file at the given path to SVG.
    /// - Parameter pageWidth: SVG width in user-units. Pass 0 for the default (820).
    /// - Parameter transpose: Semitones to transpose (0 = no change).
    static func renderFile(at path: String, pageWidth: Double = 0, transpose: Int32 = 0) -> String? {
        guard let cResult = scorelib_render_file(path, pageWidth, transpose) else {
            return nil
        }
        let svg = String(cString: cResult)
        scorelib_free_string(cResult)
        return svg
    }

    /// Render MusicXML data (bytes) to SVG.
    /// - Parameter pageWidth: SVG width in user-units. Pass 0 for the default (820).
    /// - Parameter transpose: Semitones to transpose (0 = no change).
    static func renderData(_ data: Data, extension ext: String? = nil, pageWidth: Double = 0, transpose: Int32 = 0) -> String? {
        let result: UnsafeMutablePointer<CChar>? = data.withUnsafeBytes { buffer in
            guard let baseAddress = buffer.baseAddress?.assumingMemoryBound(to: UInt8.self) else {
                return nil
            }
            if let ext = ext {
                return ext.withCString { extPtr in
                    scorelib_render_bytes(baseAddress, buffer.count, extPtr, pageWidth, transpose)
                }
            } else {
                return scorelib_render_bytes(baseAddress, buffer.count, nil, pageWidth, transpose)
            }
        }

        guard let cResult = result else {
            return nil
        }
        let svg = String(cString: cResult)
        scorelib_free_string(cResult)
        return svg
    }

    // MARK: - Playback Map

    /// Generate a playback map JSON string from MusicXML data.
    ///
    /// The playback map contains measure visual positions, system positions,
    /// and the unrolled timemap — everything needed for cursor synchronization.
    /// - Parameter transpose: Semitones to transpose (0 = no change). Must match render transpose.
    static func playbackMap(_ data: Data, extension ext: String? = nil, pageWidth: Double = 0, transpose: Int32 = 0) -> String? {
        let result: UnsafeMutablePointer<CChar>? = data.withUnsafeBytes { buffer in
            guard let baseAddress = buffer.baseAddress?.assumingMemoryBound(to: UInt8.self) else {
                return nil
            }
            if let ext = ext {
                return ext.withCString { extPtr in
                    scorelib_playback_map(baseAddress, buffer.count, extPtr, pageWidth, transpose)
                }
            } else {
                return scorelib_playback_map(baseAddress, buffer.count, nil, pageWidth, transpose)
            }
        }

        guard let cResult = result else {
            return nil
        }
        let json = String(cString: cResult)
        scorelib_free_string(cResult)
        return json
    }

    // MARK: - MIDI Generation

    /// Generate MIDI bytes from MusicXML data.
    ///
    /// Returns Standard MIDI File (SMF Type 1) data that can be played
    /// with AVMIDIPlayer.
    static func generateMidi(_ data: Data, extension ext: String? = nil, optionsJson: String? = nil) -> Data? {
        var outLen: Int = 0
        let result: UnsafeMutablePointer<UInt8>? = data.withUnsafeBytes { buffer in
            guard let baseAddress = buffer.baseAddress?.assumingMemoryBound(to: UInt8.self) else {
                return nil
            }

            if let ext = ext {
                return ext.withCString { extPtr in
                    if let opts = optionsJson {
                        return opts.withCString { optsPtr in
                            scorelib_generate_midi_from_bytes(baseAddress, buffer.count, extPtr, optsPtr, &outLen)
                        }
                    } else {
                        return scorelib_generate_midi_from_bytes(baseAddress, buffer.count, extPtr, nil, &outLen)
                    }
                }
            } else {
                if let opts = optionsJson {
                    return opts.withCString { optsPtr in
                        scorelib_generate_midi_from_bytes(baseAddress, buffer.count, nil, optsPtr, &outLen)
                    }
                } else {
                    return scorelib_generate_midi_from_bytes(baseAddress, buffer.count, nil, nil, &outLen)
                }
            }
        }

        guard let ptr = result, outLen > 0 else {
            return nil
        }
        let midiData = Data(bytes: ptr, count: outLen)
        scorelib_free_midi(ptr, outLen)
        return midiData
    }
}
