#include "fake_obs.h"

#include <stdarg.h>
#include <stddef.h>
#include <stdio.h>

struct obs_module {
  char file_name[512];
  char binary_path[512];
};

#define GE_FAKE_OBS_MAX_MODULES 8
static struct obs_module g_modules[GE_FAKE_OBS_MAX_MODULES];
static size_t g_module_count = 0;

void fake_obs_reset(void) { g_module_count = 0; }

obs_module_t *fake_obs_register_module(const char *file_name, const char *binary_path) {
  if (g_module_count >= GE_FAKE_OBS_MAX_MODULES) {
    return NULL;
  }
  struct obs_module *m = &g_modules[g_module_count++];
  snprintf(m->file_name, sizeof(m->file_name), "%s", file_name);
  snprintf(m->binary_path, sizeof(m->binary_path), "%s", binary_path ? binary_path : "");
  return m;
}

const char *obs_get_module_file_name(obs_module_t *module) { return module ? module->file_name : NULL; }

const char *obs_get_module_binary_path(obs_module_t *module) {
  return module && module->binary_path[0] ? module->binary_path : NULL;
}

void obs_enum_modules(obs_enum_module_callback_t callback, void *param) {
  for (size_t i = 0; i < g_module_count; i++) {
    callback(param, &g_modules[i]);
  }
}

void blog(int log_level, const char *format, ...) {
  (void)log_level;
  va_list args;
  va_start(args, format);
  vfprintf(stderr, format, args);
  va_end(args);
  fputc('\n', stderr);
}
