import subprocess

from sqlalchemy import text

from tests.fixtures.db_admin import get_admin_engine
from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.spatial.test_layers import _path
from tests.integration.spatial.test_sources import _client


def test_persistent_raster_lifecycle(migrated_database, tmp_path):
    world = seed_access_world()
    base = _path(world).rsplit("/", 1)[0] + "/raster/layers"
    file = tmp_path / "large.tif"
    subprocess.run(
        [
            "/usr/bin/python3",
            "-c",
            """
from osgeo import gdal,osr
import sys
d=gdal.GetDriverByName('GTiff').Create(sys.argv[1],4000,4000,2,gdal.GDT_UInt16,options=['COMPRESS=DEFLATE','TILED=YES'])
s=osr.SpatialReference();s.ImportFromEPSG(32648)
d.SetProjection(s.ExportToWkt());d.SetGeoTransform((675000,10,0,1180000,0,-10))
d.GetRasterBand(1).Fill(6000);d.GetRasterBand(2).Fill(2000);d=None
""",
            str(file),
        ],
        check=True,
    )
    with _client(world.owner_user_id) as c:
        assert c.get(base).json()["items"] == []
        r = c.post(
            base,
            files={"file": ("large.tif", file.read_bytes(), "image/tiff")},
            data={"name": "NDVI test", "method": "ndvi"},
        )
        assert r.status_code == 201, r.text
        j = r.json()
        assert j["stats"]["valid"] == 16000000 and j["stats"]["mean"] == 0.5
        assert 1 <= j["tile_count"] <= 192
        assert j["width"] == 4000 and j["epsg"] == 32648
        path = base + "/" + j["id"]
        assert c.get(base).json()["items"][0] == j
        f = c.get(path + "/file")
        assert f.status_code == 200 and f.content.startswith(b"II*\x00")
        tile = c.get(path + "/tiles/0/0/0.png")
        assert tile.status_code == 200 and tile.content.startswith(b"\x89PNG")
        assert tile.headers["cache-control"] == "private, no-store"
        assert c.get(path + "/tiles/25/0/0.png").status_code == 404
        with _client(world.wrong_user_id) as other:
            for url in (base, path + "/file", path + "/tiles/0/0/0.png"):
                assert other.get(url).status_code in (403, 404)
        with get_admin_engine().begin() as db:
            db.execute(
                text(
                    "INSERT INTO gis_access.scoped_members(organization_id,p"
                    "roject_id,user_id) VALUES(:o,:p,:u)"
                ),
                {"o": world.organization_id, "p": world.project_id, "u": world.member_user_id},
            )
        with _client(world.member_user_id) as scoped:
            for url in (base, path + "/file", path + "/tiles/0/0/0.png"):
                assert scoped.get(url).status_code in (403, 404)
        assert c.delete(path).status_code == 204
        assert c.get(base).json()["items"] == []
        assert c.get(path + "/file").status_code == 404
        assert c.get(path + "/tiles/0/0/0.png").status_code == 404
