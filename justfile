model := "https://huggingface.co/unsloth/gemma-4-E2B-it-GGUF/resolve/main/gemma-4-E2B-it-Q4_K_M.gguf?download=true"
mmproj := "https://huggingface.co/unsloth/gemma-4-E2B-it-GGUF/resolve/main/mmproj-BF16.gguf?download=true"
llama_cpp_macos := "https://github.com/ggml-org/llama.cpp/releases/download/b9106/llama-b9106-bin-macos-arm64.tar.gz"
llama_cpp_linux := "https://github.com/ggml-org/llama.cpp/releases/download/b9106/llama-b9106-bin-ubuntu-x64.tar.gz"

_default:
  just -l

run:
  npm run obs

repl:
  npm run repl

# download models and download llama-server
setup:
  npm install

  mkdir -p models
  wget --no-clobber -O models/gemma-4-E2B-it-Q4_K_M.gguf {{model}} || true
  wget --no-clobber -O models/mmproj-BF16.gguf {{mmproj}} || true

  mkdir -p llama
  if [ "$(uname)" = "Darwin" ]; then \
    wget -O - {{llama_cpp_macos}} | tar xz -C llama --strip-components=1; \
    xattr -d com.apple.quarantine llama/* 2>/dev/null; \
  else \
    wget -O - {{llama_cpp_linux}} | tar xz -C llama --strip-components=1; \
  fi
