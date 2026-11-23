import io
from uuid import uuid4

import ezdxf
from sqlalchemy import text

from tests.fixtures.db_admin import get_admin_engine
from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.spatial.test_layers import _path as _layer_path
from tests.integration.spatial.test_sources import _client, _get
from tests.integration.spatial.test_sources import _path as _source_path


def _drawing() -> bytes:
    doc = ezdxf.new("R2010")
    doc.header["$INSUNITS"] = 4
    doc.modelspace().add_lwpolyline([(0, 0), (10000, 0), (10000, 5000), (0, 5000)], close=True)
    target = io.StringIO()
    doc.write(target)
    return target.getvalue().encode()


def test_version_pinned_georeference(migrated_database):
    world = seed_access_world()
    base = (
        f"/api/v1/organizations/{world.organization_id}/projects/{world.project_id}/cad/documents"
    )
    control = {
        "crs_code": 3857,
        "units": "millimeter",
        "tolerance_m": 0.005,
        "max_scale_deviation": 0.01,
        "controls": [
            [[0, 0], [11800000, 1200000]],
            [[10000, 0], [11800000, 1200010]],
            [[0, 5000], [11799995, 1200000]],
        ],
    }
    with _client(world.owner_user_id) as owner:
        response = owner.post(base, files={"file": ("test.dxf", _drawing())})
        assert response.status_code == 201, response.text
        file = response.json()
        path = f"{base}/{file['id']}/versions/1"
        invalid = control | {
            "controls": [[[0, 0], [100, 100]], [[1000, 0], [101, 100]], [[2000, 0], [102, 100]]]
        }
        assert owner.post(path + "/inspect-georef", json=invalid).status_code == 422
        r = owner.post(path + "/inspect-georef", json=control)
        assert r.status_code == 200, r.text
        inspection = r.json()
        assert inspection["published"] is False
        summary = inspection["entity_summary"]
        assert summary["cad_document_id"] == file["id"]
        assert summary["cad_document_version"] == 1
        assert summary["placement"]["max_residual_grid_m"] < 0.000001
        assert abs(summary["placed_extent"]["min_x"] - 11799995) < 1e-6
        body = control | {
            "layer_name": "Placed CAD",
            "expected_sha256": file["sha256"],
            "expected_inspection_fingerprint": inspection["inspection_fingerprint"],
            "acknowledge_loss": True,
        }
        changed = body | {
            "controls": [
                [[0, 0], [11800001, 1200000]],
                [[10000, 0], [11800001, 1200010]],
                [[0, 5000], [11799996, 1200000]],
            ]
        }
        assert owner.post(path + "/import-georef", json=changed).status_code == 409
        r = owner.post(path + "/import-georef", json=body)
        assert r.status_code == 201, r.text
        published = r.json()
        repeat = owner.post(path + "/import-georef", json=body)
        assert repeat.status_code == 201, repeat.text
        assert repeat.json()["layer_id"] == published["layer_id"]
        with get_admin_engine().begin() as conn:
            row = conn.execute(
                text("""SELECT ST_SRID(geometry),ST_XMin(geometry),ST_XMax(geometry),
                 ST_YMin(geometry),ST_YMax(geometry),ST_Area(geometry)
                 FROM spatial.reference_features WHERE source_id=:sid"""),
                {"sid": published["source_id"]},
            ).one()
            assert row[0] == 3857
            assert abs(row[1] - 11799995) < 1e-6 and abs(row[2] - 11800000) < 1e-6
            assert abs(row[3] - 1200000) < 1e-6 and abs(row[4] - 1200010) < 1e-6
            assert abs(row[5] - 50) < 1e-6
        feature = summary["entity_mapping"][0]
        links = owner.get(
            base + "/links",
            params={"source_id": published["source_id"], "feature_id": feature["feature_id"]},
        )
        assert links.status_code == 200, links.text
        link = links.json()["items"][0]
        assert link["document_id"] == file["id"] and link["version"] == 1
        assert link["handle"] == feature["handle"] and link["original_revision"] is True
        reverse = owner.get(path + "/links", params={"handle": feature["handle"]})
        assert reverse.json()["items"] == [link]
        assert owner.get(path + "/links", params={"handle": "FFFF"}).json()["items"] == []
        # CAD conversion has no height attribute: explicit uniform height leaves GIS unchanged.
        prefix = base.rsplit("/cad/", 1)[0]
        layer = owner.get(prefix + "/layers/" + published["layer_id"]).json()
        request = {
            "name": "CAD buildings",
            "layer_id": layer["id"],
            "expected_layer_revision": layer["layer_revision"],
            "expected_content_revision": layer["source"]["content_revision"],
            "expected_schema_revision": layer["source"]["schema_revision"],
            "uniform_height_m": 12,
            "base_ellipsoid_m": 0,
            "acknowledge_ellipsoid": True,
        }
        tiles = prefix + "/3d/tilesets"
        assert (
            owner.post(
                tiles + "/generate", json=request | {"height_field": "cad_layer"}
            ).status_code
            == 422
        )
        assert (
            owner.post(tiles + "/generate", json=request | {"uniform_height_m": None}).status_code
            == 422
        )
        generated = owner.post(tiles + "/generate", json=request)
        assert generated.status_code == 201, generated.text
        record = generated.json()
        assert record["provenance"]["uniform_height_m"] == 12
        tile_path = tiles + "/" + record["id"]
        uri = owner.get(tile_path + "/tileset.json").json()["root"]["children"][0]["content"]["uri"]
        picked = owner.get(tile_path + "/feature", params={"uri": uri, "triangle": 0})
        assert picked.status_code == 200, picked.text
        assert picked.json()["state"] == "current"
        assert picked.json()["feature_id"] == feature["feature_id"]
        assert picked.json()["cad_links"] == [link]
        assert set(picked.json()["attributes"]) == {"cad_layer", "cad_entity"}
        assert (
            owner.get(prefix + "/layers/" + published["layer_id"]).json()["source"][
                "content_revision"
            ]
            == request["expected_content_revision"]
        )
        # Explicit dual local placement, not a generic datum transformation.
        vn = control | {
            "crs_code": 5899,
            "controls": [
                [[0, 0], [439000, 2290000]],
                [[10000, 0], [439000, 2290010]],
                [[0, 5000], [438995, 2290000]],
            ],
            "web_controls": [
                [[0, 0], [11929000, 2359000]],
                [[10000, 0], [11929000, 2359010]],
                [[0, 5000], [11928995, 2359000]],
            ],
        }
        assert (
            owner.post(path + "/inspect-georef", json=vn | {"web_controls": None}).status_code
            == 422
        )
        wrong = vn | {
            "web_controls": [
                [[2, 0], [11800000, 1200000]],
                *control["controls"][1:],
            ]
        }
        assert owner.post(path + "/inspect-georef", json=wrong).status_code == 422
        unrelated = vn | {"web_controls": control["controls"]}
        assert owner.post(path + "/inspect-georef", json=unrelated).status_code == 422
        inspected_vn = owner.post(path + "/inspect-georef", json=vn)
        assert inspected_vn.status_code == 200, inspected_vn.text
        details = inspected_vn.json()
        assert details["inspection_fingerprint"] != inspection["inspection_fingerprint"]
        native = details["entity_summary"]["placement"]["native_reference"]
        assert native["crs_code"] == 5899
        assert abs(details["entity_summary"]["native_placed_extent"]["min_x"] - 438995) < 1e-6
        confirm_vn = vn | {
            "layer_name": "VN local calibrated map",
            "expected_sha256": file["sha256"],
            "expected_inspection_fingerprint": details["inspection_fingerprint"],
            "acknowledge_loss": True,
        }
        posted_vn = owner.post(path + "/import-georef", json=confirm_vn)
        assert posted_vn.status_code == 201, posted_vn.text
        assert posted_vn.json()["layer_id"] != published["layer_id"]
        with get_admin_engine().begin() as conn:
            checked = conn.execute(
                text("""
                SELECT ST_SRID(geometry), ST_XMin(geometry), ST_Area(geometry)
                FROM spatial.reference_features WHERE source_id=:sid
            """),
                {"sid": posted_vn.json()["source_id"]},
            ).one()
            assert checked[0] == 3857
            assert abs(checked[1] - 11928995) < 1e-6 and abs(checked[2] - 50) < 1e-6
        vn_result = posted_vn.json()
        source = _get(owner, _source_path(world), vn_result["source_id"])
        displayed = owner.get(
            f"{_layer_path(world)}/{vn_result['layer_id']}/features",
            params={
                "content_revision": source["content_revision"],
                "schema_revision": source["schema_revision"],
                "include_geometry": True,
                "response_crs": 4326,
            },
        )
        assert displayed.status_code == 200, displayed.text
        geom = displayed.json()["items"][0]["geometry"]
        assert geom["coordinate_reference"]["code"] == 4326
        assert 107 < geom["coordinates"][0][0][0] < 108
        assert 20 < geom["coordinates"][0][0][1] < 21
        fake = base + "/" + str(uuid4()) + "/versions/1/inspect-georef"
        assert owner.post(fake, json=control).status_code == 404
    with _client(world.wrong_user_id) as outsider:
        assert outsider.post(path + "/inspect-georef", json=control).status_code in (403, 404)
        assert outsider.post(path + "/import-georef", json=body).status_code in (403, 404)
        assert outsider.get(
            base + "/links",
            params={"source_id": published["source_id"], "feature_id": feature["feature_id"]},
        ).status_code in (403, 404)
        assert outsider.get(path + "/links", params={"handle": feature["handle"]}).status_code in (
            403,
            404,
        )
