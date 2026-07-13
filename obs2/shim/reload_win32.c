// Windows implementation of the platform primitives in reload_platform.h.
// See reload_unix.c for the POSIX counterpart -- kept as two straight-line
// files instead of one full of #ifdef _WIN32 blocks.

#include "reload_platform.h"

#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

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
  char dir[MAX_PATH];
  DWORD dir_len = GetTempPathA((DWORD)sizeof(dir), dir);
  if (dir_len == 0 || dir_len > sizeof(dir)) {
    return false;
  }
  char tmp[MAX_PATH];
  if (GetTempFileNameA(dir, "gec", 0, tmp) == 0) {
    return false;
  }
  return (size_t)snprintf(out, out_size, "%s", tmp) < out_size;
}

bool ge_platform_replace_file(const char *staged, const char *canonical, char *err, size_t err_size) {
  const DWORD flags = MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH;
  const int max_attempts = 6;
  DWORD delay_ms = 50;
  for (int attempt = 1; attempt <= max_attempts; attempt++) {
    if (MoveFileExA(staged, canonical, flags)) {
      return true;
    }
    DWORD last_error = GetLastError();
    if (last_error != ERROR_SHARING_VIOLATION && last_error != ERROR_ACCESS_DENIED) {
      set_err(err, err_size, "failed to move '%s' to '%s' (error %lu)", staged, canonical, (unsigned long)last_error);
      return false;
    }
    if (attempt == max_attempts) {
      set_err(err, err_size, "failed to move '%s' to '%s' after %d attempts (sharing violation)", staged, canonical,
              max_attempts);
      return false;
    }
    Sleep(delay_ms);
    delay_ms *= 2;
  }
  return false;
}

void ge_platform_remove_dir_recursive(const char *path) {
  char pattern[PATH_MAX];
  if ((size_t)snprintf(pattern, sizeof(pattern), "%s\\*", path) >= sizeof(pattern)) {
    return;
  }
  WIN32_FIND_DATAA data;
  HANDLE find = FindFirstFileA(pattern, &data);
  if (find != INVALID_HANDLE_VALUE) {
    do {
      if (strcmp(data.cFileName, ".") == 0 || strcmp(data.cFileName, "..") == 0) {
        continue;
      }
      char child[PATH_MAX];
      if ((size_t)snprintf(child, sizeof(child), "%s\\%s", path, data.cFileName) >= sizeof(child)) {
        continue;
      }
      if (data.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY) {
        ge_platform_remove_dir_recursive(child);
      } else {
        DeleteFileA(child);
      }
    } while (FindNextFileA(find, &data));
    FindClose(find);
  }
  RemoveDirectoryA(path);
}

bool ge_platform_dir_exists(const char *path) {
  DWORD attrs = GetFileAttributesA(path);
  return attrs != INVALID_FILE_ATTRIBUTES && (attrs & FILE_ATTRIBUTE_DIRECTORY);
}

void ge_cond_lock_init(ge_cond_lock *lock) {
  InitializeCriticalSection(&lock->cs);
  InitializeConditionVariable(&lock->cv);
}

void ge_cond_lock_destroy(ge_cond_lock *lock) { DeleteCriticalSection(&lock->cs); }

void ge_cond_lock_acquire(ge_cond_lock *lock) { EnterCriticalSection(&lock->cs); }

void ge_cond_lock_release(ge_cond_lock *lock) { LeaveCriticalSection(&lock->cs); }

void ge_cond_lock_wait(ge_cond_lock *lock) { SleepConditionVariableCS(&lock->cv, &lock->cs, INFINITE); }

void ge_cond_lock_signal(ge_cond_lock *lock) { WakeConditionVariable(&lock->cv); }

// CreateThread's callback is `DWORD WINAPI (*)(LPVOID)`, not the
// `void *(*)(void *)` this header standardizes on -- casting between the two
// calling conventions is UB, so this heap-allocated trampoline bridges them.
typedef struct {
  void *(*run)(void *);
  void *arg;
} win32_thread_start_args;

static DWORD WINAPI win32_thread_start(LPVOID param) {
  win32_thread_start_args *args = (win32_thread_start_args *)param;
  void *(*run)(void *) = args->run;
  void *arg = args->arg;
  free(args);
  run(arg);
  return 0;
}

bool ge_platform_thread_spawn(ge_platform_thread *out_thread, void *(*run)(void *), void *arg) {
  win32_thread_start_args *args = malloc(sizeof(*args));
  if (!args) {
    return false;
  }
  args->run = run;
  args->arg = arg;

  HANDLE thread = CreateThread(NULL, 0, win32_thread_start, args, 0, NULL);
  if (!thread) {
    free(args);
    return false;
  }
  *out_thread = thread;
  return true;
}

void ge_platform_thread_join(ge_platform_thread thread) {
  WaitForSingleObject(thread, INFINITE);
  CloseHandle(thread);
}
