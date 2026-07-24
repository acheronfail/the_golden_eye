// Thin shim: the library OBS actually loads. No real logic beyond OBS-module
// lifecycle and path resolution -- it dlopens the "core" (Rust staticlib + OBS
// bridge + OpenCV) via reload.c, so the core can be hot-swapped while OBS runs.

#ifndef _WIN32
#define _GNU_SOURCE
#endif

#include "dynlib.h"
#include "reload.h"
#include "version.h"

#include <obs/libobs/obs-module.h>
#include <obs/libobs/util/base.h>

#include <limits.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

OBS_DECLARE_MODULE()

#define GE_LOG(level, fmt, ...) blog(level, "[the_golden_eye] " fmt, ##__VA_ARGS__)

#ifndef PATH_MAX
#define PATH_MAX 4096
#endif

#ifndef GE_CORE_LIB_NAME
#error "GE_CORE_LIB_NAME must be defined by the build"
#endif

// Sibling directory, next to the core library, where a verified update is
// staged before being applied. Rust (update_apply.rs) and this shim agree on
// this path purely by convention -- neither depends on the other's code.
#define GE_STAGED_UPDATE_DIR_NAME ".ge_update_staged"

static ge_core_handle *g_handle = NULL;
static char g_canonical_path[PATH_MAX];
static char g_staged_dir[PATH_MAX];

static bool copy_path(char *out, size_t out_size, const char *path) {
  if (snprintf(out, out_size, "%s", path) >= (int)out_size) {
    GE_LOG(LOG_WARNING, "path too long: %s", path);
    return false;
  }
  return true;
}

static bool copy_dirname(const char *path, char *out, size_t out_size) {
  const char *slash = strrchr(path, '/');
#ifdef _WIN32
  const char *backslash = strrchr(path, '\\');
  if (!slash || (backslash && backslash > slash)) {
    slash = backslash;
  }
#endif
  if (!slash) {
    if (snprintf(out, out_size, ".") >= (int)out_size) {
      GE_LOG(LOG_ERROR, "module directory path too long");
      return false;
    }
    return true;
  }

  size_t len = (size_t)(slash - path);
  if (len == 0) {
    len = 1;
  }
  if (len >= out_size) {
    GE_LOG(LOG_ERROR, "module directory path too long");
    return false;
  }

  memcpy(out, path, len);
  out[len] = '\0';
  return true;
}

static bool module_path(char *out, size_t out_size) {
  if (!ge_module_path(out, out_size)) {
    GE_LOG(LOG_ERROR, "failed to resolve plugin module path");
    return false;
  }
  return true;
}

static bool module_dir(char *out, size_t out_size) {
  char path[PATH_MAX];
  if (!module_path(path, sizeof(path))) {
    return false;
  }
  return copy_dirname(path, out, out_size);
}

static bool join_path(char *out, size_t out_size, const char *dir, const char *leaf) {
  size_t dir_len = strlen(dir);
  bool has_sep = dir_len > 0 && (dir[dir_len - 1] == '/' || dir[dir_len - 1] == '\\');
  char sep_buf[2] = {GE_PATH_SEP, '\0'};
  const char *sep = has_sep ? "" : sep_buf;
  if (snprintf(out, out_size, "%s%s%s", dir, sep, leaf) >= (int)out_size) {
    GE_LOG(LOG_ERROR, "path too long");
    return false;
  }
  return true;
}

static bool bundled_path(const char *relative_path, char *out, size_t out_size) {
  char dir[PATH_MAX];
  return module_dir(dir, sizeof(dir)) && join_path(out, out_size, dir, relative_path);
}

struct duplicate_module_check {
  obs_module_t *current;
  const char *current_file;
  size_t matches;
  char other_path[PATH_MAX];
};

