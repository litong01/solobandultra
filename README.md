# SoloBand Ultra

A native iOS and Android mobile app for rendering and playing MusicXML sheet music, powered by a Rust rendering engine.

## Project Structure

```
solobandultra/
├── sheetmusic/              # MusicXML sample files
│   ├── asa-branca.musicxml  # "Asa Branca" by Luiz Gonzaga (uncompressed)
│   └── 童年.mxl              # "Childhood" by 罗大佑 (compressed MXL)
├── rust/                    # Rust libraries
│   └── scorelib/            # MusicXML parser & SVG score renderer
│       ├── src/
│       │   ├── lib.rs       # Public API + C FFI + convenience functions
│       │   ├── model.rs     # Data model (Score, Part, Measure, Note, etc.)
│       │   ├── parser.rs    # MusicXML XML parser
│       │   ├── mxl.rs       # Compressed MXL (ZIP) support
│       │   ├── renderer.rs  # SVG score rendering engine
│       │   └── android.rs   # JNI bindings for Android
│       └── tests/           # Integration tests
├── ios/                     # Native iOS app (SwiftUI)
│   └── SoloBandUltra/
│       ├── SoloBandUltra.xcodeproj/
│       ├── include/         # C headers for Rust FFI
│       ├── lib/             # Rust static library (built by build-rust.sh)
│       └── SoloBandUltra/   # Swift source files
├── android/                 # Native Android app (Kotlin/Jetpack Compose)
│   ├── app/
│   │   └── src/main/
│   │       ├── jniLibs/     # Rust shared libraries (built by build-rust.sh)
│   │       └── assets/      # MusicXML sample files
│   ├── build.gradle.kts
│   └── settings.gradle.kts
├── build-rust.sh            # Script to build Rust for iOS & Android
└── README.md
```

## Getting Started

### Prerequisites

