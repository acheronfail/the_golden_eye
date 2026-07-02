#
# Build variables
#

set tempdir := "/tmp"

obs_headers := justfile_directory() / "obs2/vendor/obs"
source_archive_cache := justfile_directory() / "obs2/vendor/archives"
plugin_version := "0.1.0"
obsapi_version := "32.1.2"
opencv_version := "4.11.0"
ffmpeg_version := "8.0"
cmake_version := "3.31.7"

export DYLD_FALLBACK_LIBRARY_PATH := "/Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain/usr/lib:/Library/Developer/CommandLineTools/usr/lib"
export BROWSER_BUNDLE := justfile_directory() / "obs2/browser/build/index.html"
export GE_PLUGIN_VERSION := plugin_version
export VITE_GE_PLUGIN_VERSION := plugin_version
export OPENCV_PREFIX := justfile_directory() / "obs2/vendor/opencv-static"
export FFMPEG_PREFIX := justfile_directory() / "obs2/vendor/ffmpeg-static"

_default:
    just -l

# runs the project in dev mode (hot reloads for the UI and the plugin)
#
# The plugin is split into a thin shim (loaded by OBS) and a "core" library
# (the Rust logic + OpenCV), which the shim dlopen's. In dev mode the shim
# watches the core library on disk and hot-reloads it whenever it's rebuilt —
# so editing the SvelteKit UI *or* the Rust code reloads live without
# restarting OBS. The loop below relinks the core whenever Rust sources change.
dev:
    #!/usr/bin/env bash
    set -euo pipefail
    root="$(pwd)"
    # FIFO the rebuild loop pings to tell the plugin shim to hot-reload the core.
    # Shared with the shim (dev_reload.c) via this env var.
    export GE_RELOAD_FIFO="${TMPDIR:-/tmp}/ge_the_golden_eye.reload"
    mkdir -p obs2/build
    cd obs2/build
    cmake .. -DCMAKE_BUILD_TYPE=Debug -DBROWSER_DEV=ON -DGE_PLUGIN_VERSION="{{ plugin_version }}"
    make
    build_dir="$(pwd)"

    dev_pids=""
    cleanup() { [ -n "$dev_pids" ] && kill $dev_pids 2>/dev/null || true; }
    trap cleanup EXIT

    # Vite dev server: hot reload for the SvelteKit SPA.
    ( cd ../browser && npm run dev ) &
    dev_pids="$dev_pids $!"

    # Rebuild the core library when the Rust sources change, then ping the shim
    # so it hot-reloads the new core — a save in obs2/rust reloads inside the
    # running OBS with no restart. (The ping is skipped until the shim has
    # created the FIFO, i.e. once OBS has loaded the plugin.)
    (
      cd "$build_dir"
      stamp="$(mktemp)"
      while true; do
        if [ -n "$(find "$root/obs2/rust/src" "$root/obs2/rust/Cargo.toml" -newer "$stamp" 2>/dev/null)" ]; then
          touch "$stamp"
          echo "[dev] rust change detected — rebuilding core…"
          if make golden_core; then
            [ -p "$GE_RELOAD_FIFO" ] && ( printf '\n' > "$GE_RELOAD_FIFO" ) &
          else
            echo "[dev] core build failed; fix and save again"
          fi
        fi
        sleep 1
      done
    ) &
    dev_pids="$dev_pids $!"

    OBS_PLUGINS_PATH="$build_dir" OBS_PLUGINS_DATA_PATH="$build_dir" obs

test *filter: make-release
    cd test && npm run test -- {{ filter }}

test-watch *filter: make-release
    cd test && npm run test-watch -- {{ filter }}

fmt:
    cd obs2/browser && npm run format:repo
    cd obs2/rust && rustup run nightly cargo fmt --
    find obs2 -maxdepth 1 \( -name '*.c' -o -name '*.h' \) ! -name ge_rust.h -print0 | xargs -0 clang-format -style=file -i

