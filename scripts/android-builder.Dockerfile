FROM --platform=linux/amd64 ubuntu:22.04

ARG ANDROID_NDK_VERSION=29.0.13846066
ARG RUST_TOOLCHAIN=stable
ARG ANDROID_SDK_PLATFORM=android-33
ARG ANDROID_BUILD_TOOLS=34.0.0
ARG ANDROID_CMAKE_VERSION=3.22.1

ENV DEBIAN_FRONTEND=noninteractive \
    ANDROID_SDK_ROOT=/opt/android-sdk \
    ANDROID_NDK_VERSION=${ANDROID_NDK_VERSION} \
    CARGO_HOME=/opt/cargo \
    RUSTUP_HOME=/opt/rustup \
    PATH=/opt/cargo/bin:/opt/android-sdk/cmdline-tools/latest/bin:/opt/rustup/toolchains/${RUST_TOOLCHAIN}-x86_64-unknown-linux-gnu/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin \
    AWS_LC_SYS_PREBUILT_NASM=1

RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    git \
    unzip \
    zip \
    xz-utils \
    openjdk-17-jdk \
    protobuf-compiler \
    pkg-config \
    build-essential \
    cmake \
    ninja-build \
    && rm -rf /var/lib/apt/lists/*

RUN mkdir -p "$ANDROID_SDK_ROOT/cmdline-tools" \
    && curl -fsSL -o /tmp/cmdline-tools.zip https://dl.google.com/android/repository/commandlinetools-linux-11076708_latest.zip \
    && unzip -q /tmp/cmdline-tools.zip -d /tmp/android-cmdline-tools \
    && mv /tmp/android-cmdline-tools/cmdline-tools "$ANDROID_SDK_ROOT/cmdline-tools/latest" \
    && rm -rf /tmp/cmdline-tools.zip /tmp/android-cmdline-tools

RUN bash -lc 'set -euo pipefail \
    && set +o pipefail \
    && yes | "$ANDROID_SDK_ROOT/cmdline-tools/latest/bin/sdkmanager" --sdk_root="$ANDROID_SDK_ROOT" --licenses >/dev/null \
    && set -o pipefail \
    && "$ANDROID_SDK_ROOT/cmdline-tools/latest/bin/sdkmanager" --sdk_root="$ANDROID_SDK_ROOT" --update \
    && "$ANDROID_SDK_ROOT/cmdline-tools/latest/bin/sdkmanager" --sdk_root="$ANDROID_SDK_ROOT" \
      "platforms;${ANDROID_SDK_PLATFORM}" \
      "build-tools;${ANDROID_BUILD_TOOLS}" \
      "cmake;${ANDROID_CMAKE_VERSION}" \
      "ndk;${ANDROID_NDK_VERSION}"'

RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --default-toolchain "$RUST_TOOLCHAIN" \
    && rustup target add aarch64-linux-android \
    && cargo install cargo-ndk

WORKDIR /workspace