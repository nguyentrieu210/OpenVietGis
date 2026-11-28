#include "raster_window.h"
#include <gdal.h>
#include <cpl_string.h>
#include <filesystem>
#include <vector>
#include <cstdio>
#include <cstdlib>
static void check(bool b,const char* s){if(!b){std::fprintf(stderr,"FAIL %s\n",s);std::exit(1);}}
int main(int argc,char** argv){
 check(argc==2,"absolute fixture path");check(std::filesystem::path(argv[1]).is_absolute(),"absolute");
 check(!std::filesystem::exists(argv[1]),"no overwrite");GDALAllRegister();
 auto driver=GDALGetDriverByName("GTiff");check(driver!=nullptr,"GTiff driver");
 char** opts=nullptr;opts=CSLSetNameValue(opts,"TILED","YES");opts=CSLSetNameValue(opts,"BLOCKXSIZE","32");opts=CSLSetNameValue(opts,"BLOCKYSIZE","32");
 auto ds=GDALCreate(driver,argv[1],64,64,1,GDT_Int16,opts);CSLDestroy(opts);check(ds!=nullptr,"create");
 double transform[6]={100,2,0,200,0,-2};check(GDALSetGeoTransform(ds,transform)==CE_None,"transform");
 auto band=GDALGetRasterBand(ds,1);check(GDALSetRasterNoDataValue(band,-9999)==CE_None,"nodata");
 std::vector<int16_t> values(4096);
 for(int y=0;y<64;++y)for(int x=0;x<64;++x)values[y*64+x]=int16_t(y*64+x);
 values[10*64+8]=-9999;
 check(GDALRasterIO(band,GF_Write,0,0,64,64,values.data(),64,64,GDT_Int16,0,0)==CE_None,"write");GDALClose(ds);
 OvgRasterInfo info{};check(ovg_raster_info(argv[1],1,&info)==0,"info");
 check(info.width==64&&info.height==64&&info.block_width==32&&info.block_height==32,"dimensions");
 check(info.has_transform&&info.transform[5]==-2&&info.has_nodata&&info.nodata==-9999,"metadata");
 float out[4]={};
 check(ovg_raster_window(argv[1],1,8,10,2,2,out,4)==0,"window");
 check(out[0]==-9999&&out[1]==649&&out[2]==712&&out[3]==713,"numeric oracle");
 check(ovg_raster_window(argv[1],1,63,63,2,2,out,4)==OVG_RASTER_INVALID,"range");
 check(ovg_raster_window(argv[1],1,0,0,2,2,out,3)==OVG_RASTER_INVALID,"capacity");
 check(ovg_raster_window("/vsicurl/https://example.invalid/a.tif",1,0,0,1,1,out,4)!=0,"reject URL");
 std::filesystem::remove(argv[1]);std::puts("RASTER_WINDOW_PASS");
}
