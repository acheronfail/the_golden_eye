// Bridges the Rust core's tracing output into OBS's log. Kept in its own
// translation unit because it needs `GeLogLevel` (and the `ge_obs_blog`
// prototype) from the cbindgen-generated ge_rust.h, which declares the capture
// API with types that conflict with obs_bridge.h -- so the two headers can't be
// included together. See obs_bridge.h.
//
// This is the single source of truth for the OBS log constants: `GeLogLevel` is
// defined once in Rust (rust/src/ffi.rs) and generated into ge_rust.h, and the
// OBS `LOG_*` values come straight from <util/base.h>. Neither is duplicated.

#include "ge_rust.h"

#include <obs/libobs/util/base.h>

void ge_obs_blog(GeLogLevel level, const char *msg) {
  if (!msg) {
    return;
  }

  int obs_level;
  switch (level) {
  case GeLogLevel_Error:
    obs_level = LOG_ERROR;
    break;
  case GeLogLevel_Warning:
    obs_level = LOG_WARNING;
    break;
  case GeLogLevel_Info:
    obs_level = LOG_INFO;
    break;
  case GeLogLevel_Debug:
  default:
    obs_level = LOG_DEBUG;
    break;
  }

  /* "%s" so log content is never treated as a printf format string. */
  blog(obs_level, "%s", msg);
}
