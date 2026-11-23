import json
import math
import struct
from uuid import uuid4

from sqlalchemy import text

from tests.fixtures.db_admin import get_admin_engine
from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.spatial.test_sources import _client


def polygon(x, y, size):
    return [[x, y], [x + size, y], [x + size, y + size], [x, y + size], [x, y]]


def decoded(payload, matrix):
    _, version, size, js, kind = struct.unpack_from("<5I", payload)
    assert version == 2 and size == len(payload) and kind == 0x4E4F534A
    asset = json.loads(payload[20 : 20 + js])
    count = asset["accessors"][0]["count"]
    assert count % 3 == 0
    xyz = struct.unpack_from(f"<{3 * count}f", payload, 28 + js)
    # glTF y-up -> z-up and explicit world transform (column major).
    return [
        (
            matrix[0] * x - matrix[4] * z + matrix[8] * y + matrix[12],
            matrix[1] * x - matrix[5] * z + matrix[9] * y + matrix[13],
            matrix[2] * x - matrix[6] * z + matrix[10] * y + matrix[14],
        )
        for x, y, z in zip(xyz[::3], xyz[1::3], xyz[2::3], strict=True)
    ]


def test_generate_tileset(migrated_database, tmp_path):
    world = seed_access_world()
    prefix = f"/api/v1/organizations/{world.organization_id}/projects/{world.project_id}"
    path = prefix + "/3d/tilesets"
    layers = prefix + "/layers"
    outer = polygon(106.61, 10.71, 0.0008)
    hole = polygon(106.6102, 10.7102, 0.0004)[::-1]
    with _client(world.owner_user_id) as owner:
        created = owner.post(
            layers + "/manual",
            json={
                "operation_id": str(uuid4()),
                "name": "Courtyard",
                "geometry_family": "Polygon",
                "fields": [
                    {
                        "field_id": "fld_height",
                        "name": "Height",
                        "type": "decimal",
                        "nullable": True,
                    }
                ],
            },
        )
        assert created.status_code == 201, created.text
        lid, sid = created.json()["layer_id"], created.json()["source_id"]

        def layer():
            r = owner.get(layers + "/" + lid)
            assert r.status_code == 200, r.text
            return r.json()

        def commit(ops):
            s = layer()["source"]
            r = owner.post(
                prefix + "/sources/" + sid + "/changesets",
                json={
                    "operation_id": str(uuid4()),
                    "base_content_revision": s["content_revision"],
                    "base_schema_revision": s["schema_revision"],
                    "summary": "Synthetic courtyard",
                    "operations": ops,
                },
            )
            assert r.status_code in (200, 201), r.text

        commit(
            [
                {
                    "op_type": "create",
                    "feature_id": "court",
                    "geometry": {"type": "Polygon", "coordinates": [outer, hole]},
                    "properties": {"fld_height": "18.5"},
                },
                {
                    "op_type": "create",
                    "feature_id": "other",
                    "geometry": {
                        "type": "Polygon",
                        "coordinates": [polygon(106.611, 10.71, 0.0005)],
                    },
                    "properties": {"fld_height": "10"},
                },
            ]
        )

        def body():
            current = layer()
            return {
                "name": "Buildings",
                "layer_id": lid,
                "height_field": "fld_height",
                "expected_layer_revision": current["layer_revision"],
                "expected_content_revision": current["source"]["content_revision"],
                "expected_schema_revision": current["source"]["schema_revision"],
                "base_ellipsoid_m": 20,
                "acknowledge_ellipsoid": True,
            }

        request = body()
        for invalid in (
            {"acknowledge_ellipsoid": False},
            {"height_field": "unknown"},
            {"expected_content_revision": "0" * 64},
            {"layer_id": str(uuid4())},
        ):
            response = owner.post(path + "/generate", json=request | invalid)
            assert response.status_code in (404, 409, 422), response.text
        assert owner.get(path).json()["items"] == []
        response = owner.post(path + "/generate", json=request)
        assert response.status_code == 201, response.text
        record = response.json()
        assert record["provenance"]["feature_count"] == 2
        assert record["provenance"]["triangle_count"] == 44
        assert record["provenance"]["content_revision"] == request["expected_content_revision"]
        assert record["provenance"]["source_crs"] == 4326
        assert layer()["source"]["content_revision"] == request["expected_content_revision"]
        resource = path + "/" + record["id"]
        doc = owner.get(resource + "/tileset.json").json()
        assert doc["asset"]["version"] == "1.1" and doc["root"]["refine"] == "REPLACE"
        (tmp_path / "tileset.json").write_text(json.dumps(doc))
        (tmp_path / "record.json").write_text(json.dumps(record))
        (tmp_path / "layer.json").write_text(json.dumps(layer()))
        all_points = []
        roofs = []
        with get_admin_engine().connect() as conn:
            for child in doc["root"]["children"]:
                uri = child["content"]["uri"]
                payload = owner.get(resource + "/" + uri).content
                target = tmp_path / uri
                target.parent.mkdir(exist_ok=True)
                target.write_bytes(payload)
                points = decoded(payload, child["transform"])
                coordinates = conn.execute(
                    text("""
                    SELECT ST_X(q),ST_Y(q),ST_Z(q) FROM (
                      SELECT ST_Transform(ST_SetSRID(ST_MakePoint(
                        (p->>0)::float8,(p->>1)::float8,(p->>2)::float8),4978),4979) AS q
                      FROM jsonb_array_elements(CAST(:points AS jsonb)) AS p
                    ) AS t
                """),
                    {"points": json.dumps(points)},
                ).all()
                r = child["boundingVolume"]["region"]
                for lon, lat, h in coordinates:
                    assert r[0] <= math.radians(lon) <= r[2] and r[1] <= math.radians(lat) <= r[3]
                    assert r[4] <= h <= r[5]
                    assert min(abs(h - level) for level in (20, 30, 38.5)) < 0.001
                all_points.extend(coordinates)
                for a, b, c in zip(
                    coordinates[::3], coordinates[1::3], coordinates[2::3], strict=True
                ):
                    if all(abs(p[2] - 38.5) < 0.001 for p in (a, b, c)):
                        roofs.append((a, b, c))
            assert len(all_points) == 44 * 3 and len(roofs) == 8
            # An independent Geographic CRS control and roof area, hole must remain empty.
            assert any(
                abs(x - outer[0][0]) < 1e-8
                and abs(y - outer[0][1]) < 1e-8
                and abs(h - 38.5) < 0.001
                for x, y, h in all_points
            )
            area = 0
            for a, b, c in roofs:
                x = sum(p[0] for p in (a, b, c)) / 3
                y = sum(p[1] for p in (a, b, c)) / 3
                assert not (106.6102 < x < 106.6106 and 10.7102 < y < 10.7106)
                area += abs((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])) / 2
            assert abs(area - (0.0008**2 - 0.0004**2)) < 1e-10
            assert (
                conn.execute(
                    text("""
                SELECT count(*) FROM audit.audit_events WHERE organization_id=:org
                    AND resource_id=:id AND action='3d.tileset.generate'
                """),
                    {"org": world.organization_id, "id": record["id"]},
                ).scalar_one()
                == 1
            )
        # Invalid height after editing is all-or-nothing; old generated asset remains immutable.
        commit([{"op_type": "update", "feature_id": "other", "properties": {"fld_height": None}}])
        assert owner.post(path + "/generate", json=request).status_code == 409
        bad = owner.post(path + "/generate", json=body())
        assert bad.status_code == 422, bad.text
        assert owner.get(path).json()["items"] == [record]
        org = prefix.split("/projects/")[0]
        grant = owner.post(
            org + "/grants",
            json={
                "subject_kind": "membership",
                "subject_id": str(world.member_membership_id),
                "scope_kind": "project",
                "project_id": str(world.project_id),
                "role": "viewer",
            },
        )
        assert grant.status_code == 201, grant.text
    with _client(world.member_user_id) as member:
        assert member.get(resource + "/tileset.json").status_code == 200
        assert member.post(path + "/generate", json=request).status_code == 403
    with _client(world.wrong_user_id) as other:
        assert other.get(resource + "/tileset.json").status_code in (403, 404)
        assert other.post(path + "/generate", json=request).status_code in (403, 404)
