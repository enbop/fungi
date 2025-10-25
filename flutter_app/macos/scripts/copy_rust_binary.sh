#!/bin/bash

# Script to copy or link Rust binary to Flutter macOS build
set -e

echo "========== Copying Rust Binary =========="

# Determine build type (debug or release)
if [ "${CONFIGURATION}" == "Debug" ] || [ "${CONFIGURATION}" == "" ]; then
    BUILD_TYPE="debug"
else
    BUILD_TYPE="release"
fi

echo "Build Type: ${BUILD_TYPE}"

# Paths
PROJECT_ROOT="${SRCROOT}/../.."
DEST_DIR="${BUILT_PRODUCTS_DIR}/${PRODUCT_NAME}.app/Contents/Resources"
DEST_BINARY="${DEST_DIR}/fungi"

# Try to find Rust binary in multiple locations
# 1. Standard location (dev environment): target/{debug,release}/fungi
# 2. Universal binary location (CI): target/universal-apple-darwin/{release}/fungi
# 3. CI environment with target triple: target/{triple}/{debug,release}/fungi
RUST_BINARY_PATH=""
POSSIBLE_PATHS=(
    "${PROJECT_ROOT}/target/${BUILD_TYPE}/fungi"
    "${PROJECT_ROOT}/target/universal-apple-darwin/${BUILD_TYPE}/fungi"
    "${PROJECT_ROOT}/target/x86_64-apple-darwin/${BUILD_TYPE}/fungi"
    "${PROJECT_ROOT}/target/aarch64-apple-darwin/${BUILD_TYPE}/fungi"
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
    ln -s "${RUST_BINARY_PATH}" "${DEST_BINARY}"
    echo "Created symlink (debug)"
else
    cp "${RUST_BINARY_PATH}" "${DEST_BINARY}"
    chmod +x "${DEST_BINARY}"
    echo "Copied binary (release)"
fi

echo "========== Done =========="
