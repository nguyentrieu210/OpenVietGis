import base64
import subprocess

from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.spatial.test_layers import _path
from tests.integration.spatial.test_sources import _client


def test_raster_analysis_is_bounded_and_read_only(migrated_database, tmp_path):
    world = seed_access_world()
    url = _path(world).rsplit("/", 1)[0] + "/raster/analyze"
    path = tmp_path / "sample.tif"
    subprocess.run(
        [
            "/usr/bin/python3",
            "-c",
            """
from osgeo import gdal,osr
import numpy as np,sys
d=gdal.GetDriverByName('GTiff').Create(sys.argv[1],3,3,2,gdal.GDT_Float32)
s=osr.SpatialReference();s.ImportFromEPSG(4326)
d.SetProjection(s.ExportToWkt());d.SetGeoTransform((106.6,.001,0,10.7,0,-.001))
a=np.full((3,3),6,dtype=np.float32);a[0,0]=-9999
b=np.full((3,3),2,dtype=np.float32)
d.GetRasterBand(1).SetNoDataValue(-9999);d.GetRasterBand(1).WriteArray(a);d.GetRasterBand(2).WriteArray(b)
d=None
""",
            str(path),
        ],
        check=True,
    )
    second = tmp_path / "second.tif"
    dem = tmp_path / "dem.tif"
    subprocess.run(
        [
            "/usr/bin/python3",
            "-c",
            (
                "\nfrom osgeo import gdal,osr\nimport numpy as np,sys\nfor "
                "filename,code,geo,value in [(sys.argv[1],4326,(106.6,.0"
                "01,0,10.7,0,-.001),2),(sys.argv[2],32648,(675000,10,0,1"
                "180000,0,-10),100)]:\n    d=gdal.GetDriverByName('GTiff'"
                ").Create(filename,3,3,1,gdal.GDT_Float32);s=osr.Spatial"
                "Reference();s.ImportFromEPSG(code)\n    "
                "d.SetProjection(s.ExportToWkt());d.SetGeoTransform(geo)"
                ";d.GetRasterBand(1).WriteArray(np.full((3,3),value,dtyp"
                "e=np.float32));d=None\n"
            ),
            str(second),
            str(dem),
        ],
        check=True,
    )
    payload = path.read_bytes()
    with _client(world.owner_user_id) as c:
        r = c.post(
            url,
            files={"file": ("input.tif", payload, "image/tiff")},
            data={"method": "ndvi", "band_a": "1", "band_b": "2"},
        )
        assert r.status_code == 200, r.text
        j = r.json()
        assert j["stats"]["valid"] == 8 and j["stats"]["nodata"] == 1
        assert j["stats"]["mean"] == 0.5 and j["epsg"] == 4326
        assert len(j["corners"]) == 4 and j["persisted"] is False
        assert base64.b64decode(j["png"]).startswith(b"\x89PNG")
        assert base64.b64decode(j["tiff"]).startswith(b"II*\x00")
        for method in ("ndwi", "ndbi"):
            r = c.post(
                url,
                files={
                    "file": ("input.tif", payload, "image/tiff"),
                    "second": ("second.tif", second.read_bytes(), "image/tiff"),
                },
                data={"method": method, "band_a": "1", "band_b": "1"},
            )
            assert r.status_code == 200, r.text
            assert r.json()["stats"]["mean"] == 0.5
        r = c.post(
            url,
            files={"file": ("dem.tif", dem.read_bytes(), "image/tiff")},
            data={"method": "hillshade", "band_a": "1"},
        )
        assert r.status_code == 200, r.text
        assert abs(r.json()["stats"]["mean"] - 180.312) < 0.001
        assert (
            c.post(
                url,
                files={
                    "file": ("input.tif", payload, "image/tiff"),
                    "second": ("dem.tif", dem.read_bytes(), "image/tiff"),
                },
                data={"method": "ndvi", "band_b": "1"},
            ).status_code
            == 422
        )
        assert (
            c.post(
                url,
                files={"file": ("fake.tif", b"not TIFF", "image/tiff")},
                data={"method": "band"},
            ).status_code
            == 422
        )
        assert (
            c.post(
                url,
                files={"file": ("input.tif", payload, "image/tiff")},
                data={"method": "ndvi", "band_a": "16"},
            ).status_code
            == 422
        )
        assert (
            c.post(
                url,
                files={"file": ("input.tif", payload, "image/tiff")},
                data={"method": "hillshade"},
            ).status_code
            == 422
        )
    with _client(world.wrong_user_id) as other:
        assert other.post(
            url, files={"file": ("input.tif", payload, "image/tiff")}, data={"method": "band"}
        ).status_code in (403, 404)
