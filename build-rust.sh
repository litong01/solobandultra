#!/bin/bash
#
# Build the Rust scorelib library for iOS and Android using Docker.
# No Rust/Cargo installation required on the host system — everything
# runs inside a container.
#
# Prerequisites:
#   - Docker Desktop (https://www.docker.com/products/docker-desktop/)
#   - Xcode Command Line Tools (for `lipo` on macOS, iOS builds only)
#
# Usage:
#   ./build-rust.sh              # Build all targets
#   ./build-rust.sh ios          # Build iOS targets only
#   ./build-rust.sh android      # Build Android targets only
#   ./build-rust.sh test         # Run all Rust tests
#   ./build-rust.sh test NAME    # Run tests matching NAME (e.g. top_layer or test_top_layer_print_summary)
#   ./build-rust.sh coverage     # Run tests with code coverage report
#

set -euo pipefail
cd "$(dirname "$0")"

DOCKER_IMAGE="scorelib-builder"
RUST_SRC="rust/scorelib"
# With rust/Cargo.toml workspace, both crates build into rust/target/, not $CRATE/target/
RUST_TARGET_DIR="rust/target"
IOS_LIB_DIR="ios/SoloBandUltra/lib"
ANDROID_JNI_DIR="android/app/src/main/jniLibs"
FONTS_SRC="assets/fonts"
IOS_FONTS_DST="ios/SoloBandUltra/SoloBandUltra/Fonts"
ANDROID_FONTS_DST="android/app/src/main/assets/fonts"
# ─── Preflight checks ──────────────────────────────────────────────

check_docker() {
    if ! command -v docker &>/dev/null; then
        echo "✗ Docker not found."
        echo "  Install Docker Desktop: https://www.docker.com/products/docker-desktop/"
        exit 1
    fi
    if ! docker info &>/dev/null 2>&1; then
        echo "✗ Docker daemon is not running."
        echo "  Start Docker Desktop and try again."
        exit 1
    fi
}

# ─── Build the Docker image (cached) ───────────────────────────────

ensure_image() {
    echo "═══ Ensuring Docker build image ($DOCKER_IMAGE)... ═══"
    # Don't swallow build failures — pipe through grep for cleaner output
    # but let a non-zero exit code from docker build propagate (pipefail is on).
    docker build --platform linux/amd64 -t "$DOCKER_IMAGE" -f Dockerfile.build .
    echo ""
}

# ─── Helper: run cargo inside the container ─────────────────────────

docker_cargo() {
    docker run --rm --platform linux/amd64 \
        -v "$(pwd):/project" \
        -w "/project/$RUST_SRC" \
        -v scorelib-cargo-registry:/usr/local/cargo/registry \
        -v scorelib-cargo-git:/usr/local/cargo/git \
        "$DOCKER_IMAGE" \
        bash -c "set -euo pipefail; $1"
}

# ─── Font deployment ────────────────────────────────────────────────
#
# Fonts (Lora, LXGW WenKai, JianpuASCII, etc.) live once in assets/fonts/
# (source of truth). This script deploys them to both platform destinations.
# Android and iOS also copy from assets/fonts/ during their own builds
# (Gradle copyFonts task, Xcode "Deploy Fonts" run script).

deploy_fonts() {
    if [ ! -d "$FONTS_SRC" ]; then
        echo "✗ Font source directory '$FONTS_SRC' not found."
        exit 1
    fi

    echo "→ Deploying bundled fonts..."

    # Android: copy .ttf files so WebView can load them via file:///android_asset/fonts/
    mkdir -p "$ANDROID_FONTS_DST"
    cp "$FONTS_SRC"/*.ttf "$ANDROID_FONTS_DST/"
    echo "  Android: $ANDROID_FONTS_DST"

    # iOS: copy font files into the Xcode bundle's Fonts/ folder.
    # The folder is registered as a folder-reference resource in project.pbxproj so
    # all files placed here are automatically included in the app bundle.
    # SheetMusicView loads them via @font-face relative URLs with
    # baseURL = Bundle.main.resourceURL.
    mkdir -p "$IOS_FONTS_DST"
    cp "$FONTS_SRC"/*.ttf "$IOS_FONTS_DST/"
    echo "  iOS:     $IOS_FONTS_DST"
}

# ─── iOS build ──────────────────────────────────────────────────────

build_ios() {
    echo "═══ Building Rust for iOS (in Docker container) ═══"

    # On Linux (in Docker), we can build staticlib for iOS targets without
    # the Apple SDK — static libraries are just archives of object files,
    # no linker invocation required. We temporarily remove cdylib from
    # crate-type since that DOES need an Apple linker.

    docker_cargo '
        echo "→ Adjusting Cargo.toml for iOS staticlib build..."
        cp Cargo.toml Cargo.toml.orig
        # Ensure Cargo.toml is restored even if a build step fails.
        trap "mv Cargo.toml.orig Cargo.toml 2>/dev/null || true" EXIT
        sed -i '\''s/crate-type = .*/crate-type = ["lib", "staticlib"]/'\'' Cargo.toml

        echo "→ Building aarch64-apple-ios-sim (ARM64 Simulator)..."
        cargo build --release --target aarch64-apple-ios-sim 2>&1

        echo "→ Building x86_64-apple-ios (x86_64 Simulator)..."
        cargo build --release --target x86_64-apple-ios 2>&1

        echo "→ Building aarch64-apple-ios (ARM64 Device)..."
        cargo build --release --target aarch64-apple-ios 2>&1

        # trap will restore Cargo.toml on exit
        echo "✓ iOS Rust compilation complete"
    '

    # Create universal (fat) simulator library using macOS lipo.
    # We can't put device arm64 and simulator arm64 in the same fat binary —
    # they share the architecture but have different platform markers.
    # XCFramework solves this by bundling separate slices per platform.
    echo "→ Creating universal simulator library (lipo on host)..."
    mkdir -p "$IOS_LIB_DIR"
    local SIM_FAT="$RUST_TARGET_DIR/libscorelib-sim.a"
    lipo -create \
        "$RUST_TARGET_DIR/aarch64-apple-ios-sim/release/libscorelib.a" \
        "$RUST_TARGET_DIR/x86_64-apple-ios/release/libscorelib.a" \
        -output "$SIM_FAT"

    echo "→ Creating XCFramework..."
    local XCFW="$IOS_LIB_DIR/libscorelib.xcframework"
    local INCLUDE_DIR="ios/SoloBandUltra/include"

    # Remove any previous XCFramework (xcodebuild refuses to overwrite)
    rm -rf "$XCFW"
    # Also remove the old flat .a if present (no longer used)
    rm -f "$IOS_LIB_DIR/libscorelib.a"

    xcodebuild -create-xcframework \
        -library "$RUST_TARGET_DIR/aarch64-apple-ios/release/libscorelib.a" \
        -headers "$INCLUDE_DIR" \
        -library "$SIM_FAT" \
        -headers "$INCLUDE_DIR" \
        -output "$XCFW"

    rm -f "$SIM_FAT"

    echo "✓ iOS: $XCFW"
    echo "  Device:    $(lipo -info "$XCFW/ios-arm64/libscorelib.a" 2>/dev/null || echo 'see xcframework')"
    echo "  Simulator: $(lipo -info "$XCFW/ios-arm64_x86_64-simulator/libscorelib.a" 2>/dev/null || echo 'see xcframework')"

    deploy_fonts
    echo ""
}

