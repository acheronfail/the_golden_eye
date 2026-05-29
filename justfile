set dotenv-load

model := "gemma-4-E4B-it"
gguf := "https://huggingface.co/unsloth/" + model + "-GGUF/resolve/main/" + model + "-Q4_K_M.gguf?download=true"
mmproj := "https://huggingface.co/unsloth/" + model + "-GGUF/resolve/main/mmproj-BF16.gguf?download=true"
llama_cpp_macos := "https://github.com/ggml-org/llama.cpp/releases/download/b9106/llama-b9106-bin-macos-arm64.tar.gz"
llama_cpp_linux := "https://github.com/ggml-org/llama.cpp/releases/download/b9113/llama-b9113-bin-ubuntu-vulkan-x64.tar.gz"

obs_version := "32.1.2"

export LLAMA_GGUF_PATH := "models/" + model + "-llm.gguf"
export LLAMA_MMPROJ_PATH := "models/" + model + "-mmproj.gguf"

_default:
  just -l

obs:
  mkdir -p obs2/build
  cd obs2/build && cmake .. && make
  cd obs2/build && OBS_PLUGINS_PATH=$(pwd) OBS_PLUGINS_DATA_PATH=$(pwd) obs 2>&1 \
    | sh -c 'trap "" INT; while IFS= read -r line; do case "$line" in *"[The Golden Eye]"*) printf "%s\n" "$line"; esac; done'

obs-headers:
  #!/usr/bin/env bash
  set -euxo pipefail
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

  dest_dir="obs2/vendor/obs"
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
