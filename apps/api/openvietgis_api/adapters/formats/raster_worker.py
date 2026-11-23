"""Isolated GDAL worker for bounded uploaded GeoTIFF analysis. No network inputs."""

import base64
import json
import math
import os
import resource
import sys

resource.setrlimit(resource.RLIMIT_AS, (1024 * 1024 * 1024, 1024 * 1024 * 1024))
resource.setrlimit(
    resource.RLIMIT_CPU, (55, 60) if os.environ.get("OVG_RASTER_PERSIST") == "1" else (22, 24)
)
os.environ.update(PROJ_NETWORK="OFF", OPENBLAS_NUM_THREADS="1", GDAL_NUM_THREADS="1")


def _load_guarded_deps():
    import numpy as np
    from osgeo import gdal, osr  # type: ignore[import-not-found]

    return np, gdal, osr


np, gdal, osr = _load_guarded_deps()

gdal.UseExceptions()
gdal.SetConfigOption("GDAL_DISABLE_READDIR_ON_OPEN", "EMPTY_DIR")
gdal.SetConfigOption("GDAL_PAM_ENABLED", "NO")
gdal.SetCacheMax(32 * 1024 * 1024)
NODATA = -9999.0
PIXELS = 2_000_000


def epsg_ref(code):
    s = osr.SpatialReference()
    s.ImportFromEPSG(code)
    s.SetAxisMappingStrategy(osr.OAMS_TRADITIONAL_GIS_ORDER)
    return s


def opened(path):
    ds = gdal.OpenEx(path, gdal.OF_RASTER | gdal.OF_READONLY, allowed_drivers=["GTiff"])
    if ds is None or ds.RasterXSize * ds.RasterYSize > PIXELS or ds.RasterCount not in range(1, 17):
        raise ValueError(f"GeoTIFF must have 1–16 bands and at most {PIXELS:,} pixels")
    geo = ds.GetGeoTransform(can_return_null=True)
    if (
        geo is None
        or not all(math.isfinite(x) for x in geo)
        or geo[1] <= 0
        or geo[5] >= 0
        or geo[2]
        or geo[4]
    ):
        raise ValueError("North-up GeoTIFF with valid affine georeferencing required")
    srs = ds.GetSpatialRef()
    if srs is None:
        raise ValueError("CRS is missing")
    srs.SetAxisMappingStrategy(osr.OAMS_TRADITIONAL_GIS_ORDER)
    try:
        srs.AutoIdentifyEPSG()
    except RuntimeError:
        pass
    code = int(srs.GetAuthorityCode(None) or 0)
    if code not in (4326, 3857) and not (32601 <= code <= 32660 or 32701 <= code <= 32760):
        raise ValueError("Only WGS84 geographic, Web Mercator and WGS84 UTM are supported")
    if not srs.IsSame(epsg_ref(code)):
        raise ValueError("CRS definition does not match its EPSG code")
    return ds, geo, srs, code


def band(ds, index):
    if not 1 <= index <= ds.RasterCount:
        raise ValueError("Band index outside raster")
    b = ds.GetRasterBand(index)
    if gdal.DataTypeIsComplex(b.DataType):
        raise ValueError("Complex-valued bands are unsupported")
    raw = b.ReadAsArray().astype(np.float64)
    valid = np.isfinite(raw) & (b.GetMaskBand().ReadAsArray() != 0)
    nd = b.GetNoDataValue()
    if nd is not None:
        valid &= raw != nd
    scale = b.GetScale()
    offset = b.GetOffset()
    values = raw * (1 if scale is None else scale) + (0 if offset is None else offset)
    return values, valid & np.isfinite(values)


