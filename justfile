#
# Build variables
#

set tempdir := "/tmp"

git_plugin_version := `
tag="$(git describe --tags --exact-match --match 'v*' 2>/dev/null || true)"

  if printf '%s\n' "$tag" | grep -Eq '^v[0-9]+\.[0-9]+\.[0-9]+$'; then
    printf '%s' "${tag#v}"
  else
    sha="$(git rev-parse --short HEAD 2>/dev/null || printf unknown)"
    printf '0.0.0-dev+%s' "$sha"
  fi
`
plugin_version := env_var_or_default("GE_PLUGIN_VERSION", git_plugin_version)
export DYLD_FALLBACK_LIBRARY_PATH := "/Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain/usr/lib:/Library/Developer/CommandLineTools/usr/lib"
export BROWSER_BUNDLE := justfile_directory() / "obs2/browser/build/index.html"
export GE_PLUGIN_VERSION := plugin_version
export VITE_GE_PLUGIN_VERSION := plugin_version
export OPENCV_PREFIX := justfile_directory() / "obs2/vendor/opencv-static"
export FFMPEG_PREFIX := justfile_directory() / "obs2/vendor/ffmpeg-static"

_default:
    just -l

configure build_type browser_dev build_dir *cmake_args:
    #!/usr/bin/env bash
    set -euo pipefail
    build_dir="{{ build_dir }}"
    source_dir="{{ justfile_directory() }}/obs2"
    cache="${build_dir}/CMakeCache.txt"
    if [ -f "${cache}" ] && ! grep -qx "CMAKE_HOME_DIRECTORY:INTERNAL=${source_dir}" "${cache}"; then
      echo "Removing stale CMake build directory ${build_dir}"
      rm -rf "${build_dir}"
    fi
    mkdir -p "${build_dir}"
    cd "${build_dir}"
    cmake "${source_dir}" \
      -DCMAKE_BUILD_TYPE="{{ build_type }}" \
      -DBROWSER_DEV="{{ browser_dev }}" \
      -DGE_PLUGIN_VERSION="{{ plugin_version }}" {{ cmake_args }}

configure-debug:
    just configure Debug OFF obs2/build

configure-dev:
    just configure Debug ON obs2/build

configure-release:
    just configure Release OFF obs2/build

vscode-settings: configure-release
    mkdir -p .vscode
    cp obs2/build/vscode-settings.json .vscode/settings.json

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
    just configure-dev
    cmake --build obs2/build
    build_dir="${root}/obs2/build"

    dev_pids=""
    cleanup() { [ -n "$dev_pids" ] && kill $dev_pids 2>/dev/null || true; }
    trap cleanup EXIT

    # Vite dev server: hot reload for the SvelteKit SPA.
    ( cd "$root/obs2/browser" && npm run dev ) &
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
          if cmake --build . --target golden_core; then
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

    build_dir="{{ justfile_directory() }}/obs2/build"
    just configure-release
    source "$build_dir/rust-cargo-env.sh"

    # Keep cargo test artifacts separate from the plugin build artifacts.
    export CARGO_TARGET_DIR="{{ justfile_directory() }}/obs2/rust/target/test"
    cd "{{ justfile_directory() }}/obs2/rust" && cargo test --release {{ args }}

make: configure-debug
    cmake --build obs2/build

[macos]
make-release: configure-release
    cmake --build obs2/build

[linux]
make-release: make-release-flatpak

[macos]
make-package: configure-release
    cmake --build obs2/build --target package-plugin

[linux]
make-package: make-release-flatpak
    just _flatpak-build package-plugin

[macos]
install: configure-release
    cmake --build obs2/build --target install-plugin

[linux]
install: make-release-flatpak
    just _flatpak-build install-plugin

make-install: install

[macos]
uninstall: configure-release
    cmake --build obs2/build --target uninstall-plugin

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
    just configure-release
    cmake --build obs2/build --target rust_build
    just _flatpak-build all

[linux]
_flatpak-build target:
    #!/usr/bin/env bash
    set -euo pipefail
    root="{{ justfile_directory() }}"
    app="com.obsproject.Studio"
    sdk_ref="$(flatpak info --show-sdk "${app}")"

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
      --env=GE_REUSE_HOST_BUILD_INPUTS="ON" \
      --env=PKG_CONFIG_PATH="/app/lib/pkgconfig:/app/lib/x86_64-linux-gnu/pkgconfig:/app/share/pkgconfig:/usr/lib/x86_64-linux-gnu/pkgconfig:/usr/lib/pkgconfig:/usr/share/pkgconfig" \
      --env=LD_LIBRARY_PATH="/app/lib" \
      --command=bash \
      "${app}" \
      -lc 'set -euo pipefail
        cd "${GE_REPO_ROOT}"
        build_dir="obs2/build-flatpak"
        source_dir="${GE_REPO_ROOT}/obs2"
        cache="${build_dir}/CMakeCache.txt"
        if [ -f "${cache}" ] && ! grep -qx "CMAKE_HOME_DIRECTORY:INTERNAL=${source_dir}" "${cache}"; then
          echo "Removing stale CMake build directory ${build_dir}"
          rm -rf "${build_dir}"
        fi
        mkdir -p "${build_dir}"
        cd "${build_dir}"
        cmake "${source_dir}" \
          -DCMAKE_BUILD_TYPE=Release \
          -DBROWSER_DEV=OFF \
          -DGE_PLUGIN_VERSION="{{ plugin_version }}" \
          -DGE_LINUX_NATIVE_OBS_BUILD=ON \
          -DGE_PLUGIN_INSTALL_ROOT:PATH="${XDG_CONFIG_HOME}/obs-studio/plugins" \
          -DGE_REUSE_HOST_BUILD_INPUTS="${GE_REUSE_HOST_BUILD_INPUTS}"
        if [ "{{ target }}" = "all" ]; then
          cmake --build .
        else
          cmake --build . --target "{{ target }}"
        fi
      '

obs-headers:
    "{{ justfile_directory() }}/obs2/scripts/obs-headers.sh"

# Compile OpenCV statically, so we don't have to depend on users having it
# installed on their system in order to use the plugin.
# Delete the prefix to force a rebuild.
opencv-static:
    "{{ justfile_directory() }}/obs2/scripts/opencv-static.sh"

# Compile FFmpeg statically, the same way we do OpenCV, so we don't have to
# depend on users having it installed in order to use the plugin. `ffmpeg-next`
# (the Rust crate) links these archives via pkg-config; CMake links them into
# the plugin's own link step (see cmake/FFmpegStatic.cmake).
# Delete the prefix to force a rebuild.
#
# x86_64 hosts need `nasm` (or `yasm`) for FFmpeg's hand-written assembly;
# arm64 (Apple Silicon) does not.
ffmpeg-static:
    "{{ justfile_directory() }}/obs2/scripts/ffmpeg-static.sh"

# setup the repository for local development
setup: obs-headers opencv-static ffmpeg-static vscode-settings
    cd obs2/browser && npm install
    cd test && npm install

clean:
    rm -rf "node_modules"
    rm -rf "obs2/browser/node_modules"
    rm -rf "test/node_modules"
    rm -rf "obs2/ge_rust.h"
    rm -rf "obs2/build"
    rm -rf "esp32-input-monitor/.pio"
    @echo "Keeping static OBS/OpenCV/FFmpeg prefixes and cached source archives."
    cd "obs2/rust" && cargo clean

clean_all: clean
    rm -rf "obs2/vendor/obs"
    rm -rf "obs2/vendor/opencv-static"
    rm -rf "obs2/vendor/ffmpeg-static"
