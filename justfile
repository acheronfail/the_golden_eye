set dotenv-load

model := "gemma-4-E4B-it"
gguf := "https://huggingface.co/unsloth/" + model + "-GGUF/resolve/main/" + model + "-Q4_K_M.gguf?download=true"
mmproj := "https://huggingface.co/unsloth/" + model + "-GGUF/resolve/main/mmproj-BF16.gguf?download=true"
llama_cpp_macos := "https://github.com/ggml-org/llama.cpp/releases/download/b9106/llama-b9106-bin-macos-arm64.tar.gz"
llama_cpp_linux := "https://github.com/ggml-org/llama.cpp/releases/download/b9113/llama-b9113-bin-ubuntu-vulkan-x64.tar.gz"

export LLAMA_GGUF_PATH := "models/" + model + "-llm.gguf"
export LLAMA_MMPROJ_PATH := "models/" + model + "-mmproj.gguf"

_default:
  just -l

obs:
  cd obs2/build && cmake ..
  cd obs2/build && make
  cd obs2/build && OBS_PLUGINS_PATH=$(pwd) OBS_PLUGINS_DATA_PATH=$(pwd) obs 2>&1 \
    | sh -c 'trap "" INT; while IFS= read -r line; do case "$line" in *"[The Golden Eye]"*) printf "%s\n" "$line";; esac; done'

run:
  npm run obs

repl:
  npm run repl

upload dir:
  npm run upload -- {{dir}}

# download models and download llama-server
setup:
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
