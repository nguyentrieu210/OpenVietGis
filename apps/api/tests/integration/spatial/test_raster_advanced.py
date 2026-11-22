import json
import subprocess

from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.spatial.test_layers import _path
from tests.integration.spatial.test_sources import _client


def test_advanced_raster_operations(migrated_database, tmp_path):
    world = seed_access_world()
    base = _path(world).rsplit("/", 1)[0] + "/raster/layers"
    path = tmp_path / "source.tif"
    subprocess.run(
        [
            "/usr/bin/python3",
            "-c",
            """
from osgeo import gdal,osr
import numpy as np,sys
d=gdal.GetDriverByName('GTiff').Create(sys.argv[1],4,4,3,gdal.GDT_Float32)
s=osr.SpatialReference();s.ImportFromEPSG(4326)
d.SetProjection(s.ExportToWkt());d.SetGeoTransform((106,.125,0,10,0,-.125))
a=np.arange(16,dtype=np.float32).reshape(4,4)+1
for i,vals in enumerate([a,a+4,2*a],1):
    b=d.GetRasterBand(i);b.SetNoDataValue(-9999);b.WriteArray(vals)
d=None
""",
            str(path),
        ],
        check=True,
    )
    payload = path.read_bytes()

    def inspect(data, name, check):
        out = tmp_path / name
        out.write_bytes(data)
        subprocess.run(
            [
                "/usr/bin/python3",
                "-c",
                "from osgeo import gdal;import numpy as np,sys;d=gdal.Open(sys.argv[1]);" + check,
                str(out),
            ],
            check=True,
        )

    with _client(world.owner_user_id) as c:

        def create(method, **data):
            r = c.post(
                base,
                files={"file": ("source.tif", payload, "image/tiff")},
                data={"name": method, "method": method, **data},
            )
            assert r.status_code == 201, r.text
            return r.json()

        rgb = create("rgb", band_a=1, band_b=2, band_c=3)
        assert rgb["output_bands"] == 3 and rgb["channels"][1]["mean"] == 12.5
        inspect(
            c.get(base + "/" + rgb["id"] + "/file").content,
            "rgb.tif",
            (
                "assert d.RasterCount==3;assert np.array_equal(d.GetRast"
                "erBand(3).ReadAsArray(),2*d.GetRasterBand(1).ReadAsArray())"
            ),
        )
        calc = create("calculator", formula="(B - A) * 2", band_a=1, band_b=2)
        assert calc["stats"]["mean"] == 8
        assert calc["formula"] == "(B - A) * 2"
        change = create("change", band_a=1, band_b=2)
        assert change["stats"]["mean"] == 4 and change["legend"] == [-4, 4]
        threshold = create("calculator", formula="A > 8", band_a=1)
        assert threshold["stats"]["mean"] == 0.5
        clip = {
            "type": "Polygon",
            "coordinates": [
                [
                    [106.125, 9.625],
                    [106.375, 9.625],
                    [106.375, 9.875],
                    [106.125, 9.875],
                    [106.125, 9.625],
                ]
            ],
        }
        cropped = create("clip", clip_geojson=json.dumps(clip), band_a=1)
        assert (cropped["width"], cropped["height"]) == (2, 2) and cropped["stats"]["mean"] == 8.5
        inspect(
            c.get(base + "/" + cropped["id"] + "/file").content,
            "clip.tif",
            (
                "assert d.GetGeoTransform()==(106.125,.125,0.,9.875,0.,-"
                ".125);assert np.array_equal(d.ReadAsArray(),np.array([["
                "6,7],[10,11]]))"
            ),
        )
        hole = {
            "type": "Polygon",
            "coordinates": [
                [[106, 9.5], [106.5, 9.5], [106.5, 10], [106, 10], [106, 9.5]],
                clip["coordinates"][0],
            ],
        }
        masked = create("clip", clip_geojson=json.dumps(hole), band_a=1)
        assert masked["stats"]["valid"] == 12 and masked["stats"]["nodata"] == 4
        inspect(
            c.get(base + "/" + masked["id"] + "/file").content,
            "hole.tif",
            "assert np.all(d.ReadAsArray()[1:3,1:3]==-9999)",
        )
        divided = create("calculator", formula="A / (A - 1)", band_a=1)
        assert divided["stats"]["valid"] == 15 and divided["stats"]["nodata"] == 1
        # Real stored layer inputs, no upload; raw RGB COG retains all three bands.
        r = c.post(
            base,
            data={
                "name": "derived",
                "method": "calculator",
                "source_id": rgb["id"],
                "band_a": 1,
                "band_b": 2,
                "formula": "B-A",
            },
        )
        assert r.status_code == 201, r.text
        assert r.json()["stats"]["mean"] == 4 and r.json()["inputs"]["a"] == rgb["id"]
        r = c.post(
            base,
            data={
                "name": "change stored",
                "method": "change",
                "source_id": calc["id"],
                "second_id": change["id"],
                "band_a": 1,
                "band_b": 1,
            },
        )
        assert r.status_code == 201, r.text
        assert r.json()["stats"]["mean"] == -4
        r = c.post(
            base,
            data={
                "name": "rgb stored",
                "method": "rgb",
                "source_id": rgb["id"],
                "second_id": calc["id"],
                "third_id": change["id"],
                "band_a": 1,
                "band_b": 1,
                "band_c": 1,
            },
        )
        assert r.status_code == 201, r.text
        assert r.json()["output_bands"] == 3 and r.json()["channels"][2]["mean"] == 4
        r = c.post(
            base,
            data={
                "name": "mismatch",
                "method": "change",
                "source_id": cropped["id"],
                "second_id": calc["id"],
                "band_a": 1,
                "band_b": 1,
            },
        )
        assert r.status_code == 422, r.text
        for formula in ('__import__("os")', "A.__class__", "A[0]", "A ** 2", "A / 0"):
            r = c.post(
                base,
                data={
                    "name": "bad",
                    "method": "calculator",
                    "source_id": rgb["id"],
                    "formula": formula,
                },
            )
            assert r.status_code == 422, r.text
        r = c.post(
            base,
            data={
                "name": "bad",
                "method": "clip",
                "source_id": rgb["id"],
                "clip_geojson": '{"type":"Point","coordinates":[106,10]}',
            },
        )
        assert r.status_code == 422, r.text
        missing = "00000000-0000-0000-0000-000000000001"
        assert (
            c.post(base, data={"name": "bad", "method": "band", "source_id": missing}).status_code
            == 404
        )
        with _client(world.wrong_user_id) as other:
            assert other.post(
                base, data={"name": "bad", "method": "band", "source_id": rgb["id"]}
            ).status_code in (403, 404)
