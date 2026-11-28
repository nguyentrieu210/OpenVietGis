#include "raster_window.h"
#include <gdal.h>
#include <cpl_conv.h>
#include <cstring>
#include <filesystem>
#include <memory>
#include <mutex>
#include <cmath>
namespace {
std::once_flag init;
using Dataset=std::unique_ptr<void,decltype(&GDALClose)>;
struct ThreadOptions {
    ThreadOptions() {
        CPLSetThreadLocalConfigOption("GDAL_DISABLE_READDIR_ON_OPEN","EMPTY_DIR");
        CPLSetThreadLocalConfigOption("PROJ_NETWORK","OFF");
        CPLSetThreadLocalConfigOption("GDAL_PAM_ENABLED","NO");
        CPLSetThreadLocalConfigOption("GDAL_NUM_THREADS","1");
        CPLSetThreadLocalConfigOption("VSI_CACHE","FALSE");
    }
    ~ThreadOptions(){
        for(const char* k:{"GDAL_DISABLE_READDIR_ON_OPEN","PROJ_NETWORK","GDAL_PAM_ENABLED","GDAL_NUM_THREADS","VSI_CACHE"})
            CPLSetThreadLocalConfigOption(k,nullptr);
    }
};
Dataset open(const char* path){
    Dataset empty(nullptr,GDALClose);
    if(!path||std::strlen(path)>4096)return empty;
    std::filesystem::path p(path);std::error_code ec;
    if(!p.is_absolute()||!std::filesystem::is_regular_file(std::filesystem::symlink_status(p,ec))||ec)return empty;
    for(const auto& part:p){if(part==".."||part==".")return empty;}
    std::call_once(init,[]{GDALAllRegister();});
    const char* allowed[]={"GTiff",nullptr};
    return Dataset(GDALOpenEx(path,GDAL_OF_RASTER|GDAL_OF_READONLY,allowed,nullptr,nullptr),GDALClose);
}
int32_t inspect(GDALDatasetH ds,int band,OvgRasterInfo* out){
    if(!ds||!out||band<1||band>GDALGetRasterCount(ds))return OVG_RASTER_INVALID;
    OvgRasterInfo value{};
    value.width=GDALGetRasterXSize(ds);value.height=GDALGetRasterYSize(ds);value.bands=GDALGetRasterCount(ds);
    if(value.width<=0||value.height<=0||value.bands>32)return OVG_RASTER_LIMIT;
    auto b=GDALGetRasterBand(ds,band);
    const auto type=GDALGetRasterDataType(b);
    if(type!=GDT_Byte&&type!=GDT_UInt16&&type!=GDT_Int16&&type!=GDT_Float32)return OVG_RASTER_UNSUPPORTED;
    GDALGetBlockSize(b,&value.block_width,&value.block_height);
    // A small requested window can still cause the decoder to allocate an entire
    // source block. Reject pathological source blocks before reading.
    if(value.block_width<=0||value.block_height<=0||
       int64_t(value.block_width)*value.block_height*4*value.bands>16*1024*1024)return OVG_RASTER_LIMIT;
    value.has_transform=GDALGetGeoTransform(ds,value.transform)==CE_None;
    if(value.has_transform){for(double x:value.transform)if(!std::isfinite(x))return OVG_RASTER_INVALID;}
    int has=0;value.nodata=GDALGetRasterNoDataValue(b,&has);value.has_nodata=has;
    *out=value;return OVG_RASTER_OK;
}
}
extern "C" int32_t ovg_raster_info(const char* path,int32_t band,OvgRasterInfo* out){
    try{ThreadOptions options;auto ds=open(path);if(!ds)return OVG_RASTER_UNSUPPORTED;return inspect(ds.get(),band,out);}
    catch(...){return OVG_RASTER_IO;}
}
extern "C" int32_t ovg_raster_window(const char* path,int32_t band,int32_t x,int32_t y,
    int32_t width,int32_t height,float* out,size_t capacity){
    if(!out||x<0||y<0||width<=0||height<=0||width>512||height>512||
       size_t(width)*size_t(height)>capacity)return OVG_RASTER_INVALID;
    try{
        ThreadOptions options;auto ds=open(path);if(!ds)return OVG_RASTER_UNSUPPORTED;
        OvgRasterInfo info{};int32_t rc=inspect(ds.get(),band,&info);if(rc!=OVG_RASTER_OK)return rc;
        if(int64_t(x)+width>info.width||int64_t(y)+height>info.height)return OVG_RASTER_INVALID;
        return GDALRasterIO(GDALGetRasterBand(ds.get(),band),GF_Read,x,y,width,height,
            out,width,height,GDT_Float32,0,0)==CE_None?OVG_RASTER_OK:OVG_RASTER_IO;
    }catch(...){return OVG_RASTER_IO;}
}
