# Static OpenCV wiring.
#
# OpenCV is referenced by the Rust staticlib (the `opencv-rust` crate compiles
# a C++ shim into libge_rust.a). When cargo builds the test_match binary it
# follows the crate's cargo:rustc-link-lib directives, but those don't carry
# over when CMake links the static archive into the plugin — we have to add
# OpenCV (and the C++ stdlib) to the plugin's link line ourselves.
#
# OpenCV is always linked statically from vendor/opencv-static (built by
# `just opencv-static`), so the distributed plugin is self-contained and
# portable — no dependency on a matching system OpenCV at runtime on any platform.
#
# Exports: GE_OPENCV_LINK (the link line for the core's own link step), and
# appends the OPENCV_* probe vars to RUST_BUILD_ENV (consumed by the Rust build).

find_package(PkgConfig REQUIRED)

set(GE_OPENCV_STATIC_PREFIX "${CMAKE_CURRENT_SOURCE_DIR}/vendor/opencv-static")
if(NOT EXISTS "${GE_OPENCV_STATIC_PREFIX}/lib/pkgconfig/opencv4.pc")
  message(FATAL_ERROR
          "Vendored static OpenCV not found at ${GE_OPENCV_STATIC_PREFIX}.\n"
          "Build it first with:  just opencv-static")
endif()
message(STATUS "Linking static OpenCV from ${GE_OPENCV_STATIC_PREFIX}")

# Put the vendored static install at the front of pkg-config's search path so
# both CMake (below) and cargo (the opencv-rust build) resolve this opencv4.pc
# instead of the system one. PKG_CONFIG_PATH is colon-separated.
set(_ge_static_pc
    "${GE_OPENCV_STATIC_PREFIX}/lib/pkgconfig:${GE_OPENCV_STATIC_PREFIX}/lib64/pkgconfig")
if(DEFINED ENV{PKG_CONFIG_PATH})
  set(ENV{PKG_CONFIG_PATH} "${_ge_static_pc}:$ENV{PKG_CONFIG_PATH}")
else()
  set(ENV{PKG_CONFIG_PATH} "${_ge_static_pc}")
endif()

# Read the *static* link set (Libs + Libs.private) so we pick up OpenCV's
# bundled third-party archives (libpng, zlib, ...) and the required system
# libs, in the correct link order. The plugin links these raw flags directly.
pkg_check_modules(OPENCV REQUIRED opencv4)

# opencv-rust's own pkg-config probe doesn't pass `--static`, so it would miss
# the bundled third-party archives and fail to link test_match. Hand it the
# link set explicitly via the crate's `environment` probe, and disable the
# other probes so it can't silently fall back to the system OpenCV. This also
# makes cargo compile the C++ shim against the vendored headers, keeping the
# ABI in sync with the archives CMake links into the plugin below.
#
# We must feed opencv-rust the *real* archive basenames, not the pkg-config
# short names: OpenCV installs its bundled 3rdparty archives with a doubled
# "lib" prefix (liblibpng.a, liblibjpeg-turbo.a) and opencv-rust strips one
# leading "lib" (+ extension) from each name before emitting -l. Feeding it
# pkg-config's "libpng"/"libjpeg-turbo" yields -lpng/-ljpeg-turbo, which fails
# to find the archive (and would silently grab the system libpng); feeding the
# real "liblibpng.a" yields -llibpng, which resolves correctly.
file(GLOB _ge_cv_3rdparty "${GE_OPENCV_STATIC_PREFIX}/lib/opencv4/3rdparty/*.a")
# OpenCV modules first, in dependency order, then the bundled 3rdparty
# archives (GLOB sorts alphabetically: jpeg, png, zlib — png before zlib, as
# required), then the system libs the static build pulls in.
set(_ge_cv_libs_list
      "static=libopencv_imgcodecs.a"
      "static=libopencv_imgproc.a"
      "static=libopencv_core.a")
foreach(_a ${_ge_cv_3rdparty})
  get_filename_component(_b "${_a}" NAME)
  list(APPEND _ge_cv_libs_list "static=${_b}")
endforeach()
# System libs that OpenCV's static archives pull in.  rt and dl are
# Linux-only; macOS provides equivalent functionality in libc itself.
if(APPLE)
  list(APPEND _ge_cv_libs_list m pthread)
else()
  list(APPEND _ge_cv_libs_list dl m pthread rt)
endif()

string(REPLACE ";" "," _ge_cv_include "${OPENCV_INCLUDE_DIRS}")
set(_ge_cv_libpaths
      "${GE_OPENCV_STATIC_PREFIX}/lib,${GE_OPENCV_STATIC_PREFIX}/lib/opencv4/3rdparty")
string(REPLACE ";" "," _ge_cv_libs "${_ge_cv_libs_list}")
list(APPEND RUST_BUILD_ENV
      "OPENCV_INCLUDE_PATHS=${_ge_cv_include}"
      "OPENCV_LINK_PATHS=${_ge_cv_libpaths}"
      "OPENCV_LINK_LIBS=${_ge_cv_libs}"
      "OPENCV_DISABLE_PROBES=pkg_config,cmake,vcpkg_cmake,vcpkg")

# The full static link line (-L.../-l...) for the plugin's own link step.
# OPENCV_STATIC_LDFLAGS is a CMake list (semicolon-separated). pkg_check_modules
# splits "-framework AppKit" into two items: "-framework" and "AppKit". When
# passed to the linker the bare "AppKit" token becomes "-lAppKit", which fails.
# Remove both items; the framework is added explicitly via find_library() below.
set(GE_OPENCV_LINK ${OPENCV_STATIC_LDFLAGS})
list(REMOVE_ITEM GE_OPENCV_LINK "-framework" "AppKit")
# OpenCV's static build always compiles opengl.cpp into opencv_core and records
# -lGL/-lGLU as link-time dependencies (even with -D WITH_OPENGL=OFF). Our code
# doesn't reference any GL/GLU symbols, so strip them to avoid a hard runtime
# dependency — libGLU.so.1 is absent in the OBS Flatpak sandbox and causes a
# dlopen failure at plugin load time.
list(REMOVE_ITEM GE_OPENCV_LINK "-lGL" "-lGLU")

list(FILTER GE_OPENCV_LINK EXCLUDE REGEX "^-l.+\.framework$")
