import Foundation
import SwiftUI

/// User-selectable app language. When `preferredCode` is empty, system locale is used.
/// Persisted in UserDefaults under "app_language".
final class AppLanguage: ObservableObject {
    static let key = "app_language"

    @Published var preferredCode: String {
        didSet {
            UserDefaults.standard.set(preferredCode, forKey: Self.key)
        }
    }

    init() {
        self.preferredCode = UserDefaults.standard.string(forKey: Self.key) ?? ""
    }

    /// Bundle for the given language code (e.g. "en", "zh-Hans"). Returns main bundle for system/default.
    static func bundle(for code: String) -> Bundle {
        guard !code.isEmpty,
              let path = Bundle.main.path(forResource: code, ofType: "lproj"),
              let bundle = Bundle(path: path) else {
            return .main
        }
        return bundle
    }

    /// Localized string for key using the given language code (nil = system).
    static func string(_ key: String, language code: String?) -> String {
        let b = bundle(for: code ?? "")
        return b.localizedString(forKey: key, value: key, table: nil)
    }
}

/// Convenience: use from a view that has `@EnvironmentObject var appLanguage: AppLanguage`.
enum L10n {
    static func string(_ key: String, language code: String?) -> String {
        AppLanguage.string(key, language: code)
    }
}
