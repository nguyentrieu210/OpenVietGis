"""Spec 051: sign-off of a reconciled hamlet / road locks its survey points (real PostGIS + HTTP)."""

from __future__ import annotations

from uuid import uuid4

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
            "a2": {"ap": "Ấp 1", "ten_duong": "Đường A", "trang_thai": "khop", "xu_ly": "chua_xu_ly", "so_nha": "12"},
            "b1": {"ap": "Ấp 1", "ten_duong": "Đường B", "trang_thai": "sai_so", "xu_ly": "chua_xu_ly", "so_nha": "3"},
            "c1": {"ap": "Ấp 2", "ten_duong": "Đường C", "trang_thai": "thieu_so", "xu_ly": "chua_xu_ly", "so_nha": "5"},
        }

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
            points[fid] = {**points[fid], **patch}
            return commit([{"op_type": "update", "feature_id": fid, "properties": points[fid]}])

        seeded = commit([
            {"op_type": "create", "feature_id": fid, "geometry": {"type": "Point", "coordinates": [106.6 + i * 0.001, 10.7]}, "properties": p}
            for i, (fid, p) in enumerate(points.items())
        ])
        assert seeded.status_code == 201, seeded.text

        # Road A still has one pending point (a2 is "khop" and needs no work).
        early = owner.post(f"{module}/survey-signoffs", json={"source_id": source_id, "ap": "Ấp 1", "ten_duong": "Đường A"})
        assert early.status_code == 409 and early.json()["code"] == "SURVEY_SCOPE_INCOMPLETE", early.text
        assert early.json()["details"] == {"pending": 1, "total": 2}

        assert update("a1", xu_ly="giu").status_code == 201
        signed = owner.post(f"{module}/survey-signoffs", json={"source_id": source_id, "ap": "Ấp 1", "ten_duong": "Đường A"})
        assert signed.status_code == 201, signed.text
        assert signed.json()["point_count"] == 2
        again = owner.post(f"{module}/survey-signoffs", json={"source_id": source_id, "ap": "Ấp 1", "ten_duong": "Đường A"})
        assert again.status_code == 409 and again.json()["code"] == "SURVEY_SCOPE_SIGNED"
        # The whole hamlet cannot be signed while road B is pending.
        hamlet = owner.post(f"{module}/survey-signoffs", json={"source_id": source_id, "ap": "Ấp 1"})
        assert hamlet.status_code == 409 and hamlet.json()["details"]["pending"] == 1

        locked = update("a2", so_nha="12A")
        assert locked.status_code == 409, locked.text
        assert locked.json()["code"] == "DATA_LOCKED" and "Đường A" in locked.json()["message"]
        points["a2"]["so_nha"] = "12"
        deleted = commit([{"op_type": "delete", "feature_id": "a1"}])
        assert deleted.status_code == 409 and deleted.json()["code"] == "DATA_LOCKED"
        # Editing outside the signed road still works although Core bumps every row's revision.
        assert update("b1", xu_ly="da_sua", so_nha="5").status_code == 201
        assert update("c1", xu_ly="bo").status_code == 201

        listed = owner.get(f"{module}/survey-signoffs", params={"source_id": source_id})
        assert listed.status_code == 200 and [s["ten_duong"] for s in listed.json()["items"]] == ["Đường A"]

        short = owner.post(f"{module}/survey-signoffs/{signed.json()['id']}/revoke", json={"reason": "x"})
        assert short.status_code == 422
        revoked = owner.post(f"{module}/survey-signoffs/{signed.json()['id']}/revoke", json={"reason": "Sửa lại số 12"})
        assert revoked.status_code == 200 and revoked.json()["revoked_at"]
        assert update("a2", so_nha="12A", xu_ly="da_sua").status_code == 201
        assert owner.get(f"{module}/survey-signoffs", params={"source_id": source_id}).json()["items"] == []

        whole = owner.post(f"{module}/survey-signoffs", json={"source_id": source_id, "ap": "Ấp 1"})
        assert whole.status_code == 201 and whole.json()["point_count"] == 3
        assert update("b1", so_nha="7").status_code == 409

    with _client(world.wrong_user_id) as outsider:
        denied = outsider.get(f"{module}/survey-signoffs", params={"source_id": source_id})
        assert denied.status_code in (403, 404)
