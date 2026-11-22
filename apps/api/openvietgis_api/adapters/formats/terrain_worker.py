"""Isolated bounded DEM sampling. Null cells stay nodata; no inferred vertical datum."""

import json
import math
import sys

import raster_worker
from raster_worker import gdal, np, opened

raster_worker.PIXELS = 20_000_000
WORLD = 20037508.342789244
MAX_SIDE = 128
MAX_SPAN = 100_000.0
MIN_ELEVATION = -12000.0
MAX_ELEVATION = 10000.0


def sample(path):
    ds, _, _, code = opened(path)
    if ds.RasterCount != 1 or gdal.DataTypeIsComplex(ds.GetRasterBand(1).DataType):
        raise ValueError("DEM must have one real-valued elevation band")
    if min(ds.RasterXSize, ds.RasterYSize) < 2:
        raise ValueError("DEM needs at least 2 by 2 pixels")
    vrt = gdal.AutoCreateWarpedVRT(ds, None, "EPSG:3857", gdal.GRA_NearestNeighbour)
    if vrt is None:
        raise ValueError("Cannot project DEM")
    geo = vrt.GetGeoTransform()
    left, top = geo[0], geo[3]
    right = left + vrt.RasterXSize * geo[1]
    bottom = top + vrt.RasterYSize * geo[5]
    if not all(math.isfinite(v) and abs(v) <= WORLD for v in (left, bottom, right, top)):
        raise ValueError("DEM outside Web Mercator domain")
    dx, dy = right - left, top - bottom
    if not 0 < dx <= MAX_SPAN or not 0 < dy <= MAX_SPAN:
        raise ValueError("Local terrain coverage limited to 100 km Web Mercator grid per side")
    cols = max(2, min(MAX_SIDE, vrt.RasterXSize, math.ceil(MAX_SIDE * dx / max(dx, dy))))
    rows = max(2, min(MAX_SIDE, vrt.RasterYSize, math.ceil(MAX_SIDE * dy / max(dx, dy))))
    raw = gdal.Warp(
        "",
        ds,
        format="MEM",
        dstSRS="EPSG:3857",
        outputBounds=(left, bottom, right, top),
        width=cols,
        height=rows,
        resampleAlg="near",
        dstNodata=float("nan"),
        outputType=gdal.GDT_Float64,
    )
    if raw is None:
        raise ValueError("Cannot sample DEM")
    band = raw.GetRasterBand(1)
    elevations = band.ReadAsArray()
    valid = np.isfinite(elevations) & (band.GetMaskBand().ReadAsArray() != 0)
    if not valid.any():
        raise ValueError("DEM contains no valid elevations")
    if np.any(valid & ((elevations < MIN_ELEVATION) | (elevations > MAX_ELEVATION))):
        raise ValueError("Declared elevation metres outside supported range")
    # A renderable cell requires four valid corners; don't bridge nodata holes.
    cells = valid[:-1, :-1] & valid[1:, :-1] & valid[:-1, 1:] & valid[1:, 1:]
    if not cells.any():
        raise ValueError("No complete DEM mesh cell")
    step_x, step_y = dx / cols, dy / rows
    values = [
        float(v) if good else None
        for v, good in zip(elevations.ravel(), valid.ravel(), strict=True)
    ]
    return {
        "crs_code": 3857,
        "source_crs_code": code,
        "width": cols,
        "height": rows,
        "extent": [left + step_x / 2, bottom + step_y / 2, right - step_x / 2, top - step_y / 2],
        "values": values,
        "min_m": float(elevations[valid].min()),
        "max_m": float(elevations[valid].max()),
        "valid_count": int(valid.sum()),
        "nodata_count": int(valid.size - valid.sum()),
        "resampling": "nearest-cell-centre",
        "sampled": True,
        "survey_grade": False,
    }


if __name__ == "__main__":
    try:
        print(json.dumps(sample(sys.argv[1]), allow_nan=False))
    except Exception as exc:
        print(json.dumps({"error": str(exc)[:250]}))
        sys.exit(1)
