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

    // MARK: - Note Timeline

    /// Generate a note timeline JSON array from MusicXML data.
    ///
    /// Returns melody notes (voice 1, part 0) with absolute timestamps.
    /// Decode with `JSONDecoder` into `[NoteEvent]`.
    /// - Parameter transpose: Must match the transpose used for rendering.
    static func noteTimeline(_ data: Data, extension ext: String? = nil, transpose: Int32 = 0) -> String? {
        let result: UnsafeMutablePointer<CChar>? = data.withUnsafeBytes { buffer in
            guard let baseAddress = buffer.baseAddress?.assumingMemoryBound(to: UInt8.self) else {
                return nil
            }
            if let ext = ext {
                return ext.withCString { extPtr in
                    scorelib_note_timeline(baseAddress, buffer.count, extPtr, transpose)
                }
            } else {
                return scorelib_note_timeline(baseAddress, buffer.count, nil, transpose)
            }
        }
        guard let cResult = result else { return nil }
        let json = String(cString: cResult)
        scorelib_free_string(cResult)
        return json
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

    // MARK: - Feedback Overlay

    /// Add the feedback overlay layer (colored dots) to a score SVG for the performance report.
    /// - Parameter svg: The score SVG string.
    /// - Parameter overlayDotsJson: JSON array of { "x", "y", "colors": string[] } in SVG coordinates.
    /// - Returns: New SVG string with overlay inserted, or nil on error.
    static func addFeedbackOverlay(svg: String, overlayDotsJson: String) -> String? {
        let cResult = svg.withCString { svgPtr in
            overlayDotsJson.withCString { dotsPtr in
                scorelib_add_feedback_overlay(svgPtr, dotsPtr)
            }
        }
        guard let cResult = cResult else { return nil }
        let out = String(cString: cResult)
        scorelib_free_string(cResult)
        return out
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

    // MARK: - Audio Rendering

    /// Cached SoundFont data — loaded once from the app bundle.
    private static let soundfontData: Data? = {
        guard let url = Bundle.main.url(forResource: "GeneralUser_GS", withExtension: "sf2") else {
            print("[ScoreLib] WARNING: GeneralUser_GS.sf2 not found in bundle")
            return nil
        }
        return try? Data(contentsOf: url)
    }()

    /// Render MusicXML data to WAV audio using the bundled SoundFont.
    ///
    /// Internally generates MIDI and synthesizes it offline via rustysynth.
    /// Returns a complete WAV file (44100 Hz, stereo, 16-bit) as Data.
    static func renderAudio(_ data: Data, extension ext: String? = nil, optionsJson: String? = nil) -> Data? {
        guard let sfData = soundfontData else {
            print("[ScoreLib] Cannot render audio: no SoundFont available")
            return nil
        }

        var outLen: Int = 0
        let result: UnsafeMutablePointer<UInt8>? = data.withUnsafeBytes { xmlBuf in
            guard let xmlBase = xmlBuf.baseAddress?.assumingMemoryBound(to: UInt8.self) else {
                return nil
            }
            return sfData.withUnsafeBytes { sfBuf in
                guard let sfBase = sfBuf.baseAddress?.assumingMemoryBound(to: UInt8.self) else {
                    return nil
                }

                if let ext = ext {
                    return ext.withCString { extPtr in
                        if let opts = optionsJson {
                            return opts.withCString { optsPtr in
                                scorelib_render_audio_from_bytes(
                                    xmlBase, xmlBuf.count,
                                    extPtr, optsPtr,
                                    sfBase, sfBuf.count,
                                    &outLen
                                )
                            }
                        } else {
                            return scorelib_render_audio_from_bytes(
                                xmlBase, xmlBuf.count,
                                extPtr, nil,
                                sfBase, sfBuf.count,
                                &outLen
                            )
                        }
                    }
                } else {
                    if let opts = optionsJson {
                        return opts.withCString { optsPtr in
                            scorelib_render_audio_from_bytes(
                                xmlBase, xmlBuf.count,
                                nil, optsPtr,
                                sfBase, sfBuf.count,
                                &outLen
                            )
                        }
                    } else {
                        return scorelib_render_audio_from_bytes(
                            xmlBase, xmlBuf.count,
                            nil, nil,
                            sfBase, sfBuf.count,
                            &outLen
                        )
                    }
                }
            }
        }

        guard let ptr = result, outLen > 0 else {
            return nil
        }
        let wavData = Data(bytes: ptr, count: outLen)
        scorelib_free_midi(ptr, outLen)  // Same dealloc pattern as MIDI bytes
        return wavData
    }
}
