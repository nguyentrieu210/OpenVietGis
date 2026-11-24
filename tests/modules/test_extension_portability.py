"""R014 independent module histories and copies; run with the guarded DB wrapper.

Reference module names and schemas belong in tests/modules, outside Core runtime
scope. tools.run_with_test_database migrates the dedicated test database before
collection; CI's integration job likewise provisions/migrates that database.
"""

from __future__ import annotations

import json
from pathlib import Path
from uuid import uuid4

from openvietgis_api.domains.extensions.domain import bundle_digest
from sqlalchemy import text
from tests.fixtures.db_admin import get_admin_engine
from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.m2.test_r006_r009_real import _import_geojson
from tests.integration.spatial.test_authorization import _grant
from tests.integration.spatial.test_sources import _client


def manifest(name: str) -> dict:
    path = Path(__file__).resolve().parents[2] / "modules" / name / "manifest.json"
    return json.loads(path.read_text())


def registration(client, org, project, body):
    created = client.post(f"{org}/extensions/manifests", json=body)
    assert created.status_code == 201, created.text
    mid = created.json()["id"]
    activation = client.post(
        f"{project}/extensions/activations", json={"manifest_id": mid}
    )
    assert activation.status_code == 201, activation.text
    return mid, activation.json()["id"]


def test_linked_house_record_history_disabled_portability_and_scope():
    world = seed_access_world()
    org = f"/api/v1/organizations/{world.organization_id}"
    project = f"{org}/projects/{world.project_id}"
    records = f"{project}/extensions/house_numbering/records"
    export = f"{project}/extensions/house_numbering/portable-export"
    other = seed_access_world()
    with (
        _client(world.owner_user_id) as owner,
        _client(world.member_user_id) as editor,
        _client(other.owner_user_id) as outsider,
    ):
        mid, aid = registration(owner, org, project, manifest("house_numbering"))
        grant = _grant(
            owner, world, "membership", str(world.member_membership_id), "editor"
        )
        imported = _import_geojson(
            owner,
            world,
            "address.geojson",
            [
                {
                    "type": "Feature",
                    "id": "address1",
                    "properties": {"name": "Original point"},
                    "geometry": {"type": "Point", "coordinates": [105.8, 21.0]},
                }
            ],
            "Address points",
            "address-import",
        )
        lid = imported["target_layer_id"]
        source = owner.get(f"{project}/layers/{lid}").json()["source"]
        pin = {
            "status": "pinned",
            "source_id": source["id"],
            "content_revision": source["content_revision"],
            "schema_revision": source["schema_revision"],
            "stored_crs": 4326,
        }
        payload = {
            "street_name": "Tran Hung Dao",
            "house_number": "12A",
            "side": "right",
            "ward_code": None,
            "verified": False,
        }
        create = {
            "entity_name": "address_plate",
            "payload": payload,
            "linked_layer_id": lid,
            "linked_feature_id": "address1",
        }
        assert owner.post(records, json=create).status_code == 409
        made = owner.post(records, json={**create, "expected_link_context": pin})
        assert made.status_code == 201, made.text
        rid = made.json()["id"]
        assert made.json()["link_context"] == pin
        assert made.json()["revision"] == 1 and made.json()["manifest_id"] == mid
        changed = {**payload, "house_number": "14A", "verified": True}
        update = {
            "expected_revision": 1,
            "expected_manifest_id": mid,
            "payload": changed,
            "expected_link_context": pin,
        }
        second = editor.put(f"{records}/{rid}", json=update)
        assert second.status_code == 200, second.text
        assert second.json()["revision"] == 2 and second.json()["payload"] == changed
        assert owner.put(f"{records}/{rid}", json=update).status_code == 409
        assert outsider.get(f"{records}/{rid}/history").status_code in (403, 404)
        history = owner.get(f"{records}/{rid}/history").json()["items"]
        assert [h["revision"] for h in history] == [1, 2]
        assert [h["payload"] for h in history] == [payload, changed]
        assert [h["actor_id"] for h in history] == [
            str(world.owner_user_id),
            str(world.member_user_id),
        ]
        assert [h["link_context"] for h in history] == [pin, pin]
        # A Core edit invalidates the linked spatial pin without touching module data.
        cs = owner.post(
            f"{project}/sources/{source['id']}/changesets",
            json={
                "base_content_revision": source["content_revision"],
                "operation_id": str(uuid4()),
                "summary": "Independent address point",
                "operations": [
                    {
                        "op_type": "update",
                        "feature_id": "address1",
                        "properties": {"name": "Moved label"},
                    },
                ],
            },
        )
        assert cs.status_code == 201, cs.text
        assert (
            editor.put(
                f"{records}/{rid}", json={**update, "expected_revision": 2}
            ).status_code
            == 409
        )
        disabled = owner.post(f"{project}/extensions/activations/{aid}/deactivate")
        assert disabled.status_code == 200
        assert (
            owner.put(
                f"{records}/{rid}", json={**update, "expected_revision": 2}
            ).status_code
            == 404
        )
        assert owner.get(records).json()["items"][0]["payload"] == changed
        returned = owner.get(export)
        assert returned.status_code == 200, returned.text
        assert returned.headers["cache-control"] == "no-store"
        portable = returned.json()
        b = portable["bundle"]
        assert bundle_digest(b) == portable["sha256"]
        assert b["activation_status"] == "disabled"
        assert b["origin"] == {
            "organization_id": str(world.organization_id),
            "project_id": str(world.project_id),
        }
        assert (
            b["records"][0]["record_id"] == rid
            and b["records"][0]["history"] == history
        )
        assert (
            b["manifests"][0]["entity_schemas"]
            == manifest("house_numbering")["entity_schemas"]
        )
        assert [e["event"] for e in b["activation_history"]] == [
            "activate",
            "deactivate",
        ]
        assert outsider.get(export).status_code in (403, 404)
        assert owner.post(f"{org}/grants/{grant}/revoke").status_code == 204
        assert editor.get(export).status_code in (403, 404)
        assert editor.get(f"{records}/{rid}/history").status_code in (403, 404)

        # New scope can independently register the same module but an exported Core
        # link must not be silently remapped to a different tenant/project.
        oorg = f"/api/v1/organizations/{other.organization_id}"
        oproject = f"{oorg}/projects/{other.project_id}"
        omid, _ = registration(outsider, oorg, oproject, manifest("house_numbering"))
        bad = outsider.post(
            f"{oproject}/extensions/house_numbering/portable-import",
            json={**portable, "expected_manifest_id": omid},
        )
        assert bad.status_code == 409, bad.text
        assert (
            outsider.get(f"{oproject}/extensions/house_numbering/records").json()[
                "items"
            ]
            == []
        )


