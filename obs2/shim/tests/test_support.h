#ifndef GE_TEST_SUPPORT_H
#define GE_TEST_SUPPORT_H

// Small helpers shared by every obs2/shim test binary: a failure-counting
// CHECK macro plus temp directories, whole-file copy/compare, and env vars.
// Each test_*.c file defines its own `static int g_failures = 0;` before
// using CHECK (see any test_*.c for the pattern) -- kept per translation
// unit rather than shared across binaries, since every test file here is
// its own standalone executable with its own main().

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

#define CHECK(cond, ...)                                                                                               \
  do {                                                                                                                 \
    if (!(cond)) {                                                                                                     \
      fprintf(stderr, "FAIL (%s:%d): ", __FILE__, __LINE__);                                                           \
      fprintf(stderr, __VA_ARGS__);                                                                                    \
      fprintf(stderr, "\n");                                                                                           \
      g_failures++;                                                                                                    \
    }                                                                                                                  \
  } while (0)

static inline bool test_join(char *out, size_t out_size, const char *dir, const char *leaf) {
  return (size_t)snprintf(out, out_size, "%s/%s", dir, leaf) < out_size;
}

static inline bool test_make_subdir(const char *parent, const char *leaf, char *out, size_t out_size) {
  if (!test_join(out, out_size, parent, leaf)) {
    return false;
  }
#ifdef _WIN32
  return CreateDirectoryA(out, NULL) != 0;
#else
  return mkdir(out, 0700) == 0;
#endif
}

static inline bool test_copy_file(const char *src, const char *dst) {
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

// Writes `contents` (a NUL-terminated string) to `path`, overwriting any
// existing file -- for setting up synthetic fixture trees, as opposed to
// test_copy_file's whole-file duplication of an existing one.
static inline bool test_write_file(const char *path, const char *contents) {
  FILE *f = fopen(path, "wb");
  if (!f) {
    return false;
  }
  size_t len = strlen(contents);
  bool ok = fwrite(contents, 1, len, f) == len;
  return fclose(f) == 0 && ok;
}

static inline bool test_files_equal(const char *a, const char *b) {
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

static inline bool test_make_temp_dir(char *out, size_t out_size) {
#ifdef _WIN32
  char base[MAX_PATH];
  DWORD len = GetTempPathA((DWORD)sizeof(base), base);
  if (len == 0 || len > sizeof(base)) {
    return false;
  }
  char tmpl[MAX_PATH];
  if (GetTempFileNameA(base, "gest", 0, tmpl) == 0) {
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

static inline char *test_read_file(const char *path) {
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

static inline void test_set_env(const char *name, const char *value) {
#ifdef _WIN32
  _putenv_s(name, value);
#else
  setenv(name, value, 1);
#endif
}

static inline void test_unset_env(const char *name) {
#ifdef _WIN32
  _putenv_s(name, "");
#else
  unsetenv(name);
#endif
}

#endif /* GE_TEST_SUPPORT_H */
