#include "http.h"

#include "bmp.h"
#include "logger.h"
#include "obs_bridge.h"
#include "vendor/mongoose.h"

#include <obs/libobs/obs-module.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <threads.h>

static thrd_t ge_server_thread;
static struct mg_mgr ge_mgr;
static int ge_server_running = 0;

static void ge_mongoose_log_callback(char c, void *param) {
  static char ge_log_buffer[1024];
  static int ge_log_idx = 0;

  (void)param;

  if (c == '\n' || ge_log_idx >= (int)sizeof(ge_log_buffer) - 1) {
    if (ge_log_idx > 0) {
      ge_log_buffer[ge_log_idx] = '\0';
      ge_log_info("%s", ge_log_buffer);
      ge_log_idx = 0;
    }
  } else if (c != '\r') {
    ge_log_buffer[ge_log_idx++] = c;
  }
}

static void ge_handle_http(struct mg_connection *c, int ev, void *ev_data) {
  if (ev != MG_EV_HTTP_MSG) {
    return;
  }

  struct mg_http_message *hm = (struct mg_http_message *)ev_data;

  if (mg_strcmp(hm->uri, mg_str("/api/hello")) == 0 && mg_strcmp(hm->method, mg_str("POST")) == 0) {
    char names_buffer[4096];
    ge_obs_collect_source_names(names_buffer, sizeof(names_buffer));
    mg_http_reply(c, 200, "Content-Type: text/plain\r\n", "%s\n", names_buffer);
    return;
  }

  if (mg_strcmp(hm->uri, mg_str("/api/screenshot")) == 0 && mg_strcmp(hm->method, mg_str("GET")) == 0) {
    char source_name[256] = {0};
    mg_http_get_var(&hm->query, "source", source_name, sizeof(source_name));

    uint32_t width = 0;
    uint32_t height = 0;
    uint8_t *raw_pixels = ge_obs_get_source_frame(source_name[0] != '\0' ? source_name : NULL, &width, &height);

    if (raw_pixels) {
      size_t bmp_size = 0;
      uint8_t *bmp_data = ge_encode_bmp(raw_pixels, width, height, &bmp_size);
      free(raw_pixels);

      if (bmp_data) {
        mg_printf(c,
                  "HTTP/1.1 200 OK\r\nContent-Type: "
                  "image/bmp\r\nContent-Length: %lu\r\n\r\n",
                  (unsigned long)bmp_size);
        mg_send(c, bmp_data, bmp_size);
        free(bmp_data);
      } else {
        mg_http_reply(c, 500, "Content-Type: text/plain\r\n", "Error: Failed to encode BMP.\n");
      }
    } else {
      mg_http_reply(c, 500, "Content-Type: text/plain\r\n", "Error: No active video source found.\n");
    }
    return;
  }

  if (mg_strcmp(hm->uri, mg_str("/api/record/start")) == 0 && mg_strcmp(hm->method, mg_str("POST")) == 0) {
    ge_obs_recording_start();
    mg_http_reply(c, 200, "Content-Type: application/json\r\n", "{\"status\": \"recording started\"}\n");
    return;
  }

  if (mg_strcmp(hm->uri, mg_str("/api/record/stop")) == 0 && mg_strcmp(hm->method, mg_str("POST")) == 0) {
    ge_obs_recording_stop();
    mg_http_reply(c, 200, "Content-Type: application/json\r\n", "{\"status\": \"recording stopped\"}\n");
    return;
  }

  mg_http_reply(c, 404, "Content-Type: text/plain\r\n", "Not Found\n");
}

static int ge_webserver_worker(void *arg) {
  (void)arg;

  mg_log_set_fn(ge_mongoose_log_callback, NULL);
  mg_mgr_init(&ge_mgr);
  mg_http_listen(&ge_mgr, "http://0.0.0.0:8080", ge_handle_http, NULL);

  ge_log_info("Listening on http://localhost:8080");

  while (ge_server_running) {
    mg_mgr_poll(&ge_mgr, 100);
  }

  mg_mgr_free(&ge_mgr);
  ge_log_info("Stopped.");
  return 0;
}

bool ge_http_server_start(void) {
  ge_server_running = 1;
  if (thrd_create(&ge_server_thread, ge_webserver_worker, NULL) != thrd_success) {
    ge_server_running = 0;
    ge_log_error("Failed to create server thread!");
    return false;
  }

  return true;
}

void ge_http_server_stop(void) {
  if (!ge_server_running) {
    return;
  }

  ge_server_running = 0;
  thrd_join(ge_server_thread, NULL);
}