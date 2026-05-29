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

// Matches the GoldenEye level-stats overlay in a single BGRA frame against the
// template PNGs in `templates_dir`. `bgra` is `width * height * 4` bytes of
// BGRA pixels, exactly as returned by ge_obs_get_source_frame(). `lang` selects
// the template set ("en" or "jp"); templates are loaded from
// `<templates_dir>/<lang>-<name>.png` (e.g. "en-mission1.png").
//
// On success the returned struct's `mission`/`part`/`difficulty` hold the
// matched 1-based numbers (or -1 when no confident match), and `times` points
// to a heap-allocated array of `times_len` times-in-seconds. The caller owns
// `times` and must release it with ge_cv_match_result_free().
ge_level_match_result_t ge_cv_match_level(const uint8_t *bgra, uint32_t width, uint32_t height, const char *lang,
                                          const char *templates_dir);

// Releases the `times` array owned by a result from ge_cv_match_level() and
// resets the struct to the empty state. Safe to call on a zeroed result.
void ge_cv_match_result_free(ge_level_match_result_t *result);

#ifdef __cplusplus
}
#endif

#endif
