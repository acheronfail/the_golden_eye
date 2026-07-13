#include "reload.h"

#include "dynlib.h"
#include "reload_platform.h"

#include <errno.h>
#include <stdarg.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef bool (*ge_core_load_fn)(void *module_arg, const char *canonical_path, bool is_reload,
                                ge_request_reload_fn request_reload);
typedef void (*ge_core_post_load_fn)(void);
typedef void (*ge_core_unload_fn)(void);

struct ge_core_handle {
  ge_dynlib_handle dl;
  char temp_copy_path[PATH_MAX];
  ge_core_load_fn load;
  ge_core_post_load_fn post_load;
  ge_core_unload_fn unload;
};

static void set_err(char *err, size_t err_size, const char *fmt, ...) {
  if (!err || err_size == 0) {
    return;
  }
  va_list args;
  va_start(args, fmt);
  vsnprintf(err, err_size, fmt, args);
  va_end(args);
}

/* ---------------------------------------------------------------------- */
/* Small path helpers (kept local to this file -- plugin.c has its own set */
/* for OBS-relative resolution, an unrelated concern).                    */
/* ---------------------------------------------------------------------- */

/* Checks both separators unconditionally rather than special-casing by
 * platform: Windows paths may use forward slashes, and these are always our own
 * install/staging paths, so one shared check is simpler than two. */
static const char *leaf_name(const char *path) {
  const char *slash = strrchr(path, '/');
  const char *backslash = strrchr(path, '\\');
  if (!slash || (backslash && backslash > slash)) {
    slash = backslash;
  }
  return slash ? slash + 1 : path;
}

static bool join_path(char *out, size_t out_size, const char *dir, const char *leaf) {
  size_t dir_len = strlen(dir);
  bool has_sep = dir_len > 0 && (dir[dir_len - 1] == '/' || dir[dir_len - 1] == '\\');
  return (size_t)snprintf(out, out_size, "%s%s%s", dir, has_sep ? "" : "/", leaf) < out_size;
}

/* Directory containing `path` ("." if path has no separator). */
static bool dirname_of(const char *path, char *out, size_t out_size) {
  const char *leaf = leaf_name(path);
  size_t dir_len = (leaf == path) ? 0 : (size_t)(leaf - path - 1);
  if (dir_len == 0) {
    return (size_t)snprintf(out, out_size, ".") < out_size;
  }
  if (dir_len >= out_size) {
    return false;
  }
  memcpy(out, path, dir_len);
  out[dir_len] = '\0';
  return true;
}

/* ---------------------------------------------------------------------- */
/* Temp-copy-then-open: never dlopen the canonical path directly. A fresh, */
/* never-before-used path can never be a stale cached image, on any       */
/* platform -- sidesteps loader path-caching uniformly instead of relying */
/* on platform-specific unload-timing guarantees.                        */
/* ---------------------------------------------------------------------- */

static bool copy_file(const char *src, const char *dst, char *err, size_t err_size) {
  FILE *in = fopen(src, "rb");
  if (!in) {
    set_err(err, err_size, "failed to open '%s' for reading: %s", src, strerror(errno));
    return false;
  }

  FILE *out = fopen(dst, "wb");
  if (!out) {
    set_err(err, err_size, "failed to open '%s' for writing: %s", dst, strerror(errno));
    fclose(in);
    return false;
  }

  char buf[64 * 1024];
  size_t n;
  bool ok = true;
  while (ok && (n = fread(buf, 1, sizeof(buf), in)) > 0) {
    ok = fwrite(buf, 1, n, out) == n;
  }
  ok = ok && !ferror(in);

  fclose(in);
  if (fclose(out) != 0) {
    ok = false;
  }

  if (!ok) {
    set_err(err, err_size, "failed to copy '%s' to '%s': %s", src, dst, strerror(errno));
    remove(dst);
  }
  return ok;
}

static ge_core_handle *open_handle(const char *canonical_path, char *err, size_t err_size) {
  char temp_path[PATH_MAX];
  if (!ge_platform_make_temp_path(temp_path, sizeof(temp_path))) {
    set_err(err, err_size, "failed to create a temp path for the core library");
    return NULL;
  }

  if (!copy_file(canonical_path, temp_path, err, err_size)) {
    remove(temp_path);
    return NULL;
  }

  ge_dynlib_handle dl = ge_dynlib_open(temp_path);
  if (!dl) {
    set_err(err, err_size, "failed to load '%s': %s", temp_path, ge_dynlib_error());
    remove(temp_path);
    return NULL;
  }

  ge_core_load_fn load = (ge_core_load_fn)ge_dynlib_symbol(dl, "ge_core_load");
  ge_core_post_load_fn post_load = (ge_core_post_load_fn)ge_dynlib_symbol(dl, "ge_core_post_load");
  ge_core_unload_fn unload = (ge_core_unload_fn)ge_dynlib_symbol(dl, "ge_core_unload");
  if (!load || !post_load || !unload) {
    set_err(err, err_size, "core entry points missing from '%s'", canonical_path);
    ge_dynlib_close(dl);
    remove(temp_path);
    return NULL;
  }

  ge_core_handle *handle = calloc(1, sizeof(*handle));
  if (!handle) {
    set_err(err, err_size, "out of memory opening core handle");
    ge_dynlib_close(dl);
    remove(temp_path);
    return NULL;
  }

  handle->dl = dl;
  handle->load = load;
  handle->post_load = post_load;
  handle->unload = unload;
  snprintf(handle->temp_copy_path, sizeof(handle->temp_copy_path), "%s", temp_path);
  return handle;
}

