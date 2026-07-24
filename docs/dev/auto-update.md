# Auto-update architecture

OBS loads a small resident shim, which in turn loads the replaceable core. Rust, linked into the
core, owns release selection, package verification, data installation, and update policy. The shim
only owns path resolution, dynamic loading, and core replacement—the operations that must survive
unloading Rust. OBS supplies the module data path and plugin lifecycle.

The current updater contract is `u1`, read from `obs2/updater-version.txt`.

## Compatibility and packages

Release packages are named:

```text
the_golden_eye-u<updater>-v<plugin-version>-<platform>-<arch>.zip
```

For example:

```text
the_golden_eye-u1-v0.7.0-windows-x86_64.zip
```

The updater number versions the installation and shim/core contract; it is independent of plugin
SemVer. A package can update automatically only when its updater number exactly matches the running
core. A mismatch is never downloaded; the UI asks for a manual installation instead.

Increment `obs2/updater-version.txt` only when the existing installation cannot safely apply the new
release, such as when changing the resident shim or the ABI below. The shim is absent from automatic
update payloads because OBS has already loaded it.

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
4. When monitoring and recording work are idle, Rust wakes the shim reload worker.
5. The shim prechecks the staged core, unloads the old core, and loads the new core through a fresh
   temporary copy to avoid platform loader caching.
6. The new Rust core provisionally swaps OBS's complete module data directory, retaining a backup.
7. The shim replaces the canonical core, calls `ge_core_commit_update()`, and removes staging.

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

## Shim ABI contract

The `u1` shim resolves these C symbols from every core:

```c
typedef void (*ge_request_reload_fn)(void);

bool ge_core_load(
    void *module,
    const char *canonical_core_path,
    const char *staged_directory,
    bool is_reload,
    ge_request_reload_fn request_reload);
void ge_core_post_load(void);
void ge_core_commit_update(void);
void ge_core_unload(void);
```

Their behavioral contract is:

- `ge_core_load` stores its arguments, starts Rust, and returns `false` unless the core is ready. On
  reload, readiness includes provisional module-data installation.
  - `request_reload` must only wake the shim worker and return. It runs on a stack inside the core
    being replaced and must never load, unload, or call back into that core.
- `ge_core_post_load` performs work that must wait for OBS's post-load lifecycle hook.
- `ge_core_commit_update` commits the pending module-data transaction only after the shim has
  replaced the canonical core.
- `ge_core_unload` synchronously stops callbacks, Rust tasks, threads, and HTTP before returning.

Every `u1` core must preserve these symbols, signatures, and semantics. A breaking change requires a
new updater number and manual installation.

## Local simulation

```sh
# Compatible: run these in separate terminals.
just simulate-update --updater-version 1
GE_UPDATE_CHECK_URL=http://127.0.0.1:31339/latest just obs

# Incompatible: shows manual installation and makes no package request.
just simulate-update --updater-version 2
```

- `GE_UPDATE_CHECK_URL` overrides the release API
- `GE_UPDATE_INCLUDE_PRERELEASES` includes a check for prereleases (drafts are always ignored)
- `GE_UPDATER_VERSION` overrides the updater number at build time
