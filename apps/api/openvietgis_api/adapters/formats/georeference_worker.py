"""Bounded affine GCP georeferencing of an uploaded, unreferenced TIFF.

Pixel coordinates use GDAL pixel-corner convention; optional bounded north-up resampling.
"""
import base64
import hashlib
import json
import math
import os
import resource
import sys

resource.setrlimit(resource.RLIMIT_AS, (1024*1024*1024,)*2)
resource.setrlimit(resource.RLIMIT_CPU, (22,24))
resource.setrlimit(resource.RLIMIT_FSIZE, (32*1024*1024,)*2)
os.environ.update(PROJ_NETWORK="OFF", OPENBLAS_NUM_THREADS="1", GDAL_NUM_THREADS="1")
# Native imports must follow the resource and environment limits above.
import numpy as np  # noqa: E402, I001
from osgeo import gdal, osr  # type: ignore[import-not-found]  # noqa: E402

gdal.UseExceptions()
gdal.SetConfigOption("GDAL_DISABLE_READDIR_ON_OPEN","EMPTY_DIR")
gdal.SetConfigOption("GDAL_PAM_ENABLED","NO")
gdal.SetConfigOption("GDAL_TIFF_INTERNAL_MASK","YES")
gdal.SetCacheMax(32*1024*1024)


def fit(points, width, height, epsg):
    if not isinstance(points,list) or not 3<=len(points)<=32 or epsg not in (4326,3857):
        raise ValueError("Use 3–32 control points and EPSG:4326 or EPSG:3857")
    if any(
        not isinstance(p, list) or len(p) != 4
        or any(type(v) not in (int, float) or not math.isfinite(v) for v in p)
        for p in points
    ):
        raise ValueError("Invalid control point")
    p=np.asarray(points,dtype=np.float64)
    if (
        (p[:,0]<0).any() or (p[:,0]>width).any() or (p[:,1]<0).any()
        or (p[:,1]>height).any() or len(set(map(tuple,p[:,:2])))!=len(p)
    ):
        raise ValueError("Pixel coordinates must be unique and inside the raster")
    bounds=(180,85.05112878) if epsg==4326 else (20037508.342789244,20037508.342789244)
    if (np.abs(p[:,2:])>bounds).any():
        raise ValueError("Control point outside supported CRS domain")
    center=p[:,:2].mean(axis=0)
    scale=np.array([width,height],dtype=float)
    a=np.column_stack([np.ones(len(p)),(p[:,:2]-center)/scale])
    target_center=p[:,2:].mean(axis=0)
    coeff,_,rank,singular=np.linalg.lstsq(a,p[:,2:]-target_center,rcond=None)
    if rank!=3 or singular[-1]<singular[0]*1e-6:
        raise ValueError("Collinear or poorly distributed control points")
    slopes=coeff[1:]/scale[:,None]
    origin=coeff[0]+target_center-center@slopes
    matrix=slopes.T
    sv=np.linalg.svd(matrix,compute_uv=False)
    if not np.isfinite(matrix).all() or sv[-1]<=sv[0]*1e-8 or sv[-1]<1e-12:
        raise ValueError("Degenerate affine transform")
    predicted=a@coeff+target_center
    residuals=np.linalg.norm(predicted-p[:,2:],axis=1)
    pixel_residuals=np.linalg.norm((predicted-p[:,2:])@np.linalg.inv(slopes),axis=1)
    geo=[
        float(origin[0]), float(slopes[0,0]), float(slopes[1,0]),
        float(origin[1]), float(slopes[0,1]), float(slopes[1,1]),
    ]
    native=np.array([[0,0],[width,0],[width,height],[0,height]])@slopes+origin
    if not np.isfinite(native).all() or (np.abs(native)>bounds).any():
        raise ValueError("Raster footprint outside supported CRS domain")
    return geo,residuals,pixel_residuals,native


def spatial_ref(epsg):
    s=osr.SpatialReference()
    s.ImportFromEPSG(epsg)
    s.SetAxisMappingStrategy(osr.OAMS_TRADITIONAL_GIS_ORDER)
    return s


