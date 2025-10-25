#!/bin/bash

# Script to copy or link Rust binary to Flutter Linux build
# Usage: copy_rust_binary.sh <install_prefix> <build_type>
set -e

echo "========== Copying Rust Binary =========="

INSTALL_PREFIX="$1"
CMAKE_BUILD_TYPE="$2"

# Determine build type from CMAKE_BUILD_TYPE (Debug, Release, or Profile)
if [ "${CMAKE_BUILD_TYPE}" == "Release" ] || [ "${CMAKE_BUILD_TYPE}" == "Profile" ]; then
    BUILD_TYPE="release"
else
    BUILD_TYPE="debug"
fi

echo "Build Type: ${BUILD_TYPE}"

# Paths
SCRIPT_DIR="$(dirname "$(realpath "${BASH_SOURCE[0]}")")"
PROJECT_ROOT="$(realpath "${SCRIPT_DIR}/../../..")"
DEST_DIR="${INSTALL_PREFIX}"
DEST_BINARY="${DEST_DIR}/fungi"

# Try to find Rust binary in multiple locations
# 1. Standard location (dev environment): target/{debug,release}/fungi
# 2. CI environment with target triple: target/{triple}/{debug,release}/fungi
RUST_BINARY_PATH=""
POSSIBLE_PATHS=(
    "${PROJECT_ROOT}/target/${BUILD_TYPE}/fungi"
    "${PROJECT_ROOT}/target/x86_64-unknown-linux-gnu/${BUILD_TYPE}/fungi"
    "${PROJECT_ROOT}/target/aarch64-unknown-linux-gnu/${BUILD_TYPE}/fungi"
)

for path in "${POSSIBLE_PATHS[@]}"; do
    if [ -f "${path}" ]; then
        RUST_BINARY_PATH="${path}"
        break
    fi
done

echo "Source: ${RUST_BINARY_PATH}"
echo "Destination: ${DEST_BINARY}"

# Check if Rust binary exists
if [ -z "${RUST_BINARY_PATH}" ] || [ ! -f "${RUST_BINARY_PATH}" ]; then
    echo "Error: Rust binary not found!"
    echo "Searched in:"
    for path in "${POSSIBLE_PATHS[@]}"; do
        echo "  - ${path}"
    done
    echo "Please run: cargo build --bin fungi $([ "${BUILD_TYPE}" == "release" ] && echo "--release")"
    exit 1
fi

# Create destination directory
mkdir -p "${DEST_DIR}"

# Remove existing binary/symlink
rm -f "${DEST_BINARY}"

# Debug: symlink; Release: copy
if [ "${BUILD_TYPE}" == "debug" ]; then
    ln -s "$(realpath "${RUST_BINARY_PATH}")" "${DEST_BINARY}"
    echo "Created symlink (debug)"
else
    cp "${RUST_BINARY_PATH}" "${DEST_BINARY}"
    chmod +x "${DEST_BINARY}"
    echo "Copied binary (release)"
fi

echo "========== Done =========="
