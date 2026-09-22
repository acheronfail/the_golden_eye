# Contributing

## Project overview

- `obs2/loader/` loads the core and applies its staged replacement.
- `obs2/core/` connects OBS events, frame capture, and replay callbacks to Rust.
- `obs2/rust/runtime/` owns plugin workflows, settings, HTTP, and update policy.
- The other crates under `obs2/rust/` own game rules, matching, clips, media, and catalog storage.
- `obs2/browser/` is the SvelteKit browser dock embedded in the core.
- `obs2/cv_templates/` contains the matcher image templates.
- `frame_tests/` contains the matcher regression harness and shared frame and clip fixtures.

See the [runtime map](obs2/rust/runtime/src/README.md), [CV map](obs2/rust/cv/README.md), and
[browser map](obs2/browser/src/lib/README.md) for code ownership.

## System prerequisites

Install Git, `rustup`, Node.js (the version in `.nvmrc`), `just`, Python 3, CMake, and
`clang-format`. Add the tools for your operating system:

- **Linux:** a C/C++ compiler, `wget`, `nasm`, `pkg-config`, and development libraries for D-Bus,
  OpenSSL, and Wayland. Install the OBS Flatpak and its SDK:

  ```sh
  flatpak install flathub com.obsproject.Studio
  flatpak install flathub "$(flatpak info --show-sdk com.obsproject.Studio)"
  ```

  On Debian/Ubuntu, setup also installs Chromium's system libraries and can request sudo access. On
  other distributions, install Chromium's system libraries through the system package manager. Setup
  checks that the downloaded browser can start.

- **macOS:** OBS in `/Applications`, Xcode command-line tools, `wget`, `simde`, and `nasm`.

  ```sh
  xcode-select --install
  brew install just cmake clang-format wget simde nasm
  ```

- **Windows:** OBS Studio, Visual Studio Build Tools with MSVC, LLVM, Cygwin tools, and vcpkg. Set
  `VCPKG_ROOT` and use a shell with the MSVC environment loaded. Set
  `git config core.autocrlf input`. Install Git through Cygwin Setup when using its shell.

## Setup and daily work

Run these commands from the repository root:

```sh
just setup       # install project dependencies and Chromium; generate IDE settings
just dev         # launch OBS with browser and core hot reload
just storybook   # preview components without OBS
```

Setup installs the pinned Rust toolchain, Clippy, nightly rustfmt, npm dependencies, OBS headers,
and native dependencies. Run it again after dependency changes. `just setup-browser` reinstalls
Chromium after a Playwright update; `just setup-toolchains` reinstalls the Rust tools.

| Command             | Purpose                                                           |
| ------------------- | ----------------------------------------------------------------- |
| `just make`         | Debug plugin build; uses the OBS Flatpak SDK on Linux             |
| `just make-release` | Release plugin build; uses the same SDK on Linux                  |
| `just obs`          | Build and launch OBS with the plugin                              |
| `just make-package` | Create an installation package                                    |
| `just fmt`          | Format files without compiling or running checks                  |
| `just fmt-check`    | Check formatting without changing files                           |
| `just check`        | Regenerate contracts; check formatting, Clippy, and browser types |
| `just test`         | Run every test suite                                              |

CI runs `just check` and checks that generated files match the committed versions.

## Choose a test suite

| Command                 | Coverage                                     |
| ----------------------- | -------------------------------------------- |
| `just test-browser`     | Browser logic and components in Node/jsdom   |
| `just test-storybook`   | Storybook cases in Chromium                  |
| `just test-loader`      | Native load, reload, and rollback fixtures   |
| `just test-rust`        | Rust unit tests across the plugin crates     |
| `just test-integration` | Runtime workflows against a fake OBS host    |
| `just test-obs`         | Production plugin in real OBS on Linux       |
| `just test-cv`          | Release build and captured-frame regressions |

The browser and Rust commands accept extra test arguments. The frame harness accepts a filename
regex, for example `just test-cv flicker`. See [frame_tests/README.md](frame_tests/README.md) for
fixtures.

`just test-obs` needs the OBS Flatpak and an X11 display. See
[the real OBS test guide](frame_tests/obs/README.md) for repeated runs, failure artifacts, and CI
options.

## Clean generated files

- `just clean` removes build and test outputs, including the generated C header. It keeps
  dependencies.
- `just clean-deps` also removes the two npm dependency directories.
- `just clean-all` also removes vendored OBS headers and static OpenCV/FFmpeg builds.

Cleanup preserves tracked contracts and test fixtures, downloaded source archives, Rust toolchains,
and the shared Playwright browser cache. Run `just setup` after dependency cleanup.

## Logging

The Rust runtime logs through OBS's own logging facility, so its lines land in the OBS log alongside
OBS's own output, each prefixed with `[the_golden_eye]`.

To read them:

- **From OBS:** _Help → Log Files → View Current Log_. This is the most reliable way to see the
  logs, and the only one on Windows, where raw logs are not easily visible from a terminal.
- **On disk**, the current session's log is the newest file under:
  - macOS: `~/Library/Application Support/obs-studio/logs/`
  - Linux (Flatpak): `~/.var/app/com.obsproject.Studio/config/obs-studio/logs/`
  - Windows: `%APPDATA%\obs-studio\logs\`
- **In the terminal:** when OBS is launched from a shell (`just obs` / `just dev` on macOS and
  Linux), the same lines are also printed to stdout (look for `[the_golden_eye]`).

Verbosity is controlled by the `RUST_LOG` environment variable (a
[`tracing` filter](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html);
the crate name is `ge_runtime`). Release builds default to `info`, so `debug`-level lines are
hidden. To show them, launch OBS with `RUST_LOG` set:

```shell
RUST_LOG=ge_runtime=debug just obs
```

To enable debug logging on an _installed_ build (launched normally, not through `just`), see
[docs/debug-logging.md](docs/debug-logging.md).

## Release-note labels

GitHub release notes are generated from merged PRs and grouped by labels in `.github/release.yml`.
Every PR must have at least one label before merge.

| Release section           | PR labels                 |
| ------------------------- | ------------------------- |
| Breaking Changes          | `breaking-change`         |
| Features                  | `enhancement`             |
| Fixes                     | `bug`, `fix`              |
| Developer Experience      | `repository`, `dev`, `ci` |
| Dependencies              | `dependencies`            |
| Hidden from release notes | `ignore-for-release`      |
| Other Changes             | Any other label           |

Use one main release-note label per PR where possible. If a change needs to appear in separate
sections, split it into separate PRs.

## Creating a release

1. Pick the next commit for release (usually `HEAD` on `master`)
2. Check that the commit already has green CI builds in GitHub
3. Run `just preview-release` to preview the generated release notes (to preview a specific commit,
   run `just preview-release <sha>`)
4. Choose the next version from the previewed changes:
   - `breaking-change`: major bump
   - `enhancement`: minor bump
   - any other labels: patch bump
5. Create and push the release tag:

```shell
git tag vX.Y.Z [sha]
git push --tags
```

Pushing a `vX.Y.Z` tag starts the release workflow, which builds packages and creates the GitHub
release with generated notes and assets. Any release with a hyphen (e.g., `vX.Y.Z-beta`) will
trigger a pre-release version. Generated notes always start from the previous stable `vX.Y.Z`
release, so pre-release tags do not shorten the final stable release notes.
