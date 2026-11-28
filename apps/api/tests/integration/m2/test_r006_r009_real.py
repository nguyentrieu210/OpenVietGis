"""Real PostgreSQL/PostGIS/RLS integration suite for Wave A: R006, R007, R008, R009."""

from __future__ import annotations

import hashlib
import io
import json
from typing import Any
from uuid import uuid4

from fastapi.testclient import TestClient
from PIL import Image
from sqlalchemy import text

from openvietgis_api.adapters.persistence.db import get_engine
from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.spatial.test_sources import _client


def _import_geojson(
    client: TestClient,
    world: Any,
    filename: str,
    features: list[dict[str, object]],
    name: str,
    _op_id: str,
) -> dict[str, Any]:
    base = f"/api/v1/organizations/{world.organization_id}/projects/{world.project_id}"
    payload = json.dumps({"type": "FeatureCollection", "features": features}).encode("utf-8")
    up = client.post(
        f"{base}/imports/upload",
        files={"file": (filename, payload, "application/geo+json")},
    )
    assert up.status_code == 201, up.text
    body = up.json()
    asset_id = body["asset"]["id"]
    insp = body["inspection"]
    member = insp["selected_member"]
    fp = insp["inspection_fingerprint"]
    job = client.post(
        f"{base}/imports/confirm",
        json={
            "operation_id": str(uuid4()),
            "source_asset_id": asset_id,
            "selected_member": member,
            "source_name": name,
            "layer_name": name,
            "declared_source_crs": 4326,
            "target_crs": 4326,
            "inspection_fingerprint": fp,
        },
    )
    assert job.status_code in (200, 201), job.text
    data = job.json()
    assert data["status"] == "succeeded", data
    return data


