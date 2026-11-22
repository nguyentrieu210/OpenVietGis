"""R015 safety regressions for local recovery bytes."""

from __future__ import annotations

import hashlib
import json
import secrets
from datetime import UTC, datetime, timedelta
from pathlib import Path

import pytest

from tools.backup_restore import (
    _authenticate_manifest,
    _copy_verified,
    _finish_manifest,
    _load_manifest_key,
    _read_authenticated_manifest,
    _stage_verified_dump,
    _verify_references,
    backup_docker_postgres_and_artifacts,
    compute_artifact_manifest,
    verify_isolated_restore_docker,
)


@pytest.fixture
def signing_key(tmp_path: Path) -> Path:
    key = tmp_path / "operator.key"
    key.write_bytes(secrets.token_bytes(32))
    key.chmod(0o600)
    return key


def test_verified_artifact_copy_and_tamper(tmp_path: Path) -> None:
    store = tmp_path / "store"
    store.mkdir()
    (store / "data").mkdir()
    (store / "data" / "sample.bin").write_bytes(b"synthetic geometry")
    manifest = compute_artifact_manifest(store)
    target = tmp_path / "fresh"
    _copy_verified(store, target, manifest)
    assert (target / "data" / "sample.bin").read_bytes() == b"synthetic geometry"
    with pytest.raises(FileExistsError):
        _copy_verified(store, target, manifest)
    (store / "data" / "sample.bin").write_bytes(b"tampered")
    with pytest.raises(RuntimeError, match="corrupt"):
        _copy_verified(store, tmp_path / "bad", manifest)
    (store / "data" / "sample.bin").unlink()
    with pytest.raises(RuntimeError, match="missing"):
        _copy_verified(store, tmp_path / "missing", manifest)


def test_reject_symlink_and_traversal(tmp_path: Path) -> None:
    store = tmp_path / "store"
    store.mkdir()
    other = tmp_path / "external"
    other.write_bytes(b"outside store")
    (store / "link").symlink_to(other)
    with pytest.raises(RuntimeError, match="symbolic links"):
        compute_artifact_manifest(store)
    for key in ("../outside", "/absolute", "a/../../b", "a//b", "a\\b"):
        with pytest.raises(RuntimeError, match="Unsafe"):
            _verify_references([{"storage_key": key, "required": False}], {})


@pytest.mark.parametrize(
    ("revision", "has_documents"),
    [
        ("0019_extension_history", False),
        ("0029_cad_documents", True),
        ("0032_unified_collaboration", True),
        ("0032_unified_collaboration", False),
    ],
)
def test_inventory_includes_immutable_cad_versions(
    monkeypatch, revision: str, has_documents: bool
) -> None:
    from tools import backup_restore as recovery

    legacy = {
        "storage_key": "z/old",
        "kind": "cad",
        "required": True,
        "sha256": "a" * 64,
        "byte_size": 10,
    }
    version = {
        "storage_key": "a/version",
        "kind": "cad_document",
        "required": True,
        "sha256": "b" * 64,
        "byte_size": 20,
    }
    checks = {
        "revision": revision,
        "inconsistent_publications": 0,
    }

    def sql(container, user, database, query):
        if recovery._ARTIFACT_QUERY in query:
            return json.dumps([legacy])
        if recovery._DB_CHECK in query:
            return json.dumps(checks)
        if "to_regclass('cad.document_versions')" in query:
            return "t" if has_documents else "f"
        if recovery._CAD_DOCUMENT_ARTIFACT_QUERY in query:
            if not has_documents:
                raise RuntimeError("Missing CAD document table")
            return json.dumps([version])
        pytest.fail(f"Unexpected inventory query: {query}")

    monkeypatch.setattr(recovery, "_sql", sql)
    if revision == "0032_unified_collaboration" and not has_documents:
        with pytest.raises(RuntimeError, match="Missing CAD document table"):
            recovery._inventory("fixture", "fixture", "fixture")
        return
    refs, actual_checks = recovery._inventory("fixture", "fixture", "fixture")
    assert actual_checks == checks
    assert refs == ([version, legacy] if has_documents else [legacy])
    if has_documents:
        with pytest.raises(RuntimeError, match="missing or corrupt"):
            _verify_references(refs, {"z/old": {"sha256": "a" * 64, "byte_size": 10}})


