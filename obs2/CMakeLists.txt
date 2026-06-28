cmake_minimum_required(VERSION 3.16)

set(PLUGIN_NAME the_golden_eye)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

project(${PLUGIN_NAME} C)

# Vendored OBS headers (populated by `just obs-headers`)
set(VENDOR_OBS_DIR "${CMAKE_CURRENT_SOURCE_DIR}/vendor/obs")
set(VENDOR_LIBOBS_DIR "${VENDOR_OBS_DIR}/libobs")
set(VENDOR_FRONTEND_DIR "${VENDOR_OBS_DIR}/frontend")

if(NOT EXISTS "${VENDOR_LIBOBS_DIR}/obs-module.h")
  message(FATAL_ERROR "Vendored OBS headers not found. Run 'just obs-headers' first.")
endif()

# Generate obsconfig.h from the vendored template
set(OBS_RELEASE_CANDIDATE 0)
set(OBS_BETA 0)
configure_file(
    "${VENDOR_LIBOBS_DIR}/obsconfig.h.in"
    "${CMAKE_CURRENT_BINARY_DIR}/obsconfig.h"
)

# Find OBS libraries to link against
find_package(PkgConfig REQUIRED)
if(APPLE)
  set(OBS_FW_DIR "/Applications/OBS.app/Contents/Frameworks")
  set(OBS_LIBRARY "${OBS_FW_DIR}/libobs.framework/libobs")
  set(OBS_FRONTEND_LIBRARY "${OBS_FW_DIR}/obs-frontend-api.dylib")
  foreach(_lib OBS_LIBRARY OBS_FRONTEND_LIBRARY)
    if(NOT EXISTS "${${_lib}}")
      message(FATAL_ERROR "Could not find OBS library: ${${_lib}}")
    endif()
  endforeach()
  set(OBS_LIBRARIES ${OBS_LIBRARY})
  set(OBS_FRONTEND_LIBRARIES ${OBS_FRONTEND_LIBRARY})
else()
  pkg_check_modules(OBS REQUIRED libobs)

  # Arch exposes obs-frontend-api via pkg-config, but Debian-based distros
  # often do not. Fall back to finding the shared library directly.
  pkg_check_modules(OBS_FRONTEND QUIET obs-frontend-api)
  if(NOT OBS_FRONTEND_FOUND)
    find_library(OBS_FRONTEND_LIBRARY
            NAMES obs-frontend-api libobs-frontend-api
            HINTS ${OBS_LIBRARY_DIRS}
            PATH_SUFFIXES lib lib64 x86_64-linux-gnu
        )
    if(NOT OBS_FRONTEND_LIBRARY)
      message(FATAL_ERROR
                "Could not find OBS frontend API library. Install OBS development files for your distro."
            )
    endif()
    set(OBS_FRONTEND_LIBRARIES ${OBS_FRONTEND_LIBRARY})
  endif()
endif()

# OpenCV is referenced by the Rust staticlib (the `opencv-rust` crate compiles
# a C++ shim into libge_rust.a). When cargo builds the test_match binary it
# follows the crate's cargo:rustc-link-lib directives, but those don't carry
# over when CMake links the static archive into the plugin — we have to add
# OpenCV (and the C++ stdlib) to the plugin's link line ourselves.
#
# OpenCV is always linked statically from vendor/opencv-static (built by
# `just opencv-static`), so the distributed plugin is self-contained and
# portable — no dependency on a matching system OpenCV at runtime on any platform.
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

#
# Build frontend
#

set(BROWSER_DIR "${CMAKE_CURRENT_SOURCE_DIR}/browser")

# In dev mode we don't build/embed the real SPA. Instead the plugin embeds a
# tiny HTML file that redirects to the Vite dev server, so the frontend can be
# iterated on with hot reloads while only the plugin needs CMake rebuilds.
option(BROWSER_DEV "Embed a dev-server redirect instead of building the SPA" OFF)

# Port the Vite dev server listens on (must match browser/vite.config.ts).
set(BROWSER_DEV_PORT 5173)

if(BROWSER_DEV)
  # Generate the redirect stand-in and point the embedded bundle at it.
  set(BROWSER_BUNDLE "${CMAKE_CURRENT_BINARY_DIR}/browser-dev.html")
  configure_file(
        "${CMAKE_CURRENT_SOURCE_DIR}/templates/browser-dev.html.in"
        "${BROWSER_BUNDLE}"
        @ONLY
    )
  # No SPA build in dev mode; keep the target so rust_build's dependency
  # resolves, but make it a no-op.
  add_custom_target(browser_build
        COMMENT "Skipping frontend build (BROWSER_DEV=ON; using dev-server redirect)"
    )
