from __future__ import annotations

import hashlib
import json
import os
import shutil
import subprocess
from pathlib import Path
from uuid import UUID, uuid4

from sqlalchemy import create_engine, text
from sqlalchemy.engine import make_url
from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.spatial.test_sources import _client

from modules.house_numbering.migrate import upgrade as migrate_house_numbering
from modules.house_numbering.tests.integration.test_house_numbering_runtime import (
    _seed_source,
)
from tools.backup_restore import (
    backup_docker_postgres_and_artifacts,
    compute_artifact_manifest,
    verify_isolated_restore_docker,
)

CONTAINER = "dev-postgres-test-1"
DB_NAME = "openvietgis_test"
DB_USER = "openvietgis"
RESTORE_DB = "openvietgis_test_hn019_restore"
ARTIFACT_ROOT = Path(
    os.environ.get("OPENVIETGIS_ARTIFACT_STORE_DIR", "/tmp/hn019-recovery-artifacts")
)
BACKUP_ROOT = Path("/tmp/hn019-recovery-backup")
RESTORE_ARTIFACT_ROOT = Path("/tmp/hn019-recovery-restored-artifacts")
OUTPUT = Path("/tmp/hn019-recovery.json")


def _drop_restore() -> None:
    subprocess.run(
        [
            "docker",
            "exec",
            CONTAINER,
            "dropdb",
            "-U",
            DB_USER,
            "--if-exists",
            "--force",
            RESTORE_DB,
        ],
        check=True,
        capture_output=True,
    )


def _sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _module_artifact_files(root: Path, artifact_id: UUID) -> tuple[Path, Path]:
    matches = list(root.glob(f"extensions/*/*/*/{artifact_id}/*.json"))
    if len(matches) != 1:
        raise RuntimeError(
            f"expected one metadata file for artifact {artifact_id}, got {len(matches)}"
        )
    meta_path = matches[0]
    meta = json.loads(meta_path.read_text(encoding="utf-8"))
    data_path = meta_path.parent / str(meta["filename"])
    if not data_path.is_file():
        raise RuntimeError(f"artifact bytes missing for {artifact_id}")
    if _sha(data_path) != meta["sha256"] or data_path.stat().st_size != int(
        meta["byte_size"]
    ):
        raise RuntimeError(f"artifact integrity mismatch for {artifact_id}")
    return meta_path, data_path


def _source_module_state(engine, org: UUID, project: UUID) -> dict[str, object]:
    with engine.connect() as c:
        counts = {
            table: int(
                c.execute(
                    text(
                        f"SELECT count(*) FROM house_numbering.{table} "
                        "WHERE organization_id=:org AND project_id=:project"
                    ),
                    {"org": org, "project": project},
                ).scalar_one()
            )
            for table in (
                "rules",
                "routes",
                "buildings",
                "address_numbers",
                "import_batches",
                "surveys",
                "documents",
                "access_log",
            )
        }
        import_ids = [
            UUID(str(v))
            for v in c.execute(
                text(
                    "SELECT original_artifact_id FROM house_numbering.import_batches "
                    "WHERE organization_id=:org AND project_id=:project "
                    "AND original_artifact_id IS NOT NULL ORDER BY id"
                ),
                {"org": org, "project": project},
            ).scalars()
        ]
        document_ids = [
            UUID(str(v))
            for v in c.execute(
                text(
                    "SELECT artifact_id FROM house_numbering.documents "
                    "WHERE organization_id=:org AND project_id=:project "
                    "AND artifact_id IS NOT NULL ORDER BY id"
                ),
                {"org": org, "project": project},
            ).scalars()
        ]
        photo_rows = c.execute(
            text(
                "SELECT photo_artifact_ids FROM house_numbering.surveys "
                "WHERE organization_id=:org AND project_id=:project ORDER BY id"
            ),
            {"org": org, "project": project},
        ).scalars()
        photo_ids = [UUID(str(v)) for row in photo_rows for v in (row or [])]
    return {
        "counts": counts,
        "referenced_artifact_ids": [
            str(v) for v in (*import_ids, *document_ids, *photo_ids)
        ],
    }


