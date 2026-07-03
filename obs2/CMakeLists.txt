cmake_minimum_required(VERSION 3.16)

set(PLUGIN_NAME the_golden_eye)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

project(${PLUGIN_NAME} C)

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

configure_file(
    "${CMAKE_CURRENT_SOURCE_DIR}/templates/VERSION.in"
    "${CMAKE_CURRENT_BINARY_DIR}/VERSION"
    @ONLY
)
set(GE_PLUGIN_VERSION_FILE "${CMAKE_CURRENT_BINARY_DIR}/VERSION")

option(GE_REUSE_HOST_BUILD_INPUTS "Reuse browser and Rust artifacts from an earlier host build" OFF)

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
