#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
DOCKERFILE_PATH="$ROOT_DIR/scripts/android-builder.Dockerfile"
ANDROID_NDK_VERSION="${ANDROID_NDK_VERSION:-29.0.13846066}"
ANDROID_PLATFORM="${ANDROID_PLATFORM:-24}"
RUST_TOOLCHAIN="${RUST_TOOLCHAIN:-stable}"
OUTPUT_TAR="${OUTPUT_TAR:-fungi-android-aarch64.tar.gz}"
ANDROID_BUILD_IMAGE="${ANDROID_BUILD_IMAGE:-fungi-android-builder}"
ANDROID_BUILD_IMAGE_TAG="${ANDROID_BUILD_IMAGE_TAG:-ndk-${ANDROID_NDK_VERSION}}"
ANDROID_BUILD_REF="${ANDROID_BUILD_IMAGE}:${ANDROID_BUILD_IMAGE_TAG}"
CARGO_REGISTRY_VOLUME="${CARGO_REGISTRY_VOLUME:-fungi-android-cargo-registry}"
CARGO_GIT_VOLUME="${CARGO_GIT_VOLUME:-fungi-android-cargo-git}"
TARGET_VOLUME="${TARGET_VOLUME:-fungi-android-target}"
RUSTFLAGS_STAMP_FILE=".android-rustflags"

if ! docker image inspect "$ANDROID_BUILD_REF" >/dev/null 2>&1; then
  docker build \
    --platform linux/amd64 \
    --build-arg ANDROID_NDK_VERSION="$ANDROID_NDK_VERSION" \
    --build-arg RUST_TOOLCHAIN="$RUST_TOOLCHAIN" \
    -f "$DOCKERFILE_PATH" \
    -t "$ANDROID_BUILD_REF" \
    "$ROOT_DIR"
fi

docker volume create "$CARGO_REGISTRY_VOLUME" >/dev/null
docker volume create "$CARGO_GIT_VOLUME" >/dev/null
docker volume create "$TARGET_VOLUME" >/dev/null

docker run --rm \
  --platform linux/amd64 \
  -v "$CARGO_REGISTRY_VOLUME:/opt/cargo/registry" \
  -v "$CARGO_GIT_VOLUME:/opt/cargo/git" \
  -v "$TARGET_VOLUME:/workspace/target" \
  -v "$ROOT_DIR:/workspace" \
  -w /workspace \
  -e ANDROID_NDK_VERSION="$ANDROID_NDK_VERSION" \
  -e ANDROID_PLATFORM="$ANDROID_PLATFORM" \
  -e RUST_TOOLCHAIN="$RUST_TOOLCHAIN" \
  -e OUTPUT_TAR="$OUTPUT_TAR" \
  -e RUSTFLAGS_STAMP_FILE="$RUSTFLAGS_STAMP_FILE" \
  "$ANDROID_BUILD_REF" \
  bash -lc '
    set -euo pipefail

    export ANDROID_NDK_HOME="$ANDROID_SDK_ROOT/ndk/$ANDROID_NDK_VERSION"
    export ANDROID_NDK_ROOT="$ANDROID_NDK_HOME"
    export ANDROID_NDK="$ANDROID_NDK_HOME"
    export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android21-clang"
    export CARGO_TARGET_AARCH64_LINUX_ANDROID_AR="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-ar"

    stamp_path="target/$RUSTFLAGS_STAMP_FILE"
    previous_rustflags=""
    if [ -f "$stamp_path" ]; then
      previous_rustflags="$(cat "$stamp_path")"
    fi
    if [ "$previous_rustflags" != "$RUSTFLAGS" ]; then
      echo "Android RUSTFLAGS changed; cleaning cached aarch64-linux-android artifacts"
      rm -rf target/aarch64-linux-android
      rm -f libfungi.so "$OUTPUT_TAR"
    fi

    cargo ndk -P "$ANDROID_PLATFORM" -t arm64-v8a build --bin fungi -r

    printf "%s" "$RUSTFLAGS" > "$stamp_path"

    cp target/aarch64-linux-android/release/fungi libfungi.so
    tar -czf "$OUTPUT_TAR" libfungi.so

    printf "Built artifacts:\n  %s/libfungi.so\n  %s/%s\nRust flags: %s\n" "/workspace" "/workspace" "$OUTPUT_TAR" "$RUSTFLAGS"
  '