set dotenv-load

model := "gemma-4-E4B-it"
gguf := "https://huggingface.co/unsloth/" + model + "-GGUF/resolve/main/" + model + "-Q4_K_M.gguf?download=true"
mmproj := "https://huggingface.co/unsloth/" + model + "-GGUF/resolve/main/mmproj-BF16.gguf?download=true"
llama_cpp_macos := "https://github.com/ggml-org/llama.cpp/releases/download/b9106/llama-b9106-bin-macos-arm64.tar.gz"
llama_cpp_linux := "https://github.com/ggml-org/llama.cpp/releases/download/b9113/llama-b9113-bin-ubuntu-vulkan-x64.tar.gz"

obs_version := "32.1.2"
obs_headers := "obs2/vendor/obs"

export LLAMA_GGUF_PATH := "models/" + model + "-llm.gguf"
export LLAMA_MMPROJ_PATH := "models/" + model + "-mmproj.gguf"

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

make:
  mkdir -p obs2/build
  cd obs2/build && cmake .. -DCMAKE_BUILD_TYPE=Debug -DBROWSER_DEV=OFF && make

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
    --env=OBS_PLUGINS_PATH="$(pwd)" \
    --env=OBS_PLUGINS_DATA_PATH="$(pwd)" \
    com.obsproject.Studio

obs-headers:
  #!/usr/bin/env bash
  set -euo pipefail

  dest_dir="{{obs_headers}}"
  if [ -d "${dest_dir}" ]; then
    echo "OBS Headers already found."
    echo "If you want to re-download, delete \"${dest_dir}\""
    exit 0
  fi

  clone_dir=$(mktemp -d)
  git clone \
    --depth 1 \
    --branch "{{obs_version}}" \
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

  echo "{{obs_version}}" > "${dest_dir}/OBS_VERSION"

run:
  npm run obs

repl:
  npm run repl

upload dir:
  npm run upload -- {{dir}}

# download models and download llama-server
setup: obs-headers
  OPENCV4NODEJS_DISABLE_AUTOBUILD=1 npm install
  cd obs2/browser && npm install

  mkdir -p models
  wget --no-clobber -O {{LLAMA_GGUF_PATH}} {{gguf}} || true
  wget --no-clobber -O {{LLAMA_MMPROJ_PATH}} {{mmproj}} || true

  mkdir -p llama
  if [ "$(uname)" = "Darwin" ]; then \
    wget --no-clobber -O - {{llama_cpp_macos}} | tar xz -C llama --strip-components=1; \
    xattr -d com.apple.quarantine llama/* 2>/dev/null || true; \
  else \
    wget --no-clobber -O - {{llama_cpp_linux}} | tar xz -C llama --strip-components=1; \
  fi

clean:
  rm -rf "{{obs_headers}}"
  rm -rf "node_modules"
  rm -rf "obs2/browser/node_modules"
  rm -rf "obs2/ge_rust.h"
  rm -rf "obs2/build"
  rm -rf "esp32-input-monitor/.pio"
  cd "obs2/rust" && cargo clean