def test_land_asset_atomic_copy_explicit_transition_and_replay():
    origin, dest = seed_access_world(), seed_access_world()
    oorg = f"/api/v1/organizations/{origin.organization_id}"
    op = f"{oorg}/projects/{origin.project_id}"
    dorg = f"/api/v1/organizations/{dest.organization_id}"
    dp = f"{dorg}/projects/{dest.project_id}"
    rec = f"{dp}/extensions/land_asset/records"
    with (
        _client(origin.owner_user_id) as source,
        _client(dest.owner_user_id) as owner,
        _client(dest.member_user_id) as viewer,
    ):
        body = manifest("land_asset")
        _smid, said = registration(source, oorg, op, body)
        mid, aid = registration(owner, dorg, dp, body)
        source_records = f"{op}/extensions/land_asset/records"
        payload = {
            "asset_code": "LND-01",
            "land_use_code": "ODT",
            "area_m2": 185.5,
            "condition_score": 5,
            "inspector_note": None,
        }
        one = source.post(
            source_records, json={"entity_name": "asset_inspection", "payload": payload}
        )
        assert one.status_code == 201, one.text
        two = source.post(
            source_records,
            json={
                "entity_name": "asset_inspection",
                "payload": {**payload, "asset_code": "LND-02"},
            },
        )
        assert two.status_code == 201, two.text
        assert (
            source.post(f"{op}/extensions/activations/{said}/deactivate").status_code
            == 200
        )
        portable = source.get(f"{op}/extensions/land_asset/portable-export").json()
        target = f"{dp}/extensions/land_asset/portable-import"
        request = {**portable, "expected_manifest_id": mid}
        _grant(owner, dest, "membership", str(dest.member_membership_id), "viewer")
        assert viewer.post(target, json=request).status_code == 403
        assert (
            viewer.get(f"{dp}/extensions/land_asset/portable-export").status_code == 200
        )
        assert (
            owner.post(target, json={**request, "sha256": "0" * 64}).status_code == 422
        )
        malformed = json.loads(json.dumps(portable["bundle"]))
        malformed["records"][1]["payload"]["condition_score"] = True
        invalid = owner.post(
            target,
            json={
                "bundle": malformed,
                "sha256": bundle_digest(malformed),
                "expected_manifest_id": mid,
            },
        )
        assert invalid.status_code == 422, invalid.text
        assert owner.get(rec).json()["items"] == []
        imported = owner.post(target, json=request)
        assert imported.status_code == 200, imported.text
        receipt = imported.json()
        assert receipt["replayed"] is False and len(receipt["mappings"]) == 2
        again = owner.post(target, json=request).json()
        assert again == {**receipt, "replayed": True}
        copied = owner.get(rec).json()["items"]
        assert sorted(r["payload"]["asset_code"] for r in copied) == [
            "LND-01",
            "LND-02",
        ]
        assert all(
            r["payload"]["inspector_note"] is None and r["payload"]["area_m2"] == 185.5
            for r in copied
        )
        assert all(r["id"] not in (one.json()["id"], two.json()["id"]) for r in copied)
        rid = copied[0]["id"]
        history = owner.get(f"{rec}/{rid}/history").json()["items"]
        assert history[0]["event"] == "import" and history[0]["actor_id"] == str(
            dest.owner_user_id
        )
        assert history[0]["import_origin"]["verified_origin"] is False
        assert history[0]["import_origin"]["bundle_sha256"] == portable["sha256"]

        # Only an explicitly compatible forward same-major metadata transition is accepted.
        new = owner.post(
            f"{dorg}/extensions/manifests", json={**body, "version": "1.1.0"}
        )
        assert new.status_code == 201, new.text
        nextid = new.json()["id"]
        route = f"{dp}/extensions/activations/{aid}/transition"
        transition = {"expected_manifest_id": mid, "target_manifest_id": nextid}
        assert viewer.post(route, json=transition).status_code == 403
        assert (
            owner.post(
                f"{dp}/extensions/activations", json={"manifest_id": nextid}
            ).status_code
            == 409
        )
        assert owner.post(route, json=transition).status_code == 200
        assert owner.post(route, json=transition).status_code == 409
        assert (
            owner.post(
                route, json={"expected_manifest_id": nextid, "target_manifest_id": mid}
            ).status_code
            == 409
        )
        bad_body = {
            **body,
            "version": "1.2.0",
            "permissions": [*body["permissions"], "ext.land_asset.extra"],
        }
        invalid_manifest = owner.post(f"{dorg}/extensions/manifests", json=bad_body)
        assert invalid_manifest.status_code == 201
        assert (
            owner.post(
                route,
                json={
                    "expected_manifest_id": nextid,
                    "target_manifest_id": invalid_manifest.json()["id"],
                },
            ).status_code
            == 409
        )
        assert (
            owner.put(
                f"{rec}/{rid}",
                json={
                    "expected_revision": 1,
                    "expected_manifest_id": mid,
                    "payload": copied[0]["payload"],
                },
            ).status_code
            == 409
        )
        changed = owner.put(
            f"{rec}/{rid}",
            json={
                "expected_revision": 1,
                "expected_manifest_id": nextid,
                "payload": {**copied[0]["payload"], "condition_score": 4},
            },
        )
        assert changed.status_code == 200, changed.text
        assert changed.json()["revision"] == 2
        versions = owner.get(f"{rec}/{rid}/history").json()["items"]
        assert [v["manifest_id"] for v in versions] == [mid, nextid]
        events = owner.get(f"{dp}/extensions/activations/{aid}/history").json()["items"]
        assert [e["event"] for e in events] == ["activate", "transition"]
        assert (
            events[1]["previous_manifest_id"] == mid
            and events[1]["manifest_id"] == nextid
        )
        # An unrelated module has no activation in this project. A 404 is
        # the existing contract; verify directly that no records leaked.
        unrelated = owner.get(f"{dp}/extensions/house_numbering/records")
        assert unrelated.status_code == 404, unrelated.text
        with get_admin_engine().connect() as conn:
            assert conn.execute(
                text("SELECT count(*) FROM extensions.records WHERE organization_id=:o "
                     "AND project_id=:p AND extension_key='house_numbering'"),
                {"o": dest.organization_id, "p": dest.project_id},
            ).scalar_one() == 0

    # Database enforceability, not merely absence of an HTTP DELETE endpoint.
    with get_admin_engine().begin() as conn:
        for name in ("record_revisions", "activation_events", "import_receipts"):
            public_name = "extensions." + name
            privileges = conn.execute(
                text("""
                SELECT has_table_privilege('openvietgis_runtime', :table, 'SELECT'),
                       has_table_privilege('openvietgis_runtime', :table, 'INSERT'),
                       has_table_privilege('openvietgis_runtime', :table, 'UPDATE'),
                       has_table_privilege('openvietgis_runtime', :table, 'DELETE')
            """),
                {"table": public_name},
            ).one()
            assert tuple(privileges) == (True, True, False, False)
            forced = conn.execute(
                text(
                    "SELECT relrowsecurity AND relforcerowsecurity FROM pg_class "
                    "WHERE oid=CAST(:table AS regclass)"
                ),
                {"table": public_name},
            ).scalar_one()
            assert forced is True
