// Drives shim/dynlib.c directly: the raw dlopen/dlsym/dlclose wrapper and
// self-path resolution, with no OBS dependency. reload.c's own tests
// exercise this transitively (every ge_core_open goes through it), but only
// ever on paths that are already known to exist and load cleanly -- this
// file covers dynlib.c's own contract directly, including the failure
// paths reload.c never triggers.

#include "../dynlib.h"
#include "test_support.h"

#include <stdio.h>
#include <string.h>

static int g_failures = 0;

int main(int argc, char **argv) {
  if (argc != 2) {
    fprintf(stderr, "usage: %s <fixture_v1>\n", argv[0]);
    return 2;
  }
  const char *fixture_v1 = argv[1];

  // ge_module_path resolves the shared object containing the calling
  // address -- for this statically-linked test binary, that's the test
  // executable's own path, which should exist on disk.
  char self_path[PATH_MAX];
  CHECK(ge_module_path(self_path, sizeof(self_path)), "ge_module_path should resolve this test binary's own path");
  FILE *self_file = fopen(self_path, "rb");
  CHECK(self_file != NULL, "the path ge_module_path returned ('%s') should exist and be readable", self_path);
  if (self_file) {
    fclose(self_file);
  }

  // A too-small buffer must fail cleanly, not overflow or truncate silently.
  char tiny[1];
  CHECK(!ge_module_path(tiny, sizeof(tiny)), "ge_module_path should report failure when out_size is too small");

  // Open/resolve/close round trip against a real fixture library.
  ge_dynlib_handle dl = ge_dynlib_open(fixture_v1);
  CHECK(dl != NULL, "ge_dynlib_open should succeed for an existing shared library: %s", ge_dynlib_error());
  if (dl) {
    void *load_sym = ge_dynlib_symbol(dl, "ge_core_load");
    CHECK(load_sym != NULL, "ge_dynlib_symbol should resolve ge_core_load, which fixture.c exports");

    void *missing_sym = ge_dynlib_symbol(dl, "ge_this_symbol_does_not_exist");
    CHECK(missing_sym == NULL, "ge_dynlib_symbol should return NULL for a symbol the library doesn't export");

    ge_dynlib_close(dl);
  }

  // Opening a nonexistent path must fail, not crash, and leave a non-empty
  // error message behind for the caller to log.
  ge_dynlib_handle missing_dl = ge_dynlib_open("/no/such/path/ge_definitely_missing.lib");
  CHECK(missing_dl == NULL, "ge_dynlib_open should fail for a nonexistent path");
  const char *dlerr = ge_dynlib_error();
  CHECK(dlerr != NULL && dlerr[0] != '\0', "ge_dynlib_error should report a non-empty message after a failed open");

  if (g_failures == 0) {
    printf("all shim dynlib tests passed\n");
    return 0;
  }
  fprintf(stderr, "%d shim dynlib test failure(s)\n", g_failures);
  return 1;
}
