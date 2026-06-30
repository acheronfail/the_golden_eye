// The "core" plugin: all of the heavy logic (the Rust staticlib, OpenCV, the
// HTTP server, the OBS bridge). It is NOT loaded by OBS directly — instead the
// thin shim in `plugin.c` registers itself as the OBS module and `dlopen`s this
// library, calling `ge_core_load`/`ge_core_unload`. Splitting it out this way
// lets the shim unload + reload this library at runtime (see `plugin.c`) so the
// Rust code can be rebuilt without restarting OBS.

#include "ge_rust.h"

#include <obs/frontend/obs-frontend-api.h>
#include <obs/libobs/obs-data.h>
#include <obs/libobs/obs-service.h>
#include <obs/libobs/util/bmem.h>
#include <string.h>

// Make the entry points visible to dlsym even if the library is built with
// -fvisibility=hidden.
#define GE_EXPORT __attribute__((visibility("default")))

static void ge_on_frontend_event(enum obs_frontend_event event, void *private_data) {
  (void)private_data;

  if (event == OBS_FRONTEND_EVENT_STREAMING_STARTED) {
    obs_service_t *service = obs_frontend_get_streaming_service();
    if (service) {
      obs_data_t *settings = obs_service_get_settings(service);
      if (settings) {
        const char *service_name = obs_data_get_string(settings, "service");
        if (service_name && strcasestr(service_name, "youtube") != NULL) {
          const char *settings_json = obs_data_get_json_pretty(settings);
          ge_stream_notifier_start(settings_json ? settings_json : "{}");
        }
        obs_data_release(settings);
      }
    }
  } else if (event == OBS_FRONTEND_EVENT_STREAMING_STOPPED) {
    ge_stream_notifier_stop();
  } else if (event == OBS_FRONTEND_EVENT_REPLAY_BUFFER_SAVED) {
    // The replay buffer finished writing a file: hand its path to Rust, which
    // wakes whichever save is waiting on it (no polling). obs_frontend_get_last_replay
    // returns a bstr we own and must bfree.
    char *path = obs_frontend_get_last_replay();
    ge_replay_buffer_saved(path);
    if (path) {
      bfree(path);
    }
  }
}

// Called by the shim once this library has been dlopen'd. Mirrors what the old
// monolithic `obs_module_load` did. Returns false on failure so the shim can
// log a useful error and refuse to come up.
GE_EXPORT bool ge_core_load(void) {
  ge_rust_start();
  obs_frontend_add_event_callback(ge_on_frontend_event, NULL);
  return true;
}

// Called by the shim before it dlcloses this library (on OBS shutdown, or
// before a dev-mode hot reload). `ge_rust_stop` blocks until the tokio runtime
// is fully torn down, so no Rust threads survive the dlclose that follows.
GE_EXPORT void ge_core_unload(void) {
  obs_frontend_remove_event_callback(ge_on_frontend_event, NULL);
  ge_rust_stop();
}
