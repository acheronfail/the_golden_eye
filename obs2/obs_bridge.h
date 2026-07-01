#ifndef GE_OBS_BRIDGE_H
#define GE_OBS_BRIDGE_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

void ge_obs_collect_source_names(char *buffer, size_t buffer_size);
uint8_t *ge_obs_get_source_frame(const char *source_name, uint32_t *out_width, uint32_t *out_height);
void ge_obs_recording_start(void);
void ge_obs_recording_stop(void);

/* Whether the replay buffer is *enabled* in the active profile's output
 * settings (the "Enable Replay Buffer" checkbox). This is distinct from whether
 * it is currently running -- a disabled replay buffer can never be started, so
 * the frontend checks this before letting the user begin a session. Reads the
 * profile config (SimpleOutput.RecRB / AdvOut.RecRB depending on the output
 * mode); returns false if the config can't be read. */
bool ge_obs_replay_buffer_enabled(void);

/* Whether OBS currently has a replay-buffer output object for the active
 * profile. OBS can leave the checkbox enabled while making replay buffer
 * unavailable for some output modes (for example simple lossless recording, or
 * advanced custom FFmpeg output); in those cases the frontend output pointer is
 * NULL and starting/saving replay buffer clips cannot work. */
bool ge_obs_replay_buffer_available(void);

/* Configured maximum replay-buffer duration in seconds (RecRBTime). Returns -1
 * if the active profile config cannot be read. */
int64_t ge_obs_replay_buffer_max_seconds(void);

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
 * failure. Release it with ge_capture_destroy.
 *
 * When double_buffered is true, the context stages each frame into one surface
 * while mapping the previous frame from another, so the GPU readback never
 * stalls the graphics thread (at the cost of one frame of latency). The first
 * ge_capture_get_frame call after creation (and after any resolution change)
 * then only primes the pipeline and returns NULL even on success -- callers must
 * treat that as "no frame yet" and try again, not as an error. A synchronous
 * (false) context maps the frame it just staged, so every successful call
 * returns a frame; use it for one-shot captures. */
ge_capture_ctx *ge_capture_create(bool double_buffered);

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
 * The captured dimensions are written to out_width/out_height. The stage
 * surface(s) are recreated automatically when those dimensions change. Returns
 * NULL if the source can't be found or rendered -- and also, for a
 * double-buffered context, on the priming call after creation/resize (see
 * ge_capture_create); treat that NULL as "no frame yet". */
uint8_t *ge_capture_get_frame(ge_capture_ctx *ctx, const char *source_name, uint32_t max_height,
                              const struct ge_capture_region *region, uint32_t *out_width, uint32_t *out_height);

/* Destroy a capture context and its surfaces. */
void ge_capture_destroy(ge_capture_ctx *ctx);

#endif
