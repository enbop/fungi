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
# TODO release universal binary support
PROJECT_ROOT="${SRCROOT}/../.."
RUST_BINARY_PATH="${PROJECT_ROOT}/target/${BUILD_TYPE}/fungi"
DEST_DIR="${BUILT_PRODUCTS_DIR}/${PRODUCT_NAME}.app/Contents/Resources"
DEST_BINARY="${DEST_DIR}/fungi"

echo "Source: ${RUST_BINARY_PATH}"
echo "Destination: ${DEST_BINARY}"

# Check if Rust binary exists
if [ ! -f "${RUST_BINARY_PATH}" ]; then
    echo "Error: Rust binary not found!"
    echo "Please run: cargo build $([ "${BUILD_TYPE}" == "release" ] && echo "--release")"
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
