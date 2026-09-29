#ifndef _WIN32
#define _GNU_SOURCE
#endif
#include <stdbool.h>
#include <stddef.h>

#ifdef _WIN32
#include <windows.h>
#else
#include <dlfcn.h>
#endif

// Called once per core by Rust before handing a callback to OBS's UI queue.
// Keep this image mapped until process exit, including the callback's return path.
bool ge_obs_pin_core_for_ui(void) {
#ifdef _WIN32
  HMODULE module;
  return GetModuleHandleExA(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_PIN,
                            (LPCSTR)&ge_obs_pin_core_for_ui, &module) != 0;
#else
  Dl_info info;
  if (!dladdr((const void *)&ge_obs_pin_core_for_ui, &info) || !info.dli_fname) {
    return false;
  }
  // Intentionally retain this reference; a UI callback may survive runtime teardown.
  return dlopen(info.dli_fname, RTLD_NOW | RTLD_LOCAL) != NULL;
#endif
}
