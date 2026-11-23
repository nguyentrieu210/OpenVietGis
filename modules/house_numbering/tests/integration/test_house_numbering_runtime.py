"""Real PostGIS vertical-slice acceptance for House Numbering US1-US3."""

from __future__ import annotations

import json
from uuid import uuid4

from sqlalchemy import text
from tests.fixtures.db_admin import get_admin_engine
from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.spatial.test_sources import _client

from modules.house_numbering.migrate import upgrade as migrate_house_numbering


def _seed_source(
    *,
    organization_id,
    project_id,
    reference_key: str,
    family: str,
    feature_id: str,
    wkt: str,
    extent: dict[str, float | int],
):
    source_id = uuid4()
    content = "a" * 64
    schema = "b" * 64
    manifest = "c" * 64
    with get_admin_engine().begin() as connection:
        connection.execute(
            text("""
                INSERT INTO spatial.data_sources (
                    id, organization_id, project_id, name, kind, reference_key, fixture_version,
                    manifest_digest, source_asset_id, lifecycle, validation_state,
                    validation_reason,
                    source_crs, stored_crs, schema_state, geometry_family, dimensions,
                    feature_count_state, feature_count, extent_state, extent,
                    catalog_revision, content_revision, schema_revision, created_at,
                    updated_at, retired_at
                ) VALUES (
                    :id,:org,:project,:name,'reference_fixture',:key,'hn-test-v1',
                    :manifest,NULL,'active','ready',NULL,
                    3857,3857,'known',:family,'XY',
                    'known',1,'available',CAST(:extent AS jsonb),
                    1,:content,:schema,now(),now(),NULL
                )
            """),
            {
                "id": source_id,
                "org": organization_id,
                "project": project_id,
                "name": f"HN test {family}",
                "key": reference_key,
                "manifest": manifest,
                "family": family,
                "extent": json.dumps(extent),
                "content": content,
                "schema": schema,
            },
        )
        connection.execute(
            text("""
                INSERT INTO spatial.reference_features (
                    organization_id,project_id,source_id,feature_id,attributes,geometry,
                    geometry_state,content_revision,schema_revision
                ) VALUES (
                    :org,:project,:source,:feature,'{}'::jsonb,
                    ST_GeomFromText(:wkt,3857),'present',:content,:schema
                )
            """),
            {
                "org": organization_id,
                "project": project_id,
                "source": source_id,
                "feature": feature_id,
                "wkt": wkt,
                "content": content,
                "schema": schema,
            },
        )
    return source_id


