#include "http.h"
#include "logger.h"
#include "stream_notifier.h"
#include <obs/frontend/obs-frontend-api.h>
#include <obs/libobs/obs-data.h>
#include <obs/libobs/obs-module.h>
#include <obs/libobs/obs-service.h>
#include <stdlib.h>
#include <string.h>

OBS_DECLARE_MODULE()

static void ge_on_frontend_event(enum obs_frontend_event event, void *private_data) {
  (void)private_data;

  if (event == OBS_FRONTEND_EVENT_STREAMING_STARTED) {
    obs_service_t *service = obs_frontend_get_streaming_service();
    if (service) {
      obs_data_t *settings = obs_service_get_settings(service);
      if (settings) {
        const char *service_name = obs_data_get_string(settings, "service");
        if (service_name && strcasestr(service_name, "youtube") != NULL) {
          ge_log_info("YouTube stream started. Activating stream notifier...");
          ge_stream_notifier_start();
        }
        obs_data_release(settings);
      }
      obs_service_release(service);
    }
  }
}

bool obs_module_load(void) {
  if (!ge_http_server_start()) {
    return false;
  }

  obs_frontend_add_event_callback(ge_on_frontend_event, NULL);
  return true;
}

void obs_module_unload(void) {
  obs_frontend_remove_event_callback(ge_on_frontend_event, NULL);
  ge_http_server_stop();
}
