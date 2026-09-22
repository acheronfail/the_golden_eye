#ifndef GE_RELOAD_PLATFORM_H
#define GE_RELOAD_PLATFORM_H

// Platform-specific primitives used by reload.c's platform-agnostic
// orchestration, implemented per platform in reload_win32.c / reload_unix.c
// so reload.c itself never needs an #ifdef _WIN32.

#include <limits.h>
#include <stdbool.h>
#include <stddef.h>

#ifndef PATH_MAX
#define PATH_MAX 4096
#endif

#ifdef _WIN32
#include <windows.h>
#else
#include <pthread.h>
#endif

// A path to a not-yet-existing, guaranteed-unique file suitable as a temp
// copy target (see reload.c's temp-copy-then-open scheme).
bool ge_platform_make_temp_path(char *out, size_t out_size);

// Atomically replaces `canonical` with `staged` (same-volume rename;
// retries transient sharing violations on Windows).
bool ge_platform_replace_file(const char *staged, const char *canonical, char *err, size_t err_size);

// Recursively deletes the directory at `path`, best-effort.
void ge_platform_remove_dir_recursive(const char *path);

bool ge_platform_dir_exists(const char *path);

// Cross-platform mutex+condvar pair backing the reload worker thread.
typedef struct {
#ifdef _WIN32
  CRITICAL_SECTION cs;
  CONDITION_VARIABLE cv;
#else
  pthread_mutex_t mutex;
  pthread_cond_t cond;
#endif
} ge_cond_lock;

void ge_cond_lock_init(ge_cond_lock *lock);
void ge_cond_lock_destroy(ge_cond_lock *lock);
void ge_cond_lock_acquire(ge_cond_lock *lock);
void ge_cond_lock_release(ge_cond_lock *lock);
// Must be called with the lock held; returns with it held again.
void ge_cond_lock_wait(ge_cond_lock *lock);
void ge_cond_lock_signal(ge_cond_lock *lock);

// A single joinable OS thread running `run` until ge_platform_thread_join()
// is called. `run` is expected to check its own stop condition -- this is
// not a cancellation mechanism.
#ifdef _WIN32
typedef HANDLE ge_platform_thread;
#else
typedef pthread_t ge_platform_thread;
#endif

// Returns false (leaving *out_thread unset) if the thread couldn't be
// created.
bool ge_platform_thread_spawn(ge_platform_thread *out_thread, void *(*run)(void *), void *arg);
void ge_platform_thread_join(ge_platform_thread thread);

#endif /* GE_RELOAD_PLATFORM_H */
