// Drives the core-only dlopen/replace/rollback path and reload worker against
// fixture libraries, with no OBS or Rust dependency.

#ifndef _WIN32
#define _GNU_SOURCE
#endif

#include "../reload.h"
#include "../reload_platform.h"
#include "test_support.h"

#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#if defined(_WIN32)
#define GE_CORE_LEAF "custom core fixture.dll"
#elif defined(__APPLE__)
#define GE_CORE_LEAF "custom core fixture.dylib"
#else
#define GE_CORE_LEAF "custom core fixture.so"
#endif

static int g_failures = 0;

static void dummy_request_reload(void) {}

static ge_core_handle **g_worker_handle;
static const char *g_worker_canonical;
static const char *g_worker_staged;
static bool g_worker_reload_ok;
static char g_worker_err[256];

static void worker_reload_callback(void) {
  g_worker_reload_ok = ge_core_reload(g_worker_handle, g_worker_canonical, g_worker_staged, NULL, dummy_request_reload,
                                      g_worker_err, sizeof(g_worker_err));
}

static bool make_staging_dir(const char *core_dir, char *out, size_t out_size) {
  return test_make_subdir(core_dir, ".ge_update_staged", out, out_size);
}

static bool stage_core(const char *fixture, const char *staged_dir, char *out, size_t out_size) {
  test_join(out, out_size, staged_dir, GE_CORE_LEAF);
  return test_copy_file(fixture, out);
}

