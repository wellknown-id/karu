#!/usr/bin/env bash
set -euo pipefail

# Build Karu shared libraries for Android using cargo-ndk.
#
# Prerequisites:
#   cargo install cargo-ndk
#   Android NDK installed (set ANDROID_NDK_HOME if not in default location)
#
# Usage:
#   ./scripts/build-android.sh [--release]
#
# Output:
#   dist/android/
#     arm64-v8a/libkaru.so
#     armeabi-v7a/libkaru.so
#     x86_64/libkaru.so
#     include/karu.h

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

PROFILE="debug"
CARGO_FLAG=""
if [[ "${1:-}" == "--release" ]]; then
    PROFILE="release"
    CARGO_FLAG="--release"
fi

TARGETS=(
    "aarch64-linux-android:arm64-v8a"
    "armv7-linux-androideabi:armeabi-v7a"
    "x86_64-linux-android:x86_64"
)

OUT_DIR="$PROJECT_ROOT/dist/android"
rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR/include"

echo "Building Karu for Android ($PROFILE)..."

for entry in "${TARGETS[@]}"; do
    TARGET="${entry%%:*}"
    ABI="${entry##*:}"

    echo "  → $ABI ($TARGET)"

    # Install Rust target if missing
    rustup target add "$TARGET" 2>/dev/null || true

    cargo ndk --target "$TARGET" -- build \
        -p karu \
        --features ffi \
        --no-default-features \
        $CARGO_FLAG \
        --manifest-path "$PROJECT_ROOT/Cargo.toml"

    mkdir -p "$OUT_DIR/$ABI"

    # Find built library
    SO_PATH="$PROJECT_ROOT/target/$TARGET/$PROFILE/libkaru.so"
    if [[ ! -f "$SO_PATH" ]]; then
        # Try cdylib name variant
        SO_PATH="$PROJECT_ROOT/target/$TARGET/$PROFILE/libkaru.so"
    fi

    if [[ -f "$SO_PATH" ]]; then
        cp "$SO_PATH" "$OUT_DIR/$ABI/libkaru.so"
    else
        echo "  ⚠ No .so found for $TARGET at $SO_PATH"
        exit 1
    fi
done

# Copy header (generate first if needed)
HEADER="$PROJECT_ROOT/crates/karu/include/karu.h"
if [[ -f "$HEADER" ]]; then
    cp "$HEADER" "$OUT_DIR/include/karu.h"
else
    echo "  ⚠ karu.h not found at $HEADER — build with --features ffi first"
fi

echo ""
echo "Android build complete:"
find "$OUT_DIR" -type f | sort | sed 's|^|  |'
