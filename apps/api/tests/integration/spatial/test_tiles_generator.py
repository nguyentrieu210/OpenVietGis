import json
import math
import struct
from uuid import uuid4

from sqlalchemy import text

from openvietgis_api.adapters.formats.geopackage import write_geopackage
from openvietgis_api.domains.spatial.domain import FieldType, NativeGeometry
from openvietgis_api.domains.transfers.domain import ExportFieldMapping
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
        assert record["provenance"]["feature_picking"] is True
        picks = []
        uri = doc["root"]["children"][0]["content"]["uri"]
        pick_path = resource + "/feature"
        for triangle, fid, first, count, height in (
            (0, "court", 0, 32, 18.5),
            (31, "court", 0, 32, 18.5),
            (32, "other", 32, 12, 10),
            (43, "other", 32, 12, 10),
        ):
            picked = owner.get(pick_path, params={"uri": uri, "triangle": triangle})
            assert picked.status_code == 200, picked.text
            v = picked.json()
            assert v["state"] == "current" and v["feature_id"] == fid
            assert v["first_triangle"] == first and v["triangle_count"] == count
            assert float(v["attributes"]["fld_height"]) == height
            assert v["field_names"]["fld_height"] == "Height" and v["cad_links"] == []
            assert v["source_id"] == sid and v["layer_id"] == lid
            picks.append(v)
        (tmp_path / "picks.json").write_text(json.dumps(picks))
        reverse_path = resource + "/feature-by-id"
        base_query = {"source_id": sid, "layer_id": lid, "feature_id": "court"}
        for i, fid in ((0, "court"), (2, "other")):
            reply = owner.get(reverse_path, params=base_query | {"feature_id": fid})
            assert reply.status_code == 200, reply.text
            assert reply.json() == picks[i]
        for wrong in (
            {"feature_id": "missing"},
            {"source_id": str(uuid4())},
            {"layer_id": str(uuid4())},
        ):
            assert owner.get(reverse_path, params=base_query | wrong).status_code == 404
        assert owner.get(pick_path, params={"uri": uri, "triangle": 44}).status_code == 404
        assert owner.get(pick_path, params={"uri": "../bad.glb", "triangle": 0}).status_code == 404
        # Invalid height after editing is all-or-nothing; old generated asset remains immutable.
        commit([{"op_type": "update", "feature_id": "other", "properties": {"fld_height": None}}])
        stale = owner.get(pick_path, params={"uri": uri, "triangle": 0}).json()
        assert stale["state"] == "stale" and stale["feature_id"] == "court"
        assert "attributes" not in stale and "cad_links" not in stale
        assert owner.get(reverse_path, params=base_query).json() == stale
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
        assert member.get(pick_path, params={"uri": uri, "triangle": 0}).status_code == 200
        assert member.get(reverse_path, params=base_query).status_code == 200
        assert member.post(path + "/generate", json=request).status_code == 403
    with _client(world.wrong_user_id) as other:
        assert other.get(resource + "/tileset.json").status_code in (403, 404)
        assert other.get(reverse_path, params=base_query).status_code in (403, 404)
        assert other.get(pick_path, params={"uri": uri, "triangle": 0}).status_code in (403, 404)
        assert other.post(path + "/generate", json=request).status_code in (403, 404)


def test_generate_projected_tileset(migrated_database):
    world = seed_access_world()
    prefix = f"/api/v1/organizations/{world.organization_id}/projects/{world.project_id}"
    lon, lat = 106.61, 10.71
    radius = 6378137
    x = radius * math.radians(lon)
    y = radius * math.log(math.tan(math.pi / 4 + math.radians(lat) / 2))
    ring = tuple(tuple(p) for p in polygon(x, y, 40))
    payload, _, _ = write_geopackage(
        layer_name="buildings",
        crs_code=3857,
        geometry_family="Polygon",
        field_mappings=[ExportFieldMapping("height", "Height", "height", FieldType.INTEGER)],
        features=[("building", {"height": 12}, NativeGeometry("Polygon", (ring,), 3857))],
    )
    with _client(world.owner_user_id) as owner:
        uploaded = owner.post(
            prefix + "/imports/upload",
            files={"file": ("buildings.gpkg", payload, "application/geopackage+sqlite3")},
        )
        assert uploaded.status_code == 201, uploaded.text
        up = uploaded.json()
        inspected = up["inspection"]
        imported = owner.post(
            prefix + "/imports/confirm",
            json={
                "operation_id": str(uuid4()),
                "source_asset_id": up["asset"]["id"],
                "selected_member": inspected["selected_member"],
                "source_name": "Projected",
                "layer_name": "Projected buildings",
                "declared_source_crs": 3857,
                "target_crs": 3857,
                "declared_encoding": "utf-8",
                "approved_conversions": [],
                "inspection_fingerprint": inspected["inspection_fingerprint"],
            },
        )
        assert imported.status_code == 200, imported.text
        lid = imported.json()["target_layer_id"]
        layer = owner.get(prefix + "/layers/" + lid).json()
        source = layer["source"]
        assert source["stored_crs"]["code"] == 3857
        field = next(f["field_id"] for f in source["schema"]["fields"] if f["type"] == "integer")
        generated = owner.post(
            prefix + "/3d/tilesets/generate",
            json={
                "name": "Projected tiles",
                "layer_id": lid,
                "height_field": field,
                "expected_layer_revision": layer["layer_revision"],
                "expected_content_revision": source["content_revision"],
                "expected_schema_revision": source["schema_revision"],
                "base_ellipsoid_m": 0,
                "acknowledge_ellipsoid": True,
            },
        )
        assert generated.status_code == 201, generated.text
        record = generated.json()
        assert record["provenance"]["source_crs"] == 3857
        assert record["provenance"]["triangle_count"] == 12
        path = prefix + "/3d/tilesets/" + record["id"]
        child = owner.get(path + "/tileset.json").json()["root"]["children"][0]
        data = owner.get(path + "/" + child["content"]["uri"]).content
        points = decoded(data, child["transform"])
        with get_admin_engine().connect() as conn:
            expected = conn.execute(
                text("""
              SELECT ST_X(q),ST_Y(q),ST_Z(q) FROM (
                SELECT ST_Transform(ST_SetSRID(ST_MakePoint(:lon,:lat,12),4979),4978) AS q
              ) AS s
            """),
                {"lon": lon, "lat": lat},
            ).one()
            assert min(math.dist(p, expected) for p in points) < 0.001
