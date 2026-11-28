"""Running-image verification must fail closed without mutating deployment."""
import copy
import importlib.util
import json
import subprocess
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("image_manifest", ROOT / "infra/release/image_manifest.py")
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)
SHA = "a" * 40
PROJECT = "ovg-staging"


def fixture():
    images = {role: f"ghcr.io/example/repo-{role}@sha256:" + str(i) * 64
              for i, role in enumerate(MODULE.ROLES, 1)}
    containers = [{
        "Image": "sha256:" + str(i) * 64,
        "State": {"Running": True, "Status": "running", "Health": {"Status": "healthy"}},
        "Config": {"Image": images[role], "Env": ["TOKEN=must-never-print"],
                   "Labels": {"com.docker.compose.project": PROJECT,
                              "com.docker.compose.service": role}},
    } for i, role in enumerate(("api", "web"), 1)]
    local = {images[role]: [{
        "Id": c["Image"], "RepoDigests": [images[role]], "Os": "linux", "Architecture": "amd64",
        "Config": {"Labels": {"org.opencontainers.image.revision": SHA}},
    }] for role, c in zip(("api", "web"), containers, strict=True)}
    return images, containers, local


def install(monkeypatch, containers, local):
    calls = []
    def run(args, **kwargs):
        calls.append(args)
        assert kwargs["timeout"] == 30
        if args[1:3] == ["ps", "--all"]:
            data = "1" * 12 + "\n" + "2" * 12
        elif args[1] == "inspect":
            data = json.dumps(containers)
        else:
            assert args[1:3] == ["image", "inspect"]
            data = json.dumps(local[args[3]])
        return subprocess.CompletedProcess(args, 0, data, "")
    monkeypatch.setattr(MODULE.subprocess, "run", run)
    return calls


def test_verify_running_reads_only_app_images_and_never_echoes_env(monkeypatch, capsys):
    images, containers, local = fixture()
    calls = install(monkeypatch, containers, local)
    MODULE.verify_running(images, SHA, PROJECT)
    assert "must-never-print" not in capsys.readouterr().out
    assert not any(any(action in call for action in ("up", "pull", "build", "rm", "exec"))
                   for call in calls)
    assert not any(images[role] in " ".join(call) for role in ("postgres", "migrate") for call in calls)


@pytest.mark.parametrize("bad", ["wrong_digest", "mutable_tag", "unhealthy", "no_health",
                                "wrong_source", "missing", "duplicate", "foreign_project"])
def test_running_receipt_rejects_mismatch_or_missing_health(monkeypatch, bad):
    images, containers, local = fixture()
    if bad == "wrong_digest": containers[0]["Image"] = "sha256:" + "f" * 64
    if bad == "mutable_tag": containers[0]["Config"]["Image"] = "api:latest"
    if bad == "unhealthy": containers[0]["State"]["Health"]["Status"] = "unhealthy"
    if bad == "no_health": containers[0]["State"].pop("Health")
    if bad == "wrong_source": local[images["api"]][0]["Config"]["Labels"]["org.opencontainers.image.revision"] = "b" * 40
    if bad == "missing": containers.pop()
    if bad == "duplicate": containers.append(copy.deepcopy(containers[0]))
    if bad == "foreign_project": containers[0]["Config"]["Labels"]["com.docker.compose.project"] = "other"
    install(monkeypatch, containers, local)
    with pytest.raises(ValueError):
        MODULE.verify_running(images, SHA, PROJECT)


def test_running_verification_requires_explicit_project_before_docker(monkeypatch):
    monkeypatch.setattr(MODULE.subprocess, "run", lambda *a, **k: pytest.fail("docker"))
    images, _, _ = fixture()
    for project in (None, "", "../live", "live;echo secret"):
        with pytest.raises(ValueError):
            MODULE.verify_running(images, SHA, project)
