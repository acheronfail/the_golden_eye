// The "core" plugin: all of the heavy logic (the Rust staticlib, OpenCV, the
// HTTP server, the OBS bridge). It is NOT loaded by OBS directly — instead the
// thin shim in `plugin.c` registers itself as the OBS module and `dlopen`s this
// library, calling `ge_core_load`/`ge_core_unload`. Splitting it out this way
// lets the shim unload + reload this library at runtime (see `plugin.c`) so the
// Rust code can be rebuilt without restarting OBS.

#include "ge_rust.h"

#include <ctype.h>
#include <obs/frontend/obs-frontend-api.h>
#include <obs/libobs/callback/signal.h>
#include <obs/libobs/obs-data.h>
#include <obs/libobs/obs-service.h>
#include <obs/libobs/obs.h>
#include <obs/libobs/util/bmem.h>
#include <string.h>

void ge_obs_set_module(obs_module_t *module);

// Make the entry points visible to dlsym even if the library is built with
// -fvisibility=hidden.
#ifdef _WIN32
#define GE_EXPORT __declspec(dllexport)
#else
#define GE_EXPORT __attribute__((visibility("default")))
#endif

static bool ge_ascii_contains_case(const char *haystack, const char *needle) {
  if (!haystack || !needle || !*needle) {
    return false;
  }

  size_t needle_len = strlen(needle);
  for (const char *p = haystack; *p; p++) {
    size_t i = 0;
    while (i < needle_len && p[i] && tolower((unsigned char)p[i]) == tolower((unsigned char)needle[i])) {
      i++;
    }
    if (i == needle_len) {
      return true;
    }
  }
  return false;
}

static void ge_on_source_changed(void *private_data, calldata_t *calldata) {
  (void)private_data;
  (void)calldata;
  ge_sources_changed();
}

static void ge_connect_source_signals(void) {
  signal_handler_t *signals = obs_get_signal_handler();
  if (!signals) {
    return;
  }

  signal_handler_connect(signals, "source_create", ge_on_source_changed, NULL);
  signal_handler_connect(signals, "source_create_canvas", ge_on_source_changed, NULL);
  signal_handler_connect(signals, "source_destroy", ge_on_source_changed, NULL);
  signal_handler_connect(signals, "source_remove", ge_on_source_changed, NULL);
  signal_handler_connect(signals, "source_update", ge_on_source_changed, NULL);
  signal_handler_connect(signals, "source_rename", ge_on_source_changed, NULL);
}

static void ge_disconnect_source_signals(void) {
  signal_handler_t *signals = obs_get_signal_handler();
  if (!signals) {
    return;
  }

  signal_handler_disconnect(signals, "source_create", ge_on_source_changed, NULL);
  signal_handler_disconnect(signals, "source_create_canvas", ge_on_source_changed, NULL);
  signal_handler_disconnect(signals, "source_destroy", ge_on_source_changed, NULL);
  signal_handler_disconnect(signals, "source_remove", ge_on_source_changed, NULL);
  signal_handler_disconnect(signals, "source_update", ge_on_source_changed, NULL);
  signal_handler_disconnect(signals, "source_rename", ge_on_source_changed, NULL);
}

static void ge_on_frontend_event(enum obs_frontend_event event, void *private_data) {
  (void)private_data;

  if (event == OBS_FRONTEND_EVENT_STREAMING_STARTED) {
    obs_service_t *service = obs_frontend_get_streaming_service();
    if (service) {
      obs_data_t *settings = obs_service_get_settings(service);
      if (settings) {
        const char *service_name = obs_data_get_string(settings, "service");
        if (ge_ascii_contains_case(service_name, "youtube")) {
          const char *settings_json = obs_data_get_json_pretty(settings);
          ge_stream_notifier_start(settings_json ? settings_json : "{}");
        }
        obs_data_release(settings);
      }
    }
  } else if (event == OBS_FRONTEND_EVENT_STREAMING_STOPPED) {
    ge_stream_notifier_stop();
  } else if (event == OBS_FRONTEND_EVENT_REPLAY_BUFFER_STARTING) {
    ge_replay_buffer_starting();
  } else if (event == OBS_FRONTEND_EVENT_REPLAY_BUFFER_STARTED) {
    ge_replay_buffer_started();
  } else if (event == OBS_FRONTEND_EVENT_REPLAY_BUFFER_STOPPING) {
    ge_replay_buffer_stopping();
  } else if (event == OBS_FRONTEND_EVENT_REPLAY_BUFFER_STOPPED) {
    ge_replay_buffer_stopped();
  } else if (event == OBS_FRONTEND_EVENT_REPLAY_BUFFER_SAVED) {
    // The replay buffer finished writing a file: hand its path to Rust, which
    // wakes whichever save is waiting on it (no polling). obs_frontend_get_last_replay
    // returns a bstr we own and must bfree.
    char *path = obs_frontend_get_last_replay();
    ge_replay_buffer_saved(path);
    if (path) {
      bfree(path);
    }
  } else if (event == OBS_FRONTEND_EVENT_FINISHED_LOADING || event == OBS_FRONTEND_EVENT_SCENE_COLLECTION_CHANGED) {
    ge_sources_changed();
  }
}

// Called by the shim once this library has been dlopen'd. Mirrors what the old
// monolithic `obs_module_load` did. Returns false on failure so the shim can
// log a useful error and refuse to come up.
GE_EXPORT bool ge_core_load(obs_module_t *module) {
  ge_obs_set_module(module);
  ge_rust_start();
  ge_connect_source_signals();
  ge_sources_changed();
  obs_frontend_add_event_callback(ge_on_frontend_event, NULL);
  return true;
}

// Called by the shim from OBS's post-load hook. Keep this separate from
// ge_core_load so OBS's user config is queried at the same lifecycle point as
// the frontend normally expects.
GE_EXPORT void ge_core_post_load(void) {
  ge_sources_changed();
  ge_browser_dock_post_load();
}

// Called by the shim before it dlcloses this library (on OBS shutdown, or
// before a dev-mode hot reload). `ge_rust_stop` blocks until the tokio runtime
// is fully torn down, so no Rust threads survive the dlclose that follows.
GE_EXPORT void ge_core_unload(void) {
  ge_disconnect_source_signals();
  obs_frontend_remove_event_callback(ge_on_frontend_event, NULL);
  ge_rust_stop();
}
