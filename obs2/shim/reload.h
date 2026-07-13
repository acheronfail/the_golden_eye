#ifndef GE_RELOAD_H
#define GE_RELOAD_H

// Core-library lifecycle: opening/closing the "core" shared library (the Rust
// staticlib + OpenCV + OBS bridge) and swapping it for a freshly staged
// version while the host process keeps running. Deliberately has NO
// dependency on OBS headers, so it can be exercised directly by a small
// standalone test binary (see shim/tests/) against fixture libraries.
//
// plugin.c owns everything OBS-coupled (obs_module_load/post_load/unload,
// resolving paths relative to the loaded shim, the duplicate-module check)
// and calls into this file for the actual dlopen/rename/rollback mechanics.

#include <stdbool.h>
#include <stddef.h>

// Function pointer the core calls (via ge_core_trigger_reload, exported by
// core.c) to ask the shim to check for and apply a staged update. Passed
// into ge_core_open/ge_core_reload so it can be handed to the core's
// ge_core_load(). Its implementation (ge_reload_worker_request, below) must
// do nothing but wake the reload worker thread: it runs on a call stack that
// is still inside the core being asked to reload, so it must never itself
// touch a dlopen handle or call back into the core.
typedef void (*ge_request_reload_fn)(void);

// Opaque handle to an open core library: its dynlib handle, the temp-copy
// path it was actually dlopen'd from (removed on close), and its resolved
// entry points.
typedef struct ge_core_handle ge_core_handle;

// Opens `canonical_path` -- via a fresh, uniquely-named temp copy, never the
// canonical path directly, so a stale cached image can never be handed back
// by the loader on any platform -- and calls its ge_core_load(module_arg,
// canonical_path, is_reload, request_reload). On success, *out_handle owns
// the loaded library and the caller must eventually release it with
// ge_core_close(). On failure, returns false with a message written to err
// (best-effort truncated to err_size) and *out_handle is untouched.
//
// `is_reload` is forwarded to the core as-is, so it can tell a genuine
// update apply (this session already had a running core, now replaced)
// apart from a cold OBS start -- e.g. to show a "plugin updated" notice.
// Pass false for the initial open in obs_module_load; ge_core_reload passes
// true for the new core it opens (but false when rolling back to the
// original after a failed swap -- that's a revert, not an applied update).
bool ge_core_open(const char *canonical_path, void *module_arg, bool is_reload, ge_request_reload_fn request_reload,
                  ge_core_handle **out_handle, char *err, size_t err_size);

// Calls the handle's ge_core_post_load(). No-op if handle is NULL. Named
// distinctly from the core's own exported ge_core_post_load() -- this file
// is included by core.c (for the ge_request_reload_fn typedef), and that
// symbol name is already taken by core.c's real, dlsym'd entry point.
void ge_core_handle_post_load(ge_core_handle *handle);

// Calls the handle's ge_core_unload(), then closes the dynlib handle and
// removes its temp copy. No-op if handle is NULL.
void ge_core_close(ge_core_handle *handle);

// Attempts to replace *handle with a freshly staged version found under
// staged_dir (a file with the same leaf name as canonical_path). Sequencing
// is strict -- the old handle is only ever closed after a cheap precheck of
// the staged binary succeeds, and the new handle is only opened after the
// old one has been fully closed (ge_core_unload + dlclose/FreeLibrary),
// never both open at once: the core binds a fixed TCP port that only one
// live instance may hold.
//
// On success: *handle points at the newly opened core, the canonical path
// (and, best-effort, sibling cv_templates/locale directories) has been
// synced from staged_dir, and staged_dir has been removed. Returns true.
//
// On failure: *handle is guaranteed to still point at a *running* core --
// the original one, reopened, unless reopening it also fails (logged via
// err; the plugin is then down until OBS restarts, having never been in a
// state worse than "reload failed"). The canonical path is never touched
// unless the new core successfully loaded.
//
// Must only be called from the reload worker thread's own stack -- never
// from request_reload itself or any callback invoked from inside the core.
bool ge_core_reload(ge_core_handle **handle, const char *canonical_path, const char *staged_dir, void *module_arg,
                    ge_request_reload_fn request_reload, char *err, size_t err_size);

// Starts the dedicated reload worker thread. `on_request` fires on the
// worker's own stack (never on the caller's) each time
// ge_reload_worker_request() wakes it. Returns false if the thread could not
// be started.
bool ge_reload_worker_start(void (*on_request)(void));

// Signals the worker to stop and joins it, waiting for any in-flight
// on_request() call to finish first. Safe to call even if the worker was
// never started.
void ge_reload_worker_stop(void);

// Wakes the reload worker thread. Safe to call from ANY stack, including one
// currently executing inside the core library -- must never do anything but
// signal and return immediately.
void ge_reload_worker_request(void);

#endif /* GE_RELOAD_H */
