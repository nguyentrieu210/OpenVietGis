import io
from uuid import uuid4

import ezdxf
from sqlalchemy import text

from tests.fixtures.db_admin import get_admin_engine
from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.spatial.test_sources import _client


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
        fake = base + "/" + str(uuid4()) + "/versions/1/inspect-georef"
        assert owner.post(fake, json=control).status_code == 404
    with _client(world.wrong_user_id) as outsider:
        assert outsider.post(path + "/inspect-georef", json=control).status_code in (403, 404)
        assert outsider.post(path + "/import-georef", json=body).status_code in (403, 404)