def test_house_numbering_us1_us3_real_postgis(migrated_database: None) -> None:
    migrate_house_numbering()
    world = seed_access_world()
    line_source = _seed_source(
        organization_id=world.organization_id,
        project_id=world.project_id,
        reference_key=f"hn_line_{uuid4().hex[:8]}",
        family="LineString",
        feature_id="route-1",
        wkt="LINESTRING(0 0,100 0)",
        extent={"min_x": 0, "min_y": 0, "max_x": 100, "max_y": 0, "crs_code": 3857},
    )
    point_source = _seed_source(
        organization_id=world.organization_id,
        project_id=world.project_id,
        reference_key=f"hn_point_{uuid4().hex[:8]}",
        family="Point",
        feature_id="house-1",
        wkt="POINT(20 -10)",
        extent={"min_x": 20, "min_y": -10, "max_x": 20, "max_y": -10, "crs_code": 3857},
    )

    org_base = f"/api/v1/organizations/{world.organization_id}"
    proj_base = f"{org_base}/projects/{world.project_id}"
    module_base = f"{proj_base}/modules/house-numbering"

    with _client(world.owner_user_id) as client:
        registered = client.post(
            f"{org_base}/extensions/official/house_numbering/register"
        )
        assert registered.status_code == 201, registered.text
        manifest_id = registered.json()["id"]
        assert registered.json()["version"] == "2.0.0"
        assert (
            registered.json()["ui_contributions"][0]["route_suffix"]
            == "/extensions/house_numbering"
        )

        activated = client.post(
            f"{proj_base}/extensions/activations", json={"manifest_id": manifest_id}
        )
        assert activated.status_code == 201, activated.text
        assert activated.json()["status"] == "active"

        rule = client.post(
            f"{module_base}/rules",
            json={
                "version": "test-v1",
                "status": "authoritative",
                "authority_ref": "TEST-ONLY-AUTHORITY",
                "left_odd": True,
                "suffix_style": "letters",
                "max_distance_m": 50,
            },
        )
        assert rule.status_code == 201, rule.text
        rule_id = rule.json()["id"]

        route = client.post(
            f"{module_base}/routes",
            json={
                "name": "Tuyến thử",
                "route_type": "street",
                "source_id": str(line_source),
                "feature_id": "route-1",
                "direction_state": "forward",
                "source_label": "synthetic integration fixture",
                "status": "confirmed",
            },
        )
        assert route.status_code == 201, route.text
        route_id = route.json()["id"]

        building = client.post(
            f"{module_base}/buildings",
            json={
                "route_id": route_id,
                "source_id": str(point_source),
                "feature_id": "house-1",
                "eligibility": "eligible",
            },
        )
        assert building.status_code == 201, building.text
        building_id = building.json()["id"]

        inspected = client.post(
            f"{module_base}/inspect",
            json={
                "x": 20,
                "y": -10,
                "crs_code": 3857,
                "route_id": route_id,
                "rule_id": rule_id,
                "existing_building_id": building_id,
            },
        )
        assert inspected.status_code == 200, inspected.text
        body = inspected.json()
        assert body["location"]["side"] == "right"
        assert body["suggestion"]["number"] == "2"
        assert body["suggestion"]["reason"] == "empty_route_side"

        preview = client.post(
            f"{module_base}/previews",
            json={
                "building_ids": [building_id],
                "rule_id": rule_id,
                "operation_id": str(uuid4()),
            },
        )
        assert preview.status_code == 201, preview.text
        preview_body = preview.json()
        assert preview_body["candidate_changes"][0]["number"] == "2"

        # Preview is read-only.
        with get_admin_engine().begin() as connection:
            before = connection.execute(
                text(
                    "SELECT count(*) FROM house_numbering.address_numbers "
                    "WHERE organization_id=:org AND project_id=:project"
                ),
                {"org": world.organization_id, "project": world.project_id},
            ).scalar_one()
        assert before == 0

        applied = client.post(f"{module_base}/previews/{preview_body['id']}/apply")
        assert applied.status_code == 200, applied.text
        assert applied.json()["items"][0]["number_text"] == "2"
        assert applied.json()["items"][0]["kind"] == "proposed"

        proposal = client.post(
            f"{module_base}/proposals",
            json={
                "scope_kind": "route",
                "scope_id": route_id,
                "rule_id": rule_id,
                "building_ids": [building_id],
            },
        )
        assert proposal.status_code == 201, proposal.text
        proposal_id = proposal.json()["proposal"]["id"]
        assert proposal.json()["proposal"]["status"] == "draft"
        queue = client.get(f"{module_base}/proposals?status=draft")
        assert queue.status_code == 200, queue.text
        assert any(
            row["id"] == proposal_id and row["item_count"] == 1
            for row in queue.json()["items"]
        )
        assert client.get(f"{module_base}/proposals?status=unknown").status_code == 422
        draft_pdf = client.get(
            f"{module_base}/proposals/{proposal_id}/packets/decision"
        )
        assert draft_pdf.status_code == 200 and draft_pdf.content.startswith(b"%PDF")
        import io
        import zipfile

        registry = client.get(f"{module_base}/proposals/{proposal_id}/packets/register")
        assert registry.status_code == 200 and zipfile.is_zipfile(
            io.BytesIO(registry.content)
        )
        appendix = client.get(f"{module_base}/proposals/{proposal_id}/packets/appendix")
        assert appendix.status_code == 200 and appendix.content.startswith(b"%PDF")
        assert (
            client.get(
                f"{module_base}/proposals/{proposal_id}/packets/plates"
            ).status_code
            == 409
        )

        submitted = client.post(
            f"{module_base}/proposals/{proposal_id}/transition",
            json={"expected_revision": 1, "action": "submit"},
        )
        assert submitted.status_code == 200, submitted.text
        assert submitted.json()["status"] == "submitted"

        approved = client.post(
            f"{module_base}/proposals/{proposal_id}/transition",
            json={
                "expected_revision": 2,
                "action": "approve",
                "comment": "test approval",
            },
        )
        assert approved.status_code == 200, approved.text
        assert approved.json()["status"] == "approved"
        approved_queue = client.get(f"{module_base}/proposals?status=approved")
        assert approved_queue.status_code == 200
        assert any(row["id"] == proposal_id for row in approved_queue.json()["items"])
        plates = client.get(f"{module_base}/proposals/{proposal_id}/packets/plates")
        assert plates.status_code == 200 and plates.content.startswith(b"%PDF"), (
            plates.text[:300] if plates.status_code != 200 else ""
        )
        assert (
            client.get(
                f"{module_base}/proposals/{proposal_id}/packets/plates?offset=50"
            ).status_code
            == 422
        )

        recheck = client.post(
            f"{module_base}/inspect",
            json={
                "x": 20,
                "y": -10,
                "crs_code": 3857,
                "route_id": route_id,
                "rule_id": rule_id,
                "existing_building_id": building_id,
            },
        )
        assert recheck.status_code == 200, recheck.text
        assert recheck.json()["suggestion"]["number"] == "2"
        assert recheck.json()["suggestion"]["reason"] == "existing_number"

        # US4/US5: import certificate + survey evidence, replay safely, reconcile.
        certificate_csv = (
            f"building_id,route_id,raw_number\n{building_id},{route_id},2\n".encode()
        )
        import_form = {
            "source_format": "csv",
            "origin": "certificate",
            "mapping_json": json.dumps(
                {
                    "building_id": "building_id",
                    "route_id": "route_id",
                    "raw_number": "raw_number",
                }
            ),
            "default_source_reference": "CERT-TEST-001",
            "default_confidence": "authoritative",
        }
        imported = client.post(
            f"{module_base}/imports",
            data=import_form,
            files={"file": ("certificate.csv", certificate_csv, "text/csv")},
        )
        assert imported.status_code == 201, imported.text
        assert imported.json()["batch"]["accepted_count"] == 1
        assert imported.json()["batch"]["rejected_count"] == 0
        replayed_import = client.post(
            f"{module_base}/imports",
            data=import_form,
            files={"file": ("certificate.csv", certificate_csv, "text/csv")},
        )
        assert replayed_import.status_code == 201, replayed_import.text
        assert replayed_import.json()["replayed"] is True

        survey_evidence_form = {**import_form, "origin": "survey"}
        survey_evidence = client.post(
            f"{module_base}/imports",
            data=survey_evidence_form,
            files={"file": ("survey.csv", certificate_csv, "text/csv")},
        )
        assert survey_evidence.status_code == 201, survey_evidence.text
        reconciled = client.post(
            f"{module_base}/reconciliation/rebuild",
            json={"removed_building_ids": [], "estimated_building_ids": []},
        )
        assert reconciled.status_code == 200, reconciled.text
        assert any(
            item["case_class"] == "matched" for item in reconciled.json()["items"]
        )

        # US6/US7: offline survey replay + review + real progress aggregation.
        assignment = client.post(
            f"{module_base}/assignments",
            json={
                "scope_kind": "route",
                "scope_id": route_id,
                "assignee_actor_id": str(world.owner_user_id),
                "starts_on": "2020-01-01",
                "target_count": 1,
            },
        )
        assert assignment.status_code == 201, assignment.text

        client_uuid = str(uuid4())
        survey_operation = {
            "client_uuid": client_uuid,
            "building_id": building_id,
            "x": 20,
            "y": -10,
            "crs_code": 3857,
            "condition": "house",
            "photo_artifact_ids": [],
        }
        synced = client.post(
            f"{module_base}/survey/sync", json={"operations": [survey_operation]}
        )
        assert synced.status_code == 200, synced.text
        survey_id = synced.json()["applied"][0]
        replay = client.post(
            f"{module_base}/survey/sync", json={"operations": [survey_operation]}
        )
        assert replay.status_code == 200, replay.text
        assert replay.json()["replayed"] == [survey_id]

        reviewed = client.post(
            f"{module_base}/surveys/{survey_id}/review",
            json={"expected_revision": 1, "decision": "accepted"},
        )
        assert reviewed.status_code == 200, reviewed.text
        assert reviewed.json()["state"] == "accepted"

        progress = client.get(f"{module_base}/progress")
        assert progress.status_code == 200, progress.text
        assert progress.json()["positions"] == 1
        assert progress.json()["houses"] == 1
        assert progress.json()["completion"] == 100.0
        xlsx = client.get(f"{module_base}/progress/report.xlsx")
        assert xlsx.status_code == 200, xlsx.text
        assert xlsx.content.startswith(b"PK")

        # US8/US9: official dossier generated only from approved authoritative state.
        document = client.post(
            f"{module_base}/documents",
            json={
                "building_id": building_id,
                "doc_type": "certificate",
                "output_format": "pdf",
                "template_revision": "test-template-v1",
                "template_verified": True,
            },
        )
        assert document.status_code == 201, document.text
        assert document.json()["status"] == "official"
        document_id = document.json()["id"]
        downloaded = client.get(f"{module_base}/documents/{document_id}/download")
        assert downloaded.status_code == 200, downloaded.text
        assert downloaded.headers["content-type"].startswith("application/pdf")
        assert downloaded.content.startswith(b"%PDF")

        # US10/US11: search + standard export remain public-field allowlisted.
        searched = client.get(f"{module_base}/search", params={"query": "2"})
        assert searched.status_code == 200, searched.text
        assert any(
            item["building_id"] == building_id for item in searched.json()["items"]
        )

        private = client.post(
            f"{module_base}/private-households",
            json={
                "building_id": building_id,
                "purpose": "integration privacy verification",
                "consent": True,
                "lawful_basis_ref": "TEST-CONSENT",
                "protected_data": {
                    "owner_name": "SENSITIVE OWNER",
                    "phone": "0900000000",
                },
            },
        )
        assert private.status_code == 201, private.text
        private_read = client.get(
            f"{module_base}/buildings/{building_id}/private-households",
            params={"purpose": "integration privacy verification"},
        )
        assert private_read.status_code == 200, private_read.text
        assert (
            private_read.json()["items"][0]["protected_data"]["owner_name"]
            == "SENSITIVE OWNER"
        )

        exported = client.post(
            f"{module_base}/exports",
            json={
                "fields": ["building_id", "house_number", "route_name"],
                "output_format": "csv",
                "include_private": False,
            },
        )
        assert exported.status_code == 201, exported.text
        export_id = exported.json()["artifact_id"]
        export_download = client.get(f"{module_base}/exports/{export_id}/download")
        assert export_download.status_code == 200, export_download.text
        assert b"SENSITIVE OWNER" not in export_download.content
        assert b"0900000000" not in export_download.content
        assert b"house_number" in export_download.content

        disabled = client.post(f"{proj_base}/extensions/house_numbering/disable")
        assert disabled.status_code == 200, disabled.text
        assert client.get(f"{module_base}/routes").status_code == 404

        enabled = client.post(
            f"{proj_base}/extensions/activations", json={"manifest_id": manifest_id}
        )
        assert enabled.status_code == 201, enabled.text
        routes = client.get(f"{module_base}/routes")
        assert routes.status_code == 200, routes.text
        assert routes.json()["items"][0]["id"] == route_id
