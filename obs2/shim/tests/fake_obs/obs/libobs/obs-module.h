#ifndef GE_FAKE_OBS_MODULE_H
#define GE_FAKE_OBS_MODULE_H

// Minimal stand-in for real libobs' obs-module.h, declaring only the exact
// symbols shim/plugin.c uses. Lets test_plugin.c compile and link the real,
// unmodified plugin.c against a controllable fake OBS module registry
// instead of the real OBS SDK -- see fake_obs.c for that registry, and
// fake_obs.h for the test-only API that controls it.

typedef struct obs_module obs_module_t;

#define MODULE_EXPORT

// Declared here (not only inside the OBS_DECLARE_MODULE() expansion below)
// so test code in a different translation unit can call it, simulating
// what the real OBS host does right after dlopen'ing a plugin -- handing it
// its own module handle -- before calling obs_module_load().
void obs_module_set_pointer(obs_module_t *module);
obs_module_t *obs_current_module(void);

#define OBS_DECLARE_MODULE()                                                                                           \
  static obs_module_t *obs_module_pointer;                                                                             \
  void obs_module_set_pointer(obs_module_t *module) { obs_module_pointer = module; }                                   \
  obs_module_t *obs_current_module(void) { return obs_module_pointer; }

typedef void (*obs_enum_module_callback_t)(void *param, obs_module_t *module);

const char *obs_get_module_file_name(obs_module_t *module);
const char *obs_get_module_binary_path(obs_module_t *module);
void obs_enum_modules(obs_enum_module_callback_t callback, void *param);

#endif /* GE_FAKE_OBS_MODULE_H */