def test_r006_editing_changesets_conflicts_validation_and_revert(migrated_database: None) -> None:
    world = seed_access_world()
    base = f"/api/v1/organizations/{world.organization_id}/projects/{world.project_id}"

    with _client(world.owner_user_id) as client:
        imp = _import_geojson(
            client,
            world,
            "parcels.geojson",
            [
                {
                    "type": "Feature",
                    "id": "p1",
                    "properties": {"code": "A1", "area_m2": 120},
                    "geometry": {
                        "type": "Polygon",
                        "coordinates": [
                            [
                                [105.80, 21.00],
                                [105.81, 21.00],
                                [105.81, 21.01],
                                [105.80, 21.01],
                                [105.80, 21.00],
                            ]
                        ],
                    },
                },
                {
                    "type": "Feature",
                    "id": "p2",
                    "properties": {"code": "A2", "area_m2": 240},
                    "geometry": {
                        "type": "Polygon",
                        "coordinates": [
                            [
                                [105.82, 21.00],
                                [105.83, 21.00],
                                [105.83, 21.01],
                                [105.82, 21.01],
                                [105.82, 21.00],
                            ]
                        ],
                    },
                },
            ],
            "Parcels",
            "r006-imp-1",
        )
        source_id = str(imp["target_source_id"])
        rev0 = str(imp["source_content_revision"])

        # 1. Invalid self-intersecting bowtie polygon MUST be rejected (no silent repair)
        bad = client.post(
            f"{base}/sources/{source_id}/changesets",
            json={
                "base_content_revision": rev0,
                "operation_id": "cs-bad-bowtie",
                "summary": "Invalid bowtie",
                "operations": [
                    {
                        "op_type": "create",
                        "feature_id": "bad1",
                        "geometry": {
                            "type": "Polygon",
                            "coordinates": [
                                [
                                    [105.85, 21.00],
                                    [105.86, 21.01],
                                    [105.85, 21.01],
                                    [105.86, 21.00],
                                    [105.85, 21.00],
                                ]
                            ],
                        },
                        "properties": {"code": "BAD", "area_m2": 10},
                    }
                ],
            },
        )
        assert bad.status_code == 422, bad.text
        assert bad.json()["code"] == "INVALID_GEOMETRY"

        # 2. Valid atomic changeset: create p3, update p1, delete p2
        cs1_payload = {
            "base_content_revision": rev0,
            "operation_id": "cs-edit-1",
            "summary": "Add p3, update p1, delete p2",
            "operations": [
                {
                    "op_type": "create",
                    "feature_id": "p3",
                    "geometry": {
                        "type": "Polygon",
                        "coordinates": [
                            [
                                [105.84, 21.00],
                                [105.85, 21.00],
                                [105.85, 21.01],
                                [105.84, 21.01],
                                [105.84, 21.00],
                            ]
                        ],
                    },
                    "properties": {"code": "A3", "area_m2": 360},
                },
                {
                    "op_type": "update",
                    "feature_id": "p1",
                    "properties": {"code": "A1-UPDATED", "area_m2": 150},
                },
                {"op_type": "delete", "feature_id": "p2"},
            ],
        }
        cs1 = client.post(f"{base}/sources/{source_id}/changesets", json=cs1_payload)
        assert cs1.status_code == 201, cs1.text
        cs1_data = cs1.json()
        rev1 = cs1_data["resulting_content_revision"]
        assert rev1 != rev0
        assert cs1_data["operation_count"] == 3
        assert len(cs1_data["operations"]) == 3

        # 3. Idempotent replay returns same changeset
        cs1_replay = client.post(
            f"{base}/sources/{source_id}/changesets",
            json=cs1_payload,
        )
        assert cs1_replay.status_code == 201
        assert cs1_replay.json()["id"] == cs1_data["id"]
        for changed in (
            {**cs1_payload, "operations": []},
            {**cs1_payload, "operations": [{"op_type": "delete", "feature_id": "p1"}]},
            {**cs1_payload, "summary": "Different intent"},
        ):
            conflict = client.post(f"{base}/sources/{source_id}/changesets", json=changed)
            assert conflict.status_code == 409, conflict.text
            assert conflict.json()["code"] == "OPERATION_ID_CONFLICT"

        # 4. Stale revision returns 409 Conflict with draft_preserved=True
        stale = client.post(
            f"{base}/sources/{source_id}/changesets",
            json={
                "base_content_revision": rev0,
                "operation_id": "cs-stale-2",
                "summary": "Stale draft",
                "operations": [{"op_type": "delete", "feature_id": "p1"}],
            },
        )
        assert stale.status_code == 409, stale.text
        assert stale.json()["code"] == "STALE_CONTENT_REVISION"
        assert stale.json()["details"]["draft_preserved"] is True
        assert stale.json()["details"]["current_content_revision"] == rev1

        # 5. History listing & revert-as-new-changeset
        hist = client.get(f"{base}/sources/{source_id}/changesets")
        assert hist.status_code == 200
        assert len(hist.json()["items"]) == 1

        reverted = client.post(
            f"{base}/changesets/{cs1_data['id']}/revert",
            json={"operation_id": "cs-revert-1"},
        )
        assert reverted.status_code == 201, reverted.text
        assert reverted.json()["reverted_from_changeset_id"] == cs1_data["id"]

        # Verify p2 is back and p1 is restored
        q_back = client.post(
            f"{base}/layers/{imp['target_layer_id']}/query",
            json={"limit": 50, "offset": 0},
        )
        assert q_back.status_code == 200, q_back.text
        by_id = {f["feature_id"]: f for f in q_back.json()["items"]}
        assert set(by_id.keys()) == {"p1", "p2"}
        assert by_id["p1"]["attributes"]["code"] == "A1"

        # Redo by reverting the just-written undo changeset, preserving the audit log.
        undo_data = reverted.json()
        redo_payload = {"operation_id": "cs-redo-1", "summary": "REDO · " + undo_data["id"]}
        redo = client.post(f"{base}/changesets/{undo_data['id']}/revert", json=redo_payload)
        assert redo.status_code == 201, redo.text
        assert redo.json()["reverted_from_changeset_id"] == undo_data["id"]
        assert redo.json()["summary"] == redo_payload["summary"]
        replay = client.post(f"{base}/changesets/{undo_data['id']}/revert", json=redo_payload)
        assert replay.status_code == 201, replay.text
        assert replay.json()["id"] == redo.json()["id"]
        journal = client.get(f"{base}/integrations/events", params={"limit": 2})
        assert journal.status_code == 200, journal.text
        page = journal.json()
        assert [e["event_kind"] for e in page["items"]] == [
            "changeset.committed",
            "changeset.reverted",
        ]
        tail = client.get(
            f"{base}/integrations/events", params={"after": page["next_after_sequence"]}
        )
        assert tail.status_code == 200, tail.text
        assert [e["id"] for e in tail.json()["items"]] == [redo.json()["id"]]
        q_redo = client.post(
            f"{base}/layers/{imp['target_layer_id']}/query", json={"limit": 50, "offset": 0}
        )
        assert q_redo.status_code == 200, q_redo.text
        redo_features = {f["feature_id"]: f for f in q_redo.json()["items"]}
        assert set(redo_features) == {"p1", "p3"}
        assert redo_features["p1"]["attributes"]["code"] == "A1-UPDATED"

        # 6. Snap helper endpoint
        snap = client.post(
            f"{base}/editing/snap",
            json={
                "target": [105.8001, 21.0001],
                "vertices": [[105.80, 21.00]],
                "segments": [],
                "tolerance": 0.001,
            },
        )
        assert snap.status_code == 200
        assert snap.json()["snapped"] is True
        assert snap.json()["kind"] == "vertex"

        # 7. Reference fixture stays read-only (403)
        reg = client.post(
            f"{base}/spatial/sources",
            json={"reference_key": "reference_points_5899", "name": "Readonly Fixture"},
        )
        assert reg.status_code == 201, reg.text
        fix_id = reg.json()["resource_id"]
        fix_src = client.get(f"{base}/spatial/sources/{fix_id}").json()
        ro_err = client.post(
            f"{base}/sources/{fix_id}/changesets",
            json={
                "base_content_revision": fix_src["content_revision"],
                "operation_id": "cs-ro",
                "summary": "Cannot edit fixture",
                "operations": [{"op_type": "delete", "feature_id": "pt-1"}],
            },
        )
        assert ro_err.status_code == 403


