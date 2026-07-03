# Static FFmpeg wiring.
#
# FFmpeg is referenced by the Rust staticlib (the `ffmpeg-next` crate links
# against libav* via ffmpeg-sys-next). When cargo builds the test_match binary
# it follows the crate's cargo:rustc-link-lib directives, but those don't carry
# over when CMake links the static archive into the plugin — so, exactly like
# OpenCV, we add FFmpeg (and the libs its static archives pull in) to the
# plugin's own link line ourselves.
#
# FFmpeg is always linked statically from vendor/ffmpeg-static (built by
# `just ffmpeg-static`), so the distributed plugin is self-contained — no
# dependency on a matching system FFmpeg at runtime on any platform.
#
# Exports: GE_FFMPEG_LINK (the link line for the core's own link step), and
# prepends the vendored prefix to PKG_CONFIG_PATH in RUST_BUILD_ENV so cargo's
# ffmpeg-sys-next build resolves these .pc files (and links statically — the
# crate's `static` feature is enabled in Cargo.toml).

if(WIN32)
  find_package(unofficial-ffmpeg CONFIG REQUIRED)
  message(STATUS "Linking FFmpeg from vcpkg/CMake package")

  if(DEFINED VCPKG_TARGET_TRIPLET)
    set(_ge_vcpkg_triplet "${VCPKG_TARGET_TRIPLET}")
  elseif(DEFINED ENV{VCPKGRS_TRIPLET})
    set(_ge_vcpkg_triplet "$ENV{VCPKGRS_TRIPLET}")
  else()
    set(_ge_vcpkg_triplet "x64-windows-static-md")
  endif()

  # vcpkg's FFmpeg port exports CMake targets under this namespace. The
  # "unofficial" prefix is vcpkg's naming, not a fork of FFmpeg.
  set(GE_FFMPEG_LINK
      unofficial::ffmpeg::avformat
      unofficial::ffmpeg::avcodec
      unofficial::ffmpeg::swscale
      unofficial::ffmpeg::swresample
      unofficial::ffmpeg::avutil
  )
  list(APPEND RUST_BUILD_ENV "VCPKGRS_TRIPLET=${_ge_vcpkg_triplet}")
  return()
endif()

find_package(PkgConfig REQUIRED)

set(GE_FFMPEG_STATIC_PREFIX "${CMAKE_CURRENT_SOURCE_DIR}/vendor/ffmpeg-static")
if(NOT EXISTS "${GE_FFMPEG_STATIC_PREFIX}/lib/pkgconfig/libavcodec.pc")
  message(FATAL_ERROR
          "Vendored static FFmpeg not found at ${GE_FFMPEG_STATIC_PREFIX}.\n"
          "Build it first with:  just ffmpeg-static")
endif()
message(STATUS "Linking static FFmpeg from ${GE_FFMPEG_STATIC_PREFIX}")

# Put the vendored static install at the front of pkg-config's search path so
# both CMake (below) and cargo (the ffmpeg-sys-next build) resolve these .pc
# files instead of any system FFmpeg. PKG_CONFIG_PATH is colon-separated.
set(_ge_ffmpeg_pc "${GE_FFMPEG_STATIC_PREFIX}/lib/pkgconfig")
if(DEFINED ENV{PKG_CONFIG_PATH})
  set(ENV{PKG_CONFIG_PATH} "${_ge_ffmpeg_pc}:$ENV{PKG_CONFIG_PATH}")
else()
  set(ENV{PKG_CONFIG_PATH} "${_ge_ffmpeg_pc}")
endif()

# ffmpeg-sys-next probes via pkg-config (with --static, since we enable the
# crate's `static` feature). Hand it the same search path we use here so it
# resolves the vendored install. Appended to RUST_BUILD_ENV, which CMake passes
# through to the cargo invocation in RustLib.cmake.
list(APPEND RUST_BUILD_ENV "PKG_CONFIG_PATH=$ENV{PKG_CONFIG_PATH}")

# Read the *static* link set (Libs + Libs.private) so we pick up the system
# libs FFmpeg's static archives need, in the correct order. These libs match
# the crate features in Cargo.toml (codec, format, swresample, swscale) plus
# avutil, which the vendored build provides.
pkg_check_modules(FFMPEG REQUIRED
      libavformat
      libavcodec
      libswscale
      libswresample
      libavutil)

# The full static link line (-L.../-l...) for the plugin's own link step.
# pkg_check_modules splits "-framework Foo" into two list items
# ("-framework";"Foo"); the bare "Foo" would be turned into "-lFoo" by the
# linker (the same gotcha OpenCVStatic notes for AppKit). Pull those pairs out
# and re-add each framework via find_library so it resolves to a real path.
set(GE_FFMPEG_LINK "")
set(_ge_ff_expect_framework OFF)
foreach(_item ${FFMPEG_STATIC_LDFLAGS})
  if(_ge_ff_expect_framework)
    find_library(_ge_ff_fw_${_item} "${_item}")
    if(_ge_ff_fw_${_item})
      list(APPEND GE_FFMPEG_LINK "${_ge_ff_fw_${_item}}")
    endif()
    set(_ge_ff_expect_framework OFF)
  elseif(_item STREQUAL "-framework")
    set(_ge_ff_expect_framework ON)
  else()
    list(APPEND GE_FFMPEG_LINK "${_item}")
  endif()
endforeach()
