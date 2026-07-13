#ifndef GE_FIXTURE_COMMON_H
#define GE_FIXTURE_COMMON_H

// Shared by every generation of the fixture core library (see fixture.c).
// Appends to the GE_FIXTURE_LOG file each time an ABI entry point runs, so
// test_reload.c can observe cross-instance ordering (statics reset per dlopen).

#include <stdio.h>
#include <stdlib.h>

#ifndef GE_FIXTURE_GENERATION
#error "GE_FIXTURE_GENERATION must be defined by the build"
#endif

static inline void ge_fixture_log(const char *event) {
  const char *path = getenv("GE_FIXTURE_LOG");
  if (!path || !*path) {
    return;
  }
  // Binary mode: on Windows, text-mode append ("a") translates each '\n'
  // written here into "\r\n", but test_reload.c reads this file back in
  // binary mode and compares against '\n'-only expected strings.
  FILE *f = fopen(path, "ab");
  if (!f) {
    return;
  }
  fprintf(f, "%s gen=%d\n", event, GE_FIXTURE_GENERATION);
  fclose(f);
}

// Overwrites GE_FIXTURE_CANONICAL_OUT (when set) with the canonical_path the
// core was handed at load. test_reload.c uses it to assert a reloaded core is
// told its durable install path, not the transient staged path it loaded from.
static inline void ge_fixture_record_canonical(const char *canonical_path) {
  const char *out = getenv("GE_FIXTURE_CANONICAL_OUT");
  if (!out || !*out) {
    return;
  }
  FILE *f = fopen(out, "wb");
  if (!f) {
    return;
  }
  if (canonical_path) {
    fputs(canonical_path, f);
  }
  fclose(f);
}

#endif /* GE_FIXTURE_COMMON_H */
