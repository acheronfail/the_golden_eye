#include "bmp.h"

#include <stdlib.h>
#include <string.h>

uint8_t *ge_encode_bmp(const uint8_t *bgra, uint32_t width, uint32_t height,
                       size_t *out_size) {
  uint32_t row_stride = (width * 3 + 3) & ~3u;
  uint32_t pixel_data_size = row_stride * height;
  uint32_t file_size = 54 + pixel_data_size;

  uint8_t *bmp = (uint8_t *)malloc(file_size);
  if (!bmp)
    return NULL;

  // File header (14 bytes)
  bmp[0] = 'B';
  bmp[1] = 'M';
  memcpy(bmp + 2, &file_size, 4);
  memset(bmp + 6, 0, 4); // reserved
  uint32_t offset = 54;
  memcpy(bmp + 10, &offset, 4);

  // BITMAPINFOHEADER (40 bytes)
  uint32_t hdr_size = 40;
  memcpy(bmp + 14, &hdr_size, 4);
  memcpy(bmp + 18, &width, 4);
  int32_t neg_height = -(int32_t)height; // negative = top-down
  memcpy(bmp + 22, &neg_height, 4);
  uint16_t planes = 1;
  memcpy(bmp + 26, &planes, 2);
  uint16_t bpp = 24;
  memcpy(bmp + 28, &bpp, 2);
  memset(bmp + 30, 0, 24); // compression, sizes, resolution, colors

  // Pixel data: BGRA -> BGR (drop alpha, same channel order)
  uint8_t *dst = bmp + 54;
  for (uint32_t y = 0; y < height; y++) {
    const uint8_t *src = bgra + y * width * 4;
    for (uint32_t x = 0; x < width; x++) {
      dst[x * 3 + 0] = src[x * 4 + 0]; // B
      dst[x * 3 + 1] = src[x * 4 + 1]; // G
      dst[x * 3 + 2] = src[x * 4 + 2]; // R
    }
    memset(dst + width * 3, 0, row_stride - width * 3);
    dst += row_stride;
  }

  *out_size = file_size;
  return bmp;
}
