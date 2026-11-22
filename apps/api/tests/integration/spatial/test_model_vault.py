import base64
import hashlib
import json
import math
import struct
from uuid import uuid4

from sqlalchemy import text

from tests.fixtures.db_admin import get_admin_engine
from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.spatial.test_cad_georef import _drawing
from tests.integration.spatial.test_sources import _client


def _model():
    binary = struct.pack("<9f", 0, 0, 0, 1, 0, 0, 0, 1, 0)
    doc = {
        "asset": {"version": "2.0"},
        "buffers": [
            {
                "byteLength": len(binary),
                "uri": "data:application/octet-stream;base64," + base64.b64encode(binary).decode(),
            }
        ],
        "bufferViews": [{"buffer": 0, "byteLength": len(binary)}],
        "accessors": [{"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3"}],
        "meshes": [{"primitives": [{"attributes": {"POSITION": 0}}]}],
        "nodes": [{"mesh": 0}],
        "scenes": [{"nodes": [0]}],
    }
    return json.dumps(doc).encode()


def test_model_vault(migrated_database):
    world = seed_access_world()
    prefix = f"/api/v1/organizations/{world.organization_id}/projects/{world.project_id}"
    base = prefix + "/3d/models"
    data = _model()
    with _client(world.owner_user_id) as owner:
        # Actual CAD import -> GIS feature -> model, no artificial insert of model rows.
        d = owner.post(prefix + "/cad/documents", files={"file": ("shape.dxf", _drawing())})
        assert d.status_code == 201, d.text
        doc = d.json()
        cad = prefix + "/cad/documents/" + doc["id"] + "/versions/1"
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
        inspected = owner.post(cad + "/inspect-georef", json=control)
        assert inspected.status_code == 200, inspected.text
        inspect = inspected.json()
        imported = owner.post(
            cad + "/import-georef",
            json=control
            | {
                "layer_name": "Model anchor",
                "expected_sha256": doc["sha256"],
                "expected_inspection_fingerprint": inspect["inspection_fingerprint"],
                "acknowledge_loss": True,
            },
        )
        assert imported.status_code == 201, imported.text
        profile = {
            "source_id": imported.json()["source_id"],
            "feature_id": inspect["entity_summary"]["entity_mapping"][0]["feature_id"],
            "scale": 1,
            "rotation_deg": 0,
            "offset_m": -2,
        }

        def upload(payload=data, placement=profile):
            return owner.post(
                base,
                files={"file": ("triangle.gltf", payload)},
                data={"placement": json.dumps(placement)},
            )

        assert upload(b"{}").status_code == 422
        external = json.loads(data)
        external["buffers"][0]["uri"] = "https://invalid.test/model.bin"
        assert upload(json.dumps(external).encode()).status_code == 422
        assert upload(placement=profile | {"source_id": str(uuid4())}).status_code == 422
        assert (
            upload(placement=profile | {"dem_id": str(uuid4()), "reference_label": "x"}).status_code
            == 422
        )
        assert upload(placement=profile | {"scale": 0}).status_code == 422
        posted = upload()
        assert posted.status_code == 201, posted.text
        model = posted.json()
        assert model["sha256"] == hashlib.sha256(data).hexdigest()
        assert model["profile"]["crs_code"] == 4326 and model["profile"]["offset_m"] == -2
        lon = (11799995 + 11800000) / 2 * 180 / 20037508.342789244
        lat = (
            sum(
                math.degrees(math.atan(math.sinh(y * math.pi / 20037508.342789244)))
                for y in (1200000, 1200010)
            )
            / 2
        )
        assert abs(model["profile"]["longitude"] - lon) < 1e-9
        assert abs(model["profile"]["latitude"] - lat) < 1e-9
        assert model["profile"]["altitude_mode"] == "relative-flat"
        path = base + "/" + model["id"]
        download = owner.get(path + "/download")
        assert download.content == data and "no-store" in download.headers["cache-control"]
        revise = owner.post(
            path + "/placements", json=profile | {"expected_version": 1, "rotation_deg": 90}
        )
        assert revise.status_code == 201, revise.text
        assert revise.json()["version"] == 2 and revise.json()["profile"]["rotation_deg"] == 90
        assert (
            owner.post(path + "/placements", json=profile | {"expected_version": 1}).status_code
            == 409
        )
        assert owner.get(base).json()["items"] == [revise.json()]
        assert owner.get(path + "/placements").json()["items"] == [revise.json(), model]
        with get_admin_engine().begin() as conn:
            assert (
                conn.execute(
                    text("SELECT count(*) FROM spatial.model_assets WHERE id=:id"),
                    {"id": model["id"]},
                ).scalar_one()
                == 1
            )
            assert (
                conn.execute(
                    text("SELECT count(*) FROM spatial.model_placements WHERE model_id=:id"),
                    {"id": model["id"]},
                ).scalar_one()
                == 2
            )
    with _client(world.owner_user_id) as owner, _client(world.member_user_id) as member:
        org = prefix.split("/projects/")[0]
        grant = owner.post(
            org + "/grants",
            json={
                "subject_kind": "membership",
                "subject_id": str(world.member_membership_id),
                "scope_kind": "project",
                "project_id": str(world.project_id),
                "role": "editor",
            },
        )
        assert grant.status_code == 201, grant.text
        assert (
            next(v for v in member.get(base).json()["items"] if v["id"] == model["id"])["version"]
            == 2
        )
        assert member.get(path + "/download").content == data
        rule = owner.post(
            prefix + "/access/rules",
            json={
                "user_id": str(world.member_user_id),
                "layer_id": imported.json()["layer_id"],
                "actions": ["read", "geometry", "attributes"],
            },
        )
        assert rule.status_code == 201, rule.text
        assert member.get(prefix + "/access/mine").json()["restricted"]
        for url in (base, path + "/download", path + "/placements"):
            assert member.get(url).status_code == 403
        assert (
            member.post(path + "/placements", json=profile | {"expected_version": 2}).status_code
            == 403
        )
    with _client(world.wrong_user_id) as other:
        for url in (base, path + "/download", path + "/placements"):
            assert other.get(url).status_code in (403, 404)
        assert other.post(
            path + "/placements", json=profile | {"expected_version": 2}
        ).status_code in (403, 404)
