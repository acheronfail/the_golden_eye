# Auto-update architecture

OBS loads a small resident loader, which in turn loads the replaceable core. Rust, linked into the
core, owns release selection, package verification, data installation, and update policy. The loader
only owns path resolution, dynamic loading, and core replacement—the operations that must survive
unloading Rust. OBS supplies the module data path and plugin lifecycle.

The current updater contract is `u2`, read from `obs2/updater-version.txt`.

## Compatibility and packages

Release packages are named:

```text
the_golden_eye-u<updater>-v<plugin-version>-<platform>-<arch>.zip
```

For example:

```text
the_golden_eye-u1-v0.7.0-windows-x86_64.zip
```

The updater number versions the installation and loader/core contract; it is independent of plugin
SemVer. A package can update automatically only when its updater number exactly matches the running
core. A mismatch is never downloaded; the UI asks for a manual installation instead.

Increment `obs2/updater-version.txt` only when the existing installation cannot safely apply the new
release, such as when changing the resident loader or the ABI below. The loader is absent from
automatic update payloads because OBS has already loaded it.

## Update sequence

1. Rust selects the package matching the release, platform, architecture, and updater number, then
   verifies it against `checksums.txt`.
2. It finds the packaged core and complete data root:

   | Platform | Core                                  | Data root            |
   | -------- | ------------------------------------- | -------------------- |
   | macOS    | `Contents/MacOS/libgolden_core.dylib` | `Contents/Resources` |
   | Linux    | `bin/<arch>/libgolden_core.so`        | `data`               |
   | Windows  | `bin/<arch>/golden_core.dll`          | `data`               |

3. Rust stages the core under its installed filename and the data as `module-data/**` beside it.
4. When monitoring and recording work are idle, Rust wakes the loader reload worker.
5. The loader prechecks the staged core, unloads the old core, and loads the new core through a
   fresh temporary copy to avoid platform loader caching.
6. The new Rust core provisionally swaps OBS's complete module data directory, retaining a backup.
7. The loader replaces the canonical core, calls `ge_core_commit_update()`, and removes staging.

Only 1 core is loaded at any instant. This matters because each core binds the same local HTTP port.

## Module-data behavior

The packaged data root is an authoritative snapshot:

- Arbitrary regular files, empty directories, and future paths such as
  `data/new-runtime-dir/config.json` are included without updater changes.
- Files omitted by a newer package disappear after a successful update.
- Symbolic links and special filesystem entries are rejected.
- Settings and `runs.sqlite` are unaffected because they live in the application config directory,
  outside the OBS module data root.

The incoming copy and backup sit beside OBS's resolved data directory, so staging and data may use
different filesystems, custom paths, spaces, or a custom core filename.

## Failure and rollback

- Failures before reload leave the installation untouched.
- A core that fails precheck or startup is discarded and the canonical core is reopened.
- If replacing the canonical core fails, unloading the new core restores the old data before
  reopening the old core.

## Loader ABI contract

The `u2` loader resolves these C symbols from every core:

```c
typedef void (*ge_request_reload_fn)(void);
typedef enum {
    GE_CORE_COLD_START = 0,
    GE_CORE_APPLY_UPDATE = 1,
    GE_CORE_ROLLBACK = 2,
} ge_core_load_reason;

bool ge_core_load_v2(
    void *module,
    const char *canonical_core_path,
    const char *staged_directory,
    ge_core_load_reason reason,
    ge_request_reload_fn request_reload);
void ge_core_post_load(void);
void ge_core_commit_update(void);
void ge_core_unload(void);
```

Their behavioral contract is:

- `ge_core_load_v2` stores its arguments, starts Rust, and returns `false` unless the core is ready.
  On update, readiness includes provisional module-data installation.
  - `GE_CORE_COLD_START` waits for OBS's frontend startup event.
  - `GE_CORE_APPLY_UPDATE` starts with the frontend ready and installs staged data provisionally.
  - `GE_CORE_ROLLBACK` starts with the frontend ready, without staged-data installation or an update
    notice.
  - `request_reload` must only wake the loader worker and return. It runs on a stack inside the core
    being replaced and must never load, unload, or call back into that core.
- `ge_core_post_load` performs work that must wait for OBS's post-load lifecycle hook.
- `ge_core_commit_update` commits the pending module-data transaction only after the loader has
  replaced the canonical core. Only this commit enables the update notice, for existing and new
  event clients.
- `ge_core_unload` synchronously stops callbacks, Rust tasks, threads, and HTTP before returning.

Every `u2` core must preserve these symbols, signatures, and semantics. A breaking change requires a
new updater number and manual installation.

## Moving from u1 to u2

The u2 contract fixes recovery after a failed core startup. The u1 loader described rollback as a
cold start. The restored runtime then waited for an OBS startup event that had already occurred,
leaving replay state stale.

The new loader calls `ge_core_load_v2` with an explicit load reason. The old `ge_core_load` symbol
is absent. A mismatched loader and core therefore fail symbol checks before startup. The updater
number prevents normal clients from downloading an incompatible package. Existing u1 clients show
the manual-install flow for a u2 release. This change does not migrate settings or the run catalog.

1. Close OBS before installation.
2. Install the full release package, including both the loader and core, with the normal platform
   installation procedure.
3. Preserve the existing plugin settings and run catalog.
4. Restart OBS after installation.

Later u2 releases can update automatically. Release notes must call out this one-time manual
installation.

## Local simulation

```sh
# Compatible: run these in separate terminals.
just simulate-update --updater-version 2
GE_UPDATE_CHECK_URL=http://127.0.0.1:8990/latest just obs

# Incompatible: shows manual installation and makes no package request.
just simulate-update --updater-version 3
```

- `GE_UPDATE_CHECK_URL` overrides the release API
- `GE_UPDATE_INCLUDE_PRERELEASES` includes a check for prereleases (drafts are always ignored)
- `GE_UPDATER_VERSION` overrides the updater number at build time