# ─── Android build ──────────────────────────────────────────────────

build_android() {
    echo "═══ Building Rust for Android (in Docker container) ═══"

    docker_cargo '
        NDK_BIN="${ANDROID_NDK_HOME}/toolchains/llvm/prebuilt/linux-x86_64/bin"
        export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="${NDK_BIN}/aarch64-linux-android21-clang"
        export CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER="${NDK_BIN}/x86_64-linux-android21-clang"

        # 16 KB page alignment required for Google Play (Android 15+).
        export CARGO_TARGET_AARCH64_LINUX_ANDROID_RUSTFLAGS="-C link-arg=-z -C link-arg=max-page-size=16384"
        export CARGO_TARGET_X86_64_LINUX_ANDROID_RUSTFLAGS="-C link-arg=-z -C link-arg=max-page-size=16384"

        echo "→ Building aarch64-linux-android (arm64-v8a)..."
        cargo build --release --target aarch64-linux-android 2>&1

        echo "→ Building x86_64-linux-android (x86_64)..."
        cargo build --release --target x86_64-linux-android 2>&1

        echo "✓ Android Rust compilation complete"
    '

    echo "→ Copying .so files to Android jniLibs..."
    mkdir -p "$ANDROID_JNI_DIR/arm64-v8a" "$ANDROID_JNI_DIR/x86_64"
    cp "$RUST_TARGET_DIR/aarch64-linux-android/release/libscorelib.so" "$ANDROID_JNI_DIR/arm64-v8a/"
    cp "$RUST_TARGET_DIR/x86_64-linux-android/release/libscorelib.so"  "$ANDROID_JNI_DIR/x86_64/"

    echo "✓ Android: $ANDROID_JNI_DIR/arm64-v8a/libscorelib.so"
    echo "✓ Android: $ANDROID_JNI_DIR/x86_64/libscorelib.so"

    deploy_fonts
    echo ""
}

# ─── Run tests ──────────────────────────────────────────────────────

run_tests() {
    echo "═══ Running Rust tests (in Docker container) ═══"
    # Pass any extra args to cargo test (e.g. test name filter). Examples:
    #   ./build-rust.sh test                    # all tests
    #   ./build-rust.sh test top_layer          # tests whose name contains 'top_layer'
    #   ./build-rust.sh test test_top_layer_print_summary  # one specific test
    docker_cargo "cargo test $* -- --nocapture 2>&1"
    echo ""
}

# ─── Run tests with coverage ────────────────────────────────────────

run_coverage() {
    echo "═══ Running Rust tests with coverage (in Docker container) ═══"

    # We use cargo-llvm-cov (LLVM source-based coverage) instead of
    # tarpaulin because it works reliably inside Docker/QEMU on ARM Macs.
    docker_cargo '
        if ! command -v cargo-llvm-cov &>/dev/null; then
            echo "→ Installing cargo-llvm-cov..."
            cargo install cargo-llvm-cov 2>&1
        fi

        echo "→ Installing llvm-tools component..."
        rustup component add llvm-tools-preview 2>&1

        echo "→ Running tests with coverage instrumentation..."
        cargo llvm-cov --no-clean -- --nocapture 2>&1
    '

    echo ""
}

# ─── Main ───────────────────────────────────────────────────────────

check_docker

TARGET="${1:-all}"

case "$TARGET" in
    ios)
        ensure_image
        build_ios
        ;;
    android)
        ensure_image
        build_android
        ;;
    test)
        ensure_image
        run_tests "${@:2}"
        ;;
    coverage)
        ensure_image
        run_coverage
        ;;
    all)
        ensure_image
        build_ios
        build_android
        ;;
    *)
        echo "Usage: $0 [ios|android|test|coverage|all]"
        exit 1
        ;;
esac

echo "═══ Build complete! ═══"
