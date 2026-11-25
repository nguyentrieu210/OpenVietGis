"""Spec 051: sign-off of a reconciled hamlet / road locks its survey points (real PostGIS + HTTP)."""

from __future__ import annotations

from uuid import uuid4

from sqlalchemy import text
from tests.fixtures.db_admin import get_admin_engine
from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.spatial.test_sources import _client

from modules.house_numbering.migrate import upgrade as migrate_house_numbering

FIELDS = ["ap", "ten_duong", "trang_thai", "xu_ly", "so_nha"]


def test_signoff_requires_full_reconciliation_and_locks_points(migrated_database: None) -> None:
    migrate_house_numbering()
    world = seed_access_world()
    org = f"/api/v1/organizations/{world.organization_id}"
    root = f"{org}/projects/{world.project_id}"
    module = f"{root}/modules/house-numbering"
    with _client(world.owner_user_id) as owner:
        manifest = owner.post(f"{org}/extensions/official/house_numbering/register")
        assert manifest.status_code == 201, manifest.text
        activated = owner.post(f"{root}/extensions/activations", json={"manifest_id": manifest.json()["id"]})
        assert activated.status_code == 201, activated.text
        layer = owner.post(
            f"{root}/layers/manual",
            json={
                "operation_id": str(uuid4()),
                "name": "Khảo sát số nhà kiểm thử",
                "geometry_family": "Point",
                "fields": [{"field_id": f, "name": f, "type": "text", "nullable": True} for f in FIELDS],
            },
        )
        assert layer.status_code == 201, layer.text
        layer_id, source_id = layer.json()["layer_id"], layer.json()["source_id"]
        points = {
            "a1": {"ap": "Ấp 1", "ten_duong": "Đường A", "trang_thai": "thieu_so", "xu_ly": "chua_xu_ly", "so_nha": "10"},
            # Stray spaces and an empty xu_ly: grouped and counted like the UI does (trimmed, pending).
            "a2": {"ap": "Ấp 1 ", "ten_duong": "Đường A", "trang_thai": "sai_so", "xu_ly": "", "so_nha": "12"},
            "a3": {"ap": "Ấp 1", "ten_duong": "Đường A", "trang_thai": "khop", "xu_ly": "chua_xu_ly", "so_nha": "14"},
            "b1": {"ap": "Ấp 1", "ten_duong": "Đường B", "trang_thai": "sai_so", "xu_ly": "chua_xu_ly", "so_nha": "3"},
            "c1": {"ap": "Ấp 2", "ten_duong": "Đường C", "trang_thai": "thieu_so", "xu_ly": "chua_xu_ly", "so_nha": "5"},
        }
        road_a = {"source_ids": [source_id], "ap": "Ấp 1", "ten_duong": "Đường A"}

        def commit(ops: list[dict]):
            source = owner.get(f"{root}/layers/{layer_id}").json()["source"]
            return owner.post(
                f"{root}/sources/{source_id}/changesets",
                json={
                    "operation_id": str(uuid4()),
                    "base_content_revision": source["content_revision"],
                    "base_schema_revision": source["schema_revision"],
                    "summary": "spec 051 test",
                    "operations": ops,
                },
            )

        def update(fid: str, **patch: str):
            response = commit([{"op_type": "update", "feature_id": fid, "properties": {**points[fid], **patch}}])
            if response.status_code == 201:
                points[fid] = {**points[fid], **patch}
            return response

        def create(fid: str, attrs: dict):
            return commit([{"op_type": "create", "feature_id": fid,
                            "geometry": {"type": "Point", "coordinates": [106.61, 10.71]}, "properties": attrs}])

        seeded = commit([
            {"op_type": "create", "feature_id": fid, "geometry": {"type": "Point", "coordinates": [106.6 + i * 0.001, 10.7]}, "properties": p}
            for i, (fid, p) in enumerate(points.items())
        ])
        assert seeded.status_code == 201, seeded.text

        early = owner.post(f"{module}/survey-signoffs", json=road_a)
        assert early.status_code == 409 and early.json()["code"] == "SURVEY_SCOPE_INCOMPLETE", early.text
        assert early.json()["details"] == {"pending": 2, "total": 3}
        blank = owner.post(f"{module}/survey-signoffs", json={**road_a, "ten_duong": "   "})
        assert blank.status_code == 422

        assert update("a1", xu_ly="giu").status_code == 201
        assert update("a2", xu_ly="bo").status_code == 201
        signed = owner.post(f"{module}/survey-signoffs", json=road_a)
        assert signed.status_code == 201, signed.text
        [signoff] = signed.json()["items"]
        assert signoff["point_count"] == 3
        again = owner.post(f"{module}/survey-signoffs", json=road_a)
        assert again.status_code == 409 and again.json()["code"] == "SURVEY_SCOPE_SIGNED"
        hamlet = owner.post(f"{module}/survey-signoffs", json={"source_ids": [source_id], "ap": "Ấp 1"})
        assert hamlet.status_code == 409 and hamlet.json()["details"]["pending"] == 1

        locked = update("a3", so_nha="14A")
        assert locked.status_code == 409, locked.text
        assert locked.json()["code"] == "DATA_LOCKED" and "Đường A" in locked.json()["message"]
        assert update("a2", so_nha="12B").status_code == 409  # matched although stored as "Ấp 1 "
        assert commit([{"op_type": "delete", "feature_id": "a1"}]).status_code == 409
        # Nothing can be moved or created into the signed road either.
        assert update("c1", ap="Ấp 1", ten_duong="Đường A").status_code == 409
        new_point = {"ap": "Ấp 1", "ten_duong": "Đường A", "trang_thai": "thieu_so", "xu_ly": "chua_xu_ly", "so_nha": "16"}
        assert create("a4", new_point).status_code == 409
        # Editing outside the signed road still works although Core bumps every row's revision.
        assert update("b1", xu_ly="da_sua", so_nha="5").status_code == 201
        assert update("c1", xu_ly="bo").status_code == 201
        # Adding a field rewrites attributes with a new null key; existing values are unchanged, so it passes.
        with get_admin_engine().begin() as connection:
            changed = connection.execute(
                text("UPDATE spatial.reference_features SET attributes=attributes||jsonb_build_object('ghi_chu',NULL) WHERE source_id=:s"),
                {"s": source_id},
            ).rowcount
            assert changed == 5

        listed = owner.get(f"{module}/survey-signoffs", params={"source_id": source_id})
        assert listed.status_code == 200 and [s["ten_duong"] for s in listed.json()["items"]] == ["Đường A"]

        short = owner.post(f"{module}/survey-signoffs/{signoff['id']}/revoke", json={"reason": " x "})
        assert short.status_code == 422
        revoked = owner.post(f"{module}/survey-signoffs/{signoff['id']}/revoke", json={"reason": "Sửa lại số 14"})
        assert revoked.status_code == 200 and revoked.json()["revoked_at"]
        assert update("a3", so_nha="14A", xu_ly="da_sua").status_code == 201
        assert owner.get(f"{module}/survey-signoffs", params={"source_id": source_id}).json()["items"] == []

        whole = owner.post(f"{module}/survey-signoffs", json={"source_ids": [source_id], "ap": "Ấp 1"})
        assert whole.status_code == 201 and whole.json()["items"][0]["point_count"] == 4
        assert update("b1", so_nha="7").status_code == 409

        # Deactivating the plugin lifts the lock, so Core data is never frozen.
        off = owner.post(f"{root}/extensions/activations/{activated.json()['id']}/deactivate")
        assert off.status_code == 200, off.text
        assert update("b1", so_nha="7").status_code == 201

    with _client(world.wrong_user_id) as outsider:
        denied = outsider.get(f"{module}/survey-signoffs", params={"source_id": source_id})
        assert denied.status_code in (403, 404)
