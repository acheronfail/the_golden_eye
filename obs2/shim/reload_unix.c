// POSIX (macOS/Linux) implementation of the platform primitives declared in
// reload_platform.h. See reload_win32.c for the Windows counterpart -- kept
// as two separate straight-line files instead of one file full of
// #ifdef _WIN32 blocks, so each can be read top-to-bottom for its own
// platform.

#define _GNU_SOURCE // mkstemps() is a glibc/BSD extension, not plain POSIX.

#include "reload_platform.h"

#include <dirent.h>
#include <errno.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

static void set_err(char *err, size_t err_size, const char *fmt, ...) {
  if (!err || err_size == 0) {
    return;
  }
  va_list args;
  va_start(args, fmt);
  vsnprintf(err, err_size, fmt, args);
  va_end(args);
}

bool ge_platform_make_temp_path(char *out, size_t out_size) {
  const char *tmpdir = getenv("TMPDIR");
  if (!tmpdir || !*tmpdir) {
    tmpdir = "/tmp";
  }
  if ((size_t)snprintf(out, out_size, "%s/ge_core_XXXXXX.lib", tmpdir) >= out_size) {
    return false;
  }
  int fd = mkstemps(out, 4 /* strlen(".lib") */);
  if (fd < 0) {
    return false;
  }
  close(fd);
  return true;
}

bool ge_platform_replace_file(const char *staged, const char *canonical, char *err, size_t err_size) {
  if (rename(staged, canonical) != 0) {
    set_err(err, err_size, "failed to rename '%s' to '%s': %s", staged, canonical, strerror(errno));
    return false;
  }
  return true;
}

void ge_platform_remove_dir_recursive(const char *path) {
  DIR *dir = opendir(path);
  if (!dir) {
    return;
  }
  struct dirent *entry;
  while ((entry = readdir(dir)) != NULL) {
    if (strcmp(entry->d_name, ".") == 0 || strcmp(entry->d_name, "..") == 0) {
      continue;
    }
    char child[PATH_MAX];
    if ((size_t)snprintf(child, sizeof(child), "%s/%s", path, entry->d_name) >= sizeof(child)) {
      continue;
    }
    struct stat st;
    if (lstat(child, &st) == 0 && S_ISDIR(st.st_mode)) {
      ge_platform_remove_dir_recursive(child);
    } else {
      unlink(child);
    }
  }
  closedir(dir);
  rmdir(path);
}

bool ge_platform_dir_exists(const char *path) {
  struct stat st;
  return stat(path, &st) == 0 && S_ISDIR(st.st_mode);
}

void ge_cond_lock_init(ge_cond_lock *lock) {
  pthread_mutex_init(&lock->mutex, NULL);
  pthread_cond_init(&lock->cond, NULL);
}

void ge_cond_lock_destroy(ge_cond_lock *lock) {
  pthread_mutex_destroy(&lock->mutex);
  pthread_cond_destroy(&lock->cond);
}

void ge_cond_lock_acquire(ge_cond_lock *lock) { pthread_mutex_lock(&lock->mutex); }

void ge_cond_lock_release(ge_cond_lock *lock) { pthread_mutex_unlock(&lock->mutex); }

void ge_cond_lock_wait(ge_cond_lock *lock) { pthread_cond_wait(&lock->cond, &lock->mutex); }

void ge_cond_lock_signal(ge_cond_lock *lock) { pthread_cond_signal(&lock->cond); }

bool ge_platform_thread_spawn(ge_platform_thread *out_thread, void *(*run)(void *), void *arg) {
  return pthread_create(out_thread, NULL, run, arg) == 0;
}

void ge_platform_thread_join(ge_platform_thread thread) { pthread_join(thread, NULL); }
