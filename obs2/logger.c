#include "logger.h"

#include <obs/obs-module.h>
#include <stdarg.h>
#include <stdio.h>

static const char GE_PLUGIN_TAG[] = "[The Golden Eye]";

static void ge_vlog(int level, const char *fmt, va_list args) {
  char message[2048];
  vsnprintf(message, sizeof(message), fmt, args);
  blog(level, "%s %s", GE_PLUGIN_TAG, message);
}

void ge_log_info(const char *fmt, ...) {
  va_list args;
  va_start(args, fmt);
  ge_vlog(LOG_INFO, fmt, args);
  va_end(args);
}

void ge_log_error(const char *fmt, ...) {
  va_list args;
  va_start(args, fmt);
  ge_vlog(LOG_ERROR, fmt, args);
  va_end(args);
}