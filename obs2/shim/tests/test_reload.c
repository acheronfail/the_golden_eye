// Drives shim/reload.c's dlopen/rename/rollback mechanics, the cv_templates/
// locale data-dir sync, and the reload worker thread against the fixture libs
// here, no OBS or Rust dep -- the tier obs2/rust's integration tests can't cover.

#ifndef _WIN32
#define _GNU_SOURCE
#endif

#include "../reload.h"
#include "test_support.h"

#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#if defined(_WIN32)
#define GE_CORE_LEAF "fixture_core.dll"
#elif defined(__APPLE__)
#define GE_CORE_LEAF "fixture_core.dylib"
#else
#define GE_CORE_LEAF "fixture_core.so"
#endif

static int g_failures = 0;

static void dummy_request_reload(void) {
  // No-op: nothing in this test ever calls into the shim's real reload
  // trigger machinery, so there's nothing for this to wake.
}

// State for scenario 4 (the reload worker thread). A plain callback can't take
// parameters, so it reaches its inputs/outputs through statics, like plugin.c's
// handle_reload_request() reaches g_handle/g_canonical_path/g_staged_dir.
static ge_core_handle **g_worker_handle;
static const char *g_worker_canonical;
static const char *g_worker_staged;
static bool g_worker_reload_ok;
static char g_worker_err[256];