# runs the rust tests (cv matcher + monitor loop) against the fixture screenshots
test-rust *args:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ ! -f "$BROWSER_BUNDLE" ]; then
      echo "browser bundle not found at $BROWSER_BUNDLE — run 'just make-release' first" >&2
      exit 1
    fi

    # The /usr/bin/env shebang strips DYLD_* (macOS SIP), so re-export it here for
    # opencv's build script (it needs libclang via this path). A dedicated target
    # dir keeps cargo's system-opencv build from thrashing against the CMake
    # build's static-opencv artifacts in target/release.
    export DYLD_FALLBACK_LIBRARY_PATH="{{ DYLD_FALLBACK_LIBRARY_PATH }}"
    export CARGO_TARGET_DIR="{{ justfile_directory() }}/obs2/rust/target/test"
    # ffmpeg-next is built with the `static` feature, so point pkg-config at the
    # vendored static FFmpeg (built by `just ffmpeg-static`) just like the CMake
    # build does — otherwise ffmpeg-sys-next falls back to a system FFmpeg.
    export PKG_CONFIG_PATH="{{ FFMPEG_PREFIX }}/lib/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
    cd "{{ justfile_directory() }}/obs2/rust" && cargo test --release {{ args }}

make:
    mkdir -p obs2/build
    cd obs2/build && cmake .. \
      -DCMAKE_BUILD_TYPE=Debug \
      -DBROWSER_DEV=OFF \
      -DGE_PLUGIN_VERSION="{{ plugin_version }}" \
    && make

make-release:
    mkdir -p obs2/build
    cd obs2/build && cmake .. \
      -DCMAKE_BUILD_TYPE=Release \
      -DBROWSER_DEV=OFF \
      -DGE_PLUGIN_VERSION="{{ plugin_version }}" \
    && make

[macos]
make-package:
    mkdir -p obs2/build
    cd obs2/build && cmake .. \
      -DCMAKE_BUILD_TYPE=Release \
      -DBROWSER_DEV=OFF \
      -DGE_PLUGIN_VERSION="{{ plugin_version }}" \
    && cmake --build . --target package-plugin

[linux]
make-package: make-release-flatpak
    just _flatpak-build package-plugin

[macos]
install:
    mkdir -p obs2/build
    cd obs2/build && cmake .. \
      -DCMAKE_BUILD_TYPE=Release \
      -DBROWSER_DEV=OFF \
      -DGE_PLUGIN_VERSION="{{ plugin_version }}" \
    && cmake --build . --target install-plugin

[linux]
install: make-release-flatpak
    just _flatpak-build install-plugin

make-install: install

[macos]
uninstall:
    mkdir -p obs2/build
    cd obs2/build && cmake .. \
      -DCMAKE_BUILD_TYPE=Release \
      -DBROWSER_DEV=OFF \
      -DGE_PLUGIN_VERSION="{{ plugin_version }}" \
    && cmake --build . --target uninstall-plugin

[linux]
uninstall:
    just _flatpak-build uninstall-plugin

# builds the project and runs obs
[macos]
obs: make-release
    cd obs2/build && OBS_PLUGINS_PATH=$(pwd) OBS_PLUGINS_DATA_PATH=$(pwd) obs

[linux]
obs: make-release-flatpak
    cd obs2/build-flatpak && flatpak run \
      --device=dri \
      --filesystem="$(pwd):ro" \
      --socket=session-bus \
      --talk-name=org.freedesktop.secrets \
      --talk-name=org.freedesktop.portal.Desktop \
      --env=LD_LIBRARY_PATH="/app/lib" \
      --env=OBS_PLUGINS_PATH="$(pwd)" \
      --env=OBS_PLUGINS_DATA_PATH="$(pwd)" \
      com.obsproject.Studio

[linux]
make-release-flatpak:
    mkdir -p obs2/build
    cd obs2/build && cmake .. \
      -DCMAKE_BUILD_TYPE=Release \
      -DBROWSER_DEV=OFF \
      -DGE_PLUGIN_VERSION="{{ plugin_version }}" \
    && cmake --build . --target rust_build
    just _flatpak-build all

