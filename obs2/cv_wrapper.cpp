#include "cv_wrapper.h"

#include <cstdlib>
#include <cstring>

#include <opencv2/core.hpp>
#include <opencv2/imgproc.hpp>

const char *ge_cv_version(void) { return CV_VERSION; }

uint8_t *ge_cv_bgra_to_gray(const uint8_t *bgra, uint32_t width, uint32_t height) {
  if (!bgra || width == 0 || height == 0)
    return NULL;

  // Wrap the caller's buffer without copying it.
  const cv::Mat src(static_cast<int>(height), static_cast<int>(width), CV_8UC4,
                    const_cast<uint8_t *>(bgra));

  cv::Mat gray;
  cv::cvtColor(src, gray, cv::COLOR_BGRA2GRAY);

  const size_t size = static_cast<size_t>(width) * height;
  uint8_t *out = static_cast<uint8_t *>(std::malloc(size));
  if (!out)
    return NULL;

  // cvtColor produces a continuous, tightly-packed buffer for this case.
  std::memcpy(out, gray.data, size);
  return out;
}
