# Contributing

## Project overview

- `obs2/plugin.c` is the OBS-loaded shim that finds and loads the bundled core library.
- `obs2/core.c` and `obs2/obs_bridge.c` connect OBS frontend events, source frames, and replay-buffer callbacks to Rust.
- `obs2/rust/` contains the Rust core for frame matching, recording coordination, settings, stream notifications, and HTTP routes.
- `obs2/browser/` is the SvelteKit browser dock UI embedded into the plugin build.
- `obs2/cv_templates/` contains the image templates used by the level and time matcher.
- `test/` contains the Node-based frame regression harness for the matcher CLI.
- `esp32-input-monitor/` is independent PlatformIO firmware for monitoring N64 controller input.

## Development

System dependencies:

- Common:
  - `rustup`, `nodejs` (version from `.nvmrc`), `just`, `wget`
- Linux:
  - Debian: `libdbus-1-dev libssl-dev nasm pkg-config`
  - Arch Linux: `cmake nasm pkg-config`
  - This project targets the Flatpak release of OBS Studio, so you need Flatpak installed, plus:
    - OBS: `flatpak install com.obsproject.Studio`
    - SDK: `flatpak install $(flatpak info --show-sdk com.obsproject.Studio)`
- macOS:
  - `xcode-select --install`
  - `brew install just cmake wget simde nasm`
- Windows:
  - OBS Studio
  - Visual Studio Build Tools with MSVC
  - `vcpkg`, with `VCPKG_ROOT` or `VCPKG_INSTALLATION_ROOT` set
  - `just`, `cmake`, and Git Bash

Get started:

```shell
# run once after cloning this repository to install dependencies and set up the environment:
just setup

# build and run OBS with the native plugin
# (on linux this builds inside the OBS Flatpak SDK and runs the Flatpak OBS):
just obs

# development mode with browser hot reload and plugin core hot reload:
just dev

# release-build the plugin and run frame regression tests:
just test
```

Format changes before submitting:

```shell
just fmt
```

## Release-note labels

GitHub release notes are generated from merged PRs and grouped by labels in `.github/release.yml`.
Every PR must have at least one label before merge.

| Release section           | PR labels                                                     |
| ------------------------- | ------------------------------------------------------------- |
| Breaking Changes          | `breaking-change`                                             |
| Features                  | `enhancement`                                                 |
| Fixes                     | `bug`, `fix`                                                  |
| Developer Experience      | `repository`, `dev`, `ci`                                     |
| Dependencies              | `dependencies`                                                |
| Hidden from release notes | `ignore-for-release`                                          |
| Other Changes             | Any other label                                               |

Use one main release-note label per PR where possible. If a change needs to appear in separate sections, split it into separate PRs.
