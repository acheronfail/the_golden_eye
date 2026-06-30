#ifndef GE_OBS_BRIDGE_H
#define GE_OBS_BRIDGE_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

void ge_obs_collect_source_names(char *buffer, size_t buffer_size);
uint8_t *ge_obs_get_source_frame(const char *source_name, uint32_t *out_width, uint32_t *out_height);
void ge_obs_recording_start(void);
void ge_obs_recording_stop(void);

/* Push-model per-frame notifications. While registered, `cb(param, cx, cy)` is
 * invoked once per rendered frame on the OBS graphics thread, inside an active
 * graphics context -- so the callback may capture frames via ge_capture_get_frame
 * directly (its nested obs_enter_graphics is a no-op ref-bump on this thread, not
 * a re-lock). `cx`/`cy` are the main canvas dimensions.
 *
 * Unregister before tearing down anything `param` points at:
 * obs_remove_main_render_callback serializes with callback invocation, so once
 * ge_obs_unregister_frame_callback returns no callback is running or will start. */
typedef void (*ge_frame_cb)(void *param, uint32_t cx, uint32_t cy);
void ge_obs_register_frame_callback(ge_frame_cb cb, void *param);
void ge_obs_unregister_frame_callback(ge_frame_cb cb, void *param);

/* Opaque capture context holding the reusable GPU render/stage surfaces. A
 * caller that captures repeatedly (the monitor hot loop) creates one, reuses
 * it for every frame, then destroys it, avoiding per-frame surface churn. */
typedef struct ge_capture_ctx ge_capture_ctx;

/* Optional capture transform, supplied once the matcher has calibrated the
 * source's true 4:3 picture. crop_x/crop_y/crop_w/crop_h give a sub-rectangle
 * of the source as fractions in [0, 1]; only that rectangle is rendered, scaled
 * independently per axis to fill out_width x out_height. This drops pillarbox
 * bars and undoes an anamorphic (widescreen) stretch in a single GPU pass, so
 * the returned frame is already normalized to 4:3 -- the matcher matches it
 * directly with no CPU resize, and the bar pixels are never mapped back. When a
 * region is supplied, max_height is ignored (out_width/out_height fix the size).
 */
struct ge_capture_region {
  float crop_x;
  float crop_y;
  float crop_w;
  float crop_h;
  uint32_t out_width;
  uint32_t out_height;
};

/* Create a capture context (allocating its reusable texrender). Returns NULL on
 * failure. Release it with ge_capture_destroy. */
ge_capture_ctx *ge_capture_create(void);

/* Render the named source into a freshly malloc'd BGRA buffer using the
 * context's reusable surfaces. Same ownership contract as
 * ge_obs_get_source_frame: the caller owns the returned buffer and must free
 * it.
 *
 * When region is non-NULL, only its source sub-rectangle is captured, resized
 * to region->out_width x region->out_height (max_height is ignored). When region
 * is NULL and max_height is non-zero and the source is taller, the frame is
 * downscaled on the GPU to max_height (preserving aspect ratio); pass NULL and 0
 * to capture the whole source at native resolution.
 *
 * The captured dimensions are written to out_width/out_height. The stagesurface
 * is recreated automatically when those dimensions change. Returns NULL if the
 * source can't be found or rendered. */
uint8_t *ge_capture_get_frame(ge_capture_ctx *ctx, const char *source_name, uint32_t max_height,
                              const struct ge_capture_region *region, uint32_t *out_width, uint32_t *out_height);

/* Destroy a capture context and its surfaces. */
void ge_capture_destroy(ge_capture_ctx *ctx);

#endif