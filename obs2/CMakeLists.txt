cmake_minimum_required(VERSION 3.16)

set(PLUGIN_NAME the_golden_eye)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

project(${PLUGIN_NAME} C)

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
