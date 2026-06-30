#include "obs_bridge.h"

#include <obs/libobs/graphics/graphics.h>
#include <obs/libobs/obs-module.h>
#include <stdlib.h>
#include <string.h>

#include <obs/frontend/obs-frontend-api.h>

struct ge_source_names_ctx {
  char *buffer;
  size_t buffer_size;
  size_t current_pos;
};

static bool ge_collect_source_names_callback(void *data, obs_source_t *source) {
  struct ge_source_names_ctx *ctx = (struct ge_source_names_ctx *)data;

  /* Only collect sources we can render frames from -- those that produce video.
   * Skips audio-only sources (mics, desktop audio, etc.) which have no frames to
   * grab via ge_capture_get_frame. */
  if ((obs_source_get_output_flags(source) & OBS_SOURCE_VIDEO) == 0) {
    return true;
  }

  const char *name = obs_source_get_name(source);
  const char *id = obs_source_get_id(source);
  size_t name_len = strlen(name);
  size_t id_len = strlen(id);

  /* format: name '\t' id '\n' */
  if (ctx->current_pos + name_len + 1 + id_len + 1 < ctx->buffer_size) {
    memcpy(ctx->buffer + ctx->current_pos, name, name_len);
    ctx->current_pos += name_len;
    ctx->buffer[ctx->current_pos++] = '\t';
    memcpy(ctx->buffer + ctx->current_pos, id, id_len);
    ctx->current_pos += id_len;
    ctx->buffer[ctx->current_pos++] = '\n';
  }

  return true;
}

void ge_obs_collect_source_names(char *buffer, size_t buffer_size) {
  struct ge_source_names_ctx ctx = {
      .buffer = buffer,
      .buffer_size = buffer_size,
      .current_pos = 0,
  };

  obs_enum_sources(ge_collect_source_names_callback, &ctx);

  if (ctx.current_pos > 0 && buffer[ctx.current_pos - 1] == '\n') {
    buffer[ctx.current_pos - 1] = '\0';
  } else {
    buffer[ctx.current_pos] = '\0';
  }
}

/* Reusable GPU surfaces for repeated captures. Creating and destroying a
 * texrender + stagesurface on every frame churns GPU memory; a long-running
 * caller (the monitor's hot loop) instead holds one of these and reuses the
 * surfaces across frames. */
struct ge_capture_ctx {
  gs_texrender_t *texrender;
  /* Stage surface(s) the rendered texture is copied into for CPU readback. A
   * synchronous context uses one (index 0). A double-buffered context uses both:
   * it stages frame N into one surface while mapping frame N-1 back from the
   * other, so the GPU has a full frame interval to finish the copy and the map
   * never stalls the graphics thread -- at the cost of one frame of latency. */
  gs_stagesurf_t *stagesurfaces[2];
  /* Dimensions the stagesurfaces were created for. They're bound to a
   * resolution, so they're recreated when the source size changes; the texrender
   * is resolution-agnostic (reset before each render) and reused. */
  uint32_t width;
  uint32_t height;
  /* When true, double-buffer the readback (see stagesurfaces). Set at creation. */
  bool double_buffered;
  /* Double-buffered state: which surface to stage into on the next call, and
   * whether a previously staged frame is ready to map. Unused when synchronous. */
  int stage_index;
  bool primed;
};

ge_capture_ctx *ge_capture_create(bool double_buffered) {
  struct ge_capture_ctx *ctx = (struct ge_capture_ctx *)calloc(1, sizeof(*ctx));
  if (!ctx) {
    return NULL;
  }
  ctx->double_buffered = double_buffered;

  /* The texrender is independent of resolution, so create it once up front.
   * The stagesurfaces are created lazily on the first frame (and whenever the
   * source resolution changes) since they must match the frame dimensions. */
  obs_enter_graphics();
  ctx->texrender = gs_texrender_create(GS_BGRA, GS_ZS_NONE);
  obs_leave_graphics();

  if (!ctx->texrender) {
    free(ctx);
    return NULL;
  }

  return ctx;
}

void ge_capture_destroy(ge_capture_ctx *ctx) {
  if (!ctx) {
    return;
  }

  obs_enter_graphics();
  for (int i = 0; i < 2; i++) {
    if (ctx->stagesurfaces[i]) {
      gs_stagesurface_destroy(ctx->stagesurfaces[i]);
    }
  }
  if (ctx->texrender) {
    gs_texrender_destroy(ctx->texrender);
  }
  obs_leave_graphics();

  free(ctx);
}

