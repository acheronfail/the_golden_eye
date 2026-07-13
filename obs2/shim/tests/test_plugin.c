// Drives the real, unmodified shim/plugin.c through its three OBS entry
// points (obs_module_load/post_load/unload), linked directly into this
// binary alongside a small fake OBS module registry (see fake_obs.c/.h) --
// so the duplicate-copy check and the load/post_load/unload sequencing get
// real coverage without needing the actual OBS SDK. reload.c's own tests
// cover the dlopen/reload mechanics plugin.c calls into; this file covers
// the thin OBS-facing layer on top of that.

#include "../dynlib.h"
#include "../reload.h"
#include "fake_obs.h"
#include "test_support.h"

#include <stdio.h>
#include <string.h>

static int g_failures = 0;

// The three entry points OBS itself would dlsym and call.
extern bool obs_module_load(void);
extern void obs_module_post_load(void);
extern void obs_module_unload(void);

int main(int argc, char **argv) {
  if (argc != 3) {
    fprintf(stderr, "usage: %s <fixture_v1> <fixture_bad>\n", argv[0]);
    return 2;
  }
  const char *fixture_v1 = argv[1];
  const char *fixture_bad = argv[2];

  char work_dir[PATH_MAX];
  CHECK(test_make_temp_dir(work_dir, sizeof(work_dir)), "failed to create a temp working directory");
  char log_path[PATH_MAX];
  test_join(log_path, sizeof(log_path), work_dir, "fixture.log");
  test_set_env("GE_FIXTURE_LOG", log_path);

  // --- Scenario 1: a second copy of the plugin is already loaded (same
  // --- module file name, different registered module) -- obs_module_load
  // --- must refuse to load this copy. GE_CORE_LIB is deliberately pointed
  // --- at a real, valid core here (rather than left unset) so that if the
  // --- duplicate check were ever broken, obs_module_load would actually
  // --- *succeed* -- making this assertion a specific signal of the
  // --- duplicate check itself, not a coincidental failure for some other
  // --- reason (e.g. no core found).
  fake_obs_reset();
  obs_module_t *self = fake_obs_register_module("the_golden_eye.so", "/path/a/the_golden_eye.so");
  fake_obs_register_module("the_golden_eye.so", "/path/b/the_golden_eye.so");
  obs_module_set_pointer(self);
  test_set_env("GE_CORE_LIB", fixture_v1);

  CHECK(!obs_module_load(), "obs_module_load should refuse to load when a duplicate copy is already registered");
  obs_module_unload(); // defensive: if the check above is broken, this keeps state clean for scenario 2

  // --- Scenario 2: resolving the core path falls all the way through to
  // --- the bundled default (no GE_CORE_LIB override) -- since nothing real
  // --- sits next to this test binary, this should fail cleanly rather than
  // --- crash.
  fake_obs_reset();
  self = fake_obs_register_module("the_golden_eye.so", "/path/a/the_golden_eye.so");
  obs_module_set_pointer(self);
  test_unset_env("GE_CORE_LIB");

  CHECK(!obs_module_load(), "obs_module_load should fail cleanly when the bundled core path doesn't exist");

  // --- Scenario 3: the resolved core exists but fails to load. Must fail
  // --- cleanly and leave the reload worker thread it started stopped
  // --- again -- obs_module_unload() afterward must be a safe no-op.
  fake_obs_reset();
  self = fake_obs_register_module("the_golden_eye.so", "/path/a/the_golden_eye.so");
  obs_module_set_pointer(self);
  test_set_env("GE_CORE_LIB", fixture_bad);

  CHECK(!obs_module_load(), "obs_module_load should fail cleanly when the resolved core fails to load");
  obs_module_unload(); // must not crash or hang even though load never succeeded

  // --- Scenario 4: the happy path -- a single registered module and a
  // --- valid core. load/post_load/unload should reach the core in order.
  fake_obs_reset();
  self = fake_obs_register_module("the_golden_eye.so", "/path/a/the_golden_eye.so");
  obs_module_set_pointer(self);
  test_set_env("GE_CORE_LIB", fixture_v1);

  remove(log_path);
  CHECK(obs_module_load(), "obs_module_load should succeed with a single registered module and a valid core");
  obs_module_post_load();
  obs_module_unload();

  char *log = test_read_file(log_path);
  CHECK(log != NULL && strcmp(log, "load gen=1\npost_load gen=1\nunload gen=1\n") == 0, "unexpected log sequence: %s",
        log ? log : "(missing)");
  free(log);

  if (g_failures == 0) {
    printf("all shim plugin tests passed\n");
    return 0;
  }
  fprintf(stderr, "%d shim plugin test failure(s)\n", g_failures);
  return 1;
}
