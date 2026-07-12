#ifndef GE_FIXTURE_COMMON_H
#define GE_FIXTURE_COMMON_H

// Shared by every generation of the fixture core library (see fixture.c).
// Appends a line to the file named by the GE_FIXTURE_LOG env var each time
// one of the fixture's ABI entry points runs, so test_reload.c can observe
// cross-instance ordering: each dlopen of "the same" library gets its own
// fresh statics, so an in-process counter can't see across separate opens
// the way a file on disk can.

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

#endif /* GE_FIXTURE_COMMON_H */
