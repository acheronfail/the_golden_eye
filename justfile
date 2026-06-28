set dotenv-load

#
# legacy (v1) variables
#

model := "gemma-4-E4B-it"
gguf := "https://huggingface.co/unsloth/" + model + "-GGUF/resolve/main/" + model + "-Q4_K_M.gguf?download=true"
mmproj := "https://huggingface.co/unsloth/" + model + "-GGUF/resolve/main/mmproj-BF16.gguf?download=true"
llama_cpp_macos := "https://github.com/ggml-org/llama.cpp/releases/download/b9106/llama-b9106-bin-macos-arm64.tar.gz"
llama_cpp_linux := "https://github.com/ggml-org/llama.cpp/releases/download/b9113/llama-b9113-bin-ubuntu-vulkan-x64.tar.gz"
export LLAMA_GGUF_PATH := "models/" + model + "-llm.gguf"
export LLAMA_MMPROJ_PATH := "models/" + model + "-mmproj.gguf"

#
# Build variables
#

obs_version := "32.1.2"
obs_headers := justfile_directory() / "obs2/vendor/obs"
opencv_version := "4.11.0"
cmake_version  := "3.31.7"

export BROWSER_BUNDLE := justfile_directory() / "obs2/browser/build/index.html"
export DYLD_FALLBACK_LIBRARY_PATH := "/Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain/usr/lib:/Library/Developer/CommandLineTools/usr/lib"
export OPENCV_PREFIX := justfile_directory() /  "obs2/vendor/opencv-static"

#
# Runtime variables
#

export GE_CV_LANG := "en"
export GE_CV_TEMPLATE_DIR := justfile_directory() / "obs2/cv_templates"

_default:
    just -l

# runs the project in dev mode (hot reloads for the UI)
dev:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p obs2/build
    cd obs2/build
    cmake .. -DCMAKE_BUILD_TYPE=Debug -DBROWSER_DEV=ON
    make
    ( cd ../browser && npm run dev ) &
    dev_pid=$!
    trap 'kill "$dev_pid" 2>/dev/null || true' EXIT
    OBS_PLUGINS_PATH="$(pwd)" OBS_PLUGINS_DATA_PATH="$(pwd)" obs

test: make-release
    npm run test

test-watch: make-release
    npm run test-watch

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
    export DYLD_FALLBACK_LIBRARY_PATH="{{DYLD_FALLBACK_LIBRARY_PATH}}"
    export CARGO_TARGET_DIR="{{justfile_directory()}}/obs2/rust/target/test"
    cd "{{justfile_directory()}}/obs2/rust" && cargo test --release {{args}}

make:
    mkdir -p obs2/build
    cd obs2/build && cmake .. -DCMAKE_BUILD_TYPE=Debug -DBROWSER_DEV=OFF && make

make-release:
    mkdir -p obs2/build
    cd obs2/build && cmake .. -DCMAKE_BUILD_TYPE=Release -DBROWSER_DEV=OFF && make

# builds the project and runs obs
obs: make-release
    cd obs2/build && OBS_PLUGINS_PATH=$(pwd) OBS_PLUGINS_DATA_PATH=$(pwd) obs

# builds the project and runs Flatpak OBS with this plugin build mounted
obs-flatpak: make-release
    cd obs2/build && flatpak run \
      --device=dri \
      --filesystem="$(pwd):ro" \
      --socket=session-bus \
      --talk-name=org.freedesktop.secrets \
      --talk-name=org.freedesktop.portal.Desktop \
      --env=LD_LIBRARY_PATH="/app/lib" \
      --env=GE_CV_LANG="{{GE_CV_LANG}}" \
      --env=GE_CV_TEMPLATE_DIR="{{GE_CV_TEMPLATE_DIR}}" \
      --env=OBS_PLUGINS_PATH="$(pwd)" \
      --env=OBS_PLUGINS_DATA_PATH="$(pwd)" \
      com.obsproject.Studio

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
      --branch "{{ obs_version }}" \
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

    echo "{{ obs_version }}" > "${dest_dir}/OBS_VERSION"

# Compile OpenCV statically, so we don't have to depend on users having it
# installed on their system in order to use the plugin.
# Delete the prefix to force a rebuild.
opencv-static:
    #!/usr/bin/env bash
    set -euo pipefail

    if [ -f "${OPENCV_PREFIX}/lib/pkgconfig/opencv4.pc" ]; then
      echo "Static OpenCV already built at ${OPENCV_PREFIX}"
      echo "Delete it to rebuild: rm -rf ${OPENCV_PREFIX}"
      exit 0
    fi

    work="$(mktemp -d)"
    trap 'rm -rf "${work}"' EXIT

    echo "Downloading OpenCV {{ opencv_version }} ..."
    wget -O "${work}/opencv.tar.gz" \
      "https://github.com/opencv/opencv/archive/refs/tags/{{ opencv_version }}.tar.gz"
    tar xzf "${work}/opencv.tar.gz" -C "${work}"
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

    echo
    echo "Static OpenCV installed to ${OPENCV_PREFIX}"
    echo "Now run 'just make' / 'just obs' — CMake auto-detects the static prefix."

run:
    npm run obs

repl:
    npm run repl

upload dir:
    npm run upload -- {{ dir }}

# setup the repository for local development
setup: obs-headers opencv-static
    OPENCV4NODEJS_DISABLE_AUTOBUILD=1 npm install
    cd obs2/browser && npm install

    mkdir -p models
    wget --no-clobber -O {{ LLAMA_GGUF_PATH }} {{ gguf }} || true
    wget --no-clobber -O {{ LLAMA_MMPROJ_PATH }} {{ mmproj }} || true

    mkdir -p llama
    if [ "$(uname)" = "Darwin" ]; then \
      wget --no-clobber -O - {{ llama_cpp_macos }} | tar xz -C llama --strip-components=1; \
      xattr -d com.apple.quarantine llama/* 2>/dev/null || true; \
    else \
      wget --no-clobber -O - {{ llama_cpp_linux }} | tar xz -C llama --strip-components=1; \
    fi

clean:
    rm -rf "{{obs_headers}}"
    rm -rf "node_modules"
    rm -rf "obs2/browser/node_modules"
    rm -rf "obs2/ge_rust.h"
    rm -rf "obs2/build"
    rm -rf "esp32-input-monitor/.pio"
    rm -rf "{{OPENCV_PREFIX}}"
    cd "obs2/rust" && cargo clean
