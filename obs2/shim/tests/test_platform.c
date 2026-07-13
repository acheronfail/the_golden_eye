// Drives the platform primitives in reload_platform.h directly -- temp paths,
// atomic file replace, recursive dir delete, dir-exists, the worker's
// mutex+condvar, thread spawn/join -- checking each contract in isolation.

#include "../reload_platform.h"
#include "test_support.h"

#include <stdio.h>
#include <string.h>

static int g_failures = 0;

static void test_make_temp_path(void) {
  char a[PATH_MAX];
  char b[PATH_MAX];
  CHECK(ge_platform_make_temp_path(a, sizeof(a)), "ge_platform_make_temp_path should succeed");
  CHECK(ge_platform_make_temp_path(b, sizeof(b)), "ge_platform_make_temp_path should succeed");
  CHECK(strcmp(a, b) != 0, "two calls should return two distinct paths");

  // Both platforms create the file as part of allocating a unique name
  // (mkstemps on POSIX, GetTempFileNameA on Windows) -- reload.c relies on
  // this to know the path is safe to immediately dlopen a copy into.
  FILE *fa = fopen(a, "rb");
  CHECK(fa != NULL, "the path returned should already exist as a file: %s", a);
  if (fa) {
    fclose(fa);
  }
  remove(a);
  remove(b);
}

static void test_dir_exists(void) {
  char work_dir[PATH_MAX];
  CHECK(test_make_temp_dir(work_dir, sizeof(work_dir)), "failed to create a temp working directory");

  CHECK(ge_platform_dir_exists(work_dir), "an existing directory should report as existing");

  char file_path[PATH_MAX];
  test_join(file_path, sizeof(file_path), work_dir, "plain_file.txt");
  CHECK(test_write_file(file_path, "not a directory"), "failed to write plain_file.txt");
  CHECK(!ge_platform_dir_exists(file_path), "a plain file should not report as an existing directory");

  char missing_path[PATH_MAX];
  test_join(missing_path, sizeof(missing_path), work_dir, "does_not_exist");
  CHECK(!ge_platform_dir_exists(missing_path), "a nonexistent path should not report as an existing directory");
}

static void test_replace_file(void) {
  char work_dir[PATH_MAX];
  CHECK(test_make_temp_dir(work_dir, sizeof(work_dir)), "failed to create a temp working directory");

  char staged[PATH_MAX];
  test_join(staged, sizeof(staged), work_dir, "staged.txt");
  CHECK(test_write_file(staged, "new content"), "failed to write staged.txt");

  char canonical[PATH_MAX];
  test_join(canonical, sizeof(canonical), work_dir, "canonical.txt");
  CHECK(test_write_file(canonical, "old content"), "failed to write canonical.txt");

  char err[256];
  CHECK(ge_platform_replace_file(staged, canonical, err, sizeof(err)), "ge_platform_replace_file failed: %s", err);

  char *content = test_read_file(canonical);
  CHECK(content != NULL && strcmp(content, "new content") == 0,
        "canonical should hold the staged file's content after replace");
  free(content);

  FILE *staged_file = fopen(staged, "rb");
  CHECK(staged_file == NULL, "the staged path should no longer exist after being moved into canonical's place");
  if (staged_file) {
    fclose(staged_file);
  }
}

static void test_remove_dir_recursive(void) {
  char work_dir[PATH_MAX];
  CHECK(test_make_temp_dir(work_dir, sizeof(work_dir)), "failed to create a temp working directory");

  char tree[PATH_MAX];
  CHECK(test_make_subdir(work_dir, "tree", tree, sizeof(tree)), "failed to create tree/");
  char nested[PATH_MAX];
  CHECK(test_make_subdir(tree, "nested", nested, sizeof(nested)), "failed to create tree/nested/");

  char top_file[PATH_MAX];
  test_join(top_file, sizeof(top_file), tree, "top.txt");
  CHECK(test_write_file(top_file, "top"), "failed to write tree/top.txt");
  char nested_file[PATH_MAX];
  test_join(nested_file, sizeof(nested_file), nested, "nested.txt");
  CHECK(test_write_file(nested_file, "nested"), "failed to write tree/nested/nested.txt");

  ge_platform_remove_dir_recursive(tree);

  CHECK(!ge_platform_dir_exists(tree), "the whole tree, including nested subdirectories, should be gone");
}

// State for the ge_cond_lock rendezvous test, kept in statics rather than
// threaded through the run function's void* -- matching how reload.c's own
// worker callback (which takes no arguments) reaches its state.
static ge_cond_lock g_lock;
static bool g_ready;
static int g_observed;

static void *waiter_thread(void *arg) {
  (void)arg;
  ge_cond_lock_acquire(&g_lock);
  while (!g_ready) {
    ge_cond_lock_wait(&g_lock);
  }
  g_observed = 42;
  ge_cond_lock_release(&g_lock);
  return NULL;
}

static void test_cond_lock_and_thread(void) {
  ge_cond_lock_init(&g_lock);
  g_ready = false;
  g_observed = 0;

  ge_platform_thread thread;
  CHECK(ge_platform_thread_spawn(&thread, waiter_thread, NULL), "ge_platform_thread_spawn should succeed");

  ge_cond_lock_acquire(&g_lock);
  g_ready = true;
  ge_cond_lock_signal(&g_lock);
  ge_cond_lock_release(&g_lock);

  // Joining is itself the synchronization point: it doesn't return until
  // waiter_thread has, so no polling is needed to know g_observed is set.
  ge_platform_thread_join(thread);
  CHECK(g_observed == 42, "the waiter thread should have woken on the signal and set g_observed");

  ge_cond_lock_destroy(&g_lock);
}

int main(void) {
  test_make_temp_path();
  test_dir_exists();
  test_replace_file();
  test_remove_dir_recursive();
  test_cond_lock_and_thread();

  if (g_failures == 0) {
    printf("all shim platform tests passed\n");
    return 0;
  }
  fprintf(stderr, "%d shim platform test failure(s)\n", g_failures);
  return 1;
}