uint8_t *ge_capture_get_frame(ge_capture_ctx *ctx, const char *source_name, uint32_t max_height,
                              const struct ge_capture_region *region, uint32_t *out_width, uint32_t *out_height) {
  if (!ctx || !source_name) {
    return NULL;
  }

  // Looked up fresh each frame so the capture transparently follows the named
  // source being (re)created or removed. Returns a new ref we must release.
  obs_source_t *source = obs_get_source_by_name(source_name);
  if (!source) {
    return NULL;
  }

  uint32_t src_width = obs_source_get_width(source);
  uint32_t src_height = obs_source_get_height(source);

  if (src_width == 0 || src_height == 0) {
    obs_source_release(source);
    return NULL;
  }

  // Source rectangle to project onto the render target. Defaults to the whole
  // source; a calibrated region narrows it to the 4:3 picture.
  float ortho_x0 = 0.0f;
  float ortho_y0 = 0.0f;
  float ortho_x1 = (float)src_width;
  float ortho_y1 = (float)src_height;

  uint32_t width;
  uint32_t height;
  if (region && region->out_width != 0 && region->out_height != 0) {
    // Calibrated capture: render only the source sub-rectangle holding the 4:3
    // picture into the exact output size, so the GPU crops any pillarbox bars
    // and undoes the stretch in one pass. Clamp the (possibly stale) fractions
    // to the unit square so a bad calibration can never sample outside source.
    float cx = region->crop_x < 0.0f ? 0.0f : region->crop_x;
    float cy = region->crop_y < 0.0f ? 0.0f : region->crop_y;
    float cw = region->crop_w <= 0.0f ? 1.0f : region->crop_w;
    float ch = region->crop_h <= 0.0f ? 1.0f : region->crop_h;
    if (cx + cw > 1.0f) {
      cw = 1.0f - cx;
    }
    if (cy + ch > 1.0f) {
      ch = 1.0f - cy;
    }
    ortho_x0 = cx * (float)src_width;
    ortho_y0 = cy * (float)src_height;
    ortho_x1 = (cx + cw) * (float)src_width;
    ortho_y1 = (cy + ch) * (float)src_height;
    width = region->out_width;
    height = region->out_height;
  } else {
    // The render target downscales to max_height (preserving aspect ratio) when
    // the source is taller, so a 1080p (or larger) upscaled feed is captured as
    // a cheap ~480p frame: far less data to map back and far less work for
    // OpenCV. max_height == 0, or a source already no taller, captures at native
    // size.
    width = src_width;
    height = src_height;
    if (max_height != 0 && src_height > max_height) {
      // Round to nearest to minimise aspect drift; clamp to at least 1px wide.
      width = (uint32_t)(((uint64_t)src_width * max_height + src_height / 2) / src_height);
      if (width == 0) {
        width = 1;
      }
      height = max_height;
    }
  }

  *out_width = width;
  *out_height = height;

  size_t buffer_size = width * height * 4;
  uint8_t *pixel_buffer = (uint8_t *)malloc(buffer_size);
  if (!pixel_buffer) {
    obs_source_release(source);
    return NULL;
  }

  obs_enter_graphics();

  int surface_count = ctx->double_buffered ? 2 : 1;

  /* (Re)create the stage surface(s) when the source resolution changes (incl.
   * the first frame). On a double-buffered context this also discards any
   * previously staged frame -- it was a different size -- so the pipeline
   * re-primes. Resetting width/height to 0 on failure forces a retry on the next
   * frame rather than leaving a stale size cached. */
  if (ctx->width != width || ctx->height != height || !ctx->stagesurfaces[0]) {
    for (int i = 0; i < 2; i++) {
      if (ctx->stagesurfaces[i]) {
        gs_stagesurface_destroy(ctx->stagesurfaces[i]);
        ctx->stagesurfaces[i] = NULL;
      }
    }
    bool ok = true;
    for (int i = 0; i < surface_count; i++) {
      ctx->stagesurfaces[i] = gs_stagesurface_create(width, height, GS_BGRA);
      if (!ctx->stagesurfaces[i]) {
        ok = false;
        break;
      }
    }
    ctx->width = ok ? width : 0;
    ctx->height = ok ? height : 0;
    ctx->stage_index = 0;
    ctx->primed = false;
  }

  /* Surface we stage this frame into; the synchronous path always uses [0]. */
  int cur = ctx->double_buffered ? ctx->stage_index : 0;

  gs_texrender_reset(ctx->texrender);
  if (ctx->stagesurfaces[cur] && gs_texrender_begin(ctx->texrender, width, height)) {
    struct vec4 background;
    vec4_zero(&background);
    gs_clear(GS_CLEAR_COLOR, &background, 0.0f, 0);
    // Project the selected source rectangle (the whole source, or a calibrated
    // sub-rectangle) onto the render target; the GPU scales it to the target
    // size as it rasterizes, cropping and un-stretching in the same pass.
    gs_ortho(ortho_x0, ortho_x1, ortho_y0, ortho_y1, -100.0f, 100.0f);
    gs_blend_state_push();
    gs_blend_function(GS_BLEND_ONE, GS_BLEND_ZERO);
    obs_source_video_render(source);
    gs_blend_state_pop();
    gs_texrender_end(ctx->texrender);

    /* Issue the GPU->staging copy for this frame. */
    gs_stage_texture(ctx->stagesurfaces[cur], gs_texrender_get_texture(ctx->texrender));

    /* Choose which staged frame to read back. Synchronous: map the frame we just
     * staged (the map stalls until the GPU finishes, but there's no latency).
     * Double-buffered: map the frame staged on the *previous* call from the other
     * surface, which the GPU has had a full frame interval to complete, so the
     * map doesn't stall -- except the first call after (re)priming has no
     * previous frame, so it only stages and returns no frame this tick. */
    gs_stagesurf_t *map_surface = NULL;
    if (ctx->double_buffered) {
      if (ctx->primed) {
        map_surface = ctx->stagesurfaces[cur ^ 1];
      } else {
        ctx->primed = true; /* this frame becomes the "previous" for the next call */
      }
      ctx->stage_index = cur ^ 1; /* stage into the other surface next call */
    } else {
      map_surface = ctx->stagesurfaces[cur];
    }

    if (map_surface) {
      uint8_t *mapped_data;
      uint32_t linesize;
      if (gs_stagesurface_map(map_surface, &mapped_data, &linesize)) {
        for (uint32_t y = 0; y < height; y++) {
          memcpy(pixel_buffer + y * width * 4, mapped_data + y * linesize, width * 4);
        }
        gs_stagesurface_unmap(map_surface);
      } else {
        free(pixel_buffer);
        pixel_buffer = NULL;
      }
    } else {
      /* Priming frame of a double-buffered context: nothing to return yet. */
      free(pixel_buffer);
      pixel_buffer = NULL;
    }
  } else {
    free(pixel_buffer);
    pixel_buffer = NULL;
  }

  obs_leave_graphics();
  obs_source_release(source);
  return pixel_buffer;
}

