// One source file, built into three shared libraries (see CMakeLists.txt):
// ge_fixture_v1/v2 (GE_FIXTURE_GENERATION 1/2) and ge_fixture_bad (gen 99,
// GE_FIXTURE_LOAD_FAILS so ge_core_load returns false). Implements the core ABI reload.c drives.

#include "../reload.h"
#include "fixture_common.h"

#include <stdbool.h>
#include <sys/stat.h>

#ifdef _WIN32
#define GE_EXPORT __declspec(dllexport)
#else
#define GE_EXPORT __attribute__((visibility("default")))
#endif

#ifdef GE_FIXTURE_LEGACY
#define ge_core_load_v2 ge_core_load
#endif

GE_EXPORT bool ge_core_load_v2(void *module_arg, const char *canonical_path, const char *staged_dir,
                               ge_core_load_reason reason, ge_request_reload_fn request_reload) {
  (void)module_arg;
  const char *reason_path = getenv("GE_FIXTURE_REASON_OUT");
  if (reason_path) {
    FILE *file = fopen(reason_path, "w");
    if (file) {
      fprintf(file, "%d", (int)reason);
      fclose(file);
    }
  }
  (void)request_reload;
  ge_fixture_record_canonical(canonical_path);
  ge_fixture_record_staged(staged_dir);
  ge_fixture_log("load");
#ifdef GE_FIXTURE_LOAD_FAILS
  return false;
#else
  return true;
#endif
}

GE_EXPORT void ge_core_post_load(void) { ge_fixture_log("post_load"); }

GE_EXPORT void ge_core_commit_update(void) {
  const char *staged_dir = getenv("GE_FIXTURE_STAGED_CHECK");
  struct stat info;
  if (staged_dir && stat(staged_dir, &info) == 0) {
    ge_fixture_log("commit_before_cleanup");
  }
  ge_fixture_log("commit");
}

GE_EXPORT void ge_core_unload(void) { ge_fixture_log("unload"); }
