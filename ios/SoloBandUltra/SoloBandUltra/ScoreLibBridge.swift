import Foundation

/// Swift wrapper around the Rust scorelib C FFI.
enum ScoreLib {

    /// Render a MusicXML file at the given path to SVG.
    /// - Parameter pageWidth: SVG width in user-units. Pass 0 for the default (820).
    static func renderFile(at path: String, pageWidth: Double = 0) -> String? {
        guard let cResult = scorelib_render_file(path, pageWidth) else {
            return nil
        }
        let svg = String(cString: cResult)
        scorelib_free_string(cResult)
        return svg
    }

    /// Render MusicXML data (bytes) to SVG.
    /// - Parameter pageWidth: SVG width in user-units. Pass 0 for the default (820).
    static func renderData(_ data: Data, extension ext: String? = nil, pageWidth: Double = 0) -> String? {
        let result: UnsafeMutablePointer<CChar>? = data.withUnsafeBytes { buffer in
            guard let baseAddress = buffer.baseAddress?.assumingMemoryBound(to: UInt8.self) else {
                return nil
            }
            if let ext = ext {
                return ext.withCString { extPtr in
                    scorelib_render_bytes(baseAddress, buffer.count, extPtr, pageWidth)
                }
            } else {
                return scorelib_render_bytes(baseAddress, buffer.count, nil, pageWidth)
            }
        }

        guard let cResult = result else {
            return nil
        }
        let svg = String(cString: cResult)
        scorelib_free_string(cResult)
        return svg
    }
}
