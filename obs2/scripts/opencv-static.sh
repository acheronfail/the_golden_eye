#!/usr/bin/env bash
set -euo pipefail

OPENCV_VERSION="4.11.0"
CMAKE_VERSION="3.31.7"

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../.." && pwd)"
opencv_prefix="${OPENCV_PREFIX:-${repo_root}/obs2/vendor/opencv-static}"
archive_dir="${SOURCE_ARCHIVE_CACHE:-${repo_root}/obs2/vendor/archives}"

version_file="${opencv_prefix}/.ge-static-version"
installed_version=""
if [ -f "${version_file}" ]; then
  installed_version="$(cat "${version_file}")"
elif [ -f "${opencv_prefix}/lib/pkgconfig/opencv4.pc" ]; then
  installed_version="$(awk -F': ' '/^Version: / { print $2; exit }' "${opencv_prefix}/lib/pkgconfig/opencv4.pc")"
fi

if [ -f "${opencv_prefix}/lib/pkgconfig/opencv4.pc" ]; then
  if [ "${installed_version}" != "${OPENCV_VERSION}" ]; then
    echo "Static OpenCV at ${opencv_prefix} is ${installed_version:-unknown}, expected ${OPENCV_VERSION}; rebuilding."
    rm -rf "${opencv_prefix}"
  else
    echo "Static OpenCV ${OPENCV_VERSION} already built at ${opencv_prefix}"
    echo "${OPENCV_VERSION}" > "${version_file}"
    echo "Delete it to rebuild: rm -rf ${opencv_prefix}"
    exit 0
  fi
fi

rm -rf "${opencv_prefix}"

work="$(mktemp -d)"
trap 'rm -rf "${work}"' EXIT

archive="${archive_dir}/opencv-${OPENCV_VERSION}.tar.gz"
mkdir -p "${archive_dir}"

if [ -f "${archive}" ]; then
  echo "Using cached OpenCV archive ${archive}"
else
  echo "Downloading OpenCV ${OPENCV_VERSION} ..."
  wget -O "${work}/opencv.tar.gz" \
    "https://github.com/opencv/opencv/archive/refs/tags/${OPENCV_VERSION}.tar.gz"
  mv "${work}/opencv.tar.gz" "${archive}"
fi

tar xzf "${archive}" -C "${work}"
src="${work}/opencv-${OPENCV_VERSION}"

# Download a pinned CMake so the build is independent of whatever version
# the system has installed.
if [ "$(uname)" = "Darwin" ]; then
  cmake_platform="macos-universal"
  jobs="$(sysctl -n hw.logicalcpu)"
else
  cmake_platform="linux-x86_64"
  jobs="$(nproc)"
fi
cmake_name="cmake-${CMAKE_VERSION}-${cmake_platform}"
wget -O "${work}/cmake.tar.gz" \
  "https://github.com/Kitware/CMake/releases/download/v${CMAKE_VERSION}/${cmake_name}.tar.gz"
tar xzf "${work}/cmake.tar.gz" -C "${work}"
if [ "$(uname)" = "Darwin" ]; then
  cmake_bin="${work}/${cmake_name}/CMake.app/Contents/bin/cmake"
else
  cmake_bin="${work}/${cmake_name}/bin/cmake"
fi

# Platform-specific flags: macOS doesn't have V4L/GTK/1394; LAPACK is
# available via Accelerate but we don't need it for our minimal build.
platform_flags=()
if [ "$(uname)" = "Darwin" ]; then
  platform_flags=(-D WITH_AVFOUNDATION=OFF -D WITH_LAPACK=OFF)
else
  platform_flags=(-D WITH_1394=OFF -D WITH_V4L=OFF -D WITH_GTK=OFF -D WITH_LAPACK=OFF)
fi

# Static libs are linked into the plugin's .so/.dylib, so everything must
# be PIC. We build only the modules the matcher uses and force OpenCV to
# compile its own zlib/libpng/libjpeg so nothing is pulled from the host.
"${cmake_bin}" -S "${src}" -B "${work}/build" \
  -D CMAKE_BUILD_TYPE=Release \
  -D CMAKE_INSTALL_PREFIX="${opencv_prefix}" \
  -D BUILD_LIST=core,imgproc,imgcodecs \
  -D BUILD_SHARED_LIBS=OFF \
  -D ENABLE_PIC=ON \
  -D CMAKE_POSITION_INDEPENDENT_CODE=ON \
  -D OPENCV_GENERATE_PKGCONFIG=ON \
  -D OPENCV_FORCE_3RDPARTY_BUILD=ON \
  -D PNG_PNG_INCLUDE_DIR="${src}/3rdparty/libpng" \
  -D BUILD_ZLIB=ON \
  -D BUILD_PNG=ON \
  -D BUILD_JPEG=ON \
  -D BUILD_TIFF=OFF -D WITH_TIFF=OFF \
  -D BUILD_WEBP=OFF -D WITH_WEBP=OFF \
  -D BUILD_OPENJPEG=OFF -D WITH_OPENJPEG=OFF \
  -D BUILD_OPENEXR=OFF -D WITH_OPENEXR=OFF \
  -D WITH_JASPER=OFF \
  -D WITH_FFMPEG=OFF -D WITH_GSTREAMER=OFF \
  -D WITH_QT=OFF -D WITH_OPENGL=OFF \
  -D WITH_PROTOBUF=OFF -D WITH_GPHOTO2=OFF -D WITH_GDAL=OFF \
  -D WITH_FREETYPE=OFF -D WITH_IPP=OFF -D WITH_ITT=OFF \
  -D WITH_TBB=OFF -D WITH_OPENMP=OFF -D WITH_CUDA=OFF -D WITH_OPENCL=OFF -D WITH_AVIF=OFF \
  -D BUILD_opencv_apps=OFF -D BUILD_TESTS=OFF -D BUILD_PERF_TESTS=OFF \
  -D BUILD_EXAMPLES=OFF -D BUILD_DOCS=OFF -D BUILD_JAVA=OFF \
  -D BUILD_opencv_python3=OFF \
  "${platform_flags[@]}"

"${cmake_bin}" --build "${work}/build" -j"${jobs}"
"${cmake_bin}" --install "${work}/build"
echo "${OPENCV_VERSION}" > "${version_file}"

echo
echo "Static OpenCV installed to ${opencv_prefix}"
echo "Now run 'just make' / 'just obs' - CMake auto-detects the static prefix."