def _seed() -> tuple[UUID, UUID, list[UUID]]:
    migrate_house_numbering()
    world = seed_access_world()

    line_source = _seed_source(
        organization_id=world.organization_id,
        project_id=world.project_id,
        reference_key=f"hn_recovery_line_{uuid4().hex[:8]}",
        family="LineString",
        feature_id="route-1",
        wkt="LINESTRING(0 0,100 0)",
        extent={"min_x": 0, "min_y": 0, "max_x": 100, "max_y": 0, "crs_code": 3857},
    )
    point_source = _seed_source(
        organization_id=world.organization_id,
        project_id=world.project_id,
        reference_key=f"hn_recovery_point_{uuid4().hex[:8]}",
        family="Point",
        feature_id="house-1",
        wkt="POINT(20 -10)",
        extent={"min_x": 20, "min_y": -10, "max_x": 20, "max_y": -10, "crs_code": 3857},
    )

    org_base = f"/api/v1/organizations/{world.organization_id}"
    proj_base = f"{org_base}/projects/{world.project_id}"
    module_base = f"{proj_base}/modules/house-numbering"
    created_artifacts: list[UUID] = []

    with _client(world.owner_user_id) as client:
        manifest = client.post(
            f"{org_base}/extensions/official/house_numbering/register"
        )
        assert manifest.status_code == 201, manifest.text
        activation = client.post(
            f"{proj_base}/extensions/activations",
            json={"manifest_id": manifest.json()["id"]},
        )
        assert activation.status_code == 201, activation.text

        rule = client.post(
            f"{module_base}/rules",
            json={
                "version": "RECOVERY-019",
                "status": "authoritative",
                "authority_ref": "SYNTHETIC-RECOVERY-019",
                "left_odd": True,
                "suffix_style": "letters",
                "max_distance_m": 50,
            },
        )
        assert rule.status_code == 201, rule.text
        route = client.post(
            f"{module_base}/routes",
            json={
                "name": "Recovery route",
                "route_type": "street",
                "source_id": str(line_source),
                "feature_id": "route-1",
                "direction_state": "forward",
                "source_label": "synthetic recovery fixture",
                "status": "confirmed",
            },
        )
        assert route.status_code == 201, route.text
        building = client.post(
            f"{module_base}/buildings",
            json={
                "route_id": route.json()["id"],
                "source_id": str(point_source),
                "feature_id": "house-1",
                "eligibility": "eligible",
            },
        )
        assert building.status_code == 201, building.text
        building_id = building.json()["id"]

        preview = client.post(
            f"{module_base}/previews",
            json={
                "building_ids": [building_id],
                "rule_id": rule.json()["id"],
                "operation_id": str(uuid4()),
            },
        )
        assert preview.status_code == 201, preview.text
        applied = client.post(f"{module_base}/previews/{preview.json()['id']}/apply")
        assert applied.status_code == 200, applied.text
        proposal = client.post(
            f"{module_base}/proposals",
            json={
                "scope_kind": "route",
                "scope_id": route.json()["id"],
                "rule_id": rule.json()["id"],
                "building_ids": [building_id],
            },
        )
        proposal_id = proposal.json()["proposal"]["id"]
        assert (
            client.post(
                f"{module_base}/proposals/{proposal_id}/transition",
                json={"expected_revision": 1, "action": "submit"},
            ).status_code
            == 200
        )
        assert (
            client.post(
                f"{module_base}/proposals/{proposal_id}/transition",
                json={"expected_revision": 2, "action": "approve"},
            ).status_code
            == 200
        )

        document = client.post(
            f"{module_base}/documents",
            json={
                "building_id": building_id,
                "doc_type": "certificate",
                "output_format": "pdf",
                "template_revision": "recovery-template-v1",
                "template_verified": True,
            },
        )
        assert document.status_code == 201, document.text
        created_artifacts.append(UUID(document.json()["artifact_id"]))

        csv_data = (
            f"building_id,route_id,raw_number,source_reference\n"
            f"{building_id},{route.json()['id']},2,RECOVERY-CERT\n"
        ).encode()
        imported = client.post(
            f"{module_base}/imports",
            data={
                "source_format": "csv",
                "origin": "certificate",
                "mapping_json": json.dumps(
                    {
                        "building_id": "building_id",
                        "route_id": "route_id",
                        "raw_number": "raw_number",
                        "source_reference": "source_reference",
                    }
                ),
                "default_source_reference": "recovery.csv",
                "default_confidence": "authoritative",
            },
            files={"file": ("recovery.csv", csv_data, "text/csv")},
        )
        assert imported.status_code == 201, imported.text
        created_artifacts.append(UUID(imported.json()["batch"]["original_artifact_id"]))

        photo = client.post(
            f"{module_base}/survey/photos",
            files={"file": ("field.jpg", b"synthetic-photo-bytes", "image/jpeg")},
        )
        assert photo.status_code == 201, photo.text
        photo_id = UUID(photo.json()["id"])
        created_artifacts.append(photo_id)
        synced = client.post(
            f"{module_base}/survey/sync",
            json={
                "operations": [
                    {
                        "client_uuid": str(uuid4()),
                        "building_id": building_id,
                        "condition": "house",
                        "photo_artifact_ids": [str(photo_id)],
                    }
                ]
            },
        )
        assert synced.status_code == 200, synced.text

        exported = client.post(
            f"{module_base}/exports",
            json={
                "fields": ["building_id", "house_number", "route_name"],
                "output_format": "csv",
                "include_private": False,
            },
        )
        assert exported.status_code == 201, exported.text
        created_artifacts.append(UUID(exported.json()["artifact_id"]))

    return world.organization_id, world.project_id, created_artifacts