int main(int argc, char **argv) {
  if (argc != 4) {
    fprintf(stderr, "usage: %s <fixture_v1> <fixture_v2> <fixture_bad>\n", argv[0]);
    return 2;
  }
  const char *fixture_v1 = argv[1];
  const char *fixture_v2 = argv[2];
  const char *fixture_bad = argv[3];

  char work_dir[PATH_MAX];
  CHECK(test_make_temp_dir(work_dir, sizeof(work_dir)), "failed to create a temp working directory");

  char log_path[PATH_MAX];
  test_join(log_path, sizeof(log_path), work_dir, "fixture.log");
  test_set_env("GE_FIXTURE_LOG", log_path);

  char core_dir[PATH_MAX];
  CHECK(test_make_dirs(work_dir, "unrelated install/core location with spaces", core_dir, sizeof(core_dir)),
        "failed to create custom core directory");

  char canonical[PATH_MAX];
  test_join(canonical, sizeof(canonical), core_dir, GE_CORE_LEAF);
  CHECK(test_copy_file(fixture_v1, canonical), "failed to seed custom-named canonical core");

  char staged_dir[PATH_MAX];
  CHECK(make_staging_dir(core_dir, staged_dir, sizeof(staged_dir)), "failed to create adjacent staging directory");

  ge_core_handle *handle = NULL;
  char err[256];
  CHECK(ge_core_open(canonical, canonical, staged_dir, NULL, false, dummy_request_reload, &handle, err, sizeof(err)),
        "initial open failed: %s", err);

  CHECK(!ge_core_staged_present(canonical, staged_dir), "an empty staged dir should report nothing present");
  bool ok = ge_core_reload(&handle, canonical, staged_dir, NULL, dummy_request_reload, err, sizeof(err));
  CHECK(!ok, "reload with nothing staged should report failure");
  CHECK(handle != NULL, "handle should remain valid after a no-op reload");

  char staged_lib[PATH_MAX];
  test_join(staged_lib, sizeof(staged_lib), staged_dir, GE_CORE_LEAF);
  CHECK(test_write_file(staged_lib, "not a shared library"), "failed to stage invalid precheck fixture");
  ok = ge_core_reload(&handle, canonical, staged_dir, NULL, dummy_request_reload, err, sizeof(err));
  CHECK(!ok, "reload should reject a staged core that fails precheck");
  CHECK(handle != NULL, "precheck failure should leave the running core untouched");
  CHECK(!ge_platform_dir_exists(staged_dir), "precheck failure should discard its staged update");

  CHECK(make_staging_dir(core_dir, staged_dir, sizeof(staged_dir)), "failed to recreate staging after precheck");
  CHECK(stage_core(fixture_v2, staged_dir, staged_lib, sizeof(staged_lib)), "failed to stage fixture_v2");
  CHECK(ge_core_staged_present(canonical, staged_dir), "the custom-named staged core should be detected");

  char canonical_out[PATH_MAX];
  char staged_out[PATH_MAX];
  test_join(canonical_out, sizeof(canonical_out), work_dir, "canonical_out.txt");
  test_join(staged_out, sizeof(staged_out), work_dir, "staged_out.txt");
  test_set_env("GE_FIXTURE_CANONICAL_OUT", canonical_out);
  test_set_env("GE_FIXTURE_STAGED_OUT", staged_out);

  remove(log_path);
  ok = ge_core_reload(&handle, canonical, staged_dir, NULL, dummy_request_reload, err, sizeof(err));
  CHECK(ok, "reload to fixture_v2 should succeed: %s", err);
  CHECK(handle != NULL, "handle should be non-NULL after a successful reload");
  CHECK(test_files_equal(canonical, fixture_v2), "canonical core should now match fixture_v2");
  CHECK(!ge_platform_dir_exists(staged_dir), "successful reload should remove the staging directory");

  char *reported_canonical = test_read_file(canonical_out);
  CHECK(reported_canonical && strcmp(reported_canonical, canonical) == 0,
        "reloaded core should receive the exact canonical path");
  free(reported_canonical);
  char *reported_staged = test_read_file(staged_out);
  CHECK(reported_staged && strcmp(reported_staged, staged_dir) == 0,
        "reloaded core should receive the exact staging path");
  free(reported_staged);

  ge_core_handle_post_load(handle);
  char *log = test_read_file(log_path);
  CHECK(log && strcmp(log, "unload gen=1\nload gen=2\ncommit gen=2\npost_load gen=2\n") == 0,
        "unexpected successful reload sequence: %s", log ? log : "(missing)");
  free(log);

  ge_core_close(handle);
  handle = NULL;
  CHECK(test_copy_file(fixture_v1, canonical), "failed to reset canonical core");
  CHECK(make_staging_dir(core_dir, staged_dir, sizeof(staged_dir)), "failed to recreate staging directory");
  CHECK(ge_core_open(canonical, canonical, staged_dir, NULL, false, dummy_request_reload, &handle, err, sizeof(err)),
        "re-open before rollback scenario failed: %s", err);
  CHECK(stage_core(fixture_bad, staged_dir, staged_lib, sizeof(staged_lib)), "failed to stage failing fixture");

  remove(log_path);
  ok = ge_core_reload(&handle, canonical, staged_dir, NULL, dummy_request_reload, err, sizeof(err));
  CHECK(!ok, "reload to fixture_bad should report failure");
  CHECK(handle != NULL, "failed reload should roll back to a running core");
  CHECK(test_files_equal(canonical, fixture_v1), "failed reload must not replace the canonical core");
  CHECK(!ge_platform_dir_exists(staged_dir), "failed reload should discard its staged update");
  log = test_read_file(log_path);
  CHECK(log && strcmp(log, "unload gen=1\nload gen=99\nload gen=1\n") == 0, "unexpected rollback sequence: %s",
        log ? log : "(missing)");
  free(log);

  // A canonical replacement failure happens after the new core starts. Closing
  // it must roll back provisional data before the old core is reopened.
  ge_core_close(handle);
  handle = NULL;
  CHECK(make_staging_dir(core_dir, staged_dir, sizeof(staged_dir)), "failed to recreate commit-failure staging");
  CHECK(stage_core(fixture_v2, staged_dir, staged_lib, sizeof(staged_lib)), "failed to stage commit-failure fixture");
  CHECK(ge_core_open(canonical, canonical, staged_dir, NULL, false, dummy_request_reload, &handle, err, sizeof(err)),
        "re-open before commit-failure scenario failed: %s", err);

  remove(log_path);
  ge_core_test_fail_next_replace();
  ok = ge_core_reload(&handle, canonical, staged_dir, NULL, dummy_request_reload, err, sizeof(err));
  CHECK(!ok, "reload should fail when canonical replacement fails");
  CHECK(handle != NULL, "canonical replacement failure should roll back to a running core");
  CHECK(test_files_equal(canonical, fixture_v1), "canonical replacement failure must preserve the old core");
  CHECK(!ge_platform_dir_exists(staged_dir), "canonical replacement failure should discard staging");
  log = test_read_file(log_path);
  CHECK(log && strcmp(log, "unload gen=1\nload gen=2\nunload gen=2\nload gen=1\n") == 0,
        "unexpected canonical replacement rollback sequence: %s", log ? log : "(missing)");
  free(log);

  ge_core_close(handle);
  handle = NULL;
  CHECK(make_staging_dir(core_dir, staged_dir, sizeof(staged_dir)), "failed to recreate worker staging directory");
  CHECK(stage_core(fixture_v2, staged_dir, staged_lib, sizeof(staged_lib)), "failed to stage worker fixture");
  CHECK(ge_core_open(canonical, canonical, staged_dir, NULL, false, dummy_request_reload, &handle, err, sizeof(err)),
        "re-open before worker scenario failed: %s", err);

  g_worker_handle = &handle;
  g_worker_canonical = canonical;
  g_worker_staged = staged_dir;
  ge_reload_worker_stop();
  CHECK(ge_reload_worker_start(worker_reload_callback), "failed to start reload worker");

  remove(log_path);
  ge_reload_worker_request();
  ge_reload_worker_stop();
  ge_reload_worker_stop();

  CHECK(g_worker_reload_ok, "worker-triggered reload should succeed: %s", g_worker_err);
  log = test_read_file(log_path);
  CHECK(log && strcmp(log, "unload gen=1\nload gen=2\ncommit gen=2\n") == 0, "unexpected worker reload sequence: %s",
        log ? log : "(missing)");
  free(log);

  ge_core_close(handle);
  if (g_failures == 0) {
    printf("all shim reload tests passed\n");
    return 0;
  }
  fprintf(stderr, "%d shim reload test failure(s)\n", g_failures);
  return 1;
}
