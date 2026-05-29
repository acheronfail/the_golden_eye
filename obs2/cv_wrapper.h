#ifndef GE_CV_WRAPPER_H
#define GE_CV_WRAPPER_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct ge_level_match_result {
	// Matched mission number (1-9), or -1 if no confident match.
	int32_t mission;
	// Matched part number (1-5), or -1 if no confident match.
	int32_t part;
	// Matched difficulty number (1-4), or -1 if no confident match.
	int32_t difficulty;
	// Matched times in seconds, ordered top-to-bottom then left-to-right.
	int32_t *times;
	size_t times_len;
} ge_level_match_result_t;

// Returns the OpenCV version string OpenCV was built against (e.g. "4.11.0").
// The returned pointer is statically allocated; do not free it.
const char *ge_cv_version(void);

#ifdef __cplusplus
}
#endif

#endif