def test_r006_noop_and_snap_require_current_edit_authority(migrated_database: None) -> None:
    from tests.integration.spatial.test_authorization import _grant, _org

    world = seed_access_world()
    base = f"/api/v1/organizations/{world.organization_id}/projects/{world.project_id}"
    snap_body = {"target": [1, 1], "vertices": [[1, 1]], "tolerance": 0.1}
    with _client(world.owner_user_id) as owner, _client(world.member_user_id) as member:
        imp = _import_geojson(
            owner,
            world,
            "noop.geojson",
            [
                {
                    "type": "Feature",
                    "id": "p1",
                    "properties": {"label": "same"},
                    "geometry": {"type": "Point", "coordinates": [105, 21]},
                }
            ],
            "No-op fixture",
            "noop-import",
        )
        source_id = imp["target_source_id"]
        root = f"{base}/sources/{source_id}/changesets"
        before = owner.get(f"{base}/spatial/sources/{source_id}").json()
        history_before = owner.get(root).json()
        result = owner.post(
            root,
            json={
                "base_content_revision": imp["source_content_revision"],
                "operation_id": "noop-test",
                "operations": [
                    {
                        "op_type": "update",
                        "feature_id": "p1",
                        "properties": {"label": "same"},
                        "geometry": {"type": "Point", "coordinates": [105, 21]},
                    }
                ],
            },
        )
        assert result.status_code == 409, result.text
        assert result.json()["code"] == "NO_MATERIAL_CHANGE"
        assert owner.get(root).json() == history_before
        assert owner.get(f"{base}/spatial/sources/{source_id}").json() == before

        snap_url = f"{base}/editing/snap"
        assert owner.post(snap_url, json=snap_body).status_code == 200
        assert member.post(snap_url, json=snap_body).status_code == 403
        grant = _grant(owner, world, "membership", str(world.member_membership_id), "viewer")
        assert member.post(snap_url, json=snap_body).status_code == 403
        assert owner.post(f"{_org(world)}/grants/{grant}/revoke").status_code == 204
        grant = _grant(owner, world, "membership", str(world.member_membership_id), "editor")
        assert member.post(snap_url, json=snap_body).status_code == 200
        assert owner.post(f"{_org(world)}/grants/{grant}/revoke").status_code == 204
        assert member.post(snap_url, json=snap_body).status_code == 403