[linux]
_flatpak-build target:
    #!/usr/bin/env bash
    set -euo pipefail
    root="{{ justfile_directory() }}"
    app="com.obsproject.Studio"
    sdk_ref="$(flatpak info --show-sdk "${app}")"
    skip_build_inputs="ON"
    if [ "{{ target }}" = "uninstall-plugin" ]; then
      skip_build_inputs="OFF"
    fi

    if ! flatpak info "${sdk_ref}" >/dev/null 2>&1; then
      echo "Flatpak SDK ${sdk_ref} is not installed." >&2
      echo "Install it with: flatpak install flathub ${sdk_ref}" >&2
      exit 1
    fi

    flatpak run --devel \
      --filesystem="${root}" \
      --filesystem="${HOME}/.var/app/com.obsproject.Studio/config/obs-studio/plugins:create" \
      --filesystem=/tmp \
      --env=GE_REPO_ROOT="${root}" \
      --env=BROWSER_BUNDLE="${root}/obs2/browser/build/index.html" \
      --env=GE_SKIP_BUILD_INPUTS="${skip_build_inputs}" \
      --env=PKG_CONFIG_PATH="/app/lib/pkgconfig:/app/lib/x86_64-linux-gnu/pkgconfig:/app/share/pkgconfig:/usr/lib/x86_64-linux-gnu/pkgconfig:/usr/lib/pkgconfig:/usr/share/pkgconfig" \
      --env=LD_LIBRARY_PATH="/app/lib" \
      --command=bash \
      "${app}" \
      -lc 'set -euo pipefail
        cd "${GE_REPO_ROOT}"
        mkdir -p obs2/build-flatpak
        cd obs2/build-flatpak
        cmake .. \
          -DCMAKE_BUILD_TYPE=Release \
          -DBROWSER_DEV=OFF \
          -DGE_PLUGIN_VERSION="{{ plugin_version }}" \
          -DGE_PLUGIN_INSTALL_ROOT:PATH="${XDG_CONFIG_HOME}/obs-studio/plugins" \
          -DGE_SKIP_BROWSER_BUILD="${GE_SKIP_BUILD_INPUTS}" \
          -DGE_SKIP_RUST_BUILD="${GE_SKIP_BUILD_INPUTS}"
        if [ "{{ target }}" = "all" ]; then
          cmake --build .
        else
          cmake --build . --target "{{ target }}"
        fi
      '

obs-headers:
    #!/usr/bin/env bash
    set -euo pipefail

    dest_dir="{{ obs_headers }}"
    if [ -d "${dest_dir}" ]; then
      echo "OBS Headers already found."
      echo "If you want to re-download, delete \"${dest_dir}\""
      exit 0
    fi

    clone_dir=$(mktemp -d)
    git clone \
      --depth 1 \
      --branch "{{ obsapi_version }}" \
      --filter=blob:none \
      --sparse \
      https://github.com/obsproject/obs-studio.git \
      "${clone_dir}/obs-studio"

    pushd "${clone_dir}/obs-studio" > /dev/null
    git sparse-checkout set libobs frontend/api
    popd > /dev/null

    rm -rf "${dest_dir}"
    mkdir -p "${dest_dir}"
    cp -r "${clone_dir}/obs-studio/libobs" "${dest_dir}/"
    cp -r "${clone_dir}/obs-studio/frontend/api" "${dest_dir}/frontend"

    echo "{{ obsapi_version }}" > "${dest_dir}/OBS_VERSION"

