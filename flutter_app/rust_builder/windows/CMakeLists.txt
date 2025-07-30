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

# Try multiple possible paths for the prebuilt DLL based on configuration
set(DLL_NAME "rust_lib_fungi_app.dll")
set(POSSIBLE_DLL_PATHS
    "${BASE_TARGET_DIR}/${RUST_CONFIG}/${DLL_NAME}"
    "${BASE_TARGET_DIR}/x86_64-pc-windows-msvc/${RUST_CONFIG}/${DLL_NAME}"
)

message(STATUS "=== Rust DLL Configuration Detection ===")
message(STATUS "Flutter configuration mapping:")
message(STATUS "  Debug -> debug Rust artifacts")
message(STATUS "  Release -> release Rust artifacts")
message(STATUS "  Profile -> release Rust artifacts")
message(STATUS "Current paths to try:")
foreach(PATH ${POSSIBLE_DLL_PATHS})
    message(STATUS "  - ${PATH}")
endforeach()

# Check which DLLs exist at configure time for both debug and release
set(DEBUG_DLL_PATH_1 "${BASE_TARGET_DIR}/debug/${DLL_NAME}")
set(DEBUG_DLL_PATH_2 "${BASE_TARGET_DIR}/x86_64-pc-windows-msvc/debug/${DLL_NAME}")
set(RELEASE_DLL_PATH_1 "${BASE_TARGET_DIR}/release/${DLL_NAME}")
set(RELEASE_DLL_PATH_2 "${BASE_TARGET_DIR}/x86_64-pc-windows-msvc/release/${DLL_NAME}")

set(DEBUG_DLL_EXISTS FALSE)
set(RELEASE_DLL_EXISTS FALSE)

if(EXISTS ${DEBUG_DLL_PATH_1} OR EXISTS ${DEBUG_DLL_PATH_2})
    set(DEBUG_DLL_EXISTS TRUE)
    if(EXISTS ${DEBUG_DLL_PATH_1})
        message(STATUS "Found debug DLL: ${DEBUG_DLL_PATH_1}")
    else()
        message(STATUS "Found debug DLL: ${DEBUG_DLL_PATH_2}")
    endif()
endif()

if(EXISTS ${RELEASE_DLL_PATH_1} OR EXISTS ${RELEASE_DLL_PATH_2})
    set(RELEASE_DLL_EXISTS TRUE)
    if(EXISTS ${RELEASE_DLL_PATH_1})
        message(STATUS "Found release DLL: ${RELEASE_DLL_PATH_1}")
    else()
        message(STATUS "Found release DLL: ${RELEASE_DLL_PATH_2}")
    endif()
endif()


# Use generator expressions to select the right DLL at build time (no nested $<EXISTS:...>)
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
