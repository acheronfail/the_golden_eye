#ifndef GE_RELOAD_H
#define GE_RELOAD_H

// Core-library lifecycle: open/close the "core" shared lib (Rust staticlib +
// OpenCV + OBS bridge) and hot-swap a staged version. No OBS dependency (so
// shim/tests/ can exercise it standalone); plugin.c owns all OBS coupling.

#include <stdbool.h>
#include <stddef.h>

// Function pointer the core calls (via ge_core_trigger_reload) to ask the shim
// to apply a staged update. Its impl (ge_reload_worker_request) runs on a stack
// still inside the core, so it must ONLY wake the worker -- never dlopen/recurse.
typedef void (*ge_request_reload_fn)(void);

// Where bundled data dirs (cv_templates, locale) sit relative to the core lib:
// macOS bundles keep data in Contents/Resources (core in Contents/MacOS);
// Linux/Windows use a sibling data/ (core in bin/<arch>). See rust/src/lib.rs.
typedef enum {
  GE_INSTALL_LAYOUT_MACOS_BUNDLE,
  GE_INSTALL_LAYOUT_OBS_PLUGIN_DIR,
} ge_install_layout;

// The install layout this shim was built for.
#if defined(__APPLE__)
#define GE_HOST_INSTALL_LAYOUT GE_INSTALL_LAYOUT_MACOS_BUNDLE
#else
#define GE_HOST_INSTALL_LAYOUT GE_INSTALL_LAYOUT_OBS_PLUGIN_DIR
#endif

// On-disk destination for a bundled data dir `leaf` (e.g. "cv_templates") under
// `layout`, relative to the core's `canonical_path`. Exposed for cross-platform
// tests; ge_core_reload passes GE_HOST_INSTALL_LAYOUT. False if it won't fit `out`.
bool ge_core_data_dir_dest(ge_install_layout layout, const char *canonical_path, const char *leaf, char *out,
                           size_t out_size);

// Opaque handle to an open core library: its dynlib handle, the temp-copy
// path it was actually dlopen'd from (removed on close), and its resolved
// entry points.
typedef struct ge_core_handle ge_core_handle;

// Opens `canonical_path` via a fresh temp copy (never the canonical path, so no
// loader hands back a stale image) and calls its ge_core_load(...). On failure
// returns false with a message in err. is_reload=true only for a reload's new core.
bool ge_core_open(const char *load_path, const char *canonical_path, void *module_arg, bool is_reload,
                  ge_request_reload_fn request_reload, ge_core_handle **out_handle, char *err, size_t err_size);

// Calls the handle's ge_core_post_load(). No-op if NULL. Named distinctly from
// the core's own ge_core_post_load() (dlsym'd), since core.c includes this header.
void ge_core_handle_post_load(ge_core_handle *handle);

// Calls the handle's ge_core_unload(), then closes the dynlib handle and
// removes its temp copy. No-op if handle is NULL.
void ge_core_close(ge_core_handle *handle);

// Whether staged_dir currently holds a core ready to swap in. The reload worker
// can be woken with nothing staged -- `just dev` POSTs /updates/apply while the
// dev auto-apply loop also polls, so one staged core can trigger two wakeups; the
// first applies (and deletes) it, leaving the second a no-op. Callers use this to
// tell that benign case apart from a real ge_core_reload failure.
bool ge_core_staged_present(const char *canonical_path, const char *staged_dir);

// Replaces *handle with a staged version from staged_dir; the old core is fully
// closed before the new one opens (never both -- they share a fixed TCP port). On
// failure *handle still points at a running core, canonical_path untouched. Worker stack only.
bool ge_core_reload(ge_core_handle **handle, const char *canonical_path, const char *staged_dir, void *module_arg,
                    ge_request_reload_fn request_reload, char *err, size_t err_size);

// Starts the reload worker thread. `on_request` fires on the worker's own stack
// each time ge_reload_worker_request() wakes it. Returns false on failure.
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