def test_r006_concurrent_identical_save_returns_one_receipt(migrated_database: None) -> None:
    from concurrent.futures import ThreadPoolExecutor
    from threading import Barrier

    world = seed_access_world()
    base = f"/api/v1/organizations/{world.organization_id}/projects/{world.project_id}"
    with _client(world.owner_user_id) as owner:
        imp = _import_geojson(
            owner,
            world,
            "retry.geojson",
            [
                {
                    "type": "Feature",
                    "id": "p1",
                    "properties": {"label": "old"},
                    "geometry": {"type": "Point", "coordinates": [105, 21]},
                }
            ],
            "Retry fixture",
            "retry-import",
        )
    root = f"{base}/sources/{imp['target_source_id']}/changesets"
    payload = {
        "base_content_revision": imp["source_content_revision"],
        "operation_id": "identical-concurrent",
        "operations": [
            {
                "op_type": "create",
                "feature_id": "p2",
                "properties": {"label": "new"},
                "geometry": {"type": "Point", "coordinates": [106, 21]},
            }
        ],
    }
    gate = Barrier(2)

    def submit() -> Any:
        with _client(world.owner_user_id) as client:
            gate.wait(timeout=15)
            return client.post(root, json=payload)

    with ThreadPoolExecutor(max_workers=2) as workers:
        futures = [workers.submit(submit) for _ in range(2)]
        first, second = [future.result(timeout=30) for future in futures]
    assert first.status_code == second.status_code == 201, (first.text, second.text)
    assert first.json() == second.json()
    with _client(world.owner_user_id) as owner:
        history = owner.get(root).json()["items"]
        assert len(history) == 1
        assert history[0]["id"] == first.json()["id"]
        actual = owner.post(
            f"{base}/layers/{imp['target_layer_id']}/query", json={"limit": 10, "offset": 0}
        )
        assert actual.status_code == 200
        assert {feature["feature_id"] for feature in actual.json()["items"]} == {"p1", "p2"}