def warp_north_up(ds, output, geo, native, s, method):
    """Warp only homogeneous bands with explicit finite common NoData; fail closed on masks."""
    bands=[ds.GetRasterBand(i) for i in range(1,ds.RasterCount+1)]
    nodata=bands[0].GetNoDataValue()
    if (
        nodata is None or not math.isfinite(nodata)
        or any(b.GetNoDataValue()!=nodata or b.DataType!=bands[0].DataType
               or b.GetMaskFlags()!=gdal.GMF_NODATA
               or b.GetColorInterpretation()==gdal.GCI_AlphaBand
               or b.GetColorTable() is not None for b in bands)
    ):
        raise ValueError("North-up warp requires equal finite NoData, homogeneous bands and no custom masks or palettes")
    west,south=native.min(axis=0)
    east,north=native.max(axis=0)
    dx=math.hypot(geo[1],geo[4])
    dy=math.hypot(geo[2],geo[5])
    width=math.ceil((east-west)/dx)
    height=math.ceil((north-south)/dy)
    if width<1 or height<1 or width*height>2_000_000:
        raise ValueError("Warp output exceeds 2 million pixels")
    mem=gdal.GetDriverByName("MEM").CreateCopy("",ds,strict=1)
    if mem is None:
        raise ValueError("Cannot stage affine raster")
    mem.SetGeoTransform(geo)
    mem.SetSpatialRef(s)
    result=gdal.Warp(output,mem,format="GTiff",dstSRS=s.ExportToWkt(),
        outputBounds=(float(west),float(south),float(east),float(north)),
        width=width,height=height,resampleAlg="near" if method=="nearest" else "bilinear",
        srcNodata=nodata,dstNodata=nodata,multithread=False,warpMemoryLimit=32,
        creationOptions=["COMPRESS=DEFLATE","TILED=YES"])
    if result is None:
        raise ValueError("North-up warp failed")
    result.FlushCache()
    result=None
    check=gdal.OpenEx(output,gdal.OF_RASTER|gdal.OF_READONLY,allowed_drivers=["GTiff"])
    expected=[float(west),float((east-west)/width),0.0,float(north),0.0,-float((north-south)/height)]
    if (check is None or check.RasterXSize!=width or check.RasterYSize!=height
        or check.RasterCount!=ds.RasterCount or not check.GetSpatialRef().IsSame(s)
        or not np.allclose(check.GetGeoTransform(),expected,rtol=0,atol=1e-9)
        or any(check.GetRasterBand(i).GetNoDataValue()!=nodata for i in range(1,ds.RasterCount+1))):
        raise ValueError("Warp output verification failed")
    return {"method":method,"width":width,"height":height,"transform":list(check.GetGeoTransform())}


