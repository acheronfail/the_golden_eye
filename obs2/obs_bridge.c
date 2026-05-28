#include "obs_bridge.h"

#include "logger.h"

#include <obs/libobs/obs-module.h>
#include <obs/libobs/graphics/graphics.h>
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

  ge_log_info("Found source -> Name: '%s', ID: '%s'", name, id);

  if (ctx->current_pos + name_len + 1 < ctx->buffer_size) {
    memcpy(ctx->buffer + ctx->current_pos, name, name_len);
    ctx->current_pos += name_len;
    ctx->buffer[ctx->current_pos++] = '\n';
  }

  return true;
}

struct ge_find_source_by_name_ctx {
  const char *target_name;
  obs_source_t *found;
};

static bool ge_find_source_by_name_proc(void *data, obs_source_t *source) {
  struct ge_find_source_by_name_ctx *ctx = (struct ge_find_source_by_name_ctx *)data;
  const char *name = obs_source_get_name(source);
  if (strcmp(name, ctx->target_name) == 0) {
    ctx->found = obs_source_get_ref(source);
    return false;
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

uint8_t *ge_obs_get_source_frame(const char *source_name, uint32_t *out_width,
                                 uint32_t *out_height) {
  obs_source_t *source = NULL;

  if (!source_name) {
    return NULL;
  }

  struct ge_find_source_by_name_ctx ctx = {
      .target_name = source_name,
      .found = NULL,
  };
  obs_enum_sources(ge_find_source_by_name_proc, &ctx);
  source = ctx.found;

  if (!source) {
    return NULL;
  }

  uint32_t width = obs_source_get_width(source);
  uint32_t height = obs_source_get_height(source);

  if (width == 0 || height == 0) {
    obs_source_release(source);
    return NULL;
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

  gs_texrender_t *texrender = gs_texrender_create(GS_BGRA, GS_ZS_NONE);
  gs_stagesurf_t *stagesurface = gs_stagesurface_create(width, height, GS_BGRA);

  gs_texrender_reset(texrender);
  if (gs_texrender_begin(texrender, width, height)) {
    struct vec4 background;
    vec4_zero(&background);
    gs_clear(GS_CLEAR_COLOR, &background, 0.0f, 0);
    gs_ortho(0.0f, (float)width, 0.0f, (float)height, -100.0f, 100.0f);
    gs_blend_state_push();
    gs_blend_function(GS_BLEND_ONE, GS_BLEND_ZERO);
    obs_source_video_render(source);
    gs_blend_state_pop();
    gs_texrender_end(texrender);

    gs_stage_texture(stagesurface, gs_texrender_get_texture(texrender));

    uint8_t *mapped_data;
    uint32_t linesize;
    if (gs_stagesurface_map(stagesurface, &mapped_data, &linesize)) {
      for (uint32_t y = 0; y < height; y++) {
        memcpy(pixel_buffer + y * width * 4, mapped_data + y * linesize,
               width * 4);
      }
      gs_stagesurface_unmap(stagesurface);
    } else {
      free(pixel_buffer);
      pixel_buffer = NULL;
    }
  } else {
    free(pixel_buffer);
    pixel_buffer = NULL;
  }

  gs_stagesurface_destroy(stagesurface);
  gs_texrender_destroy(texrender);

  obs_leave_graphics();
  obs_source_release(source);
  return pixel_buffer;
}

void ge_obs_recording_start(void) {
  obs_frontend_recording_start();
  ge_log_info("Recording started");
}

void ge_obs_recording_stop(void) {
  obs_frontend_recording_stop();
  ge_log_info("Recording stopped");
}