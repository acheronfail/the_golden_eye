// Thin shim: this is the library OBS actually loads as a plugin. It contains no
// real logic — it only `dlopen`s the "core" library (the Rust staticlib + the
// OBS bridge + OpenCV, see core.c) and forwards load/unload to it.
//
// Splitting plugin/core this way means link errors surface here as a catchable,
// logged dlopen failure, and — in dev builds — the core can be reloaded without
// restarting OBS (see dev_reload.c). The core library and CV templates are
// resolved relative to this loaded shim, with environment overrides for devs.

#define _GNU_SOURCE

#include <obs/libobs/obs-module.h>
#include <obs/libobs/util/base.h>
#include <obs/libobs/util/bmem.h>

#include <dlfcn.h>
#include <errno.h>
#include <limits.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#ifdef GE_DEV
#include "dev_reload.h"
#include <unistd.h>
#endif

OBS_DECLARE_MODULE()

#define GE_LOG(level, fmt, ...) blog(level, "[the-golden-eye] " fmt, ##__VA_ARGS__)

#ifndef PATH_MAX
#define PATH_MAX 4096
#endif

#ifndef GE_CORE_LIB_NAME
#error "GE_CORE_LIB_NAME must be defined by the build"
#endif

#ifndef GE_BUNDLED_TEMPLATE_DIR_REL
#error "GE_BUNDLED_TEMPLATE_DIR_REL must be defined by the build"
#endif

typedef bool (*ge_core_load_fn)(void);
typedef void (*ge_core_post_load_fn)(void);
typedef void (*ge_core_unload_fn)(void);

static void *g_handle = NULL;
static ge_core_post_load_fn g_post_load = NULL;
static ge_core_unload_fn g_unload = NULL;

static bool copy_path(char *out, size_t out_size, const char *path) {
  if (snprintf(out, out_size, "%s", path) >= (int)out_size) {
    GE_LOG(LOG_WARNING, "path too long: %s", path);
    return false;
  }
  return true;
}

