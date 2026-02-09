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
    # lipo ships with Xcode Command Line Tools — no Rust needed.
    echo "→ Creating universal simulator library (lipo on host)..."
    mkdir -p "$IOS_LIB_DIR"
    lipo -create \
        "$RUST_SRC/target/aarch64-apple-ios-sim/release/libscorelib.a" \
        "$RUST_SRC/target/x86_64-apple-ios/release/libscorelib.a" \
        -output "$IOS_LIB_DIR/libscorelib.a"

    echo "✓ iOS: $IOS_LIB_DIR/libscorelib.a"
    lipo -info "$IOS_LIB_DIR/libscorelib.a"
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