else()
  # The embedded bundle path. The justfile is the single source of truth and
  # exports BROWSER_BUNDLE, so honour it from the environment when present; fall
  # back to the standard build output when CMake is invoked directly (without
  # the justfile env). Passed on to the frontend build (svelte.config.js reads
  # $BROWSER_BUNDLE to decide its output location) and to the Rust build
  # (include_str!(env!("BROWSER_BUNDLE"))).
  if(DEFINED ENV{BROWSER_BUNDLE})
    set(BROWSER_BUNDLE "$ENV{BROWSER_BUNDLE}")
  else()
    set(BROWSER_BUNDLE "${BROWSER_DIR}/build/index.html")
  endif()

  # Track frontend sources and key build inputs so the bundle only rebuilds
  # when one of them is newer than the generated HTML.
  file(GLOB_RECURSE BROWSER_BUILD_DEPENDS CONFIGURE_DEPENDS
        "${BROWSER_DIR}/src/*"
        "${BROWSER_DIR}/static/*"
    )
  list(APPEND BROWSER_BUILD_DEPENDS
        "${BROWSER_DIR}/package.json"
        "${BROWSER_DIR}/package-lock.json"
        "${BROWSER_DIR}/svelte.config.js"
        "${BROWSER_DIR}/tsconfig.json"
        "${BROWSER_DIR}/vite.config.ts"
    )

  add_custom_command(
        OUTPUT "${BROWSER_BUNDLE}"
        COMMAND ${CMAKE_COMMAND} -E env "BROWSER_BUNDLE=${BROWSER_BUNDLE}" ${RUST_BUILD_ENV}
                npm run build
        # Fail the build if the bundle the Rust crate embeds wasn't produced,
        # rather than letting cargo fail later with an opaque include_str! error.
        COMMAND test -f "${BROWSER_BUNDLE}"
        WORKING_DIRECTORY "${BROWSER_DIR}"
        DEPENDS ${BROWSER_BUILD_DEPENDS}
        COMMENT "Building frontend"
        VERBATIM
    )

  add_custom_target(browser_build ALL
        DEPENDS "${BROWSER_BUNDLE}"
    )
endif()

#
# Rust static library
#

set(RUST_DIR "${CMAKE_CURRENT_SOURCE_DIR}/rust")

# Built up as a CMake list so each flag expands to a separate cargo argument.
set(CARGO_BUILD_FLAGS "")
if(CMAKE_BUILD_TYPE STREQUAL "Debug")
  set(RUST_PROFILE "debug")
else()
  set(RUST_PROFILE "release")
  list(APPEND CARGO_BUILD_FLAGS --release)
endif()

# Enable the crate's `dev` feature (permissive CORS) when serving the SPA from
# the separate-origin Vite dev server.
if(BROWSER_DEV)
  list(APPEND CARGO_BUILD_FLAGS --features dev)
endif()

set(RUST_LIB "${RUST_DIR}/target/${RUST_PROFILE}/libge_rust.a")

# https://github.com/twistedfall/opencv-rust/blob/master/INSTALL.md#macos-package
if(APPLE)
  # Get the active developer directory (works for both Xcode and Command Line Tools)
  execute_process(
        COMMAND xcode-select --print-path
        OUTPUT_VARIABLE XCODE_DEVELOPER_DIR
        OUTPUT_STRIP_TRAILING_WHITESPACE
    )

  # 1. Define the two possible paths where libclang.dylib could live
  set(CLANG_PATH_XCODE "${XCODE_DEVELOPER_DIR}/Toolchains/XcodeDefault.xctoolchain/usr/lib")
  set(CLANG_PATH_CLT "${XCODE_DEVELOPER_DIR}/usr/lib")

  # 2. Determine which path actually exists on the machine
  if(EXISTS "${CLANG_PATH_XCODE}")
    set(CHOSEN_DYLD_PATH "${CLANG_PATH_XCODE}")
  elseif(EXISTS "${CLANG_PATH_CLT}")
    set(CHOSEN_DYLD_PATH "${CLANG_PATH_CLT}")
  else()
    message(FATAL_ERROR "Neither Xcode nor Command Line Tools, try running `xcode-select --install` first.")
  endif()

  # 3. Append to your Rust environment list
  list(APPEND RUST_BUILD_ENV
        "DYLD_FALLBACK_LIBRARY_PATH=${CHOSEN_DYLD_PATH}"
        "LDFLAGS=-L/Library/Developer/CommandLineTools/SDKs/MacOSX.sdk/usr/lib"
        "LD_LIBRARY_PATH=$ENV{LD_LIBRARY_PATH}:/usr/local/lib"
    )

  if(CMAKE_OSX_DEPLOYMENT_TARGET)
    list(APPEND RUST_BUILD_ENV "MACOSX_DEPLOYMENT_TARGET=${CMAKE_OSX_DEPLOYMENT_TARGET}")
  endif()