static bool copy_dirname(const char *path, char *out, size_t out_size) {
  const char *slash = strrchr(path, '/');
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
  Dl_info info;
  if (dladdr((void *)&module_path, &info) == 0 || !info.dli_fname || !*info.dli_fname) {
    GE_LOG(LOG_ERROR, "failed to resolve plugin module path");
    return false;
  }
  return copy_path(out, out_size, info.dli_fname);
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
  const char *sep = dir_len > 0 && dir[dir_len - 1] == '/' ? "" : "/";
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

static bool configure_template_dir_from_obs(void) {
  char *template_dir = obs_module_file("cv_templates");
  if (!template_dir) {
    return false;
  }

  int rc = setenv("GE_CV_TEMPLATE_DIR", template_dir, 1);
  if (rc != 0) {
    GE_LOG(LOG_WARNING, "failed to set GE_CV_TEMPLATE_DIR: %s", strerror(errno));
  }

  bfree(template_dir);
  return rc == 0;
}

static void configure_template_dir(void) {
  const char *existing = getenv("GE_CV_TEMPLATE_DIR");
  if (existing && *existing) {
    return;
  }

  if (configure_template_dir_from_obs()) {
    return;
  }

  char template_dir[PATH_MAX];
  if (!bundled_path(GE_BUNDLED_TEMPLATE_DIR_REL, template_dir, sizeof(template_dir))) {
    GE_LOG(LOG_WARNING, "could not resolve bundled CV templates directory");
    return;
  }

  if (setenv("GE_CV_TEMPLATE_DIR", template_dir, 1) != 0) {
    GE_LOG(LOG_WARNING, "failed to set GE_CV_TEMPLATE_DIR: %s", strerror(errno));
  }
}

#ifdef GE_DEV
// In dev we dlopen a throwaway copy of the core: the loader caches images by
// path, so reopening the same path after dlclose can hand back the stale image
// instead of the freshly rebuilt one. This is the copy currently open.
static char g_copy_path[PATH_MAX] = {0};

// Copy the core library to a fresh temp file and return its path (stored in
// g_copy_path), or NULL on failure.
static const char *stage_core_copy(const char *src) {
  const char *tmpdir = getenv("TMPDIR");
  if (!tmpdir || !*tmpdir) {
    tmpdir = "/tmp";
  }

  char tmpl[PATH_MAX];
  if ((size_t)snprintf(tmpl, sizeof(tmpl), "%s/ge_core_XXXXXX.lib", tmpdir) >= sizeof(tmpl)) {
    GE_LOG(LOG_ERROR, "temp path too long");
    return NULL;
  }

  int dst = mkstemps(tmpl, 4 /* strlen(".lib") */);
  if (dst < 0) {
    GE_LOG(LOG_ERROR, "mkstemps failed: %s", strerror(errno));
    return NULL;
  }

  FILE *in = fopen(src, "rb");
  bool ok = in != NULL;
  if (ok) {
    char buf[64 * 1024];
    size_t r;
    while (ok && (r = fread(buf, 1, sizeof(buf), in)) > 0) {
      ok = write(dst, buf, r) == (ssize_t)r;
    }
    fclose(in);
  }
  close(dst);

  if (!ok) {
    GE_LOG(LOG_ERROR, "failed to copy core '%s': %s", src, strerror(errno));
    unlink(tmpl);
    return NULL;
  }

  snprintf(g_copy_path, sizeof(g_copy_path), "%s", tmpl);
  return g_copy_path;
}

static void remove_core_copy(void) {
  if (g_copy_path[0]) {
    unlink(g_copy_path);
    g_copy_path[0] = '\0';
  }
}
#endif /* GE_DEV */

// dlopen the core library and run its ge_core_load(). Returns false (with a
// logged reason) on any failure.
static bool core_open(void) {
  configure_template_dir();

  char bundled_core[PATH_MAX];
  const char *path = getenv("GE_CORE_LIB");
  if (!path || !*path) {
    if (!bundled_path(GE_CORE_LIB_NAME, bundled_core, sizeof(bundled_core))) {
      return false;
    }
    path = bundled_core;
  }

#ifdef GE_DEV
  path = stage_core_copy(path);
  if (!path) {
    return false;
  }
#endif

  // RTLD_NOW resolves all symbols up front, so missing/mismatched symbols fail
  // here (where we can log them) rather than at first call.
  dlerror();
  void *handle = dlopen(path, RTLD_NOW | RTLD_LOCAL);
  if (!handle) {
    GE_LOG(LOG_ERROR, "failed to dlopen core: %s", dlerror());
#ifdef GE_DEV
    remove_core_copy();
#endif
    return false;
  }

  ge_core_load_fn load = (ge_core_load_fn)dlsym(handle, "ge_core_load");
  g_post_load = (ge_core_post_load_fn)dlsym(handle, "ge_core_post_load");
  g_unload = (ge_core_unload_fn)dlsym(handle, "ge_core_unload");
  if (!load || !g_post_load || !g_unload || !load()) {
    GE_LOG(LOG_ERROR, "core entry points missing or ge_core_load() failed");
    dlclose(handle);
    g_post_load = NULL;
    g_unload = NULL;
#ifdef GE_DEV
    remove_core_copy();
#endif
    return false;
  }

  g_handle = handle;
  return true;
}

// Run the core's ge_core_unload() and dlclose it.
static void core_close(void) {
  if (g_unload) {
    g_unload();
  }
  if (g_handle) {
    dlclose(g_handle);
  }
  g_handle = NULL;
  g_post_load = NULL;
  g_unload = NULL;
#ifdef GE_DEV
  remove_core_copy();
#endif
}

#ifdef GE_DEV
static void core_reload(void) {
  core_close();
  if (!core_open()) {
    GE_LOG(LOG_ERROR, "core reload failed; will retry on next rebuild");
  }
}
#endif

bool obs_module_load(void) {
  if (!ge_check_duplicate_obs_module()) {
    return false;
  }

  if (!core_open()) {
    GE_LOG(LOG_ERROR, "core failed to load; plugin disabled");
    return false;
  }
#ifdef GE_DEV
  ge_dev_reload_start(core_reload);
#endif
  return true;
}

void obs_module_post_load(void) {
  if (g_post_load) {
    g_post_load();
  }
}

void obs_module_unload(void) {
#ifdef GE_DEV
  ge_dev_reload_stop();
#endif
  core_close();
}
