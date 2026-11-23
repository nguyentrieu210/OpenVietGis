import subprocess

from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.spatial.test_layers import _path
from tests.integration.spatial.test_sources import _client


def test_project_dem_preview(migrated_database, tmp_path):
    world = seed_access_world()
    base = _path(world).rsplit("/", 1)[0] + "/raster/layers"
    tif = tmp_path / "dem.tif"
    subprocess.run(
        [
            "/usr/bin/python3",
            "-c",
            """
from osgeo import gdal,osr
import numpy as np,sys
d=gdal.GetDriverByName('GTiff').Create(sys.argv[1],4,4,1,gdal.GDT_Float32)
s=osr.SpatialReference();s.ImportFromEPSG(3857)
d.SetProjection(s.ExportToWkt());d.SetGeoTransform((11800000,10,0,1200040,0,-10))
b=d.GetRasterBand(1);b.SetNoDataValue(-9999)
b.WriteArray(np.array([[-5,0,5,10],[0,5,10,15],[5,10,15,20],[10,15,20,-9999]],dtype=np.float32))
d=None
""",
            str(tif),
        ],
        check=True,
    )
    profile = {
        "elevation_unit": "metre",
        "vertical_reference": "orthometric",
        "reference_label": "Synthetic test datum",
        "acknowledge_dem": True,
    }
    with _client(world.owner_user_id) as owner:
        saved = owner.post(
            base,
            data={"name": "Synthetic DEM", "method": "band"},
            files={"file": ("dem.tif", tif.read_bytes(), "image/tiff")},
        )
        assert saved.status_code == 201, saved.text
        rid = saved.json()["id"]
        path = f"{base}/{rid}/terrain-preview"
        assert owner.post(path, json=profile | {"acknowledge_dem": False}).status_code == 422
        assert owner.post(path, json=profile | {"elevation_unit": "feet"}).status_code == 422
        r = owner.post(path, json=profile)
        assert r.status_code == 200, r.text
        assert "no-store" in r.headers["cache-control"]
        dem = r.json()
        assert dem["source_id"] == rid and dem["crs_code"] == 3857
        assert dem["extent"] == [11800005.0, 1200005.0, 11800035.0, 1200035.0]
        assert (dem["width"], dem["height"]) == (4, 4)
        assert dem["values"] == [-5, 0, 5, 10, 0, 5, 10, 15, 5, 10, 15, 20, 10, 15, 20, None]
        assert dem["min_m"] == dem["render_origin_m"] == -5
        assert dem["max_m"] == 20 and dem["nodata_count"] == 1
        assert dem["survey_grade"] is False and dem["declared_by_user"] is True
        assert dem["reference_label"] == profile["reference_label"]
    with _client(world.wrong_user_id) as outsider:
        assert outsider.post(path, json=profile).status_code in (403, 404)
