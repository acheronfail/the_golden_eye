#!/usr/bin/env bash
set -euo pipefail

FFMPEG_VERSION="8.0"

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../.." && pwd)"
ffmpeg_prefix="${FFMPEG_PREFIX:-${repo_root}/obs2/vendor/ffmpeg-static}"
archive_dir="${SOURCE_ARCHIVE_CACHE:-${repo_root}/obs2/vendor/archives}"

version_file="${ffmpeg_prefix}/.ge-static-version"
installed_version=""
if [ -f "${version_file}" ]; then
  installed_version="$(cat "${version_file}")"
elif [ -f "${ffmpeg_prefix}/include/libavutil/ffversion.h" ]; then
  installed_version="$(awk -F'"' '/^#define FFMPEG_VERSION / { print $2; exit }' "${ffmpeg_prefix}/include/libavutil/ffversion.h")"
fi

if [ -f "${ffmpeg_prefix}/lib/pkgconfig/libavcodec.pc" ]; then
  if [ "${installed_version}" != "${FFMPEG_VERSION}" ]; then
    echo "Static FFmpeg at ${ffmpeg_prefix} is ${installed_version:-unknown}, expected ${FFMPEG_VERSION}; rebuilding."
    rm -rf "${ffmpeg_prefix}"
  else
    echo "Static FFmpeg ${FFMPEG_VERSION} already built at ${ffmpeg_prefix}"
    echo "${FFMPEG_VERSION}" > "${version_file}"
    echo "Delete it to rebuild: rm -rf ${ffmpeg_prefix}"
    exit 0
  fi
fi

rm -rf "${ffmpeg_prefix}"

work="$(mktemp -d)"
trap 'rm -rf "${work}"' EXIT

archive="${archive_dir}/ffmpeg-${FFMPEG_VERSION}.tar.xz"
mkdir -p "${archive_dir}"

if [ -f "${archive}" ]; then
  echo "Using cached FFmpeg archive ${archive}"
else
  echo "Downloading FFmpeg ${FFMPEG_VERSION} ..."
  wget -O "${work}/ffmpeg.tar.xz" \
    "https://ffmpeg.org/releases/ffmpeg-${FFMPEG_VERSION}.tar.xz"
  mv "${work}/ffmpeg.tar.xz" "${archive}"
fi

tar xJf "${archive}" -C "${work}"
src="${work}/ffmpeg-${FFMPEG_VERSION}"

if [ "$(uname)" = "Darwin" ]; then
  jobs="$(sysctl -n hw.logicalcpu)"
else
  jobs="$(nproc)"
fi

# Static, PIC, self-contained build. `--disable-autodetect` pins the build to
# FFmpeg's own built-in codecs/muxers only (no x264, no system zlib/lzma, no
# platform frameworks), so the resulting archives don't drag host libraries
# into the plugin - mirroring the way `opencv-static` forces its own zlib/png.
# The Rust code uses format/codec/swscale/swresample, not device capture or
# libavfilter. The archives are linked into the plugin's .so/.dylib, so
# everything is PIC.
cd "${src}"
./configure \
  --prefix="${ffmpeg_prefix}" \
  --enable-static \
  --disable-shared \
  --enable-pic \
  --disable-asm \
  --disable-autodetect \
  --disable-avdevice \
  --disable-avfilter \
  --disable-programs \
  --disable-doc \
  --disable-debug
make -j"${jobs}"
make install
echo "${FFMPEG_VERSION}" > "${version_file}"

echo
echo "Static FFmpeg installed to ${ffmpeg_prefix}"
echo "Now run 'just make' / 'just obs' - CMake auto-detects the static prefix."
