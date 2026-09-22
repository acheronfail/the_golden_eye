#ifndef GE_FAKE_OBS_CONTROL_H
#define GE_FAKE_OBS_CONTROL_H

// Test-only control surface for the fake OBS module registry backing
// obs_enum_modules/obs_get_module_file_name/obs_get_module_binary_path.
// plugin.c never sees this header -- it only calls the public obs_* functions.

#include "fake_obs/obs/libobs/obs-module.h"

// Empties the registry. Call before each scenario that needs a clean slate.
void fake_obs_reset(void);

// Registers a fake loaded module reporting `file_name`/`binary_path`. Returns
// its handle (pass to obs_module_set_pointer so obs_current_module() returns it,
// as the real OBS host does after loading a plugin). NULL if the registry is full.
obs_module_t *fake_obs_register_module(const char *file_name, const char *binary_path);

#endif /* GE_FAKE_OBS_CONTROL_H */
