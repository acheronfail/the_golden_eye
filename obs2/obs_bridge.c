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
  gs_stagesurf_t *stagesurface;
  /* Dimensions the stagesurface was created for. The stagesurface is bound to
   * a resolution, so it's recreated when the source size changes; the
   * texrender is resolution-agnostic (reset before each render) and reused. */
  uint32_t width;
  uint32_t height;
};

ge_capture_ctx *ge_capture_create(void) {
  struct ge_capture_ctx *ctx = (struct ge_capture_ctx *)calloc(1, sizeof(*ctx));
  if (!ctx) {
    return NULL;
  }

  /* The texrender is independent of resolution, so create it once up front.
   * The stagesurface is created lazily on the first frame (and whenever the
   * source resolution changes) since it must match the frame dimensions. */
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
  if (ctx->stagesurface) {
    gs_stagesurface_destroy(ctx->stagesurface);
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

  /* (Re)create the stagesurface when the source resolution changes (including
   * the first frame). Resetting width/height to 0 on failure forces a retry on
   * the next frame rather than leaving a stale size cached. */
  if (!ctx->stagesurface || ctx->width != width || ctx->height != height) {
    if (ctx->stagesurface) {
      gs_stagesurface_destroy(ctx->stagesurface);
    }
    ctx->stagesurface = gs_stagesurface_create(width, height, GS_BGRA);
    ctx->width = ctx->stagesurface ? width : 0;
    ctx->height = ctx->stagesurface ? height : 0;
  }

  gs_texrender_reset(ctx->texrender);
  if (ctx->stagesurface && gs_texrender_begin(ctx->texrender, width, height)) {
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

    gs_stage_texture(ctx->stagesurface, gs_texrender_get_texture(ctx->texrender));

    uint8_t *mapped_data;
    uint32_t linesize;
    if (gs_stagesurface_map(ctx->stagesurface, &mapped_data, &linesize)) {
      for (uint32_t y = 0; y < height; y++) {
        memcpy(pixel_buffer + y * width * 4, mapped_data + y * linesize, width * 4);
      }
      gs_stagesurface_unmap(ctx->stagesurface);
    } else {
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

uint8_t *ge_obs_get_source_frame(const char *source_name, uint32_t *out_width, uint32_t *out_height) {
  /* One-shot callers (the /screenshot and /match routes) capture a single
   * frame, so spin up a throwaway context. They capture at native resolution
   * (max_height 0) -- screenshots feed template authoring and want full detail.
   * The monitor's hot loop instead holds a context across frames via the
   * ge_capture_* API directly, and downscales for speed. */
  ge_capture_ctx *ctx = ge_capture_create();
  if (!ctx) {
    return NULL;
  }

  uint8_t *frame = ge_capture_get_frame(ctx, source_name, 0, NULL, out_width, out_height);
  ge_capture_destroy(ctx);
  return frame;
}
