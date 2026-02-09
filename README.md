# SoloBand Ultra

A native iOS and Android mobile app for rendering and playing MusicXML sheet music, powered by Rust libraries.

## Project Structure

```
solobandultra/
├── sheetmusic/              # MusicXML files
│   └── asa-branca.musicxml  # "Asa Branca" by Luiz Gonzaga
├── ios/                     # Native iOS app (SwiftUI)
│   └── SoloBandUltra/
│       ├── SoloBandUltra.xcodeproj/
│       └── SoloBandUltra/   # Swift source files
├── android/                 # Native Android app (Kotlin/Jetpack Compose)
│   ├── app/                 # Android application module
│   ├── build.gradle.kts     # Root build configuration
│   └── settings.gradle.kts  # Project settings
└── README.md
```

## iOS App

### Prerequisites

- **Xcode 15.0+** (with iOS 16.0+ SDK)
- macOS Sonoma or later recommended
- If Xcode is not installed, download it from the [Mac App Store](https://apps.apple.com/us/app/xcode/id497799835) or [Apple Developer Downloads](https://developer.apple.com/download/applications/)

### Opening the Project in Xcode

**Option A -- From the terminal:**

```bash
open ios/SoloBandUltra/SoloBandUltra.xcodeproj
```

This launches Xcode and opens the project directly.

**Option B -- From Xcode:**

1. Launch **Xcode** (from `/Applications` or Spotlight search)
2. On the Welcome screen, click **Open Existing Project** (or go to **File > Open...**)
3. Navigate to `ios/SoloBandUltra/` and select `SoloBandUltra.xcodeproj`
4. Click **Open**

### Building & Running

1. In Xcode, select the **SoloBandUltra** scheme from the scheme selector in the toolbar (should be auto-selected)
2. Choose an iOS Simulator destination from the device dropdown (e.g., **iPhone 16**)
3. Press **Cmd+R** to build and run

Alternatively, build from the command line:

```bash
cd ios/SoloBandUltra
xcodebuild -project SoloBandUltra.xcodeproj \
  -scheme SoloBandUltra \
  -destination 'generic/platform=iOS Simulator' \
  -configuration Debug build
```

### Audio in Silent Mode (iOS)

The app is configured with `AVAudioSession` category `.playback`, which ensures audio plays even when the device's mute/silent switch is on. The `Info.plist` also includes `UIBackgroundModes: audio` for background playback support.

Key files:
- `AudioSessionManager.swift` - Manages AVAudioSession lifecycle, route changes, and interruptions
- `SoloBandUltraApp.swift` - Configures the audio session on app launch

## Android App

### Prerequisites

- **Android Studio Hedgehog (2023.1.1)** or later
- **Android SDK 34** (API 34)
- **JDK 17** (bundled with Android Studio)
- If Android Studio is not installed, download it from [developer.android.com/studio](https://developer.android.com/studio)

### Opening the Project in Android Studio

**Option A -- From the terminal (macOS):**

```bash
open -a "Android Studio" android/
```

This launches Android Studio and opens the project directly.

**Option B -- From Android Studio:**

1. Launch **Android Studio** (from `/Applications` or Spotlight search on macOS)
2. On the Welcome screen, click **Open** (or go to **File > Open...**)
3. Navigate to the `android/` directory inside this repository and select it
4. Click **Open**
5. Android Studio will detect the Gradle project and begin syncing -- wait for the sync to complete (this may take a few minutes on the first run as it downloads the Gradle wrapper and dependencies)

> **Note:** If prompted to install missing SDK components or update Gradle, follow the on-screen instructions to accept and install.

### Building & Running

1. Once Gradle sync completes, select a device or emulator from the **device dropdown** in the toolbar
   - To create an emulator: go to **Tools > Device Manager > Create Virtual Device** and follow the wizard
2. Click the **Run** button (green play icon) or press **Shift+F10**

### Audio in Silent Mode (Android)

On Android, "silent mode" affects the ringer/notification stream, not the media stream. The app uses `AudioAttributes` with `USAGE_MEDIA` and `CONTENT_TYPE_MUSIC`, ensuring audio plays through the media volume channel which is independent of the ringer mode. As long as the media volume is not zero, audio will play regardless of silent/vibrate mode.

Key files:
- `AudioSessionManager.kt` - Manages audio focus, audio attributes, and media stream configuration
- `MainActivity.kt` - Initializes audio session and sets volume control to media stream

## Architecture

### Current State

Both apps currently display a placeholder UI with:
- Title bar ("SoloBand Ultra")
- Sheet music display area (placeholder staff lines)
- Playback controls (play/pause, stop, metronome)
- Proper audio session configuration for silent mode playback

### Future: Rust Integration

The apps are designed to integrate with Rust libraries via FFI for:
- **MusicXML parsing** - Reading and interpreting MusicXML files
- **Score rendering** - Rendering sheet music notation to a canvas/view
- **Audio playback** - Synthesizing and playing the musical score

The Rust libraries will be compiled as:
- **iOS**: Static library (`.a`) linked via Xcode
- **Android**: Shared library (`.so`) loaded via JNI

## CI/CD — GitHub Actions

The project includes a GitHub Actions workflow (`.github/workflows/build-and-release.yml`) that builds both iOS and Android bundles and publishes them as a GitHub Release.

### Triggering a Release

**Automatic — push a version tag:**

```bash
git tag v1.0.0
git push origin v1.0.0
```

This triggers the workflow, builds both platforms in parallel, and creates a GitHub Release with the artifacts attached.

**Manual — from the GitHub UI:**

1. Go to **Actions** tab in your GitHub repository
2. Select the **Build & Release** workflow on the left
3. Click **Run workflow**
4. Enter a release tag name (e.g., `v1.0.0`) and click **Run workflow**

### Build Artifacts

| Platform | Artifact | Description |
|----------|----------|-------------|
| iOS | `SoloBandUltra-iOS-Simulator.zip` | Simulator `.app` bundle (unsigned) |
| Android | `SoloBandUltra-Android-debug.apk` | Debug APK, installable directly on devices |
| Android | `SoloBandUltra-Android-release.aab` | Release AAB for Google Play Store upload |

> **Note:** The iOS build is for Simulator testing only. To produce a signed IPA for physical devices, add your Apple Developer certificate and provisioning profile as GitHub Secrets and update the workflow accordingly.

## Tech Stack

| Component | iOS | Android |
|-----------|-----|---------|
| Language | Swift 5.9 | Kotlin 1.9 |
| UI Framework | SwiftUI | Jetpack Compose |
| Min OS | iOS 16.0 | Android 8.0 (API 26) |
| Audio | AVFoundation | AudioManager |
| Build Tool | Xcode 15 | Gradle 8.5 / AGP 8.2 |
