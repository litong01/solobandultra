import Foundation

/// Swift wrapper around the Rust scorelib C FFI.
enum ScoreLib {

    /// Render a MusicXML file at the given path to SVG.
    static func renderFile(at path: String) -> String? {
        guard let cResult = scorelib_render_file(path) else {
            return nil
        }
        let svg = String(cString: cResult)
        scorelib_free_string(cResult)
        return svg
    }

    /// Render MusicXML data (bytes) to SVG.
    static func renderData(_ data: Data, extension ext: String? = nil) -> String? {
        let result: UnsafeMutablePointer<CChar>? = data.withUnsafeBytes { buffer in
            guard let baseAddress = buffer.baseAddress?.assumingMemoryBound(to: UInt8.self) else {
                return nil
            }
            if let ext = ext {
                return ext.withCString { extPtr in
                    scorelib_render_bytes(baseAddress, buffer.count, extPtr)
                }
            } else {
                return scorelib_render_bytes(baseAddress, buffer.count, nil)
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