def test_required_database_reference_cannot_be_missing() -> None:
    reference = {
        "storage_key": "assets/sample.bin",
        "required": True,
        "sha256": "a" * 64,
        "byte_size": 10,
    }
    with pytest.raises(RuntimeError, match="missing or corrupt"):
        _verify_references([reference], {})


def test_restore_rejects_old_manifest_and_requires_isolation(
    tmp_path: Path, signing_key: Path
) -> None:
    backup = tmp_path / "backup"
    backup.mkdir()
    (backup / "backup-manifest.json").write_text(json.dumps({"database": "source"}))
    kwargs = {
        "container_name": "unused",
        "db_user": "unused",
        "restore_db_name": "restore_test",
        "backup_dir": backup,
        "restore_artifact_dir": tmp_path / "restore",
        "manifest_key_file": signing_key,
        "expected_provenance": "synthetic-test",
    }
    with pytest.raises(RuntimeError, match="isolated instance"):
        verify_isolated_restore_docker(**kwargs)
    with pytest.raises(RuntimeError, match="unsupported backup"):
        verify_isolated_restore_docker(**kwargs, isolated_target_confirmed=True)


def test_backup_requires_consistency_attestation(
    tmp_path: Path, signing_key: Path
) -> None:
    with pytest.raises(RuntimeError, match="Quiesce"):
        backup_docker_postgres_and_artifacts(
            container_name="unused",
            db_user="unused",
            db_name="source",
            artifact_dir=tmp_path / "store",
            backup_dir=tmp_path / "backup",
            manifest_key_file=signing_key,
            provenance="synthetic-test",
        )


def test_completion_marker_is_atomic_and_never_overwritten(tmp_path: Path) -> None:
    root = tmp_path / "backup"
    root.mkdir()
    original = {"manifest_version": 3, "marker": "original"}
    _finish_manifest(root, original)
    assert json.loads((root / "backup-manifest.json").read_text()) == original
    assert not (root / ".backup-manifest-staged").exists()
    with pytest.raises(FileExistsError):
        _finish_manifest(root, {"marker": "replacement"})
    assert json.loads((root / "backup-manifest.json").read_text()) == original