static void worker_reload_callback(void) {
  g_worker_reload_ok = ge_core_reload(g_worker_handle, g_worker_canonical, g_worker_staged, NULL, dummy_request_reload,
                                      g_worker_err, sizeof(g_worker_err));
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

  char canonical[PATH_MAX];
  test_join(canonical, sizeof(canonical), work_dir, GE_CORE_LEAF);
  CHECK(test_copy_file(fixture_v1, canonical), "failed to seed canonical core from fixture_v1");

  ge_core_handle *handle = NULL;
  char err[256];
  CHECK(ge_core_open(canonical, canonical, NULL, false, dummy_request_reload, &handle, err, sizeof(err)),
        "initial open failed: %s", err);

  // --- Scenario 1: nothing staged -- reload should fail without touching
  // --- the running core at all.
  char empty_staged[PATH_MAX];
  CHECK(test_make_subdir(work_dir, "empty_staged", empty_staged, sizeof(empty_staged)),
        "failed to create empty_staged");

  CHECK(!ge_core_staged_present(canonical, empty_staged), "an empty staged dir should report nothing present");

  bool ok = ge_core_reload(&handle, canonical, empty_staged, NULL, dummy_request_reload, err, sizeof(err));
  CHECK(!ok, "reload with nothing staged should report failure");
  CHECK(handle != NULL, "handle should still be valid after a no-op reload attempt");

  // --- Scenario 2: a valid newer core is staged with cv_templates (canonical
  // --- has an older one) and locale (canonical has none), exercising both
  // --- sync_data_dir_best_effort branches. Old core fully closed before new opens.
  char staged_v2[PATH_MAX];
  CHECK(test_make_subdir(work_dir, "staged_v2", staged_v2, sizeof(staged_v2)), "failed to create staged_v2");
  char staged_v2_lib[PATH_MAX];
  test_join(staged_v2_lib, sizeof(staged_v2_lib), staged_v2, GE_CORE_LEAF);
  CHECK(test_copy_file(fixture_v2, staged_v2_lib), "failed to stage fixture_v2");

  char staged_templates[PATH_MAX];
  CHECK(test_make_subdir(staged_v2, "cv_templates", staged_templates, sizeof(staged_templates)),
        "failed to create staged cv_templates");
  char staged_templates_marker[PATH_MAX];
  test_join(staged_templates_marker, sizeof(staged_templates_marker), staged_templates, "marker.txt");
  CHECK(test_write_file(staged_templates_marker, "v2-templates"), "failed to write staged templates marker");

  char staged_locale[PATH_MAX];
  CHECK(test_make_subdir(staged_v2, "locale", staged_locale, sizeof(staged_locale)), "failed to create staged locale");
  char staged_locale_marker[PATH_MAX];
  test_join(staged_locale_marker, sizeof(staged_locale_marker), staged_locale, "marker.txt");
  CHECK(test_write_file(staged_locale_marker, "v2-locale"), "failed to write staged locale marker");

  char canonical_templates[PATH_MAX];
  CHECK(test_make_subdir(work_dir, "cv_templates", canonical_templates, sizeof(canonical_templates)),
        "failed to create canonical cv_templates");
  char canonical_templates_marker[PATH_MAX];
  test_join(canonical_templates_marker, sizeof(canonical_templates_marker), canonical_templates, "marker.txt");
  CHECK(test_write_file(canonical_templates_marker, "v1-templates"), "failed to write canonical templates marker");
  // Deliberately no canonical "locale" dir yet -- covers the had_old=false
  // branch for that one.

  CHECK(ge_core_staged_present(canonical, staged_v2), "a populated staged dir should report a core present");

  char canonical_out[PATH_MAX];
  test_join(canonical_out, sizeof(canonical_out), work_dir, "canonical_out.txt");
  test_set_env("GE_FIXTURE_CANONICAL_OUT", canonical_out);

  remove(log_path);
  ok = ge_core_reload(&handle, canonical, staged_v2, NULL, dummy_request_reload, err, sizeof(err));
  CHECK(ok, "reload to fixture_v2 should succeed: %s", err);
  CHECK(handle != NULL, "handle should be non-NULL after a successful reload");

  // Regression: the reloaded core loads from the staged copy but must be told its
  // canonical install path -- otherwise its own staged-update lookups resolve
  // against the staged dir (which the reload just deleted), so no later hot-reload
  // is ever detected.
  char *reported_canonical = test_read_file(canonical_out);
  CHECK(reported_canonical != NULL, "expected the reloaded core to record its canonical path");
  if (reported_canonical) {
    CHECK(strcmp(reported_canonical, canonical) == 0, "reloaded core should be told the canonical path '%s', got '%s'",
          canonical, reported_canonical);
    free(reported_canonical);
  }

  ge_core_handle_post_load(handle);

  // The log was cleared just above, so this only covers the reload itself
  // (close old, open new, then post_load) -- not the earlier setup
  // ge_core_open.
  char *log = test_read_file(log_path);
  CHECK(log != NULL, "expected a fixture log after the successful reload");
  if (log) {
    CHECK(strcmp(log, "unload gen=1\nload gen=2\npost_load gen=2\n") == 0, "unexpected log sequence: %s", log);
    free(log);
  }
  CHECK(test_files_equal(canonical, fixture_v2), "canonical core should now match fixture_v2");

  char *templates_content = test_read_file(canonical_templates_marker);
  CHECK(templates_content != NULL && strcmp(templates_content, "v2-templates") == 0,
        "canonical cv_templates should have been swapped to the staged (v2) content");
  free(templates_content);

  char canonical_locale_dir[PATH_MAX];
  test_join(canonical_locale_dir, sizeof(canonical_locale_dir), work_dir, "locale");
  char canonical_locale_marker[PATH_MAX];
  test_join(canonical_locale_marker, sizeof(canonical_locale_marker), canonical_locale_dir, "marker.txt");
  char *locale_content = test_read_file(canonical_locale_marker);
  CHECK(locale_content != NULL && strcmp(locale_content, "v2-locale") == 0,
        "canonical locale dir should have been created fresh from the staged content");
  free(locale_content);

  // --- Scenario 3: the staged core fails to load -- must roll back to a running
  // --- core (never a broken/unloaded state), never touch canonical (swap never
  // --- succeeded), and leave cv_templates exactly as scenario 2 left it.
  ge_core_close(handle);
  handle = NULL;
  CHECK(test_copy_file(fixture_v1, canonical), "failed to reset canonical core to fixture_v1");
  CHECK(ge_core_open(canonical, canonical, NULL, false, dummy_request_reload, &handle, err, sizeof(err)),
        "re-open before rollback scenario failed: %s", err);

  char staged_bad[PATH_MAX];
  CHECK(test_make_subdir(work_dir, "staged_bad", staged_bad, sizeof(staged_bad)), "failed to create staged_bad");
  char staged_bad_lib[PATH_MAX];
  test_join(staged_bad_lib, sizeof(staged_bad_lib), staged_bad, GE_CORE_LEAF);
  CHECK(test_copy_file(fixture_bad, staged_bad_lib), "failed to stage fixture_bad");

  remove(log_path);
  ok = ge_core_reload(&handle, canonical, staged_bad, NULL, dummy_request_reload, err, sizeof(err));
  CHECK(!ok, "reload to fixture_bad should report failure");
  CHECK(handle != NULL, "handle should be rolled back to a running core, not NULL");

  // Likewise, the log was cleared just above: this covers only the failed
  // reload's own close-old/attempt-new/roll-back-to-old sequence.
  log = test_read_file(log_path);
  CHECK(log != NULL, "expected a fixture log after the failed reload");
  if (log) {
    CHECK(strcmp(log, "unload gen=1\nload gen=99\nload gen=1\n") == 0, "unexpected log sequence: %s", log);
    free(log);
  }
  CHECK(test_files_equal(canonical, fixture_v1), "canonical core should remain fixture_v1 after a failed reload");

  templates_content = test_read_file(canonical_templates_marker);
  CHECK(templates_content != NULL && strcmp(templates_content, "v2-templates") == 0,
        "canonical cv_templates should be untouched by a failed reload");
  free(templates_content);

  // --- Scenario 4: the reload worker thread. ge_reload_worker_request() must be
  // --- safe from any stack and only wake the worker, which reloads on its own
  // --- stack; ge_reload_worker_stop() joins it and is safe if never started.
  ge_reload_worker_stop(); // safe to call even though the worker was never started

  // A fresh staged dir -- scenario 2's staged_v2 was already consumed (a
  // successful ge_core_reload deletes its staged_dir when it's done).
  char staged_v3[PATH_MAX];
  CHECK(test_make_subdir(work_dir, "staged_v3", staged_v3, sizeof(staged_v3)), "failed to create staged_v3");
  char staged_v3_lib[PATH_MAX];
  test_join(staged_v3_lib, sizeof(staged_v3_lib), staged_v3, GE_CORE_LEAF);
  CHECK(test_copy_file(fixture_v2, staged_v3_lib), "failed to stage fixture_v2 for the worker scenario");

  g_worker_handle = &handle;
  g_worker_canonical = canonical;
  g_worker_staged = staged_v3;

  CHECK(ge_reload_worker_start(worker_reload_callback), "failed to start the reload worker thread");

  remove(log_path);
  ge_reload_worker_request();
  ge_reload_worker_stop();
  ge_reload_worker_stop(); // stopping an already-stopped worker must also be safe

  CHECK(g_worker_reload_ok, "worker-triggered reload should have succeeded: %s", g_worker_err);
  log = test_read_file(log_path);
  CHECK(log != NULL && strcmp(log, "unload gen=1\nload gen=2\n") == 0,
        "worker-triggered reload should have produced the same close-old/open-new sequence as scenario 2: %s",
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
