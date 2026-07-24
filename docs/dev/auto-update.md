# Auto-update flow

The installed OBS module is a small shim plus a separately loaded core library. Auto-update replaces
the core library and bundled runtime data while OBS keeps the shim loaded.

## Release check

1. `updates.rs` runs on the configured interval, or immediately from the manual "check now"
   endpoint.
2. By default it fetches GitHub's `releases/latest` API endpoint, which excludes draft and
   pre-release releases.
3. If `GE_UPDATE_INCLUDE_PRERELEASES` is set and no URL override is set, it fetches GitHub's full
   releases list instead.
4. If `GE_UPDATE_CHECK_URL` is set, that exact URL is fetched regardless of the pre-release setting.
   The response may be either one release object or an array of release objects.
5. The selector always skips drafts. It includes pre-release items only when
   `GE_UPDATE_INCLUDE_PRERELEASES` is truthy, then chooses the highest SemVer release newer than the
   running plugin.
6. The updater selects exactly one canonical package for the current platform and architecture.
   Canonical packages use `the_golden_eye-uN-vX.Y.Z-<platform>-<arch>.zip`, and the package version
   must match the release tag.
7. The package's `uN` value is compared with the updater version compiled into the running core.
   Equal values are compatible; any mismatch requires a manual installation.
8. If a newer release is found, the app snapshot is updated with the release and compatibility
   details for later user-facing notices.
9. If no newer release is found, the update snapshot is cleared and the plugin is considered up to
   date.

## Download and staging

1. If auto-update is enabled in settings, a compatible update starts a background staging task. If
   it is disabled, staging only happens after the user clicks the button in options.
2. An incompatible update is never downloaded or staged, including through the explicit download
   endpoint or the lower-level staging function.
3. It downloads `checksums.txt`, verifies the package zip's SHA-256, and extracts the package into a
   temporary download directory.
4. It interprets the platform-standard package, renames its core to the installed core's exact leaf
   name, and copies the complete packaged module-data root into `module-data/` in the staging
   directory supplied by the shim. Arbitrary regular files, directories, and future nested layouts
   under that data root are included.

## Applying a staged update

1. `auto_apply_when_safe` waits until an update is staged and the app is not in a
   recording-sensitive monitoring state.
2. Rust calls `ge_core_trigger_reload()`, which only wakes the shim reload worker and returns
   immediately.
3. The shim prechecks the staged core by opening it and resolving its entry points without calling
   `ge_core_load()`.
4. The shim then shuts down the running core: `ge_core_unload()` disconnects OBS callbacks/signals
   and calls `ge_rust_stop()`, which blocks until the Tokio runtime and Rust tasks are stopped. Only
   after that does the shim `dlclose` the old core.
5. The shim starts the staged core by calling
   `ge_core_load(canonical_core, staged_directory, is_reload=true, ...)`.
6. Before reporting startup success, the new Rust core resolves OBS's module data directory and
   provisionally replaces the complete directory using destination-local incoming and backup
   directories. It retains the previous data until the core commit finishes.
7. After the new core is running, the shim moves only the staged core binary over the canonical core
   path, tells Rust to commit its pending data transaction, and removes staging.
8. If runtime-data installation, core startup, or canonical replacement fails, Rust restores the
   previous data. The shim discards staging and reopens the unchanged canonical core.

## Manual installation boundary

The updater version describes the package/install contract, independently of the plugin's SemVer.
Increment it only when the installed updater cannot safely apply the new package, such as a change
to the resident shim.

When a release's updater version differs from the running core:

- Auto-update remains saved as the user's preference but is temporarily disabled in Options.
- The app shows a manual-install dialog when monitoring is inactive and keeps a sticky notice.
- The download/apply action is replaced with a link to the GitHub release.
- The user must close OBS and install the package normally. Settings and run history are retained.

The checked-in updater version is `obs2/updater-version.txt`. `GE_UPDATER_VERSION` can override it
for builds and local simulations. Release packages and the compiled Rust core always receive the
same resolved value.

The `v0.6.1` bridge release is the only release that publishes both canonical `u0-v0.6.1` packages
and legacy `0.6.1` aliases. The aliases allow clients without updater-version support to reach the
bridge. `v0.7.0` introduces the path-safe `u1` updater contract. Later releases publish canonical
packages only.

## Local simulation

Run the simulator in one terminal, then launch OBS with the printed `GE_UPDATE_CHECK_URL` in
another:

```sh
# Compatible with the checked-in u1 build: should download, stage, and apply.
just simulate-update --updater-version 1

# Incompatible with u1: should show manual-install UI and make no package request.
just simulate-update --updater-version 2
GE_UPDATER_VERSION=1 just obs
```

The simulator resolves its updater version from `--updater-version`, then `GE_UPDATER_VERSION`, then
the checked-in file. It serves canonical packages only; the legacy alias was limited to the
published `v0.6.1` bridge.

## What the shim updates

- The shim replaces only the hosted core library, not the shim library that OBS originally loaded.
- Rust owns package interpretation and transactionally installs runtime data through the data path
  resolved by OBS. The shim has no knowledge of data filenames or packaged install layouts.
- The full package data root is authoritative. New paths such as `data/new-runtime-dir/**` require
  no updater change, and paths removed from a package are removed from the installed data snapshot
  after a successful update.
- Symbolic links and non-regular filesystem entries in module data are rejected. Files outside the
  platform package's data root and native libraries beside the core are not auto-installed.
- The canonical core and its adjacent staging directory may be unrelated to the shim and OBS data
  directories; paths containing spaces and custom core filenames are supported.
- The shim stays resident for the whole OBS session. Changes to shim code require a normal reinstall
  and OBS restart.

## Environment variables

- `GE_UPDATE_CHECK_URL`: exact release API URL override, mainly for tests or local mock servers. It
  takes precedence over the default stable/full-list URL selection and may return one release object
  or an array.
- `GE_UPDATE_INCLUDE_PRERELEASES`: when set, allows pre-release versions from the fetched response.
  Without `GE_UPDATE_CHECK_URL`, it also switches the default GitHub endpoint from `releases/latest`
  to the full releases list. Leave unset for stable-only behavior.
- `GE_UPDATER_VERSION`: build-time override for the non-negative integer in
  `obs2/updater-version.txt`. It changes both the compiled compatibility value and package name.
