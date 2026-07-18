# Contributing

## Project overview

- `obs2/shim/` is the OBS-loaded shim that finds and loads the bundled core library, also performs
  auto-update.
- `obs2/core/` connects OBS (frontend events, source frames, and replay-buffer callbacks) to Rust.
- `obs2/rust/` the main plugin - recording, frame matching, the webserver, etc.
- `obs2/browser/` is the SvelteKit browser dock UI embedded into the plugin build.
- `obs2/cv_templates/` contains the image templates used by the level and time matcher.
- `test/` contains the Node-based frame regression harness for the matcher CLI.

## Development

System dependencies:

- Common:
  - `rustup`
  - `nodejs` (version from `.nvmrc`)
  - `just`
  - `python3` (optional, for `just dev`)
- Linux:
  - Debian: `libdbus-1-dev libssl-dev just nasm pkg-config`
  - Arch Linux: `cmake just nasm pkg-config`
  - This project targets the Flatpak release of OBS Studio, so you need Flatpak installed, plus:
    - OBS: `flatpak install com.obsproject.Studio`
    - SDK: `flatpak install $(flatpak info --show-sdk com.obsproject.Studio)`
- macOS:
  - `xcode-select --install`
  - `brew install just cmake wget simde nasm`
- Windows:
  - OBS Studio
  - Visual Studio Build Tools with MSVC
  - `vcpkg` (with `VCPKG_ROOT` set)
  - `just`, `cygwin`, `cmake`, `llvm`, `python` and `nodejs` (easily installed via `scoop`)
  - Git needs to be setup:
    - for CRLFs: `git config core.autocrlf input`
    - Also make sure to install `git` via Cygwin Setup, so it knows all the cygwin tools

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

## Logging

The Rust core logs through OBS's own logging facility, so its lines land in the OBS log alongside
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
the crate name is `ge_rust`). Release builds default to `info`, so `debug`-level lines are hidden.
To show them, launch OBS with `RUST_LOG` set:

```shell
RUST_LOG=ge_rust=debug just obs
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