endif()

add_custom_target(rust_build ALL
    COMMAND ${CMAKE_COMMAND} -E env "BROWSER_BUNDLE=${BROWSER_BUNDLE}" ${RUST_BUILD_ENV}
            cargo build ${CARGO_BUILD_FLAGS} --all-targets
    WORKING_DIRECTORY "${RUST_DIR}"
    COMMENT "Building Rust static library (${RUST_PROFILE})"
    VERBATIM
)

# The Rust crate embeds browser/build/index.html via include_str!, so the
# frontend must be built (successfully) first. This also guarantees that a
# failed browser_build stops the whole build before cargo runs.
add_dependencies(rust_build browser_build)

# Declare an imported target so we can add it to target_link_libraries.
add_library(rust_libs STATIC IMPORTED GLOBAL)
set_target_properties(rust_libs PROPERTIES
    IMPORTED_LOCATION "${RUST_LIB}"
)
add_dependencies(rust_libs rust_build)

#
# Output OBS plugin
#

if(APPLE)
  add_library(${PLUGIN_NAME} MODULE)
else()
  add_library(${PLUGIN_NAME} SHARED)
endif()

target_sources(${PLUGIN_NAME} PRIVATE
    obs_bridge.c
    plugin.c
)

target_include_directories(${PLUGIN_NAME} PRIVATE
    # Root `vendor/` dir
    ${CMAKE_CURRENT_SOURCE_DIR}/vendor
    # Also expose `libobs/` directly so OBS's headers can #include <obs.h> etc.
    ${VENDOR_LIBOBS_DIR}
    ${CMAKE_CURRENT_BINARY_DIR}
    $<$<BOOL:${APPLE}>:/opt/homebrew/opt/simde/include>
)
target_link_libraries(${PLUGIN_NAME} PRIVATE
    ${OBS_LIBRARIES}
    ${OBS_FRONTEND_LIBRARIES}
    rust_libs
    # Pulls in the OpenCV static archives the C++ shim in libge_rust.a references
    # (cv::Mat, cv::imgproc, ...). Listed after rust_libs so the archives that
    # define these symbols come after the archive that uses them on the link line.
    ${GE_OPENCV_LINK}
)

if(APPLE)
  # Rust's stdlib (embedded in the staticlib) references these system
  # frameworks on macOS. find_library resolves them to full paths so the
  # linker can locate them regardless of SDK layout.
  find_library(SECURITY_FW             Security             REQUIRED)
  find_library(FOUNDATION_FW           Foundation           REQUIRED)
  find_library(SYSTEM_CONFIGURATION_FW SystemConfiguration  REQUIRED)
  find_library(CORE_FOUNDATION_FW      CoreFoundation       REQUIRED)
  find_library(APPKIT_FW               AppKit               REQUIRED)
  target_link_libraries(${PLUGIN_NAME} PRIVATE
        ${SECURITY_FW}
        ${FOUNDATION_FW}
        ${SYSTEM_CONFIGURATION_FW}
        ${CORE_FOUNDATION_FW}
        ${APPKIT_FW}
        -lresolv
        # libc++ provides the C++ runtime symbols the opencv-rust shim
        # depends on (operator new/delete, __cxa_*, __gxx_personality_v0).
        c++
        # The opencv-rust shim also links iconv on macOS.
        iconv
    )
else()
  # On Linux, the C++ shim from opencv-rust needs libstdc++.
  target_link_libraries(${PLUGIN_NAME} PRIVATE stdc++)
endif()

if(WIN32)
  target_link_libraries(${PLUGIN_NAME} PRIVATE ws2_32)
endif()

#
# Plugin output:
#   a plugin bundle on macOS, and a regular shared library on other platforms.
#

if(APPLE)
  set_target_properties(${PLUGIN_NAME} PROPERTIES
        BUNDLE TRUE
        BUNDLE_EXTENSION "plugin"
        MACOSX_BUNDLE_INFO_PLIST "${CMAKE_CURRENT_SOURCE_DIR}/templates/Info.plist.in"
        PREFIX ""
    )
else()
  set_target_properties(${PLUGIN_NAME} PROPERTIES PREFIX "")
endif()