def run(src, config, output):
    with open(config, encoding="utf-8") as f:
        raw=f.read()
    request=json.loads(raw)
    ds=gdal.OpenEx(src,gdal.OF_RASTER|gdal.OF_READONLY,allowed_drivers=["GTiff"])
    if ds is None:
        raise ValueError("TIFF could not be opened")
    w,h,bands=ds.RasterXSize,ds.RasterYSize,ds.RasterCount
    if w<2 or h<2 or w*h>2_000_000 or not 1<=bands<=16:
        raise ValueError("Raster limit: 2 million pixels, 1–16 bands")
    if (
        ds.GetSpatialRef() is not None or ds.GetGeoTransform(can_return_null=True) is not None
        or ds.GetGCPCount() or ds.GetMetadata("RPC") or ds.GetMetadata("GEOLOCATION")
    ):
        raise ValueError(
            "Only unreferenced TIFF input is accepted; existing georeferencing is not overwritten"
        )
    if any(gdal.DataTypeIsComplex(ds.GetRasterBand(i).DataType) for i in range(1,bands+1)):
        raise ValueError("Complex-valued raster is unsupported")
    # Small grayscale preview only. Original bands, masks and NoData remain in TIFF.
    ratio=min(1,512/max(w,h))
    tw=max(1,round(w*ratio))
    th=max(1,round(h*ratio))
    band=ds.GetRasterBand(1)
    data=band.ReadAsArray(buf_xsize=tw,buf_ysize=th).astype(np.float64)
    mask=band.GetMaskBand().ReadAsArray(buf_xsize=tw,buf_ysize=th)!=0
    valid=np.isfinite(data)&mask
    visual=np.zeros((th,tw),dtype=np.uint8)
    if valid.any():
        low,high=np.percentile(data[valid],[2,98])
        visual[valid]=(
            np.clip((data[valid]-low)/max(high-low,1e-12)*255,0,255).astype(np.uint8)
            if high>low else 128
        )
    mem=gdal.GetDriverByName("MEM").Create("",tw,th,1,gdal.GDT_Byte)
    mem.GetRasterBand(1).WriteArray(visual)
    png_path="/vsimem/georef-preview.png"
    png=gdal.GetDriverByName("PNG").CreateCopy(png_path,mem)
    if png is None:
        raise ValueError("Failed to create preview PNG")
    png.FlushCache()
    del png
    thumb=base64.b64encode(bytes(gdal.VSIGetMemFileBuffer_unsafe(png_path))).decode("ascii")
    gdal.Unlink(png_path)
    with open(src,"rb") as f:
        digest=hashlib.file_digest(f,"sha256").hexdigest()
    result={"width":w,"height":h,"bands":bands,"source_sha256":digest,"png":thumb,"persisted":False}
    if request.get("mode")=="inspect":
        return result
    if request.get("mode")!="solve" or request.get("source_sha256")!=digest:
        raise ValueError("Source checksum does not match preview")
    epsg=request.get("epsg")
    geo,residuals,pixel_residuals,native=fit(request.get("points"),w,h,epsg)
    s=spatial_ref(epsg)
    to_wgs=osr.CoordinateTransformation(s,spatial_ref(4326))
    corners=[list(to_wgs.TransformPoint(float(x),float(y))[:2]) for x,y in native]
    if (
        any(not all(math.isfinite(v) for v in p) or abs(p[0])>180
            or abs(p[1])>85.05112878 for p in corners)
        or max(p[0] for p in corners)-min(p[0] for p in corners)>180
    ):
        raise ValueError("Footprint crosses unsupported map domain")
    method=request.get("warp","none")
    if method not in ("none","nearest","bilinear"):
        raise ValueError("Unsupported warp method")
    if method!="none":
        result["warp"]=warp_north_up(ds,output,geo,native,s,method)
    else:
        out=gdal.GetDriverByName("GTiff").CreateCopy(
            output, ds, strict=1, options=["COMPRESS=DEFLATE","TILED=YES"]
        )
        if out is None:
            raise ValueError("Failed to copy TIFF")
        out.SetGeoTransform(geo)
        out.SetSpatialRef(s)
        out.FlushCache()
        out=None
        check=gdal.OpenEx(output,gdal.OF_RASTER|gdal.OF_READONLY,allowed_drivers=["GTiff"])
        if (
            check is None or check.RasterXSize!=w or check.RasterYSize!=h
            or check.RasterCount!=bands
            or not np.allclose(check.GetGeoTransform(),geo,rtol=0,atol=1e-12)
            or not check.GetSpatialRef().IsSame(s)
        ):
            raise ValueError("Output georeferencing verification failed")
    result.update(
        epsg=epsg, transform=geo, corners=corners, residuals=residuals.tolist(),
        pixel_residuals=pixel_residuals.tolist(), rms=float(np.sqrt(np.mean(residuals**2))),
        pixel_rms=float(np.sqrt(np.mean(pixel_residuals**2))), redundant=len(residuals)>3,
    )
    return result


if __name__=="__main__":
    try:
        print(json.dumps(run(*sys.argv[1:]),allow_nan=False,separators=(",",":")))
    except Exception as error:
        print(json.dumps({"error":str(error)[:300]}))
        sys.exit(1)
