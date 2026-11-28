#ifndef OVG_RASTER_WINDOW_H
#define OVG_RASTER_WINDOW_H
#include <stddef.h>
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif
typedef struct {
    int32_t width, height, bands, block_width, block_height;
    double transform[6];
    double nodata;
    int32_t has_transform, has_nodata;
} OvgRasterInfo;
enum { OVG_RASTER_OK=0, OVG_RASTER_INVALID=1, OVG_RASTER_UNSUPPORTED=2, OVG_RASTER_IO=3, OVG_RASTER_LIMIT=4 };
// Host-resolved private local path only. No remote/virtual file paths.
// Output belongs to caller; no returned native pointer. 1-based band.
int32_t ovg_raster_info(const char* path, int32_t band, OvgRasterInfo* out);
int32_t ovg_raster_window(const char* path,int32_t band,int32_t x,int32_t y,
    int32_t width,int32_t height,float* out,size_t capacity);
#ifdef __cplusplus
}
#endif
#endif