def main():
    path, second, method, a, b, output = sys.argv[1:]
    ds, geo, srs, code = opened(path)
    x, valid = band(ds, int(a))
    if method == "band":
        result = x
    elif method in ("ndvi", "ndwi", "ndbi"):
        if second != "-":
            ds2, geo2, srs2, _ = opened(second)
            if (
                ds2.RasterXSize != ds.RasterXSize
                or ds2.RasterYSize != ds.RasterYSize
                or not srs.IsSame(srs2)
                or not np.allclose(geo, geo2, rtol=0, atol=1e-8)
            ):
                raise ValueError("Bands must use the same CRS, extent, resolution and pixel grid")
        else:
            ds2 = ds
        y, v2 = band(ds2, int(b))
        total = x + y
        valid &= v2 & np.isfinite(total) & (np.abs(total) > 1e-12)
        result = np.divide(x - y, total, out=np.zeros_like(x), where=valid)
    elif method == "hillshade":
        if code == 4326 or abs(srs.GetLinearUnits() - 1) > 1e-9 or min(x.shape) < 3:
            raise ValueError("Hillshade requires a projected DEM in metres, at least 3×3")
        dy, dx = np.gradient(x, abs(geo[5]), geo[1])
        slope = np.arctan(np.hypot(dx, dy))
        # Array rows grow south. Aspect is clockwise from north in the downslope direction.
        aspect = np.arctan2(-dx, dy)
        zenith = math.radians(45)
        result = np.maximum(
            0,
            255
            * (
                np.cos(zenith) * np.cos(slope)
                + np.sin(zenith) * np.sin(slope) * np.cos(math.radians(315) - aspect)
            ),
        )
        neigh = valid.copy()
        neigh[1:, :] &= valid[:-1, :]
        neigh[:-1, :] &= valid[1:, :]
        neigh[:, 1:] &= valid[:, :-1]
        neigh[:, :-1] &= valid[:, 1:]
        valid = neigh
    else:
        raise ValueError("Unsupported operation")
    valid &= np.isfinite(result) & (np.abs(result) <= np.finfo(np.float32).max) & (result != NODATA)
    if not valid.any():
        raise ValueError("No valid pixels after masking NoData and invalid values")
    result = np.where(valid, result, NODATA).astype(np.float32)
    vals = result[valid].astype(np.float64)
    lo, hi = [float(x) for x in np.percentile(vals, [2, 98])]
    if method in ("ndvi", "ndwi", "ndbi"):
        lo, hi = -1.0, 1.0
    elif method == "hillshade":
        lo, hi = 0.0, 255.0
    if hi <= lo:
        hi = lo + 1
    stats = {
        "valid": int(valid.sum()),
        "nodata": int(valid.size - valid.sum()),
        "min": float(vals.min()),
        "max": float(vals.max()),
        "mean": float(vals.mean()),
        "std": float(vals.std()),
    }
    out = gdal.GetDriverByName("GTiff").Create(
        output,
        ds.RasterXSize,
        ds.RasterYSize,
        1,
        gdal.GDT_Float32,
        options=["COMPRESS=DEFLATE", "TILED=YES"],
    )
    out.SetGeoTransform(geo)
    out.SetProjection(srs.ExportToWkt())
    ob = out.GetRasterBand(1)
    ob.SetNoDataValue(NODATA)
    ob.WriteArray(result)
    ob.FlushCache()
    out.FlushCache()
    ref = epsg_ref(3857)
    vrt = gdal.AutoCreateWarpedVRT(out, None, ref.ExportToWkt(), gdal.GRA_NearestNeighbour)
    vg = vrt.GetGeoTransform()
    xmin, ymax = vg[0], vg[3]
    xmax = xmin + vrt.RasterXSize * vg[1]
    ymin = ymax + vrt.RasterYSize * vg[5]
    if (
        not all(math.isfinite(v) and abs(v) <= 20037508.35 for v in (xmin, xmax, ymin, ymax))
        or xmin >= xmax
        or ymin >= ymax
    ):
        raise ValueError("Raster extent outside Web Mercator map domain")
    warp = gdal.Warp(
        "",
        out,
        format="MEM",
        dstSRS="EPSG:3857",
        outputBounds=(xmin, ymin, xmax, ymax),
        width=512,
        height=512,
        resampleAlg="near",
        srcNodata=NODATA,
        dstNodata=NODATA,
    )
    q = warp.ReadAsArray()
    mask = np.isfinite(q) & (q != NODATA)
    fraction = np.clip((q - lo) / (hi - lo), 0, 1)
    fraction = np.where(mask, fraction, 0)
    if method in ("ndvi", "ndwi", "ndbi"):
        # red → yellow → green (NDVI), red → white → blue (water/buildings)
        ramp = np.array(
            [[165, 42, 42], [252, 242, 161], [24, 134, 65]]
            if method == "ndvi"
            else [[193, 76, 49], [244, 244, 232], [30, 102, 174]]
        )
        rgba = np.zeros((*q.shape, 4), np.uint8)
        for k in range(3):
            rgba[:, :, k] = np.interp(fraction, [0, 0.5, 1], ramp[:, k]).astype(np.uint8)
    else:
        gray = (fraction * 255).astype(np.uint8)
        rgba = np.stack([gray, gray, gray, np.zeros_like(gray)], axis=-1)
    rgba[:, :, 3] = mask.astype(np.uint8) * 255
    png_ds = gdal.GetDriverByName("MEM").Create("", rgba.shape[1], rgba.shape[0], 4, gdal.GDT_Byte)
    for k, interpretation in enumerate(
        (gdal.GCI_RedBand, gdal.GCI_GreenBand, gdal.GCI_BlueBand, gdal.GCI_AlphaBand)
    ):
        pb = png_ds.GetRasterBand(k + 1)
        pb.WriteArray(rgba[:, :, k])
        pb.SetColorInterpretation(interpretation)
    _png_output = gdal.GetDriverByName("PNG").CreateCopy("/vsimem/result.png", png_ds)
    _png_output = None
    png_data = bytes(gdal.VSIGetMemFileBuffer_unsafe("/vsimem/result.png"))
    gdal.Unlink("/vsimem/result.png")
    transform = osr.CoordinateTransformation(ref, epsg_ref(4326))
    corners = [
        list(transform.TransformPoint(x, y)[:2])
        for x, y in [(xmin, ymax), (xmax, ymax), (xmax, ymin), (xmin, ymin)]
    ]
    out = None
    ob = None
    vrt = None
    warp = None
    print(
        json.dumps(
            {
                "width": ds.RasterXSize,
                "height": ds.RasterYSize,
                "bands": ds.RasterCount,
                "epsg": code,
                "stats": stats,
                "legend": [lo, hi],
                "corners": corners,
                "png": base64.b64encode(png_data).decode("ascii"),
            },
            allow_nan=False,
        )
    )


if __name__ == "__main__":
    try:
        main()
    except (ValueError, RuntimeError, OverflowError) as e:
        print(json.dumps({"error": str(e)[:280]}))
        sys.exit(2)
