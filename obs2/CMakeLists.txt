cmake_minimum_required(VERSION 3.16)

# Newer CMake versions understand that Apple ld re-scans static archives and
# does not need repeated libraries for Rust/OpenCV/FFmpeg dependency closure.
# Keep the first occurrence so our explicit link order after libge_rust.a is
# preserved while avoiding duplicate -l warnings on macOS.
if(POLICY CMP0156)
  cmake_policy(SET CMP0156 NEW)
endif()
if(POLICY CMP0179)
  cmake_policy(SET CMP0179 NEW)
endif()

set(PLUGIN_NAME the_golden_eye)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

project(${PLUGIN_NAME} C CXX)

if(CMAKE_SIZEOF_VOID_P EQUAL 8)
  set(GE_OBS_ARCH_DIR "64bit")
else()
  set(GE_OBS_ARCH_DIR "32bit")
endif()

if(NOT DEFINED GE_PLUGIN_VERSION)
  if(DEFINED ENV{GE_PLUGIN_VERSION})
    set(GE_PLUGIN_VERSION "$ENV{GE_PLUGIN_VERSION}")
  else()
    message(FATAL_ERROR
        "GE_PLUGIN_VERSION is required. Build through the justfile or pass "
        "-DGE_PLUGIN_VERSION=x.y.z."
    )
  endif()
endif()
set(GE_PLUGIN_VERSION "${GE_PLUGIN_VERSION}" CACHE STRING "The Golden Eye plugin version")
if(NOT GE_PLUGIN_VERSION MATCHES "^[0-9]+\\.[0-9]+\\.[0-9]+(-[0-9A-Za-z.-]+)?(\\+[0-9A-Za-z.-]+)?$")
  message(FATAL_ERROR "GE_PLUGIN_VERSION must be a semantic version without a leading v, got '${GE_PLUGIN_VERSION}'.")
endif()
message(STATUS "The Golden Eye plugin version: ${GE_PLUGIN_VERSION}")

set(GE_PLUGIN_VERSION_STAMP "${CMAKE_CURRENT_BINARY_DIR}/plugin-version.stamp")
set(GE_PLUGIN_VERSION_STAMP_CONTENT "${GE_PLUGIN_VERSION}\n")
set(GE_WRITE_PLUGIN_VERSION_STAMP TRUE)
if(EXISTS "${GE_PLUGIN_VERSION_STAMP}")
  file(READ "${GE_PLUGIN_VERSION_STAMP}" GE_EXISTING_PLUGIN_VERSION_STAMP)
  if(GE_EXISTING_PLUGIN_VERSION_STAMP STREQUAL GE_PLUGIN_VERSION_STAMP_CONTENT)
    set(GE_WRITE_PLUGIN_VERSION_STAMP FALSE)
  endif()
endif()
if(GE_WRITE_PLUGIN_VERSION_STAMP)
  file(WRITE "${GE_PLUGIN_VERSION_STAMP}" "${GE_PLUGIN_VERSION_STAMP_CONTENT}")
endif()

option(GE_REUSE_HOST_BUILD_INPUTS "Reuse browser and Rust artifacts from an earlier host build" OFF)
option(GE_RUST_PACKAGE_PROFILE "Build Rust with the size-optimized packaging cargo profile" OFF)

set(GE_YOUTUBE_CLIENT_ID "$ENV{GE_YOUTUBE_CLIENT_ID}")
set(GE_YOUTUBE_CLIENT_SECRET "$ENV{GE_YOUTUBE_CLIENT_SECRET}")

# Build logic lives in focused modules under cmake/ to keep this file readable.
list(APPEND CMAKE_MODULE_PATH "${CMAKE_CURRENT_SOURCE_DIR}/cmake")

# Order matters: OpenCVStatic / FFmpegStatic seed RUST_BUILD_ENV (used by
# Frontend + RustLib), RustLib's rust_libs is linked by Targets, and Targets
# needs everything above.
include(OBSLibs)
include(OpenCVStatic)
include(FFmpegStatic)
include(Frontend)
include(RustLib)
include(Targets)
include(Package)
