#ifndef GE_OBS_BRIDGE_H
#define GE_OBS_BRIDGE_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

void ge_obs_collect_source_names(char *buffer, size_t buffer_size);
uint8_t *ge_obs_get_source_frame(const char *source_name, uint32_t *out_width, uint32_t *out_height);
void ge_obs_recording_start(void);
void ge_obs_recording_stop(void);

/* Opaque capture context holding the reusable GPU render/stage surfaces. A
 * caller that captures repeatedly (the monitor hot loop) creates one, reuses
 * it for every frame, then destroys it, avoiding per-frame surface churn. */
typedef struct ge_capture_ctx ge_capture_ctx;

/* Create a capture context (allocating its reusable texrender). Returns NULL on
 * failure. Release it with ge_capture_destroy. */
ge_capture_ctx *ge_capture_create(void);

/* Render the named source into a freshly malloc'd BGRA buffer using the
 * context's reusable surfaces. Same ownership contract as
 * ge_obs_get_source_frame: the caller owns the returned buffer and must free
 * it. When max_height is non-zero and the source is taller, the frame is
 * downscaled on the GPU to max_height (preserving aspect ratio); pass 0 to
 * capture at native resolution. The captured dimensions are written to
 * out_width/out_height. The stagesurface is recreated automatically when those
 * dimensions change. Returns NULL if the source can't be found or rendered. */
uint8_t *ge_capture_get_frame(ge_capture_ctx *ctx, const char *source_name, uint32_t max_height, uint32_t *out_width,
                              uint32_t *out_height);

/* Destroy a capture context and its surfaces. */
void ge_capture_destroy(ge_capture_ctx *ctx);

#endif