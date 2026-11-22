import json
import math
import subprocess

from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.spatial.test_layers import _path
from tests.integration.spatial.test_sources import _client


def test_terrain_and_report(migrated_database, tmp_path):
    world = seed_access_world()
    base = _path(world).rsplit("/", 1)[0] + "/raster"

    def make(x, offset, filename):
        p = tmp_path / filename
        subprocess.run(
            [
                "/usr/bin/python3",
                "-c",
                """
from osgeo import gdal,osr
import numpy as np,sys
d=gdal.GetDriverByName('GTiff').Create(sys.argv[1],4,4,1,gdal.GDT_Float32)
s=osr.SpatialReference();s.ImportFromEPSG(32648)
d.SetProjection(s.ExportToWkt());d.SetGeoTransform((float(sys.argv[2]),10,0,1200000,0,-10))
a=np.tile(np.arange(4,dtype=np.float32)*10+float(sys.argv[3]),(4,1))
b=d.GetRasterBand(1);b.SetNoDataValue(-9999);b.WriteArray(a);d=None
""",
                str(p),
                str(x),
                str(offset),
            ],
            check=True,
        )
        return p.read_bytes()

    one = make(600000, 0, "a.tif")
    two = make(600020, 100, "b.tif")
    with _client(world.owner_user_id) as c:

        def create(method, **data):
            files = {"file": ("a.tif", one, "image/tiff")}
            if method == "mosaic":
                files["second"] = ("b.tif", two, "image/tiff")
            r = c.post(
                base + "/layers", data={"name": method, "method": method, **data}, files=files
            )
            assert r.status_code == 201, r.text
            return r.json()

        a = create("band")
        slope = create("slope")
        assert math.isclose(slope["stats"]["mean"], 45)
        aspect = create("aspect")
        assert math.isclose(aspect["stats"]["mean"], 270)
        res = create("resample", factor=2, resampling="near")
        assert (
            (res["width"], res["height"]) == (8, 8)
            and res["pixel_size"] == [5, 5]
            and res["stats"]["mean"] == 15
        )
        mos = create("mosaic", band_b=1)
        assert (mos["width"], mos["height"]) == (6, 4) and mos["stats"]["mean"] == 470 / 6
        r = c.post(
            base + "/reports", data={"method": "contour", "source_id": a["id"], "interval": 10}
        )
        assert r.status_code == 200, r.text
        features = r.json()["geojson"]["features"]
        assert features and all(f["geometry"]["type"] == "LineString" for f in features)
        left_bound, b, r, t = a["bounds"]
        zone = {
            "type": "Polygon",
            "coordinates": [
                [
                    [left_bound - 0.01, b - 0.01],
                    [r + 0.01, b - 0.01],
                    [r + 0.01, t + 0.01],
                    [left_bound - 0.01, t + 0.01],
                    [left_bound - 0.01, b - 0.01],
                ]
            ],
        }
        r = c.post(
            base + "/reports",
            data={
                "method": "zonal",
                "source_id": a["id"],
                "zones_json": json.dumps([{"label": "All", "geometry": zone}]),
            },
        )
        assert r.status_code == 200, r.text
        s = r.json()["zones"][0]
        assert s["valid"] == 16 and s["mean"] == 15 and s["sum"] == 240
        assert (
            c.post(
                base + "/reports",
                data={"method": "contour", "source_id": a["id"], "interval": 0.001},
            ).status_code
            == 422
        )
        p = tmp_path / "sentinel.tif"
        subprocess.run(
            [
                "/usr/bin/python3",
                "-c",
                (
                    "\nfrom osgeo import gdal,osr\nimport numpy as "
                    "np,sys\nd=gdal.GetDriverByName('GTiff').Create(sys.argv["
                    "1],2,2,3,gdal.GDT_UInt16)\ns=osr.SpatialReference();s.Im"
                    "portFromEPSG(4326)\nd.SetProjection(s.ExportToWkt());d.S"
                    "etGeoTransform((106.66,.001,0,10.71,0,-.001))\nfor i,v "
                    "in enumerate([[[3000,3000],[0,3000]],[[2000,2000],[2000"
                    ",2000]],[[4,9],[4,6]]],1):d.GetRasterBand(i).WriteArray"
                    "(np.array(v,dtype=np.uint16))\nd=None\n"
                ),
                str(p),
            ],
            check=True,
        )
        r = c.post(
            base + "/layers",
            data={
                "name": "Sentinel NDVI",
                "method": "ndvi",
                "sentinel_scene": "S2C_MSIL2A_20250228T031721_R118_T48PXS_20250228T085312",
                "sentinel_baseline": "05.11",
            },
            files={"file": ("sentinel.tif", p.read_bytes(), "image/tiff")},
        )
        assert r.status_code == 201, r.text
        s = r.json()
        assert (
            s["stats"]["valid"] == 2
            and math.isclose(s["stats"]["mean"], 1 / 3, rel_tol=1e-6)
            and s["sentinel"]["mask"] == "SCL 4,5,6"
        )
        with _client(world.wrong_user_id) as other:
            assert other.post(
                base + "/reports", data={"method": "contour", "source_id": a["id"], "interval": 10}
            ).status_code in (403, 404)