static void free_handle(ge_core_handle *handle) {
  if (!handle) {
    return;
  }
  ge_dynlib_close(handle->dl);
  if (handle->temp_copy_path[0]) {
    remove(handle->temp_copy_path);
  }
  free(handle);
}

bool ge_core_open(const char *load_path, const char *canonical_path, void *module_arg, bool is_reload,
                  ge_request_reload_fn request_reload, ge_core_handle **out_handle, char *err, size_t err_size) {
  ge_core_handle *handle = open_handle(load_path, err, err_size);
  if (!handle) {
    return false;
  }

  // The core is dlopen'd from load_path (the staged copy on a reload) but is told
  // its canonical_path -- where the file lives after the swap below -- so its own
  // staged-update lookups resolve relative to the durable install dir, not a
  // transient staged dir that ge_core_reload deletes on success.
  if (!handle->load(module_arg, canonical_path, is_reload, request_reload)) {
    set_err(err, err_size, "ge_core_load() returned false for '%s'", canonical_path);
    free_handle(handle);
    return false;
  }

  *out_handle = handle;
  return true;
}

void ge_core_handle_post_load(ge_core_handle *handle) {
  if (handle) {
    handle->post_load();
  }
}

void ge_core_close(ge_core_handle *handle) {
  if (!handle) {
    return;
  }
  handle->unload();
  free_handle(handle);
}

/* ---------------------------------------------------------------------- */
/* Syncing the canonical on-disk files from a staged directory, so a      */
/* future cold start also picks up the new version.                       */
/* ---------------------------------------------------------------------- */

/* Best-effort: swaps `canonical_dir` for staged_dir/leaf if present. Directories
 * can't be atomically renamed, so the old is moved aside, the new into place, old
 * deleted -- a brief gap, fine for bundled data (cv_templates/locale). Never fails reload. */
static void sync_data_dir_best_effort(const char *staged_dir, const char *leaf, const char *canonical_dir, char *err,
                                      size_t err_size) {
  char staged_leaf[PATH_MAX];
  if (!join_path(staged_leaf, sizeof(staged_leaf), staged_dir, leaf) || !ge_platform_dir_exists(staged_leaf)) {
    return;
  }

  char old_aside[PATH_MAX];
  bool had_old = ge_platform_dir_exists(canonical_dir);
  if (had_old) {
    if ((size_t)snprintf(old_aside, sizeof(old_aside), "%s.updating", canonical_dir) >= sizeof(old_aside) ||
        rename(canonical_dir, old_aside) != 0) {
      set_err(err, err_size, "failed to move aside old '%s': %s", canonical_dir, strerror(errno));
      return;
    }
  }

  if (rename(staged_leaf, canonical_dir) != 0) {
    set_err(err, err_size, "failed to move staged '%s' into place: %s", staged_leaf, strerror(errno));
    if (had_old) {
      rename(old_aside, canonical_dir);
    }
    return;
  }

  if (had_old) {
    ge_platform_remove_dir_recursive(old_aside);
  }
}

/* ---------------------------------------------------------------------- */
/* The reload sequence itself.                                            */
/* ---------------------------------------------------------------------- */

bool ge_core_staged_present(const char *canonical_path, const char *staged_dir) {
  char staged_lib[PATH_MAX];
  if (!join_path(staged_lib, sizeof(staged_lib), staged_dir, leaf_name(canonical_path))) {
    return false;
  }
  FILE *probe = fopen(staged_lib, "rb");
  if (!probe) {
    return false;
  }
  fclose(probe);
  return true;
}

