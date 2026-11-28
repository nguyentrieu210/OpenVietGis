"""Deployment observations must be pinned, bounded and fail closed."""
import importlib.util
import sys
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[2]
RELEASE = ROOT / "infra/release"
SHA = "a" * 40
REPO = "example/repo"


@pytest.fixture
def receipt(monkeypatch):
    monkeypatch.syspath_prepend(str(RELEASE))
    spec = importlib.util.spec_from_file_location("deployment_receipt", RELEASE / "deployment_receipt.py")
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    monkeypatch.setitem(sys.modules, spec.name, module)
    spec.loader.exec_module(module)
    return module


def manifest():
    return {
        "schema": 1, "source_sha": SHA, "repository": REPO, "platform": "linux/amd64",
        "images": {
            role: {"source_sha": SHA, "ref": f"ghcr.io/{REPO}-{role}@sha256:" + digit * 64}
            for role, digit in zip(("api", "web", "migrate", "postgres"), "1234", strict=True)
        },
    }


def test_observation_requires_live_verification(receipt, monkeypatch):
    calls = []
    monkeypatch.setattr(receipt, "verify_running", lambda *args: calls.append(args))
    value = receipt.record(manifest(), REPO, SHA, "ovg", 1000, 1060)
    assert len(calls) == 1
    assert value["duration_seconds"] == 60
    assert set(value["images"]) == {"api", "web"}
    assert value["release_approval"] is False
    assert "project" not in value


@pytest.mark.parametrize("start,end", [(0, 1), (2, 1), (1, 86402), (True, 10)])
def test_invalid_interval_never_inspects(receipt, monkeypatch, start, end):
    monkeypatch.setattr(receipt, "verify_running", lambda *args: pytest.fail("unexpected inspect"))
    with pytest.raises(ValueError):
        receipt.record(manifest(), REPO, SHA, "ovg", start, end)


def test_failed_health_cannot_generate_observation(receipt, monkeypatch):
    def unavailable(*args):
        raise ValueError("not healthy")

    monkeypatch.setattr(receipt, "verify_running", unavailable)
    with pytest.raises(ValueError, match="not healthy"):
        receipt.record(manifest(), REPO, SHA, "ovg", 1000, 1060)


def test_mismatched_candidate_never_inspects(receipt, monkeypatch):
    monkeypatch.setattr(receipt, "verify_running", lambda *args: pytest.fail("unexpected inspect"))
    with pytest.raises(ValueError):
        receipt.record(manifest(), REPO, "b" * 40, "ovg", 1000, 1060)
