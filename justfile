#
# Build variables
#

git_plugin_version := `
tag="$(git describe --tags --exact-match --match 'v*' 2>/dev/null || true)"
release_tag_regex='^v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z][0-9A-Za-z.-]*)?(\+[0-9A-Za-z][0-9A-Za-z.-]*)?$'

  if printf '%s\n' "$tag" | grep -Eq "$release_tag_regex"; then
    printf '%s' "${tag#v}"
  else
    base_tag="$(git describe --tags --abbrev=0 --match 'v[0-9]*.[0-9]*.[0-9]*' --match 'v[0-9]*.[0-9]*.[0-9]*-*' 2>/dev/null || true)"
    base_version="${base_tag#v}"
    if [ -z "$base_version" ]; then
      base_version="0.0.0"
    fi
    sha="$(git rev-parse --short HEAD 2>/dev/null || printf unknown)"
    printf '%s-dev+%s' "$base_version" "$sha"
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

# configure the cmake project
configure build_type browser_dev *cmake_args:
    #!/usr/bin/env bash
    set -euo pipefail
    build_dir="{{ justfile_directory() }}/obs2/build"
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
      -DGE_RUST_PACKAGE_PROFILE=OFF \
      -DGE_PLUGIN_VERSION="{{ plugin_version }}" {{ cmake_args }}

# debug builds
configure-debug:
    just configure Debug OFF

# dev builds (debug + browser dev mode)
configure-dev:
    just configure Debug ON

# release builds
[unix]
configure-release:
    just configure Release OFF

# release builds (needs vcpkg's toolchain file so CMake's find_path picks up
# simde/opencv4/ffmpeg from the vcpkg-installed triplet -- see
# windows-vcpkg-deps below)
[windows]
configure-release:
    #!/usr/bin/env bash
    set -euo pipefail
    vcpkg_root="${VCPKG_ROOT:-${VCPKG_INSTALLATION_ROOT:-C:/vcpkg}}"
    if command -v cygpath >/dev/null 2>&1; then
      vcpkg_root="$(cygpath -m "${vcpkg_root}")"
    fi
    export VCPKGRS_TRIPLET="${VCPKGRS_TRIPLET:-x64-windows-static-md}"
    just configure Release OFF \
      -DCMAKE_TOOLCHAIN_FILE="${vcpkg_root}/scripts/buildsystems/vcpkg.cmake" \
      -DVCPKG_TARGET_TRIPLET="${VCPKGRS_TRIPLET}"

# configure cmake for packaging (longer compile times due to LTO/strip/etc)
[unix]
configure-package:
    just configure Release OFF -DGE_RUST_PACKAGE_PROFILE=ON

# configure cmake for packaging (needs vcpkg's toolchain file -- see
# configure-release above)
[windows]
configure-package:
    #!/usr/bin/env bash
    set -euo pipefail
    vcpkg_root="${VCPKG_ROOT:-${VCPKG_INSTALLATION_ROOT:-C:/vcpkg}}"
    if command -v cygpath >/dev/null 2>&1; then
      vcpkg_root="$(cygpath -m "${vcpkg_root}")"
    fi
    export VCPKGRS_TRIPLET="${VCPKGRS_TRIPLET:-x64-windows-static-md}"
    just configure Release OFF \
      -DGE_RUST_PACKAGE_PROFILE=ON \
      -DCMAKE_TOOLCHAIN_FILE="${vcpkg_root}/scripts/buildsystems/vcpkg.cmake" \
      -DVCPKG_TARGET_TRIPLET="${VCPKGRS_TRIPLET}"

# generate IDE settings files
ide-settings: configure-release
    mkdir -p .vscode
    cp obs2/build/vscode-settings.json .vscode/settings.json
    mkdir -p .zed
    cp obs2/build/zed-settings.json .zed/settings.json

# runs the project in dev mode (hot reloads for the UI and the plugin)
dev:
    python3 obs2/scripts/dev.py

# builds a fake "newer" release package and serves it locally, so `just obs`
# can be smoke-tested against the real production auto-update flow. Prints
# the GE_UPDATE_CHECK_URL to export before running `just obs` separately.
simulate-update:
    python3 obs2/scripts/simulate_update.py

# runs the rust tests
test-rust *args:
    #!/usr/bin/env bash
    set -euo pipefail

    build_dir="{{ justfile_directory() }}/obs2/build"
    just configure-release
    cmake --build "$build_dir" --target browser_build
    source "$build_dir/rust-cargo-env.sh"

    # Share Cargo's release target by default so builds, tests, and packaging
    # reuse the same incremental artifacts. Opt into isolation only when needed.
    if [ "${GE_ISOLATE_CARGO_TEST_TARGETS:-}" = "1" ]; then
      export CARGO_TARGET_DIR="{{ justfile_directory() }}/obs2/rust/target/test"
    fi
    cd "{{ justfile_directory() }}/obs2/rust"
    cargo test --release {{ args }}

# runs the backend against the controllable Rust OBS host (no OBS process)
test-integration *args:
    #!/usr/bin/env bash
    set -euo pipefail

    build_dir="{{ justfile_directory() }}/obs2/build"
    just configure-release
    cmake --build "$build_dir" --target browser_build
    source "$build_dir/rust-cargo-env.sh"
    # Share Cargo's release target by default so builds, tests, and packaging
    # reuse the same incremental artifacts. Opt into isolation only when needed.
    if [ "${GE_ISOLATE_CARGO_TEST_TARGETS:-}" = "1" ]; then
      export CARGO_TARGET_DIR="{{ justfile_directory() }}/obs2/rust/target/integration"
    fi
    cd "{{ justfile_directory() }}/obs2/rust"
    cargo test --release --tests -- --ignored --test-threads=1 {{ args }}

# runs browser unit/component tests
test-browser *args:
    cd obs2/browser && npm run test:unit -- --run {{ args }}

# runs the shim's dlopen/reload/rollback fixture tests (no OBS/Rust toolchain needed)
test-shim:
    #!/usr/bin/env bash
    set -euo pipefail
    build_dir="{{ justfile_directory() }}/obs2/shim/tests/build"
    cmake -S "{{ justfile_directory() }}/obs2/shim/tests" -B "$build_dir" -DCMAKE_BUILD_TYPE=Debug
    # --config/-C are ignored by single-config generators (Unix Makefiles) and
    # required by multi-config ones (Visual Studio on Windows), so pass both
    # unconditionally rather than branching on platform.
    cmake --build "$build_dir" --config Debug
    ctest --test-dir "$build_dir" --output-on-failure -C Debug

# runs opencv frame tests
test-cv *filter: make-release
    cd test && npm run test -- {{ filter }}

# runs one benchmark per unique cv frame scenario
bench-cv *filter: make-release
    cd test && npm run bench -- {{ filter }}

# runs opencv frame tests
test:
    just test-browser
    just test-shim
    just test-rust
    just test-cv

# formats the project and runs clippy
fmt:
    just clippy
    cd obs2/browser && npm run format:repo
    cd obs2/rust && rustup run nightly cargo fmt --
    find obs2 obs2/shim obs2/shim/tests obs2/core -maxdepth 1 \( -name '*.c' -o -name '*.h' \) ! -name ge_rust.h -print0 | xargs -0 clang-format -style=file -i

# runs clippy
clippy:
    #!/usr/bin/env bash
    set -euo pipefail
    build_dir="{{ justfile_directory() }}/obs2/build"
    just configure-release
    cmake --build "$build_dir" --target browser_build
    source "$build_dir/rust-cargo-env.sh"

    cd "{{ justfile_directory() }}/obs2/rust" && cargo clippy --fix -- -D warnings

# generate a markdown preview of what GitHub will put in the next release notes
preview-release sha="HEAD":
    #!/usr/bin/env bash
    set -euo pipefail

    sha="{{ sha }}"
    target_sha="$(git rev-parse "$sha")"

    last_tag="$(git describe --tags --abbrev=0 --match 'v[0-9]*.[0-9]*.[0-9]*' --match 'v[0-9]*.[0-9]*.[0-9]*-*' "$target_sha" 2>/dev/null || true)"
    if [ -z "$last_tag" ]; then
      echo "No previous release tag found for ${sha} (${target_sha})." >&2
      exit 1
    fi

    if ! gh api "repos/:owner/:repo/commits/${target_sha}" >/dev/null 2>&1; then
      branch="$(git branch --show-current)"
      echo "GitHub cannot see ${sha} (${target_sha}), so it cannot generate release notes for it." >&2
      echo "Make sure this commit exists on the remote and try again!" >&2
      exit 1
    fi

    echo "Previewing release notes from ${last_tag}..${sha} (${target_sha})" >&2
    gh api repos/:owner/:repo/releases/generate-notes \
      -f tag_name="{{ git_plugin_version }}" \
      -f previous_tag_name="$last_tag" \
      -f target_commitish="$target_sha" \
      --jq .body

# build the plugin in debug mode
make: configure-debug
    cmake --build obs2/build

# build the plugin in release mode
[macos]
make-release: configure-release
    cmake --build obs2/build

# build the plugin in release mode
[linux]
make-release: make-release-flatpak

# build the plugin in release mode
[windows]
make-release: configure-release
    cmake --build obs2/build --config Release

# package the plugin with the slower Rust dist profile
[macos]
make-package-dist: configure-package
    cmake -E rm -rf obs2/build/package
    cmake --build obs2/build --target package-plugin

# package the plugin with the normal Rust release profile
[macos]
make-package: configure-release
    cmake -E rm -rf obs2/build/package
    cmake --build obs2/build --target package-plugin

# package the plugin with the slower Rust dist profile
[linux]
make-package-dist: configure-package
    cmake -E rm -rf obs2/build-flatpak/package
    cmake --build obs2/build --target rust_build
    just _flatpak-build package-plugin ON

# package the plugin with the normal Rust release profile
[linux]
make-package: configure-release
    cmake -E rm -rf obs2/build-flatpak/package
    cmake --build obs2/build --target rust_build
    just _flatpak-build package-plugin OFF

# package the plugin with the slower Rust dist profile
[windows]
make-package-dist: configure-package
    cmake -E rm -rf obs2/build/package
    cmake --build obs2/build --config Release --target package-plugin

# package the plugin with the normal Rust release profile
[windows]
make-package: configure-release
    cmake -E rm -rf obs2/build/package
    cmake --build obs2/build --config Release --target package-plugin

# install the plugin on the current machine (release)
[macos]
install: configure-release
    cmake --build obs2/build --target install-plugin

# install the plugin on the current machine (release)
[linux]
install: make-release-flatpak
    just _flatpak-build install-plugin

# install the plugin on the current machine (release)
[windows]
install: configure-release
    cmake --build obs2/build --config Release --target install-plugin

# uninstall the plugin from the current machine (release)
[macos]
uninstall: configure-release
    cmake --build obs2/build --target uninstall-plugin

# uninstall the plugin from the current machine (release)
[linux]
uninstall:
    just _flatpak-build uninstall-plugin

# uninstall the plugin from the current machine (release)
[windows]
uninstall: configure-release
    cmake --build obs2/build --config Release --target uninstall-plugin

# runs OBS with the plugin (release)
[macos]
obs: make-release
    cd obs2/build && OBS_PLUGINS_PATH="$(pwd)" OBS_PLUGINS_DATA_PATH="$(pwd)" obs

# runs OBS with the staged package build
[macos]
obs-packaged: make-package
    cd obs2/build/package/* && OBS_PLUGINS_PATH="$(pwd)" OBS_PLUGINS_DATA_PATH="$(pwd)" obs

# runs OBS with the plugin (release)
[linux]
obs: make-release-flatpak
    cd obs2/build-flatpak && flatpak run \
      --device=dri \
      --filesystem="$(pwd):ro" \
      --socket=session-bus \
      --talk-name=org.freedesktop.secrets \
      --talk-name=org.freedesktop.portal.Desktop \
      --env=LD_LIBRARY_PATH="/app/lib" \
      --env=OBS_PLUGINS_PATH="$(pwd)/%module%/bin/64bit" \
      --env=OBS_PLUGINS_DATA_PATH="$(pwd)/%module%/data" \
      {{ if env_var_or_default("GE_UPDATE_CHECK_URL", "") != "" { "--env=GE_UPDATE_CHECK_URL=" + env_var("GE_UPDATE_CHECK_URL") } else { "" } }} \
      com.obsproject.Studio

# runs OBS with the staged package build
[linux]
obs-packaged: make-package
    cd obs2/build-flatpak/package/*/the_golden_eye && \
    flatpak run \
      --device=dri \
      --filesystem="$(pwd):ro" \
      --socket=session-bus \
      --talk-name=org.freedesktop.secrets \
      --talk-name=org.freedesktop.portal.Desktop \
      --env=LD_LIBRARY_PATH="/app/lib" \
      --env=OBS_PLUGINS_PATH="$(pwd)/bin/64bit" \
      --env=OBS_PLUGINS_DATA_PATH="$(pwd)/data" \
      com.obsproject.Studio

# runs OBS with the plugin (release)
[windows]
obs: make-release
    #!/usr/bin/env bash
    set -euo pipefail
    obs="${OBS_EXE:-C:/Program Files/obs-studio/bin/64bit/obs64.exe}"
    if command -v cygpath >/dev/null 2>&1; then
      obs="$(cygpath -u "${obs}")"
    fi
    OBS_PLUGINS_PATH="$(pwd)/obs2/build/Release" OBS_PLUGINS_DATA_PATH="$(pwd)/obs2/build" "${obs}"

# build the plugin with the flatpak SDK
[linux]
make-release-flatpak:
    just configure-release
    cmake --build obs2/build --target rust_build
    just _flatpak-build all

[linux]
_flatpak-build target rust_package_profile="OFF":
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
      --env=GE_RUST_PACKAGE_PROFILE="{{ rust_package_profile }}" \
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
          -DGE_REUSE_HOST_BUILD_INPUTS="${GE_REUSE_HOST_BUILD_INPUTS}" \
          -DGE_RUST_PACKAGE_PROFILE="${GE_RUST_PACKAGE_PROFILE}"
        if [ "{{ target }}" = "all" ]; then
          cmake --build .
        else
          cmake --build . --target "{{ target }}"
        fi
      '

# download and vendor in the OBS headers
obs-headers:
    #!/usr/bin/env bash
    set -euo pipefail
    root="$(pwd)"
    "${root}/obs2/scripts/obs-headers.sh"

# download and statically compile opencv
[unix]
opencv-static:
    #!/usr/bin/env bash
    set -euo pipefail
    root="$(pwd)"
    "${root}/obs2/scripts/opencv-static.sh"

# download and statically compile ffmpeg
[unix]
ffmpeg-static:
    #!/usr/bin/env bash
    set -euo pipefail
    root="$(pwd)"
    "${root}/obs2/scripts/ffmpeg-static.sh"

# install dependencies with vcpkg
[windows]
windows-vcpkg-deps:
    #!/usr/bin/env bash
    set -euo pipefail
    vcpkg_root="${VCPKG_ROOT:-${VCPKG_INSTALLATION_ROOT:-C:/vcpkg}}"
    if command -v cygpath >/dev/null 2>&1; then
      vcpkg_root="$(cygpath -u "${vcpkg_root}")"
    fi
    vcpkg="${vcpkg_root}/vcpkg"
    [ -x "${vcpkg}.exe" ] && vcpkg="${vcpkg}.exe"
    "${vcpkg}" install --triplet x64-windows-static-md --clean-after-build opencv4 ffmpeg simde

# setup the repository for local development
[windows]
setup: obs-headers windows-vcpkg-deps
    cd obs2/browser && npm install
    cd test && npm install

# setup the repository for local development
[unix]
setup: obs-headers opencv-static ffmpeg-static ide-settings
    cd obs2/browser && npm install
    cd test && npm install

# clean build files and outputs
clean:
    rm -rf "node_modules"
    rm -rf "obs2/browser/node_modules"
    rm -rf "test/node_modules"
    rm -rf "obs2/ge_rust.h"
    rm -rf "obs2/build"
    cd "obs2/rust" && cargo clean
    @echo "Keeping vendored packages, use 'just clean_all' to remove those as well"

# clean build files and outputs, as well as vendorered builds
clean_all: clean
    rm -rf "obs2/vendor/obs"
    rm -rf "obs2/vendor/opencv-static"
    rm -rf "obs2/vendor/ffmpeg-static"