def test_r007_r008_r009_query_processing_and_cartography(migrated_database: None) -> None:
    world = seed_access_world()
    base = f"/api/v1/organizations/{world.organization_id}/projects/{world.project_id}"

    with _client(world.owner_user_id) as client:
        # Import points for query + buffer + cartography
        pts = _import_geojson(
            client,
            world,
            "stations.geojson",
            [
                {
                    "type": "Feature",
                    "id": "s1",
                    "properties": {"zone": "north", "score": 90, "title": "Station Alpha"},
                    "geometry": {"type": "Point", "coordinates": [105.80, 21.02]},
                },
                {
                    "type": "Feature",
                    "id": "s2",
                    "properties": {"zone": "north", "score": 40, "title": "Station Beta"},
                    "geometry": {"type": "Point", "coordinates": [105.85, 21.05]},
                },
                {
                    "type": "Feature",
                    "id": "s3",
                    "properties": {"zone": "south", "score": 85, "title": "Station Gamma"},
                    "geometry": {"type": "Point", "coordinates": [106.70, 10.77]},
                },
            ],
            "Stations",
            "m2-imp-stations",
        )
        pts_layer = str(pts["target_layer_id"])

        # Import an AOI polygon covering Hanoi (s1 and s2)
        aoi = _import_geojson(
            client,
            world,
            "aoi.geojson",
            [
                {
                    "type": "Feature",
                    "id": "hanoi",
                    "properties": {"region": "Hanoi"},
                    "geometry": {
                        "type": "Polygon",
                        "coordinates": [
                            [
                                [105.70, 20.95],
                                [105.95, 20.95],
                                [105.95, 21.15],
                                [105.70, 21.15],
                                [105.70, 20.95],
                            ]
                        ],
                    },
                }
            ],
            "Hanoi AOI",
            "m2-imp-aoi",
        )
        aoi_layer = str(aoi["target_layer_id"])

        # --- R007: Typed Attribute + Spatial Query ---
        q1 = client.post(
            f"{base}/layers/{pts_layer}/query",
            json={
                "attribute_filter": {
                    "op": "and",
                    "conditions": [
                        {"field": "zone", "op": "eq", "value": "north"},
                        {"field": "score", "op": "gte", "value": 50},
                    ],
                },
                "spatial_filter": {
                    "predicate": "within",
                    "reference_layer_id": aoi_layer,
                },
                "sort_field": "score",
                "sort_direction": "desc",
                "limit": 10,
            },
        )
        assert q1.status_code == 200, q1.text
        q1_data = q1.json()
        assert q1_data["total_count"] == 1
        assert q1_data["selection_feature_ids"] == ["s1"]

        # Reject unknown field (no arbitrary SQL injection possible)
        q_bad = client.post(
            f"{base}/layers/{pts_layer}/query",
            json={"attribute_filter": {"field": "1=1; DROP TABLE users", "op": "eq", "value": "x"}},
        )
        assert q_bad.status_code == 422

        # Save query
        sq = client.post(
            f"{base}/saved-queries",
            json={
                "layer_id": pts_layer,
                "name": "High Score North",
                "attribute_filter": {"field": "score", "op": "gte", "value": 80},
            },
        )
        assert sq.status_code == 201, sq.text

        # --- R008: Processing (Buffer -> Clip -> Dissolve) ---
        algos = client.get(f"{base}/processing/algorithms")
        assert algos.status_code == 200
        assert {"buffer", "clip", "dissolve"} <= {a["id"] for a in algos.json()["items"]}

        buf = client.post(
            f"{base}/processing/jobs",
            json={
                "algorithm_id": "buffer",
                "input_layer_id": pts_layer,
                "parameters": {"distance_meters": 1000, "quad_segs": 8},
                "output_name": "Stations 1km Buffer",
                "operation_id": "proc-buf-1",
            },
        )
        assert buf.status_code == 201, buf.text
        buf_data = buf.json()
        assert buf_data["status"] == "succeeded"
        assert buf_data["feature_count"] == 3
        buf_layer = buf_data["output_layer_id"]

        # A succeeded job must publish readable canonical decimal values.
        def assert_buffer_readable(layer_id, expected_distance):
            source = client.get(f"{base}/layers/{layer_id}").json()["source"]
            pins = {key: source[key] for key in ("content_revision", "schema_revision")}
            response = client.get(f"{base}/layers/{layer_id}/features", params=pins)
            assert response.status_code == 200, response.text
            rows = response.json()["items"]
            assert len(rows) == 3
            assert all(row["attributes"]["buffer_meters"] == expected_distance for row in rows)

        assert_buffer_readable(buf_layer, "1000")
        fractional = client.post(
            f"{base}/processing/jobs",
            json={
                "algorithm_id": "buffer",
                "input_layer_id": pts_layer,
                "parameters": {"distance_meters": 12.5},
                "output_name": "Fractional Buffer",
                "operation_id": "proc-buf-fraction",
            },
        )
        assert fractional.status_code == 201, fractional.text
        assert_buffer_readable(fractional.json()["output_layer_id"], "12.5")

        clip = client.post(
            f"{base}/processing/jobs",
            json={
                "algorithm_id": "clip",
                "input_layer_id": buf_layer,
                "overlay_layer_id": aoi_layer,
                "parameters": {},
                "output_name": "Hanoi Clipped Buffers",
                "operation_id": "proc-clip-1",
            },
        )
        assert clip.status_code == 201, clip.text
        assert clip.json()["status"] == "succeeded"
        assert clip.json()["feature_count"] == 2
        clip_layer = clip.json()["output_layer_id"]

        dis = client.post(
            f"{base}/processing/jobs",
            json={
                "algorithm_id": "dissolve",
                "input_layer_id": clip_layer,
                "parameters": {},
                "output_name": "Dissolved Coverage",
                "operation_id": "proc-dis-1",
            },
        )
        assert dis.status_code == 201, dis.text
        assert dis.json()["status"] == "succeeded"
        assert dis.json()["feature_count"] >= 1

        # Queue + cancel + retry flow
        queued = client.post(
            f"{base}/processing/jobs",
            json={
                "algorithm_id": "buffer",
                "input_layer_id": pts_layer,
                "parameters": {"distance_meters": 250},
                "output_name": "Queued Buffer",
                "operation_id": "proc-queued-1",
                "execute_now": False,
            },
        )
        assert queued.status_code == 201 and queued.json()["status"] == "queued"
        cancelled = client.post(f"{base}/processing/jobs/{queued.json()['id']}/cancel")
        assert cancelled.status_code == 200 and cancelled.json()["status"] == "cancelled"
        retried = client.post(f"{base}/processing/jobs/{queued.json()['id']}/retry")
        assert retried.status_code == 200 and retried.json()["status"] == "succeeded"
        assert retried.json()["attempt_count"] == 2
        assert_buffer_readable(retried.json()["output_layer_id"], "250")

        # --- R009: Cartography Styles, Layouts, 150 DPI PNG & 300 DPI PDF ---
        st = client.put(
            f"{base}/cartography/layers/{pts_layer}/style",
            json={
                "mode": "categorical",
                "label_field": "title",
                "style_spec": {
                    "field": "zone",
                    "categories": [
                        {"value": "north", "color": "#dc2626", "label": "Northern Zone"},
                        {"value": "south", "color": "#16a34a", "label": "Southern Zone"},
                    ],
                },
            },
        )
        assert st.status_code == 200, st.text
        assert len(st.json()["legend_items"]) == 2

        layout = client.post(
            f"{base}/cartography/layouts",
            json={
                "name": "A4 Hanoi Survey Map",
                "title": "Hanoi Station Buffers",
                "paper_size": "A4",
                "orientation": "landscape",
                "crs_code": 4326,
                "attribution": "OpenVietGIS Verification",
                "layer_ids": [clip_layer, pts_layer],
            },
        )
        assert layout.status_code == 201, layout.text
        layout_id = layout.json()["id"]

        def confirmed_print(output_format, dpi):
            path = f"{base}/cartography/layouts/{layout_id}/exports"
            settings = {"output_format": output_format, "dpi": dpi}
            preview = client.post(path + "/preview", json=settings)
            assert preview.status_code == 200, preview.text
            return client.post(
                path,
                json={
                    **settings,
                    "expected_revision_fingerprint": preview.json()["revision_fingerprint"],
                },
            )

        png_exp = confirmed_print("png", 150)
        assert png_exp.status_code == 201, png_exp.text
        png_meta = png_exp.json()
        assert len(png_meta["revision_manifest"]["layers"]) == 2
        png_dl = client.get(f"{base}/cartography/exports/{png_meta['id']}/download")
        assert png_dl.status_code == 200
        assert hashlib.sha256(png_dl.content).hexdigest() == png_meta["sha256"]
        img = Image.open(io.BytesIO(png_dl.content))
        assert img.format == "PNG" and img.width > 1000 and img.height > 800

        pdf_exp = confirmed_print("pdf", 300)
        assert pdf_exp.status_code == 201, pdf_exp.text
        pdf_dl = client.get(f"{base}/cartography/exports/{pdf_exp.json()['id']}/download")
        assert pdf_dl.status_code == 200
        assert pdf_dl.content.startswith(b"%PDF-")

    # Verify RLS hides rows when no tenant GUCs are bound on openvietgis_runtime
    with get_engine().connect() as conn:
        hidden_cnt = conn.execute(
            text("SELECT COUNT(*) FROM cartography.print_exports")
        ).scalar_one()
        assert int(hidden_cnt) == 0