@pytest.mark.parametrize(
    "failure",
    [
        "missing",
        "tampered",
        "same_authority",
        "future",
        "dump_changed",
        "dump_and_manifest_changed",
        "provenance_changed",
        "wrong_key",
        "missing_mac",
        "old_unsigned",
        "wrong_provenance",
        "duplicate_key",
    ],
)
def test_v4_restore_preflight_fails_before_creation(
    tmp_path: Path, signing_key: Path, monkeypatch, failure: str
) -> None:
    backup = tmp_path / "backup"
    backup.mkdir()
    artifacts = backup / "artifacts"
    artifacts.mkdir()
    payload = b"GIS synthetic artifact"
    original = artifacts / "sample.bin"
    original.write_bytes(payload)
    sql = b"SELECT 1;"
    (backup / "database.sql").write_bytes(sql)
    manifest = {
        "created_at": datetime.now(UTC).isoformat(),
        "database": "source",
        "dump_file": "database.sql",
        "dump_bytes": len(sql),
        "dump_sha256": hashlib.sha256(sql).hexdigest(),
        "artifact_count": 1,
        "artifacts": compute_artifact_manifest(artifacts),
        "references": [
            {
                "storage_key": "sample.bin",
                "required": True,
                "sha256": hashlib.sha256(payload).hexdigest(),
                "byte_size": len(payload),
            }
        ],
    }
    kwargs = {
        "container_name": "unused",
        "db_user": "unused",
        "restore_db_name": "new_restore",
        "backup_dir": backup,
        "restore_artifact_dir": tmp_path / "restore",
        "isolated_target_confirmed": True,
        "manifest_key_file": signing_key,
        "expected_provenance": "synthetic-test",
    }
    if failure == "missing":
        original.unlink()
    elif failure == "tampered":
        original.write_bytes(b"corrupted")
    elif failure == "same_authority":
        kwargs["current_authority"] = (
            "different-name-for-same-db",
            "unused",
            "new_restore",
        )
        kwargs["current_authority_confirmed"] = True
    elif failure == "future":
        manifest["created_at"] = (datetime.now(UTC) + timedelta(hours=1)).isoformat()
    signed = _authenticate_manifest(
        manifest, signing_key.read_bytes(), "synthetic-test"
    )
    malicious = b"\\! touch /tmp/not-allowed\n"
    if failure in ("dump_changed", "dump_and_manifest_changed"):
        (backup / "database.sql").write_bytes(malicious)
        if failure == "dump_and_manifest_changed":
            signed["dump_sha256"] = hashlib.sha256(malicious).hexdigest()
            signed["dump_bytes"] = len(malicious)
    elif failure == "provenance_changed":
        signed["provenance"] = "different-organization"
    elif failure == "wrong_key":
        signing_key.write_bytes(secrets.token_bytes(32))
    elif failure == "missing_mac":
        signed.pop("manifest_hmac_sha256")
    elif failure == "old_unsigned":
        signed["manifest_version"] = 3
        signed.pop("manifest_hmac_sha256")
    elif failure == "wrong_provenance":
        kwargs["expected_provenance"] = "untrusted-source"
    _finish_manifest(backup, signed)
    if failure == "duplicate_key":
        marker = backup / "backup-manifest.json"
        marker.write_text(marker.read_text().replace("{", '{"database":"other",', 1))

    def forbidden(*args, **kwargs):
        pytest.fail("Unsafe preflight reached database creation")

    monkeypatch.setattr("tools.backup_restore.subprocess.run", forbidden)
    with pytest.raises(RuntimeError):
        verify_isolated_restore_docker(**kwargs)
    assert not (tmp_path / "restore").exists()


def test_private_independent_key_required(tmp_path: Path, signing_key: Path) -> None:
    root = tmp_path / "backup"
    root.mkdir()
    key = signing_key.read_bytes()
    assert _load_manifest_key(signing_key, root) == key
    signing_key.chmod(0o644)
    with pytest.raises(RuntimeError, match="private"):
        _load_manifest_key(signing_key, root)
    signing_key.chmod(0o600)
    inside = root / "copied.key"
    inside.write_bytes(key)
    inside.chmod(0o600)
    with pytest.raises(RuntimeError, match="disjoint"):
        _load_manifest_key(inside, root)
    alias = tmp_path / "symlink.key"
    alias.symlink_to(signing_key)
    with pytest.raises(RuntimeError, match="unsafe"):
        _load_manifest_key(alias, root)
    signing_key.write_bytes(b"weak")
    with pytest.raises(RuntimeError, match="32 random bytes"):
        _load_manifest_key(signing_key, root)


def test_authenticated_manifest_roundtrip_and_exact_dump_bytes(
    tmp_path: Path, signing_key: Path
) -> None:
    key = signing_key.read_bytes()
    original = b"SELECT 1;"
    dump = tmp_path / "database.sql"
    dump.write_bytes(original)
    signed = _authenticate_manifest(
        {
            "dump_sha256": hashlib.sha256(original).hexdigest(),
            "dump_bytes": len(original),
        },
        key,
        "source-a",
    )
    _finish_manifest(tmp_path, signed)
    checked = _read_authenticated_manifest(
        tmp_path / "backup-manifest.json", key, "source-a"
    )
    from tempfile import TemporaryFile

    with TemporaryFile(mode="w+b") as staged:
        _stage_verified_dump(dump, staged, checked)
        # Simulate substitution of the original after validation but before psql.
        dump.write_bytes(b"malicious replacement")
        assert staged.read() == original