# Compile OpenCV statically, so we don't have to depend on users having it
# installed on their system in order to use the plugin.
# Delete the prefix to force a rebuild.
opencv-static:
    #!/usr/bin/env bash
    set -euo pipefail

    version_file="${OPENCV_PREFIX}/.ge-static-version"
    installed_version=""
    if [ -f "${version_file}" ]; then
      installed_version="$(cat "${version_file}")"
    elif [ -f "${OPENCV_PREFIX}/lib/pkgconfig/opencv4.pc" ]; then
      installed_version="$(awk -F': ' '/^Version: / { print $2; exit }' "${OPENCV_PREFIX}/lib/pkgconfig/opencv4.pc")"
    fi

    if [ -f "${OPENCV_PREFIX}/lib/pkgconfig/opencv4.pc" ]; then
      if [ "${installed_version}" != "{{ opencv_version }}" ]; then
        echo "Static OpenCV at ${OPENCV_PREFIX} is ${installed_version:-unknown}, expected {{ opencv_version }}; rebuilding."
        rm -rf "${OPENCV_PREFIX}"
      else
        echo "Static OpenCV {{ opencv_version }} already built at ${OPENCV_PREFIX}"
        echo "{{ opencv_version }}" > "${version_file}"
        echo "Delete it to rebuild: rm -rf ${OPENCV_PREFIX}"
        exit 0
      fi
    fi

    rm -rf "${OPENCV_PREFIX}"

    work="$(mktemp -d)"
    trap 'rm -rf "${work}"' EXIT

    archive_dir="{{ source_archive_cache }}"
    archive="${archive_dir}/opencv-{{ opencv_version }}.tar.gz"
    mkdir -p "${archive_dir}"

    if [ -f "${archive}" ]; then
      echo "Using cached OpenCV archive ${archive}"
    else
      echo "Downloading OpenCV {{ opencv_version }} ..."
      wget -O "${work}/opencv.tar.gz" \
        "https://github.com/opencv/opencv/archive/refs/tags/{{ opencv_version }}.tar.gz"
      mv "${work}/opencv.tar.gz" "${archive}"
    fi

    tar xzf "${archive}" -C "${work}"
    src="${work}/opencv-{{ opencv_version }}"

    # Download a pinned CMake so the build is independent of whatever version
    # the system has installed.
    if [ "$(uname)" = "Darwin" ]; then
      cmake_platform="macos-universal"
      jobs="$(sysctl -n hw.logicalcpu)"
    else
      cmake_platform="linux-x86_64"
      jobs="$(nproc)"
    fi
    cmake_name="cmake-{{ cmake_version }}-${cmake_platform}"
    wget -O "${work}/cmake.tar.gz" \
      "https://github.com/Kitware/CMake/releases/download/v{{ cmake_version }}/${cmake_name}.tar.gz"
    tar xzf "${work}/cmake.tar.gz" -C "${work}"
    if [ "$(uname)" = "Darwin" ]; then
      cmake_bin="${work}/${cmake_name}/CMake.app/Contents/bin/cmake"
    else
      cmake_bin="${work}/${cmake_name}/bin/cmake"
    fi

    # Platform-specific flags: macOS doesn't have V4L/GTK/1394; LAPACK is
    # available via Accelerate but we don't need it for our minimal build.
    platform_flags=""
    if [ "$(uname)" = "Darwin" ]; then
      platform_flags="-D WITH_AVFOUNDATION=OFF -D WITH_LAPACK=OFF"
    else
      platform_flags="-D WITH_1394=OFF -D WITH_V4L=OFF -D WITH_GTK=OFF -D WITH_LAPACK=OFF"
    fi

    # Static libs are linked into the plugin's .so/.dylib, so everything must
    # be PIC. We build only the modules the matcher uses and force OpenCV to
    # compile its own zlib/libpng/libjpeg so nothing is pulled from the host.
    "${cmake_bin}" -S "${src}" -B "${work}/build" \
      -D CMAKE_BUILD_TYPE=Release \
      -D CMAKE_INSTALL_PREFIX="${OPENCV_PREFIX}" \
      -D BUILD_LIST=core,imgproc,imgcodecs \
      -D BUILD_SHARED_LIBS=OFF \
      -D ENABLE_PIC=ON \
      -D CMAKE_POSITION_INDEPENDENT_CODE=ON \
      -D OPENCV_GENERATE_PKGCONFIG=ON \
      -D OPENCV_FORCE_3RDPARTY_BUILD=ON \
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
      ${platform_flags}

    "${cmake_bin}" --build "${work}/build" -j"${jobs}"
    "${cmake_bin}" --install "${work}/build"
    echo "{{ opencv_version }}" > "${version_file}"

    echo
    echo "Static OpenCV installed to ${OPENCV_PREFIX}"
    echo "Now run 'just make' / 'just obs' — CMake auto-detects the static prefix."

