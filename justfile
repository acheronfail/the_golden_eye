set dotenv-load

model := "gemma-4-E4B-it"
gguf := "https://huggingface.co/unsloth/" + model + "-GGUF/resolve/main/" + model + "-Q4_K_M.gguf?download=true"
mmproj := "https://huggingface.co/unsloth/" + model + "-GGUF/resolve/main/mmproj-BF16.gguf?download=true"
llama_cpp_macos := "https://github.com/ggml-org/llama.cpp/releases/download/b9106/llama-b9106-bin-macos-arm64.tar.gz"
llama_cpp_linux := "https://github.com/ggml-org/llama.cpp/releases/download/b9113/llama-b9113-bin-ubuntu-vulkan-x64.tar.gz"

obs_version := "32.1.2"
obs_headers := "obs2/vendor/obs"

# Pinned OpenCV release built by `just opencv-static` and the prefix it installs
# into. The prefix is auto-detected by obs2/CMakeLists.txt (OPENCV_STATIC).
opencv_version := "4.11.0"
opencv_static_prefix := "obs2/vendor/opencv-static"

export LLAMA_GGUF_PATH := "models/" + model + "-llm.gguf"
export LLAMA_MMPROJ_PATH := "models/" + model + "-mmproj.gguf"
export DYLD_FALLBACK_LIBRARY_PATH := "/Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain/usr/lib:/Library/Developer/CommandLineTools/usr/lib"

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

make:
    mkdir -p obs2/build
    cd obs2/build && cmake .. -DCMAKE_BUILD_TYPE=Debug -DBROWSER_DEV=OFF && make

make-release:
    mkdir -p obs2/build
    cd obs2/build && cmake .. -DCMAKE_BUILD_TYPE=Release -DBROWSER_DEV=OFF && make

# builds the project and runs obs
obs: make
    cd obs2/build && OBS_PLUGINS_PATH=$(pwd) OBS_PLUGINS_DATA_PATH=$(pwd) obs

# builds the project and runs Flatpak OBS with this plugin build mounted
obs-flatpak: make
    cd obs2/build && flatpak run \
      --filesystem="$(pwd):ro" \
      --socket=session-bus \
      --talk-name=org.freedesktop.secrets \
      --talk-name=org.freedesktop.portal.Desktop \
      --env=LD_LIBRARY_PATH="/app/lib" \
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

# Build a minimal, fully static OpenCV (core + imgproc + imgcodecs, with its
# third-party codecs bundled) into vendor/opencv-static. Once present, the
# obs2 CMake build auto-detects it (OPENCV_STATIC) and links OpenCV statically,
# so the resulting plugin is portable across Linux distros without needing a
# matching system OpenCV. Delete the prefix to force a rebuild. Linux only;
# macOS links Homebrew's OpenCV dynamically.
[linux]
opencv-static:
    #!/usr/bin/env bash
    set -euo pipefail

    prefix="$(pwd)/{{ opencv_static_prefix }}"
    if [ -f "${prefix}/lib/pkgconfig/opencv4.pc" ]; then
      echo "Static OpenCV already built at ${prefix}"
      echo "Delete it to rebuild: rm -rf ${prefix}"
      exit 0
    fi

    work="$(mktemp -d)"
    trap 'rm -rf "${work}"' EXIT

    echo "Downloading OpenCV {{ opencv_version }} ..."
    wget -O "${work}/opencv.tar.gz" \
      "https://github.com/opencv/opencv/archive/refs/tags/{{ opencv_version }}.tar.gz"
    tar xzf "${work}/opencv.tar.gz" -C "${work}"
    src="${work}/opencv-{{ opencv_version }}"

    # Static libs are linked into the plugin's .so, so everything must be PIC.
    # We build only the modules the matcher uses and force OpenCV to compile its
    # own zlib/libpng/libjpeg so nothing is pulled from the host at runtime.
    cmake -S "${src}" -B "${work}/build" \
      -D CMAKE_BUILD_TYPE=Release \
      -D CMAKE_INSTALL_PREFIX="${prefix}" \
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
      -D WITH_GTK=OFF -D WITH_QT=OFF -D WITH_OPENGL=OFF \
      -D WITH_1394=OFF -D WITH_V4L=OFF -D WITH_LAPACK=OFF \
      -D WITH_PROTOBUF=OFF -D WITH_GPHOTO2=OFF -D WITH_GDAL=OFF \
      -D WITH_FREETYPE=OFF -D WITH_IPP=OFF -D WITH_ITT=OFF \
      -D WITH_TBB=OFF -D WITH_OPENMP=OFF -D WITH_CUDA=OFF \
      -D BUILD_opencv_apps=OFF -D BUILD_TESTS=OFF -D BUILD_PERF_TESTS=OFF \
      -D BUILD_EXAMPLES=OFF -D BUILD_DOCS=OFF -D BUILD_JAVA=OFF \
      -D BUILD_opencv_python3=OFF

    cmake --build "${work}/build" -j"$(nproc)"
    cmake --install "${work}/build"

    echo
    echo "Static OpenCV installed to ${prefix}"
    echo "Now run 'just make' / 'just obs' — CMake auto-detects the static prefix."

run:
    npm run obs

repl:
    npm run repl

upload dir:
    npm run upload -- {{ dir }}

# download models and download llama-server
setup: obs-headers
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
    rm -rf "{{ obs_headers }}"
    rm -rf "node_modules"
    rm -rf "obs2/browser/node_modules"
    rm -rf "obs2/ge_rust.h"
    rm -rf "obs2/build"
    rm -rf "esp32-input-monitor/.pio"
    cd "obs2/rust" && cargo clean
