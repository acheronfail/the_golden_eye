#ifndef GE_OBS_BMP_H
#define GE_OBS_BMP_H

#include <stddef.h>
#include <stdint.h>

// Encodes BGRA pixel data (OBS native format) into a 24-bit BMP.
// Caller must free() the returned pointer.
uint8_t *ge_encode_bmp(const uint8_t *bgra, uint32_t width, uint32_t height,
                       size_t *out_size);

#endif
