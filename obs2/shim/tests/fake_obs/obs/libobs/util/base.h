#ifndef GE_FAKE_OBS_BASE_H
#define GE_FAKE_OBS_BASE_H

// Minimal stand-in for real libobs' util/base.h -- see obs-module.h.

enum {
  LOG_ERROR = 100,
  LOG_WARNING = 200,
  LOG_INFO = 300,
};

void blog(int log_level, const char *format, ...);

#endif /* GE_FAKE_OBS_BASE_H */
