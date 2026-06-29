// Thin shim: this is the library OBS actually loads as a plugin. It contains no
// real logic — it only `dlopen`s the "core" library (the Rust staticlib + the
// OBS bridge + OpenCV, see core.c) and forwards load/unload to it.
//
// Splitting plugin/core this way means link errors surface here as a catchable,
// logged dlopen failure, and — in dev builds — the core can be reloaded without
// restarting OBS (see dev_reload.c). The core library path is baked in at build
// time (GE_CORE_LIB_PATH) and can be overridden at runtime via GE_CORE_LIB.

#include <obs/libobs/obs-module.h>
#include <obs/libobs/util/base.h>

#include <dlfcn.h>
#include <stdbool.h>
#include <stdlib.h>

#ifdef GE_DEV
#include "dev_reload.h"
#include <errno.h>
#include <limits.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>
#endif

OBS_DECLARE_MODULE()

#define GE_LOG(level, fmt, ...) blog(level, "[the-golden-eye] " fmt, ##__VA_ARGS__)

#ifndef GE_CORE_LIB_PATH
#error "GE_CORE_LIB_PATH must be defined by the build (absolute path to the core library)"
#endif

typedef bool (*ge_core_load_fn)(void);
typedef void (*ge_core_unload_fn)(void);

static void *g_handle = NULL;
static ge_core_unload_fn g_unload = NULL;

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
  const char *path = getenv("GE_CORE_LIB");
  if (!path || !*path) {
    path = GE_CORE_LIB_PATH;
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
  g_unload = (ge_core_unload_fn)dlsym(handle, "ge_core_unload");
  if (!load || !g_unload || !load()) {
    GE_LOG(LOG_ERROR, "core entry points missing or ge_core_load() failed");
    dlclose(handle);
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
  if (!core_open()) {
    GE_LOG(LOG_ERROR, "core failed to load; plugin disabled");
    return false;
  }
#ifdef GE_DEV
  ge_dev_reload_start(core_reload);
#endif
  return true;
}

void obs_module_unload(void) {
#ifdef GE_DEV
  ge_dev_reload_stop();
#endif
  core_close();
}
