// The "core" plugin: all the heavy logic (Rust staticlib, OpenCV, HTTP server,
// OBS bridge). Not loaded by OBS directly -- the shim (`shim/plugin.c`) dlopens
// it and can swap it for a downloaded version at runtime (see `shim/reload.c`).

#include "ge_rust.h"

#include "../shim/reload.h"

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
  } else if (event == OBS_FRONTEND_EVENT_FINISHED_LOADING) {
    ge_frontend_finished_loading();
    ge_sources_changed();
  } else if (event == OBS_FRONTEND_EVENT_SCENE_COLLECTION_CHANGED) {
    ge_sources_changed();
  }
}

// Stashed by ge_core_load so ge_core_trigger_reload can wake the shim's
// reload worker thread later, once Rust has a verified update staged. NULL
// until ge_core_load runs.
static ge_request_reload_fn g_request_reload = NULL;

// Called by the shim after dlopen. Returns false on failure (incl. HTTP port
// bind) so the shim can log and roll back. The paths are resolved by the shim;
// `is_reload` flags a post-update load.
GE_EXPORT bool ge_core_load(obs_module_t *module, const char *canonical_path, const char *staged_dir, bool is_reload,
                            ge_request_reload_fn request_reload) {
  ge_obs_set_module(module);
  g_request_reload = request_reload;
  ge_rust_set_update_paths(canonical_path, staged_dir);
  ge_rust_set_was_reloaded(is_reload);
  if (!ge_rust_start()) {
    return false;
  }
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

// Called by the shim only after the staged core has replaced the canonical
// file. Rust can now discard its runtime-data rollback copies.
GE_EXPORT void ge_core_commit_update(void) { ge_rust_commit_update(); }

// Called by the shim before it dlcloses this library (on OBS shutdown, or
// before a reload). `ge_rust_stop` blocks until the tokio runtime is fully
// torn down, so no Rust threads survive the dlclose that follows.
GE_EXPORT void ge_core_unload(void) {
  ge_disconnect_source_signals();
  obs_frontend_remove_event_callback(ge_on_frontend_event, NULL);
  ge_rust_stop();
  g_request_reload = NULL;
}

// Called by Rust (update_apply.rs) once it has downloaded, verified, and
// staged a newer core. Must only ever wake the shim's reload worker thread --
// see reload.h's ge_request_reload_fn contract for why.
GE_EXPORT void ge_core_trigger_reload(void) {
  if (g_request_reload) {
    g_request_reload();
  }
}
