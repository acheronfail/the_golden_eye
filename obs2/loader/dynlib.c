#ifndef _WIN32
#define _GNU_SOURCE
#endif

#include "dynlib.h"

#ifdef _WIN32
#include <windows.h>
#else
#include <dlfcn.h>
#endif

#include <stdio.h>

const char *ge_dynlib_error(void) {
#ifdef _WIN32
  static char msg[512];
  DWORD err = GetLastError();
  DWORD len = FormatMessageA(FORMAT_MESSAGE_FROM_SYSTEM | FORMAT_MESSAGE_IGNORE_INSERTS, NULL, err, 0, msg,
                             (DWORD)sizeof(msg), NULL);
  if (len == 0) {
    snprintf(msg, sizeof(msg), "Windows error %lu", (unsigned long)err);
  }
  return msg;
#else
  const char *msg = dlerror();
  return msg ? msg : "unknown dynamic loader error";
#endif
}

ge_dynlib_handle ge_dynlib_open(const char *path) {
#ifdef _WIN32
  DWORD previous_mode;
  const DWORD quiet_mode = SEM_FAILCRITICALERRORS | SEM_NOOPENFILEERRORBOX;
  BOOL restore_mode = SetThreadErrorMode(quiet_mode, &previous_mode);
  HMODULE module = LoadLibraryA(path);
  DWORD load_error = GetLastError();
  if (restore_mode) {
    SetThreadErrorMode(previous_mode, NULL);
  }
  SetLastError(load_error);
  return module;
#else
  dlerror();
  return dlopen(path, RTLD_NOW | RTLD_LOCAL);
#endif
}

void *ge_dynlib_symbol(ge_dynlib_handle handle, const char *name) {
#ifdef _WIN32
  return (void *)GetProcAddress(handle, name);
#else
  return dlsym(handle, name);
#endif
}

void ge_dynlib_close(ge_dynlib_handle handle) {
#ifdef _WIN32
  FreeLibrary(handle);
#else
  dlclose(handle);
#endif
}

bool ge_module_path(char *out, size_t out_size) {
#ifdef _WIN32
  HMODULE module = NULL;
  if (!GetModuleHandleExA(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                          (LPCSTR)&ge_module_path, &module)) {
    return false;
  }

  DWORD len = GetModuleFileNameA(module, out, (DWORD)out_size);
  return len > 0 && len < out_size;
#else
  Dl_info info;
  if (dladdr((void *)&ge_module_path, &info) == 0 || !info.dli_fname || !*info.dli_fname) {
    return false;
  }
  return snprintf(out, out_size, "%s", info.dli_fname) < (int)out_size;
#endif
}
