#include <obs/libobs/obs-module.h>

OBS_DECLARE_MODULE()

bool zig_obs_module_load(void);
bool obs_module_load(void) { return zig_obs_module_load(); }

void zig_obs_module_unload(void);
void obs_module_unload(void) { zig_obs_module_unload(); }
