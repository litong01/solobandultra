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
#   ./build-rust.sh test         # Run Rust tests
#

set -euo pipefail
cd "$(dirname "$0")"

DOCKER_IMAGE="scorelib-builder"
RUST_SRC="rust/scorelib"
IOS_LIB_DIR="ios/SoloBandUltra/lib"
ANDROID_JNI_DIR="android/app/src/main/jniLibs"
SHEETMUSIC_SRC="sheetmusic"
ANDROID_ASSETS_DIR="android/app/src/main/assets/sheetmusic"
IOS_RESOURCES_DIR="ios/SoloBandUltra/SoloBandUltra/SheetMusic"

# ─── Sync sheet music files ──────────────────────────────────────────

sync_sheetmusic() {
    echo "═══ Syncing sheet music files ═══"

    mkdir -p "$ANDROID_ASSETS_DIR" "$IOS_RESOURCES_DIR"

    # Remove stale files from targets that no longer exist in source
    for target_dir in "$ANDROID_ASSETS_DIR" "$IOS_RESOURCES_DIR"; do
        for f in "$target_dir"/*.musicxml "$target_dir"/*.mxl; do
            [ -e "$f" ] || continue
            basename="$(basename "$f")"
            if [ ! -e "$SHEETMUSIC_SRC/$basename" ]; then
                echo "  Removing stale: $target_dir/$basename"
                rm "$f"
            fi
        done
    done

    # Copy all sheet music files from source to both platforms
    count=0
    for f in "$SHEETMUSIC_SRC"/*.musicxml "$SHEETMUSIC_SRC"/*.mxl; do
        [ -e "$f" ] || continue
        basename="$(basename "$f")"
        cp "$f" "$ANDROID_ASSETS_DIR/$basename"
        cp "$f" "$IOS_RESOURCES_DIR/$basename"
        count=$((count + 1))
    done

    echo "✓ Synced $count sheet music file(s) to Android assets and iOS Resources"
    echo ""
}

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
    docker build --platform linux/amd64 -t "$DOCKER_IMAGE" -f Dockerfile.build . \
        | grep -E "^(Step|Successfully|CACHED)" || true
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
        bash -c "$1"
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
        sed -i '\''s/crate-type = .*/crate-type = ["lib", "staticlib"]/'\'' Cargo.toml

        echo "→ Building aarch64-apple-ios-sim (ARM64 Simulator)..."
        cargo build --release --target aarch64-apple-ios-sim 2>&1

        echo "→ Building x86_64-apple-ios (x86_64 Simulator)..."
        cargo build --release --target x86_64-apple-ios 2>&1

        echo "→ Building aarch64-apple-ios (ARM64 Device)..."
        cargo build --release --target aarch64-apple-ios 2>&1

        mv Cargo.toml.orig Cargo.toml
        echo "✓ iOS Rust compilation complete"
    '

    # Create universal (fat) simulator library using macOS lipo.
    # We can't put device arm64 and simulator arm64 in the same fat binary —
    # they share the architecture but have different platform markers.
    # XCFramework solves this by bundling separate slices per platform.
    echo "→ Creating universal simulator library (lipo on host)..."
    mkdir -p "$IOS_LIB_DIR"
    local SIM_FAT="$RUST_SRC/target/libscorelib-sim.a"
    lipo -create \
        "$RUST_SRC/target/aarch64-apple-ios-sim/release/libscorelib.a" \
        "$RUST_SRC/target/x86_64-apple-ios/release/libscorelib.a" \
        -output "$SIM_FAT"

    echo "→ Creating XCFramework..."
    local XCFW="$IOS_LIB_DIR/libscorelib.xcframework"
    local INCLUDE_DIR="ios/SoloBandUltra/include"

    # Remove any previous XCFramework (xcodebuild refuses to overwrite)
    rm -rf "$XCFW"
    # Also remove the old flat .a if present (no longer used)
    rm -f "$IOS_LIB_DIR/libscorelib.a"

    xcodebuild -create-xcframework \
        -library "$RUST_SRC/target/aarch64-apple-ios/release/libscorelib.a" \
        -headers "$INCLUDE_DIR" \
        -library "$SIM_FAT" \
        -headers "$INCLUDE_DIR" \
        -output "$XCFW"

    rm -f "$SIM_FAT"

    echo "✓ iOS: $XCFW"
    echo "  Device:    $(lipo -info "$XCFW/ios-arm64/libscorelib.a" 2>/dev/null || echo 'see xcframework')"
    echo "  Simulator: $(lipo -info "$XCFW/ios-arm64_x86_64-simulator/libscorelib.a" 2>/dev/null || echo 'see xcframework')"
    echo ""
}

# ─── Android build ──────────────────────────────────────────────────

build_android() {
    echo "═══ Building Rust for Android (in Docker container) ═══"

    docker_cargo '
        NDK_BIN="${ANDROID_NDK_HOME}/toolchains/llvm/prebuilt/linux-x86_64/bin"
        export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="${NDK_BIN}/aarch64-linux-android21-clang"
        export CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER="${NDK_BIN}/x86_64-linux-android21-clang"

        echo "→ Building aarch64-linux-android (arm64-v8a)..."
        cargo build --release --target aarch64-linux-android 2>&1

        echo "→ Building x86_64-linux-android (x86_64)..."
        cargo build --release --target x86_64-linux-android 2>&1

        echo "✓ Android Rust compilation complete"
    '

    echo "→ Copying .so files to Android jniLibs..."
    mkdir -p "$ANDROID_JNI_DIR/arm64-v8a" "$ANDROID_JNI_DIR/x86_64"
    cp "$RUST_SRC/target/aarch64-linux-android/release/libscorelib.so" "$ANDROID_JNI_DIR/arm64-v8a/"
    cp "$RUST_SRC/target/x86_64-linux-android/release/libscorelib.so"  "$ANDROID_JNI_DIR/x86_64/"

    echo "✓ Android: $ANDROID_JNI_DIR/arm64-v8a/libscorelib.so"
    echo "✓ Android: $ANDROID_JNI_DIR/x86_64/libscorelib.so"
    echo ""
}

# ─── Run tests ──────────────────────────────────────────────────────

run_tests() {
    echo "═══ Running Rust tests (in Docker container) ═══"

    docker_cargo '
        cargo test -- --nocapture 2>&1
    '

    echo ""
}

# ─── Main ───────────────────────────────────────────────────────────

check_docker

TARGET="${1:-all}"

sync_sheetmusic

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
        run_tests
        ;;
    all)
        ensure_image
        build_ios
        build_android
        ;;
    *)
        echo "Usage: $0 [ios|android|test|all]"
        exit 1
        ;;
esac

echo "═══ Build complete! ═══"
