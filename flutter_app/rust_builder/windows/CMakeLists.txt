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

# Try multiple possible paths for the prebuilt DLL
set(POSSIBLE_DLL_PATHS
    "${CMAKE_CURRENT_SOURCE_DIR}/../../../../../../../target/release/rust_lib_fungi_app.dll"
    "${CMAKE_CURRENT_SOURCE_DIR}/../../../../../../../target/x86_64-pc-windows-msvc/release/rust_lib_fungi_app.dll"
)

set(PREBUILT_DLL_PATH "")
foreach(DLL_PATH ${POSSIBLE_DLL_PATHS})
    if(EXISTS ${DLL_PATH})
        set(PREBUILT_DLL_PATH ${DLL_PATH})
        message(STATUS "Found prebuilt DLL at: ${PREBUILT_DLL_PATH}")
        break()
    endif()
endforeach()

if(NOT PREBUILT_DLL_PATH)
    message(FATAL_ERROR "Prebuilt DLL not found in any of the expected locations:
    - ${CMAKE_CURRENT_SOURCE_DIR}/../../../../../../../target/release/rust_lib_fungi_app.dll
    - ${CMAKE_CURRENT_SOURCE_DIR}/../../../../../../../target/x86_64-pc-windows-msvc/release/rust_lib_fungi_app.dll")
endif()

# List of absolute paths to libraries that should be bundled with the plugin.
# This list could contain prebuilt libraries, or libraries created by an
# external build triggered from this build file.
set(rust_lib_fungi_app_bundled_libraries
  # "${${PROJECT_NAME}_cargokit_lib}"
  ${PREBUILT_DLL_PATH}
  PARENT_SCOPE
)
