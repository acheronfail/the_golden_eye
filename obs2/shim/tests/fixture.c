// One source file, built into three shared libraries (see CMakeLists.txt):
// ge_fixture_v1/v2 (GE_FIXTURE_GENERATION 1/2) and ge_fixture_bad (gen 99,
// GE_FIXTURE_LOAD_FAILS so ge_core_load returns false). Implements the core ABI reload.c drives.

#include "fixture_common.h"

#include <stdbool.h>

#ifdef _WIN32
#define GE_EXPORT __declspec(dllexport)
#else
#define GE_EXPORT __attribute__((visibility("default")))
#endif

typedef void (*ge_request_reload_fn)(void);

GE_EXPORT bool ge_core_load(void *module_arg, const char *canonical_path, bool is_reload,
                            ge_request_reload_fn request_reload) {
  (void)module_arg;
  (void)canonical_path;
  (void)is_reload;
  (void)request_reload;
  ge_fixture_log("load");
#ifdef GE_FIXTURE_LOAD_FAILS
  return false;
#else
  return true;
#endif
}

GE_EXPORT void ge_core_post_load(void) { ge_fixture_log("post_load"); }

GE_EXPORT void ge_core_unload(void) { ge_fixture_log("unload"); }