void ge_obs_register_frame_callback(ge_frame_cb cb, void *param) {
  /* OBS invokes draw callbacks once per rendered frame, on the graphics thread
   * inside an active graphics context (see render_main_texture in obs-video.c). */
  obs_add_main_render_callback(cb, param);
}

void ge_obs_unregister_frame_callback(ge_frame_cb cb, void *param) {
  obs_remove_main_render_callback(cb, param);
}

uint8_t *ge_obs_get_source_frame(const char *source_name, uint32_t *out_width, uint32_t *out_height) {
  /* One-shot callers (the /screenshot and /match routes) capture a single
   * frame, so spin up a throwaway context. They capture at native resolution
   * (max_height 0) -- screenshots feed template authoring and want full detail.
   * The monitor's hot loop instead holds a context across frames via the
   * ge_capture_* API directly, and downscales for speed. A one-shot capture has
   * no "next frame" to map back, so it uses a synchronous (not double-buffered)
   * context: a single get_frame call returns the frame directly. */
  ge_capture_ctx *ctx = ge_capture_create(false);
  if (!ctx) {
    return NULL;
  }

  uint8_t *frame = ge_capture_get_frame(ctx, source_name, 0, NULL, out_width, out_height);
  ge_capture_destroy(ctx);
  return frame;
}
