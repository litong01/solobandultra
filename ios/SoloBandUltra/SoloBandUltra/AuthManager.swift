import Foundation
import KindeSDK

/// Actions that require authentication and should be deferred until login completes.
enum PendingAuthAction {
    case showSettings
    case openFile
    case pasteLink
    case loadExternal(Data, String) // (fileData, fileName)
}

/// Centralized authentication manager wrapping the Kinde SDK.
///
/// Provides observable auth state for SwiftUI and a "pending action" pattern:
/// when a gated action is attempted while logged out, the action is stored,
/// login is triggered, and on success the action is executed by the UI.
class AuthManager: ObservableObject {
    @Published var isAuthenticated: Bool = false
    @Published var pendingAction: PendingAuthAction?

    init() {
        isAuthenticated = KindeSDKAPI.auth.isAuthenticated()
    }

    // MARK: - Login

    /// Trigger the Kinde login flow.
    /// Optionally store an action to execute after successful authentication.
    func login(then action: PendingAuthAction? = nil) {
        pendingAction = action
        KindeSDKAPI.auth.login { [weak self] result in
            DispatchQueue.main.async {
                switch result {
                case .success:
                    self?.isAuthenticated = true
                    // pendingAction is consumed by the UI via onChange
                case .failure(let error):
                    self?.pendingAction = nil
                    if !KindeSDKAPI.auth.isUserCancellationErrorCode(error) {
                        print("[Auth] Login failed: \(error.localizedDescription)")
                    }
                }
            }
        }
    }

    // MARK: - Register

    /// Trigger the Kinde registration flow.
    func register(then action: PendingAuthAction? = nil) {
        pendingAction = action
        KindeSDKAPI.auth.register { [weak self] result in
            DispatchQueue.main.async {
                switch result {
                case .success:
                    self?.isAuthenticated = true
                case .failure(let error):
                    self?.pendingAction = nil
                    if !KindeSDKAPI.auth.isUserCancellationErrorCode(error) {
                        print("[Auth] Registration failed: \(error.localizedDescription)")
                    }
                }
            }
        }
    }

    // MARK: - Logout

    /// Log the user out of Kinde.
    func logout() {
        KindeSDKAPI.auth.logout { [weak self] success in
            DispatchQueue.main.async {
                if success {
                    self?.isAuthenticated = false
                    self?.pendingAction = nil
                }
            }
        }
    }
}