static void count_duplicate_module(void *param, obs_module_t *module) {
  struct duplicate_module_check *check = param;
  const char *file = obs_get_module_file_name(module);

  if (!file || strcmp(file, check->current_file) != 0) {
    return;
  }

  check->matches++;
  if (module != check->current && !check->other_path[0]) {
    const char *path = obs_get_module_binary_path(module);
    copy_path(check->other_path, sizeof(check->other_path), path ? path : file);
  }
}

static bool ge_check_duplicate_obs_module(void) {
  obs_module_t *current = obs_current_module();
  const char *current_file = obs_get_module_file_name(current);
  const char *current_path = obs_get_module_binary_path(current);
  struct duplicate_module_check check = {
      .current = current,
      .current_file = current_file,
  };

  if (!current || !current_file) {
    GE_LOG(LOG_WARNING, "could not inspect OBS module registry for duplicate plugin copies");
    return true;
  }

  obs_enum_modules(count_duplicate_module, &check);
  if (check.matches <= 1) {
    return true;
  }

  GE_LOG(LOG_ERROR, "found multiple loaded copies of The Golden Eye OBS plugin; disabling this copy");
  GE_LOG(LOG_ERROR, "current copy: %s", current_path ? current_path : current_file);
  if (check.other_path[0]) {
    GE_LOG(LOG_ERROR, "already loaded copy: %s", check.other_path);
  }
  return false;
}

static bool resolve_canonical_core_path(char *out, size_t out_size) {
  const char *path = getenv("GE_CORE_LIB");
  if (path && *path) {
    return copy_path(out, out_size, path);
  }
  return bundled_path(GE_CORE_LIB_NAME, out, out_size);
}

// Invoked on the reload worker thread's own stack (see reload.h) -- never on
// a stack that's inside the core itself.
static void handle_reload_request(void) {
  if (!ge_core_staged_present(g_canonical_path, g_staged_dir)) {
    GE_LOG(LOG_WARNING, "reload requested but nothing staged");
    return;
  }

  char err[256];
  if (!ge_core_reload(&g_handle, g_canonical_path, g_staged_dir, obs_current_module(), ge_reload_worker_request, err,
                      sizeof(err))) {
    GE_LOG(LOG_ERROR, "core reload failed: %s", err);
    return;
  }
  GE_LOG(LOG_INFO, "core reload succeeded");
}

bool obs_module_load(void) {
  GE_LOG(LOG_INFO, "shim v%s loading", ge_shim_version());

  if (!ge_check_duplicate_obs_module()) {
    return false;
  }

  if (!resolve_canonical_core_path(g_canonical_path, sizeof(g_canonical_path))) {
    return false;
  }
  char core_dir[PATH_MAX];
  if (!copy_dirname(g_canonical_path, core_dir, sizeof(core_dir)) ||
      !join_path(g_staged_dir, sizeof(g_staged_dir), core_dir, GE_STAGED_UPDATE_DIR_NAME)) {
    return false;
  }

  // Non-fatal if this fails: the plugin still loads and runs, it just won't
  // be able to apply a staged update without an OBS restart this session.
  if (!ge_reload_worker_start(handle_reload_request)) {
    GE_LOG(LOG_WARNING, "failed to start reload worker thread; auto-update will require an OBS restart");
  }

  char err[256];
  if (!ge_core_open(g_canonical_path, g_canonical_path, g_staged_dir, obs_current_module(), /*is_reload=*/false,
                    ge_reload_worker_request, &g_handle, err, sizeof(err))) {
    GE_LOG(LOG_ERROR, "core failed to load; plugin disabled: %s", err);
    ge_reload_worker_stop();
    return false;
  }

  return true;
}

void obs_module_post_load(void) { ge_core_handle_post_load(g_handle); }

void obs_module_unload(void) {
  // Stop (and join) the worker before touching the handle: this guarantees
  // no in-flight reload is still running against it, so no extra locking is
  // needed between this function and handle_reload_request.
  ge_reload_worker_stop();
  ge_core_close(g_handle);
  g_handle = NULL;
}
