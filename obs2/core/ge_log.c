// Bridges the Rust core's tracing output into OBS's log.

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
