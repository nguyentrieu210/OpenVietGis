"""Digest plan validation is deliberately independent of live Docker."""
import copy
import importlib.util
import json
import subprocess
from pathlib import Path

import pytest
import yaml

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("image_manifest", ROOT / "infra/release/image_manifest.py")
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)
REPO = "example/openvietgis"
SHA = "a" * 40


def receipt():
    return {
        "schema": 1, "source_sha": SHA, "repository": REPO, "platform": "linux/amd64",
        "images": {role: {"ref": f"ghcr.io/{REPO}-{role}@sha256:{str(i) * 64}", "source_sha": SHA}
                   for i, role in enumerate(MODULE.ROLES, 1)},
    }


def test_exact_digest_manifest_and_inert_compose_override():
    images = MODULE.validate(receipt(), REPO, SHA)
    override = MODULE.compose_override(images)
    assert override.count("build: !reset null") == 4
    assert override.count("pull_policy: never") == 4
    assert all(ref in override for ref in images.values())


@pytest.mark.parametrize("change", ["tag", "other_registry", "other_repo", "wrong_sha", "missing_role",
                                    "wrong_platform", "duplicate_digest", "extra_field", "newline"])
def test_rejects_untrusted_mutable_inconsistent_receipts(change):
    value = copy.deepcopy(receipt())
    if change == "tag":
        value["images"]["api"]["ref"] = f"ghcr.io/{REPO}-api:latest"
    elif change == "other_registry":
        value["images"]["api"]["ref"] = value["images"]["api"]["ref"].replace("ghcr.io", "other.test")
    elif change == "other_repo":
        value["repository"] = "other/repo"
    elif change == "wrong_sha":
        value["images"]["api"]["source_sha"] = "b" * 40
    elif change == "missing_role":
        del value["images"]["migrate"]
    elif change == "wrong_platform":
        value["platform"] = "linux/arm64"
    elif change == "duplicate_digest":
        value["images"]["api"]["ref"] = f"ghcr.io/{REPO}-api@sha256:" + "2" * 64
    elif change == "extra_field":
        value["approved"] = True
    elif change == "newline":
        value["images"]["api"]["ref"] += "\n    command: evil"
    with pytest.raises(ValueError):
        MODULE.validate(value, REPO, SHA)


def test_resolved_compose_rejects_hybrid_or_any_build():
    images = MODULE.validate(receipt(), REPO, SHA)
    config = {"services": {role: {"image": ref, "pull_policy": "never"} for role, ref in images.items()}}
    MODULE.verify_compose(config, images)
    config["services"]["web"]["build"] = {"context": "."}
    with pytest.raises(ValueError, match="build"):
        MODULE.verify_compose(config, images)
    del config["services"]["web"]["build"]
    config["services"]["api-rust"] = {"image": "old"}
    with pytest.raises(ValueError, match="Hybrid"):
        MODULE.verify_compose(config, images)


def test_local_inspection_checks_digest_platform_and_revision(monkeypatch):
    images = MODULE.validate(receipt(), REPO, SHA)
    valid = True

    def fake_run(args, **kwargs):
        assert args[:3] == ["docker", "image", "inspect"]
        result = [{"RepoDigests": [args[3]], "Os": "linux", "Architecture": "amd64",
                   "Config": {"Labels": {"org.opencontainers.image.revision": SHA if valid else "b" * 40}}}]
        return subprocess.CompletedProcess(args, 0, json.dumps(result), "")

    monkeypatch.setattr(MODULE.subprocess, "run", fake_run)
    MODULE.verify_local(images, SHA)
    valid = False
    with pytest.raises(ValueError, match="revision"):
        MODULE.verify_local(images, SHA)


def test_release_workflow_is_manual_main_only_and_no_deploy():
    data = yaml.safe_load((ROOT / ".github/workflows/release-images.yml").read_text())
    # PyYAML 1.1 parses the GitHub Actions 'on' key as True.
    assert set(data.get("on", data.get(True))) == {"workflow_dispatch"}
    jobs = data["jobs"]
    assert set(jobs) == {"build", "receipt"}
    assert all(job["if"] == "github.ref == 'refs/heads/main'" for job in jobs.values())
    assert {item["role"] for item in jobs["build"]["strategy"]["matrix"]["include"]} == set(MODULE.ROLES)
    assert jobs["receipt"]["needs"] == "build"
    build = next(s for s in jobs["build"]["steps"] if s.get("id") == "image")
    assert build["with"]["push"] is True
    assert build["with"]["sbom"] is True
    assert build["with"]["provenance"] == "mode=max"
    assert build["with"]["platforms"] == "linux/amd64"
    assert ":latest" not in build["with"]["tags"]


def test_digest_env_contains_only_the_four_validated_image_pins():
    images = MODULE.validate(receipt(), REPO, SHA)
    values = dict(line.split("=", 1) for line in MODULE.compose_env(images).splitlines())
    assert values == {f"OVG_{role.upper()}_IMAGE": ref for role, ref in images.items()}


def test_rust_release_compose_is_digest_only_and_keeps_db_private():
    data = yaml.safe_load((ROOT / "infra/release/docker-compose.rust.yml").read_text())
    services = data["services"]
    assert set(services) == set(MODULE.ROLES)
    for role, service in services.items():
        assert "build" not in service
        assert service["pull_policy"] == "never"
        assert service["image"].startswith(f"${{OVG_{role.upper()}_IMAGE:?")
        assert "container_name" not in service
        assert all(not str(volume).startswith("/") for volume in service.get("volumes", []))
        if role != "web":
            assert "ports" not in service
    assert services["web"]["ports"][0].startswith("127.0.0.1:")
    assert "8000" in services["api"]["environment"]["OPENVIETGIS_BIND_ADDR"]
    assert "OPENVIETGIS_MIGRATION_DATABASE_URL" not in services["api"]["environment"]
    assert "OPENVIETGIS_MIGRATION_DATABASE_URL" in services["migrate"]["environment"]
    assert services["api"]["depends_on"]["migrate"]["condition"] == "service_completed_successfully"
    assert set(data["volumes"]) == {"pgdata", "artifacts"}
    assert all(value is None for value in data["volumes"].values())


def test_public_proxy_does_not_expose_private_metrics():
    nginx = (ROOT / "infra/prod/nginx.conf").read_text()
    assert "location = /api/v1/ops/metrics {\n        return 404;" in nginx
    assert "proxy_pass http://api:8000/api/;" in nginx
    docker = (ROOT / "infra/prod/Dockerfile.web").read_text()
    assert "HEALTHCHECK" in docker and "http://127.0.0.1/index.html" in docker


def test_local_image_inspect_timeout_fails_closed(monkeypatch):
    images = MODULE.validate(receipt(), REPO, SHA)

    def timed_out(args, **kwargs):
        assert kwargs["timeout"] == 30
        raise subprocess.TimeoutExpired(args, 30)

    monkeypatch.setattr(MODULE.subprocess, "run", timed_out)
    with pytest.raises(subprocess.TimeoutExpired):
        MODULE.verify_local(images, SHA)
