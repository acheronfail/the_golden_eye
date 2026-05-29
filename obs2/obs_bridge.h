#ifndef GE_OBS_BRIDGE_H
#define GE_OBS_BRIDGE_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

void ge_obs_collect_source_names(char *buffer, size_t buffer_size);
uint8_t *ge_obs_get_source_frame(const char *source_name, uint32_t *out_width, uint32_t *out_height);
void ge_obs_recording_start(void);
void ge_obs_recording_stop(void);

#endif