- **[Docker Desktop](https://www.docker.com/products/docker-desktop/)** — Rust compilation runs entirely in a container; no Rust/Cargo installation needed on your system
- **Xcode 15.0+** — for building/running the iOS app (also provides `lipo` for creating universal libraries)
- **Android Studio** — for building/running the Android app

### Building the Rust Libraries

A convenience script builds the Rust scorelib inside a Docker container:

```bash
./build-rust.sh            # Build for both iOS and Android
./build-rust.sh ios        # Build for iOS only
./build-rust.sh android    # Build for Android only
./build-rust.sh test       # Run Rust tests
```

The first run builds the Docker image (downloads Rust + Android NDK, ~2 min). Subsequent runs use the cached image and cached Cargo registry, so incremental builds are fast.

This compiles the Rust code and places the binaries where each platform expects them:
- **iOS**: `ios/SoloBandUltra/lib/libscorelib.a` (universal ARM64+x86_64 simulator)
- **Android**: `android/app/src/main/jniLibs/{arm64-v8a,x86_64}/libscorelib.so`

> **How it works:** Rust compilation happens inside a Linux container. For iOS, static libraries (`.a`) are just archives of object files — no Apple linker needed. The host's `lipo` (from Xcode) merges the architectures. For Android, the container includes the NDK linker to produce `.so` shared libraries.

## Rust Score Library (`scorelib`)

The core Rust library provides MusicXML parsing and SVG rendering:

```rust
use scorelib::{parse_file, render_score_to_svg};

let score = parse_file("sheetmusic/asa-branca.musicxml").unwrap();
println!("Title: {:?}", score.title);

let svg = render_score_to_svg(&score);
std::fs::write("output.svg", &svg).unwrap();
```

### Features

- **MusicXML parsing** — Reads standard `.musicxml` (uncompressed) and `.mxl` (ZIP compressed) files
- **Score rendering** — Produces clean SVG output with:
  - Staff lines, clefs (treble, bass, alto), key signatures, time signatures
  - Notes (filled/hollow noteheads, stems, flags, beams)
  - Rests (whole, half, quarter, eighth, sixteenth)
  - Accidentals (sharp, flat, natural)
  - Barlines (single, double, repeat signs with dots)
  - Chord symbols (harmony annotations)
  - Ledger lines, dots, volta brackets
  - Title and composer header
- **Cross-platform FFI** — C API for iOS, JNI for Android
- **Auto-detection** — Determines format from extension or content

### Running Tests

```bash
./build-rust.sh test
```

Tests parse both sample files and render them to SVG in `rust/scorelib/test_output/` for visual inspection.

## iOS App

### Prerequisites

- **Xcode 15.0+** (with iOS 16.0+ SDK)
- macOS Sonoma or later recommended
- Docker Desktop running
- Rust library built (`./build-rust.sh ios`)

### Opening the Project in Xcode

**From the terminal:**

```bash
open ios/SoloBandUltra/SoloBandUltra.xcodeproj
```

**From Xcode:**

1. Launch **Xcode**
2. **File > Open...** → navigate to `ios/SoloBandUltra/` and select `SoloBandUltra.xcodeproj`

### Building & Running

1. First build the Rust library: `./build-rust.sh ios`
2. In Xcode, select the **SoloBandUltra** scheme
3. Choose an iOS Simulator destination (e.g., **iPhone 16**)
4. Press **Cmd+R** to build and run

The app displays a segmented picker to switch between the two sample scores. Each score is parsed by the Rust library, rendered to SVG, and displayed in a scrollable/zoomable WebView.

### Audio in Silent Mode (iOS)

The app uses `AVAudioSession` with `.playback` category, ensuring audio plays even when the mute switch is on.

## Android App

### Prerequisites

- **Android Studio Hedgehog (2023.1.1)** or later
- **Android SDK 34**
- Docker Desktop running
- Rust library built (`./build-rust.sh android`)

### Opening the Project in Android Studio

**From the terminal (macOS):**

```bash
open -a "Android Studio" android/
```

**From Android Studio:**

1. **File > Open...** → select the `android/` directory
2. Wait for Gradle sync to complete

### Building & Running

1. First build the Rust library: `./build-rust.sh android`
2. Select a device or emulator in Android Studio
3. Click **Run** or press **Shift+F10**

The app loads MusicXML files from assets, renders them to SVG using the Rust library via JNI, and displays the result in a zoomable WebView.

### Audio in Silent Mode (Android)

The app uses `AudioAttributes` with `USAGE_MEDIA` and `CONTENT_TYPE_MUSIC`, so audio plays through the media volume channel independent of ringer mode.

## CI/CD — GitHub Actions

The project includes a GitHub Actions workflow (`.github/workflows/build-and-release.yml`) that builds both platforms and publishes GitHub Releases.

### Triggering a Release

```bash
git tag v1.0.0
git push origin v1.0.0
```

Or manually from the GitHub Actions tab.

### Build Artifacts

| Platform | Artifact | Description |
|----------|----------|-------------|
| iOS | `SoloBandUltra-iOS-Simulator.zip` | Simulator `.app` bundle (unsigned) |
| Android | `SoloBandUltra-Android-debug.apk` | Debug APK for device testing |
| Android | `SoloBandUltra-Android-release.aab` | Release AAB for Google Play |

## Tech Stack

| Component | iOS | Android | Shared |
|-----------|-----|---------|--------|
| Language | Swift 5.9 | Kotlin 1.9 | Rust |
| UI Framework | SwiftUI | Jetpack Compose | — |
| Score Rendering | — | — | Rust → SVG |
| SVG Display | WKWebView | WebView | — |
| Min OS | iOS 16.0 | Android 8.0 (API 26) | — |
| Audio | AVFoundation | AudioManager | — |
| Build Tool | Xcode 15 | Gradle 8.5 / AGP 8.2 | Cargo (in Docker) |
