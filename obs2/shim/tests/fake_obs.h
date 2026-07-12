#ifndef GE_FAKE_OBS_CONTROL_H
#define GE_FAKE_OBS_CONTROL_H

// Test-only control surface for the fake OBS module registry backing
// obs_enum_modules/obs_get_module_file_name/obs_get_module_binary_path (see
// fake_obs.c and fake_obs/obs/libobs/obs-module.h). plugin.c never sees
// this header -- it only calls the public obs_* functions declared there,
// exactly as it would against the real OBS SDK.

#include "fake_obs/obs/libobs/obs-module.h"

// Empties the registry. Call before each scenario that needs a clean slate.
void fake_obs_reset(void);

// Registers a fake loaded module reporting `file_name`/`binary_path` for
// obs_get_module_file_name/obs_get_module_binary_path. Returns its handle
// (pass it to obs_module_set_pointer to make obs_current_module() return
// it, simulating what the real OBS host does right after loading a
// plugin). Returns NULL if the registry is full.
obs_module_t *fake_obs_register_module(const char *file_name, const char *binary_path);

#endif /* GE_FAKE_OBS_CONTROL_H */