bool ge_core_reload(ge_core_handle **handle, const char *canonical_path, const char *staged_dir, void *module_arg,
                    ge_request_reload_fn request_reload, char *err, size_t err_size) {
  char staged_lib[PATH_MAX];
  if (!join_path(staged_lib, sizeof(staged_lib), staged_dir, leaf_name(canonical_path))) {
    set_err(err, err_size, "staged core path too long");
    return false;
  }

  FILE *probe = fopen(staged_lib, "rb");
  if (!probe) {
    set_err(err, err_size, "no staged core found at '%s'", staged_lib);
    return false;
  }
  fclose(probe);

  /* Precheck: confirm the staged binary at least loads and resolves its
   * entry points, without calling ge_core_load, while the old core is still
   * fully alive and untouched. */
  char precheck_err[256];
  ge_core_handle *precheck = open_handle(staged_lib, precheck_err, sizeof(precheck_err));
  if (!precheck) {
    set_err(err, err_size, "staged core failed precheck: %s", precheck_err);
    return false;
  }
  free_handle(precheck);

  /* Sequential swap: the core binds a fixed TCP port, so old and new must
   * never both be running at once. Close old fully before opening new. */
  ge_core_handle *old = *handle;
  ge_core_close(old);
  *handle = NULL;

  ge_core_handle *fresh = NULL;
  char open_err[256];
  if (ge_core_open(staged_lib, canonical_path, module_arg, /*is_reload=*/true, request_reload, &fresh, open_err,
                   sizeof(open_err))) {
    *handle = fresh;

    char sync_err[256] = {0};
    if (!ge_platform_replace_file(staged_lib, canonical_path, sync_err, sizeof(sync_err))) {
      set_err(err, err_size, "reload succeeded but canonical sync failed: %s", sync_err);
      /* The running core is already the new one; only the on-disk sync for
       * a future cold start failed. Don't tear anything down for this. */
    }

    char data_err[256] = {0};
    char canonical_dir[PATH_MAX];
    if (dirname_of(canonical_path, canonical_dir, sizeof(canonical_dir))) {
      char templates_dir[PATH_MAX];
      if (join_path(templates_dir, sizeof(templates_dir), canonical_dir, "cv_templates")) {
        sync_data_dir_best_effort(staged_dir, "cv_templates", templates_dir, data_err, sizeof(data_err));
      }
      char locale_dir[PATH_MAX];
      if (join_path(locale_dir, sizeof(locale_dir), canonical_dir, "locale")) {
        sync_data_dir_best_effort(staged_dir, "locale", locale_dir, data_err, sizeof(data_err));
      }
    }

    ge_platform_remove_dir_recursive(staged_dir);
    return true;
  }

  /* New failed to come up. Canonical is untouched (sync only happens on
   * success, above) -- roll back by relaunching the original. Not itself an
   * applied update (it's a revert), so is_reload is false here. */
  char rollback_err[256];
  if (!ge_core_open(canonical_path, canonical_path, module_arg, /*is_reload=*/false, request_reload, handle,
                    rollback_err, sizeof(rollback_err))) {
    set_err(err, err_size, "staged core failed to load (%s); rollback also failed (%s)", open_err, rollback_err);
    return false;
  }
  set_err(err, err_size, "staged core failed to load (%s); rolled back to the running version", open_err);
  return false;
}

/* ---------------------------------------------------------------------- */
/* Reload worker thread.                                                  */
/* ---------------------------------------------------------------------- */

static ge_cond_lock g_worker_lock;
static ge_platform_thread g_worker_thread;
static bool g_worker_running = false;
static bool g_worker_pending = false;
static void (*g_worker_on_request)(void) = NULL;

static bool worker_run_once(void) {
  ge_cond_lock_acquire(&g_worker_lock);
  while (g_worker_running && !g_worker_pending) {
    ge_cond_lock_wait(&g_worker_lock);
  }
  bool pending = g_worker_pending;
  bool should_continue = g_worker_running || pending;
  g_worker_pending = false;
  ge_cond_lock_release(&g_worker_lock);

  if (pending && g_worker_on_request) {
    g_worker_on_request();
  }
  return should_continue;
}

static void *worker_thread_proc(void *arg) {
  (void)arg;
  while (worker_run_once()) {
  }
  return NULL;
}

bool ge_reload_worker_start(void (*on_request)(void)) {
  g_worker_on_request = on_request;
  g_worker_pending = false;
  g_worker_running = true;
  ge_cond_lock_init(&g_worker_lock);

  if (!ge_platform_thread_spawn(&g_worker_thread, worker_thread_proc, NULL)) {
    g_worker_running = false;
    ge_cond_lock_destroy(&g_worker_lock);
    return false;
  }
  return true;
}

void ge_reload_worker_stop(void) {
  if (!g_worker_running) {
    return;
  }

  ge_cond_lock_acquire(&g_worker_lock);
  g_worker_running = false;
  ge_cond_lock_signal(&g_worker_lock);
  ge_cond_lock_release(&g_worker_lock);

  ge_platform_thread_join(g_worker_thread);
  ge_cond_lock_destroy(&g_worker_lock);
}

void ge_reload_worker_request(void) {
  ge_cond_lock_acquire(&g_worker_lock);
  g_worker_pending = true;
  ge_cond_lock_signal(&g_worker_lock);
  ge_cond_lock_release(&g_worker_lock);
}