def run() -> dict[str, object]:
    for path in (ARTIFACT_ROOT, BACKUP_ROOT, RESTORE_ARTIFACT_ROOT):
        if path.exists():
            shutil.rmtree(path)
    ARTIFACT_ROOT.mkdir(parents=True, exist_ok=True)
    _drop_restore()

    org, project, expected_artifacts = _seed()
    source_url = os.environ["OPENVIETGIS_TEST_DATABASE_URL"]
    source_engine = create_engine(source_url)
    source_state = _source_module_state(source_engine, org, project)
    for artifact_id in expected_artifacts:
        _module_artifact_files(ARTIFACT_ROOT, artifact_id)

    source_manifest = compute_artifact_manifest(ARTIFACT_ROOT)
    backup_meta = backup_docker_postgres_and_artifacts(
        container_name=CONTAINER,
        db_user=DB_USER,
        db_name=DB_NAME,
        artifact_dir=ARTIFACT_ROOT,
        backup_dir=BACKUP_ROOT,
        quiesced=True,
    )
    restore_meta = verify_isolated_restore_docker(
        container_name=CONTAINER,
        db_user=DB_USER,
        restore_db_name=RESTORE_DB,
        backup_dir=BACKUP_ROOT,
        restore_artifact_dir=RESTORE_ARTIFACT_ROOT,
        isolated_target_confirmed=True,
    )

    restored_url = (
        make_url(source_url)
        .set(database=RESTORE_DB)
        .render_as_string(hide_password=False)
    )
    restore_engine = create_engine(restored_url)
    restored_state = _source_module_state(restore_engine, org, project)
    if restored_state != source_state:
        raise RuntimeError("House Numbering database state changed after restore")

    restored_manifest = compute_artifact_manifest(RESTORE_ARTIFACT_ROOT)
    if restored_manifest != source_manifest:
        raise RuntimeError("House Numbering artifact tree changed after restore")
    for artifact_id in expected_artifacts:
        _, source_bytes = _module_artifact_files(ARTIFACT_ROOT, artifact_id)
        _, restored_bytes = _module_artifact_files(RESTORE_ARTIFACT_ROOT, artifact_id)
        if _sha(source_bytes) != _sha(restored_bytes):
            raise RuntimeError(f"restored artifact bytes differ for {artifact_id}")

    result: dict[str, object] = {
        "organization_id": str(org),
        "project_id": str(project),
        "house_numbering_state": source_state,
        "module_artifacts_created": len(expected_artifacts),
        "artifact_files_total": len(source_manifest),
        "database_dump_sha256": backup_meta["dump_sha256"],
        "database_dump_bytes": backup_meta["dump_bytes"],
        "restore": restore_meta,
        "module_db_state_identical": True,
        "module_artifact_tree_identical": True,
        "module_artifact_references_verified": len(
            source_state["referenced_artifact_ids"]
        ),
        "synthetic_only": True,
    }

    restore_engine.dispose()
    source_engine.dispose()
    _drop_restore()
    shutil.rmtree(RESTORE_ARTIFACT_ROOT)
    result["rollback_cleanup"] = {
        "restore_database_removed": True,
        "restore_artifact_directory_removed": True,
    }
    OUTPUT.write_text(json.dumps(result, indent=2, sort_keys=True), encoding="utf-8")
    return result


if __name__ == "__main__":
    print(json.dumps(run(), indent=2, sort_keys=True))
