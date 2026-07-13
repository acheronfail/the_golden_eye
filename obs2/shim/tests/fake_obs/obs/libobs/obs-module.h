#ifndef GE_FAKE_OBS_MODULE_H
#define GE_FAKE_OBS_MODULE_H

// Minimal stand-in for real libobs' obs-module.h, declaring only the exact
// symbols shim/plugin.c uses, so test_plugin.c can link the real plugin.c
// against a controllable fake OBS registry (see fake_obs.c / fake_obs.h).

typedef struct obs_module obs_module_t;

#define MODULE_EXPORT

// Declared here (not only inside OBS_DECLARE_MODULE() below) so test code in
// another translation unit can call it, simulating the real OBS host handing a
// freshly dlopen'd plugin its own module handle before obs_module_load().
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
