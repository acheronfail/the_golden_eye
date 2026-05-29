#ifndef GE_CV_WRAPPER_H
#define GE_CV_WRAPPER_H

#include <stddef.h>
#include <stdint.h>

// Thin C interface over OpenCV's C++ API. The implementation lives in
// cv_wrapper.cpp so that OpenCV's C++ headers stay contained to a single
// translation unit and the rest of the (C) plugin only sees this header.
#ifdef __cplusplus
extern "C" {
#endif

// Returns the OpenCV version string OpenCV was built against (e.g. "4.11.0").
// The returned pointer is statically allocated; do not free it.
const char *ge_cv_version(void);

// Converts a BGRA frame (OBS native format) to a single-channel grayscale
// buffer. Returns a newly allocated `width * height` byte buffer that the
// caller must free(), or NULL on failure.
uint8_t *ge_cv_bgra_to_gray(const uint8_t *bgra, uint32_t width, uint32_t height);

#ifdef __cplusplus
}
#endif

#endif
