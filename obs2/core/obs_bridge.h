#ifndef GE_OBS_BRIDGE_H
#define GE_OBS_BRIDGE_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#include <obs/libobs/obs.h>

void ge_obs_set_module(obs_module_t *module);
void ge_obs_collect_source_names(char *buffer, size_t buffer_size);
double ge_obs_video_fps(void);
bool ge_obs_module_data_path(char *buffer, size_t buffer_size);
bool ge_obs_module_binary_path(char *buffer, size_t buffer_size);
uint8_t *ge_obs_get_source_frame(const char *source_name, uint32_t *out_width, uint32_t *out_height);
void ge_obs_recording_start(void);
void ge_obs_recording_stop(void);

/* Whether the replay buffer is *enabled* in the active profile ("Enable Replay
 * Buffer" checkbox), distinct from running. Reads the profile config
 * (SimpleOutput.RecRB / AdvOut.RecRB); returns false if it can't be read. */
bool ge_obs_replay_buffer_enabled(void);

/* Whether OBS currently has a replay-buffer output object for the active
 * profile. Some output modes (simple lossless, advanced custom FFmpeg) leave
 * the checkbox enabled but the output NULL, so clips cannot be saved. */
bool ge_obs_replay_buffer_available(void);

/* Configured maximum replay-buffer duration in seconds (RecRBTime). Returns -1
 * if the active profile config cannot be read. */
int64_t ge_obs_replay_buffer_max_seconds(void);

/* Configured directory OBS writes replay-buffer files into (derived from the
 * record output path, falling back to the profile config). Returns false if no
 * path is available or the caller's buffer is too small. */
bool ge_obs_replay_buffer_output_directory(char *buffer, size_t buffer_size);

/* Push-model per-frame notifications: while registered, `cb(param, cx, cy)` runs
 * once per rendered frame on the graphics thread (may call ge_capture_get_frame
 * directly). Unregister serializes with invocation: on return no cb runs. */
typedef void (*ge_frame_cb)(void *param, uint32_t cx, uint32_t cy);
void ge_obs_register_frame_callback(ge_frame_cb cb, void *param);
void ge_obs_unregister_frame_callback(ge_frame_cb cb, void *param);

/* Opaque capture context holding the reusable GPU render/stage surfaces. A
 * caller that captures repeatedly (the monitor hot loop) creates one, reuses
 * it for every frame, then destroys it, avoiding per-frame surface churn. */
typedef struct ge_capture_ctx ge_capture_ctx;

/* Optional capture transform once the matcher has calibrated the 4:3 picture.
 * crop_* give a source sub-rectangle (fractions in [0,1]) rendered and scaled
 * per-axis to out_width x out_height (drops pillarbox, un-stretches; max_height ignored). */
struct ge_capture_region {
  float crop_x;
  float crop_y;
  float crop_w;
  float crop_h;
  uint32_t out_width;
  uint32_t out_height;
};

/* Create a capture context (reusable texrender); NULL on failure, release via
 * ge_capture_destroy. double_buffered avoids readback stalls (one frame latency)
 * but its first call (and post-resize) only primes: NULL means "no frame yet". */
ge_capture_ctx *ge_capture_create(bool double_buffered);

/* Render the named source into a freshly malloc'd BGRA buffer (caller frees).
 * A non-NULL region captures its sub-rectangle at out_*; else non-zero max_height
 * downscales a taller source (NULL/0 = native). NULL = not found or priming call. */
uint8_t *ge_capture_get_frame(ge_capture_ctx *ctx, const char *source_name, uint32_t max_height,
                              const struct ge_capture_region *region, uint32_t *out_width, uint32_t *out_height);

/* Destroy a capture context and its surfaces. */
void ge_capture_destroy(ge_capture_ctx *ctx);

#endif
