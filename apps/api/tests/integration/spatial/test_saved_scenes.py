import json
import subprocess
from uuid import uuid4

from sqlalchemy import text

from tests.fixtures.db_admin import get_admin_engine
from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.spatial.test_model_vault import _model
from tests.integration.spatial.test_sources import _client


def test_saved_scene(migrated_database, tmp_path):
    world = seed_access_world()
    root = f"/api/v1/organizations/{world.organization_id}/projects/{world.project_id}"
    base = root + "/3d/scenes"
    fid = "feat_" + uuid4().hex
    demfile = tmp_path / "terrain.tif"
    subprocess.run(
        [
            "/usr/bin/python3",
            "-c",
            """
from osgeo import gdal,osr
import numpy as np,sys
d=gdal.GetDriverByName('GTiff').Create(sys.argv[1],4,4,1,gdal.GDT_Float32)
s=osr.SpatialReference();s.ImportFromEPSG(4326)
d.SetProjection(s.ExportToWkt());d.SetGeoTransform((106.5,.05,0,10.9,0,-.05))
d.GetRasterBand(1).WriteArray(np.arange(16,dtype=np.float32).reshape(4,4))
d=None
""",
            str(demfile),
        ],
        check=True,
    )
    with _client(world.owner_user_id) as owner:
        layer = owner.post(
            root + "/layers/manual",
            json={
                "operation_id": str(uuid4()),
                "name": "Building height",
                "geometry_family": "Polygon",
                "fields": [
                    {"field_id": "height_m", "name": "Height", "type": "integer", "nullable": False}
                ],
            },
        )
        assert layer.status_code == 201, layer.text
        layer_info = layer.json()
        source = owner.get(root + "/layers/" + layer_info["layer_id"]).json()["source"]
        change = owner.post(
            root + "/sources/" + layer_info["source_id"] + "/changesets",
            json={
                "operation_id": str(uuid4()),
                "base_content_revision": source["content_revision"],
                "base_schema_revision": source["schema_revision"],
                "summary": "Scene anchor",
                "operations": [
                    {
                        "op_type": "create",
                        "feature_id": fid,
                        "geometry": {
                            "type": "Polygon",
                            "coordinates": [
                                [
                                    [106.55, 10.75],
                                    [106.56, 10.75],
                                    [106.56, 10.76],
                                    [106.55, 10.76],
                                    [106.55, 10.75],
                                ]
                            ],
                        },
                        "properties": {"height_m": 12},
                    }
                ],
            },
        )
        assert change.status_code in (200, 201), change.text
        raster = owner.post(
            root + "/raster/layers",
            data={"name": "Scene DEM", "method": "band"},
            files={"file": ("terrain.tif", demfile.read_bytes(), "image/tiff")},
        )
        assert raster.status_code == 201, raster.text
        dem = raster.json()["id"]
        placement = {
            "source_id": layer_info["source_id"],
            "feature_id": fid,
            "scale": 1,
            "rotation_deg": 0,
            "offset_m": 0,
            "dem_id": dem,
            "vertical_reference": "local",
            "reference_label": "Test datum",
        }
        models = []
        for i in range(2):
            r = owner.post(
                root + "/3d/models",
                files={"file": (f"building{i}.gltf", _model())},
                data={"placement": json.dumps(placement)},
            )
            assert r.status_code == 201, r.text
            models.append(r.json())
        profile = {
            "name": "Factory scene",
            "models": [
                {"id": models[0]["id"], "version": 1, "visible": True},
                {"id": models[1]["id"], "version": 1, "visible": False},
            ],
            "camera": {
                "crs_code": 4326,
                "longitude": 106.55,
                "latitude": 10.75,
                "zoom": 18,
                "bearing": 35,
                "pitch": 45,
            },
            "dem": {
                "id": dem,
                "vertical_reference": "local",
                "reference_label": "Test datum",
                "visible": True,
            },
            "footprint": {
                "layer_id": layer_info["layer_id"],
                "height_field": "height_m",
                "visible": False,
            },
        }
        assert (
            owner.post(
                base, json=profile | {"camera": profile["camera"] | {"pitch": 90}}
            ).status_code
            == 422
        )
        assert (
            owner.post(base, json=profile | {"models": [profile["models"][0]] * 2}).status_code
            == 422
        )
        assert (
            owner.post(
                base, json=profile | {"models": [{"id": str(uuid4()), "version": 1}]}
            ).status_code
            == 409
        )
        assert owner.post(base, json=profile | {"dem": None}).status_code == 422
        assert (
            owner.post(
                base,
                json=profile | {"footprint": profile["footprint"] | {"height_field": "missing"}},
            ).status_code
            == 409
        )
        posted = owner.post(base, json=profile)
        assert posted.status_code == 201, posted.text
        scene = posted.json()
        path = base + "/" + scene["id"] + "/versions"
        assert scene["profile"] == {k: v for k, v in profile.items() if k != "name"}
        changed = owner.post(
            root + "/3d/models/" + models[0]["id"] + "/placements",
            json=placement | {"expected_version": 1, "rotation_deg": 70},
        )
        assert changed.status_code == 201, changed.text
        opened = owner.get(path + "/1")
        assert opened.status_code == 200, opened.text
        assert opened.json()["models"] == models
        assert opened.json()["scene"] == scene
        assert "no-store" in opened.headers["cache-control"]
        revised = owner.post(
            path,
            json=profile
            | {
                "expected_version": 1,
                "name": "Factory v2",
                "models": [
                    profile["models"][0] | {"version": 2},
                    profile["models"][1] | {"visible": True},
                ],
                "camera": profile["camera"] | {"bearing": -30},
            },
        )
        assert revised.status_code == 201, revised.text
        assert owner.post(path, json=profile | {"expected_version": 1}).status_code == 409
        assert owner.get(path).json()["items"] == [revised.json(), scene]
        assert owner.get(base).json()["items"] == [revised.json()]
        assert owner.get(path + "/2").json()["models"][0] == changed.json()
        assert owner.get(path + "/1").json()["models"][0] == models[0]
        with get_admin_engine().begin() as conn:
            assert (
                conn.execute(
                    text("SELECT count(*) FROM spatial.scene_versions WHERE scene_id=:id"),
                    {"id": scene["id"]},
                ).scalar_one()
                == 2
            )
            assert (
                conn.execute(
                    text("SELECT count(*) FROM spatial.model_assets WHERE project_id=:p"),
                    {"p": world.project_id},
                ).scalar_one()
                == 2
            )
        grant = owner.post(
            root.split("/projects/")[0] + "/grants",
            json={
                "subject_kind": "membership",
                "subject_id": str(world.member_membership_id),
                "scope_kind": "project",
                "project_id": str(world.project_id),
                "role": "editor",
            },
        )
        assert grant.status_code == 201, grant.text
        with _client(world.member_user_id) as member:
            assert member.get(path + "/1").json()["models"] == models
            rule = owner.post(
                root + "/access/rules",
                json={
                    "user_id": str(world.member_user_id),
                    "layer_id": layer_info["layer_id"],
                    "actions": ["read", "geometry", "attributes"],
                },
            )
            assert rule.status_code == 201, rule.text
            assert member.get(base).status_code == 403
            assert member.get(path + "/1").status_code == 403
            assert member.post(path, json=profile | {"expected_version": 2}).status_code == 403
    with _client(world.wrong_user_id) as other:
        for url in (base, path, path + "/1"):
            assert other.get(url).status_code in (403, 404)
        assert other.post(base, json=profile).status_code in (403, 404)
    with _client(world.owner_user_id) as owner:
        # Deleting an explicitly synthetic raster invalidates but does not erase old scene history.
        assert owner.delete(root + "/raster/layers/" + dem).status_code == 204
        assert owner.get(path + "/1").status_code == 409
        assert owner.get(path).json()["items"] == [revised.json(), scene]
