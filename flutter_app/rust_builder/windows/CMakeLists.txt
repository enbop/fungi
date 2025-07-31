# The Flutter tooling requires that developers have a version of Visual Studio
# installed that includes CMake 3.14 or later. You should not increase this
# version, as doing so will cause the plugin to fail to compile for some
# customers of the plugin.
cmake_minimum_required(VERSION 3.14)

# Project-level configuration.
set(PROJECT_NAME "rust_lib_fungi_app")
project(${PROJECT_NAME} LANGUAGES CXX)

# skip cargokit to fix windows cmake build failure
# make sure run "cargo build --release" before "flutter run"
# include("../cargokit/cmake/cargokit.cmake")
# apply_cargokit(${PROJECT_NAME} ../../../../../../rust rust_lib_fungi_app "")

# Use the same logic as cargokit to determine Rust build configuration
# This mimics cargokit's BuildConfiguration mapping:
# - Debug -> debug
# - Release -> release  
# - Profile -> release (Profile uses release Rust artifacts)

# Create generator expressions to map Flutter config to Rust target directory
set(RUST_CONFIG "$<IF:$<CONFIG:Debug>,debug,release>")
set(BASE_TARGET_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../../../../../../../target")

# Prebuilt DLL name
set(DLL_NAME "rust_lib_fungi_app.dll")

# Use generator expressions to select the right DLL at build time (debug or release)
set(PREBUILT_DLL_PATH "$<IF:$<CONFIG:Debug>,${BASE_TARGET_DIR}/debug/${DLL_NAME},${BASE_TARGET_DIR}/release/${DLL_NAME}>")

message(STATUS "Selected DLL path (with generator expressions): ${PREBUILT_DLL_PATH}")
message(STATUS "This will resolve to the appropriate path at build time")
message(STATUS "==============================================")

# List of absolute paths to libraries that should be bundled with the plugin.
# This list could contain prebuilt libraries, or libraries created by an
# external build triggered from this build file.
set(rust_lib_fungi_app_bundled_libraries
  # "${${PROJECT_NAME}_cargokit_lib}"
  ${PREBUILT_DLL_PATH}
  PARENT_SCOPE
)