# Compile FFmpeg statically, the same way we do OpenCV, so we don't have to
# depend on users having it installed in order to use the plugin. `ffmpeg-next`
# (the Rust crate) links these archives via pkg-config; CMake links them into
# the plugin's own link step (see cmake/FFmpegStatic.cmake).
# Delete the prefix to force a rebuild.
#
# x86_64 hosts need `nasm` (or `yasm`) for FFmpeg's hand-written assembly;
# arm64 (Apple Silicon) does not.
ffmpeg-static:
    #!/usr/bin/env bash
    set -euo pipefail

    version_file="${FFMPEG_PREFIX}/.ge-static-version"
    installed_version=""
    if [ -f "${version_file}" ]; then
      installed_version="$(cat "${version_file}")"
    elif [ -f "${FFMPEG_PREFIX}/include/libavutil/ffversion.h" ]; then
      installed_version="$(awk -F'"' '/^#define FFMPEG_VERSION / { print $2; exit }' "${FFMPEG_PREFIX}/include/libavutil/ffversion.h")"
    fi

    if [ -f "${FFMPEG_PREFIX}/lib/pkgconfig/libavcodec.pc" ]; then
      if [ "${installed_version}" != "{{ ffmpeg_version }}" ]; then
        echo "Static FFmpeg at ${FFMPEG_PREFIX} is ${installed_version:-unknown}, expected {{ ffmpeg_version }}; rebuilding."
        rm -rf "${FFMPEG_PREFIX}"
      else
        echo "Static FFmpeg {{ ffmpeg_version }} already built at ${FFMPEG_PREFIX}"
        echo "{{ ffmpeg_version }}" > "${version_file}"
        echo "Delete it to rebuild: rm -rf ${FFMPEG_PREFIX}"
        exit 0
      fi
    fi

    rm -rf "${FFMPEG_PREFIX}"

    work="$(mktemp -d)"
    trap 'rm -rf "${work}"' EXIT

    archive_dir="{{ source_archive_cache }}"
    archive="${archive_dir}/ffmpeg-{{ ffmpeg_version }}.tar.xz"
    mkdir -p "${archive_dir}"

    if [ -f "${archive}" ]; then
      echo "Using cached FFmpeg archive ${archive}"
    else
      echo "Downloading FFmpeg {{ ffmpeg_version }} ..."
      wget -O "${work}/ffmpeg.tar.xz" \
        "https://ffmpeg.org/releases/ffmpeg-{{ ffmpeg_version }}.tar.xz"
      mv "${work}/ffmpeg.tar.xz" "${archive}"
    fi

    tar xJf "${archive}" -C "${work}"
    src="${work}/ffmpeg-{{ ffmpeg_version }}"

    if [ "$(uname)" = "Darwin" ]; then
      jobs="$(sysctl -n hw.logicalcpu)"
    else
      jobs="$(nproc)"
    fi

    # Static, PIC, self-contained build. `--disable-autodetect` pins the build to
    # FFmpeg's own built-in codecs/muxers only (no x264, no system zlib/lzma, no
    # platform frameworks), so the resulting archives don't drag host libraries
    # into the plugin — mirroring the way `opencv-static` forces its own zlib/png.
    # The Rust code uses format/codec/swscale/swresample, not device capture or
    # libavfilter. The archives are linked into the plugin's .so/.dylib, so
    # everything is PIC.
    cd "${src}"
    ./configure \
      --prefix="${FFMPEG_PREFIX}" \
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
    echo "{{ ffmpeg_version }}" > "${version_file}"

    echo
    echo "Static FFmpeg installed to ${FFMPEG_PREFIX}"
    echo "Now run 'just make' / 'just obs' — CMake auto-detects the static prefix."

# setup the repository for local development
setup: obs-headers opencv-static ffmpeg-static
    cd obs2/browser && npm install
    cd test && npm install

clean:
    rm -rf "{{ obs_headers }}"
    rm -rf "node_modules"
    rm -rf "obs2/browser/node_modules"
    rm -rf "test/node_modules"
    rm -rf "obs2/ge_rust.h"
    rm -rf "obs2/build"
    rm -rf "esp32-input-monitor/.pio"
    @echo "Keeping static OpenCV/FFmpeg prefixes and cached source archives."
    cd "obs2/rust" && cargo clean
