import hashlib
from pathlib import Path

import pytest

from tools.release_consolidation import PLUGIN, artifact_hashes


def fixture(root: Path) -> None:
    (root / "downloads").mkdir()
    (root / "index.html").write_text("test")
    (root / PLUGIN).write_bytes(b"zip fixture")
    digest = hashlib.sha256(b"zip fixture").hexdigest()
    (root / (PLUGIN + ".sha256")).write_text(f"{digest}  {Path(PLUGIN).name}\n")


def test_artifact_set(tmp_path):
    fixture(tmp_path)
    assert len(artifact_hashes(tmp_path)) == 3
    (tmp_path / "extra.js").write_text("extra")
    assert len(artifact_hashes(tmp_path)) == 4


def test_connector_mismatch(tmp_path):
    fixture(tmp_path)
    (tmp_path / PLUGIN).write_bytes(b"changed")
    with pytest.raises(ValueError, match="checksum"):
        artifact_hashes(tmp_path)


def test_missing_connector(tmp_path):
    (tmp_path / "index.html").write_text("test")
    with pytest.raises(ValueError, match="Missing"):
        artifact_hashes(tmp_path)


def test_symlink(tmp_path):
    fixture(tmp_path)
    (tmp_path / "alias").symlink_to(tmp_path / "index.html")
    with pytest.raises(ValueError, match="symlink"):
        artifact_hashes(tmp_path)


def test_manifest_drift(tmp_path, monkeypatch):
    from tools import release_consolidation as rc

    repo = tmp_path / "repo"
    repo.mkdir()
    for name in rc.LOCKS:
        path = repo / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text("locked")
    dist = tmp_path / "dist"
    dist.mkdir()
    fixture(dist)
    monkeypatch.setattr(
        rc, "git", lambda repo, *args: "" if args[0] == "status" else "a" * 40
    )
    manifest = rc.candidate_manifest(repo, dist, "0042")
    assert rc.verify_manifest(repo, dist, manifest) == []
    (dist / "index.html").write_text("tampered")
    assert "Artifact set or hash mismatch" in rc.verify_manifest(repo, dist, manifest)
    (repo / rc.LOCKS[0]).write_text("changed")
    assert "Dependency lock mismatch" in rc.verify_manifest(repo, dist, manifest)
