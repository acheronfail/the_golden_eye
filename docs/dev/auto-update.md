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
6. If a newer release is found, the app snapshot is updated and the release version/URL are
   persisted for later user-facing notices.
7. If no newer release is found, the update snapshot is cleared and the plugin is considered up to
   date.

## Download and staging

1. If auto-update is enabled in settings, a successful check starts a background staging task. If it
   is disabled, staging only happens after the user clicks the button in options.
2. `update_apply.rs` chooses the package asset for the current platform and architecture.
3. It downloads `checksums.txt`, verifies the package zip's SHA-256, and extracts the package into a
   temporary download directory.
4. It copies the extracted update payload into `.ge_update_staged` next to the installed core
   library.

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
5. The shim starts the staged core from `.ge_update_staged` by calling
   `ge_core_load(..., is_reload=true, ...)`. The new core sets its canonical path, marks itself as
   reloaded, starts Rust, reconnects OBS signals, and refreshes sources.
6. After the new core is running, the shim syncs the staged core binary over the canonical on-disk
   core path, best-effort syncs bundled data directories such as `cv_templates` and `locale`, and
   removes `.ge_update_staged`.
7. If the staged core cannot load, the shim leaves the canonical files untouched and reopens the
   previous canonical core as a rollback.

## What the shim updates

- The shim replaces the hosted core library, not the shim library that OBS originally loaded.
- The shim can also sync bundled data directories that `reload.c` explicitly knows about, currently
  `cv_templates` and `locale`.
- The shim stays resident for the whole OBS session. Changes to shim code require a normal reinstall
  and OBS restart.

## Environment variables

- `GE_UPDATE_CHECK_URL`: exact release API URL override, mainly for tests or local mock servers. It
  takes precedence over the default stable/full-list URL selection and may return one release object
  or an array.
- `GE_UPDATE_INCLUDE_PRERELEASES`: when set, allows pre-release versions from the fetched response.
  Without `GE_UPDATE_CHECK_URL`, it also switches the default GitHub endpoint from `releases/latest`
  to the full releases list. Leave unset for stable-only behavior.
