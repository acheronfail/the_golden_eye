// Drives shim/reload.c's dlopen/rename/rollback mechanics directly against
// the fixture libraries in this directory, with no OBS or Rust dependency.
// This is the tier of testing that obs2/rust's integration tests (which link
// the Rust crate directly, never going through a real shim dlopen) cannot
// cover -- see AGENTS.md and the auto-update design notes for why.

#ifndef _WIN32
#define _GNU_SOURCE
#endif

#include "../reload.h"

#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#ifdef _WIN32
#include <windows.h>
#else
#include <sys/stat.h>
#include <unistd.h>
#endif

#ifndef PATH_MAX
#define PATH_MAX 4096
#endif

#if defined(_WIN32)
#define GE_CORE_LEAF "fixture_core.dll"
#elif defined(__APPLE__)
#define GE_CORE_LEAF "fixture_core.dylib"
#else
#define GE_CORE_LEAF "fixture_core.so"
#endif

static int g_failures = 0;

#define CHECK(cond, ...)                                                                                               \
  do {                                                                                                                 \
    if (!(cond)) {                                                                                                     \
      fprintf(stderr, "FAIL (%s:%d): ", __FILE__, __LINE__);                                                           \
      fprintf(stderr, __VA_ARGS__);                                                                                    \
      fprintf(stderr, "\n");                                                                                           \
      g_failures++;                                                                                                    \
    }                                                                                                                  \
  } while (0)

static void dummy_request_reload(void) {
  // No-op: nothing in this test ever calls into the shim's real reload
  // trigger machinery, so there's nothing for this to wake.
}

static bool join(char *out, size_t out_size, const char *dir, const char *leaf) {
  return (size_t)snprintf(out, out_size, "%s/%s", dir, leaf) < out_size;
}

static bool make_subdir(const char *parent, const char *leaf, char *out, size_t out_size) {
  if (!join(out, out_size, parent, leaf)) {
    return false;
  }
#ifdef _WIN32
  return CreateDirectoryA(out, NULL) != 0;
#else
  return mkdir(out, 0700) == 0;
#endif
}

static bool copy_file(const char *src, const char *dst) {
  FILE *in = fopen(src, "rb");
  if (!in) {
    return false;
  }
  FILE *out = fopen(dst, "wb");
  if (!out) {
    fclose(in);
    return false;
  }
  char buf[64 * 1024];
  size_t n;
  bool ok = true;
  while (ok && (n = fread(buf, 1, sizeof(buf), in)) > 0) {
    ok = fwrite(buf, 1, n, out) == n;
  }
  fclose(in);
  fclose(out);
  return ok;
}

static bool files_equal(const char *a, const char *b) {
  FILE *fa = fopen(a, "rb");
  FILE *fb = fopen(b, "rb");
  if (!fa || !fb) {
    if (fa) {
      fclose(fa);
    }
    if (fb) {
      fclose(fb);
    }
    return false;
  }
  char bufa[64 * 1024];
  char bufb[64 * 1024];
  bool equal = true;
  for (;;) {
    size_t na = fread(bufa, 1, sizeof(bufa), fa);
    size_t nb = fread(bufb, 1, sizeof(bufb), fb);
    if (na != nb || memcmp(bufa, bufb, na) != 0) {
      equal = false;
      break;
    }
    if (na == 0) {
      break;
    }
  }
  fclose(fa);
  fclose(fb);
  return equal;
}

static bool make_temp_dir(char *out, size_t out_size) {
#ifdef _WIN32
  char base[MAX_PATH];
  DWORD len = GetTempPathA((DWORD)sizeof(base), base);
  if (len == 0 || len > sizeof(base)) {
    return false;
  }
  char tmpl[MAX_PATH];
  if (GetTempFileNameA(base, "gerl", 0, tmpl) == 0) {
    return false;
  }
  // GetTempFileNameA creates a file at that unique path; we want a directory
  // there instead.
  DeleteFileA(tmpl);
  if (!CreateDirectoryA(tmpl, NULL)) {
    return false;
  }
  return (size_t)snprintf(out, out_size, "%s", tmpl) < out_size;
#else
  const char *tmpdir = getenv("TMPDIR");
  char buf[PATH_MAX];
  if ((size_t)snprintf(buf, sizeof(buf), "%s/ge_shim_test_XXXXXX", (tmpdir && *tmpdir) ? tmpdir : "/tmp") >=
      sizeof(buf)) {
    return false;
  }
  if (!mkdtemp(buf)) {
    return false;
  }
  return (size_t)snprintf(out, out_size, "%s", buf) < out_size;
#endif
}

static char *read_file_to_string(const char *path) {
  FILE *f = fopen(path, "rb");
  if (!f) {
    return NULL;
  }
  fseek(f, 0, SEEK_END);
  long size = ftell(f);
  fseek(f, 0, SEEK_SET);
  if (size < 0) {
    fclose(f);
    return NULL;
  }
  char *buf = malloc((size_t)size + 1);
  if (!buf) {
    fclose(f);
    return NULL;
  }
  size_t n = fread(buf, 1, (size_t)size, f);
  buf[n] = '\0';
  fclose(f);
  return buf;
}

static void set_env(const char *name, const char *value) {
#ifdef _WIN32
  _putenv_s(name, value);
#else
  setenv(name, value, 1);
#endif
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
  CHECK(make_temp_dir(work_dir, sizeof(work_dir)), "failed to create a temp working directory");

  char log_path[PATH_MAX];
  join(log_path, sizeof(log_path), work_dir, "fixture.log");
  set_env("GE_FIXTURE_LOG", log_path);

  char canonical[PATH_MAX];
  join(canonical, sizeof(canonical), work_dir, GE_CORE_LEAF);
  CHECK(copy_file(fixture_v1, canonical), "failed to seed canonical core from fixture_v1");

  ge_core_handle *handle = NULL;
  char err[256];
  CHECK(ge_core_open(canonical, NULL, false, dummy_request_reload, &handle, err, sizeof(err)),
        "initial open failed: %s", err);

  // --- Scenario 1: nothing staged -- reload should fail without touching
  // --- the running core at all.
  char empty_staged[PATH_MAX];
  CHECK(make_subdir(work_dir, "empty_staged", empty_staged, sizeof(empty_staged)), "failed to create empty_staged");

  bool ok = ge_core_reload(&handle, canonical, empty_staged, NULL, dummy_request_reload, err, sizeof(err));
  CHECK(!ok, "reload with nothing staged should report failure");
  CHECK(handle != NULL, "handle should still be valid after a no-op reload attempt");

  // --- Scenario 2: a valid newer core is staged -- swap should succeed,
  // --- old must be fully closed before new is opened, and canonical should
  // --- end up holding the new bytes.
  char staged_v2[PATH_MAX];
  CHECK(make_subdir(work_dir, "staged_v2", staged_v2, sizeof(staged_v2)), "failed to create staged_v2");
  char staged_v2_lib[PATH_MAX];
  join(staged_v2_lib, sizeof(staged_v2_lib), staged_v2, GE_CORE_LEAF);
  CHECK(copy_file(fixture_v2, staged_v2_lib), "failed to stage fixture_v2");

  remove(log_path);
  ok = ge_core_reload(&handle, canonical, staged_v2, NULL, dummy_request_reload, err, sizeof(err));
  CHECK(ok, "reload to fixture_v2 should succeed: %s", err);
  CHECK(handle != NULL, "handle should be non-NULL after a successful reload");

  // The log was cleared just above, so this only covers the reload itself
  // (close old, then open new) -- not the earlier setup ge_core_open.
  char *log = read_file_to_string(log_path);
  CHECK(log != NULL, "expected a fixture log after the successful reload");
  if (log) {
    CHECK(strcmp(log, "unload gen=1\nload gen=2\n") == 0, "unexpected log sequence: %s", log);
    free(log);
  }
  CHECK(files_equal(canonical, fixture_v2), "canonical core should now match fixture_v2");

  // --- Scenario 3: the staged core fails to load -- must roll back to a
  // --- running core (never end up in a broken/unloaded state), and never
  // --- touch canonical since the swap never succeeded.
  ge_core_close(handle);
  handle = NULL;
  CHECK(copy_file(fixture_v1, canonical), "failed to reset canonical core to fixture_v1");
  CHECK(ge_core_open(canonical, NULL, false, dummy_request_reload, &handle, err, sizeof(err)),
        "re-open before rollback scenario failed: %s", err);

  char staged_bad[PATH_MAX];
  CHECK(make_subdir(work_dir, "staged_bad", staged_bad, sizeof(staged_bad)), "failed to create staged_bad");
  char staged_bad_lib[PATH_MAX];
  join(staged_bad_lib, sizeof(staged_bad_lib), staged_bad, GE_CORE_LEAF);
  CHECK(copy_file(fixture_bad, staged_bad_lib), "failed to stage fixture_bad");

  remove(log_path);
  ok = ge_core_reload(&handle, canonical, staged_bad, NULL, dummy_request_reload, err, sizeof(err));
  CHECK(!ok, "reload to fixture_bad should report failure");
  CHECK(handle != NULL, "handle should be rolled back to a running core, not NULL");

  // Likewise, the log was cleared just above: this covers only the failed
  // reload's own close-old/attempt-new/roll-back-to-old sequence.
  log = read_file_to_string(log_path);
  CHECK(log != NULL, "expected a fixture log after the failed reload");
  if (log) {
    CHECK(strcmp(log, "unload gen=1\nload gen=99\nload gen=1\n") == 0, "unexpected log sequence: %s", log);
    free(log);
  }
  CHECK(files_equal(canonical, fixture_v1), "canonical core should remain fixture_v1 after a failed reload");

  ge_core_close(handle);

  if (g_failures == 0) {
    printf("all shim reload tests passed\n");
    return 0;
  }
  fprintf(stderr, "%d shim reload test failure(s)\n", g_failures);
  return 1;
}
