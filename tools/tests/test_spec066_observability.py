"""Exact-source build evidence and aggregate telemetry privacy contracts."""
import copy
import importlib.util
from pathlib import Path

import pytest
import yaml

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("qualification", ROOT / "infra/release/qualification.py")
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


def record(role="api", **changes):
    return MODULE.build_record(**{
        "role": role, "source_sha": "a" * 40, "digest": "sha256:" + "b" * 64,
        "started": 1000, "completed": 1060, "run_id": "123456789", "attempt": 1, **changes,
    })


def test_qualification_has_only_identification_and_timing_fields():
    value = record()
    assert value["duration_seconds"] == 60
    assert value["release_approval"] is False
    assert set(value) == {
        "schema", "phase", "role", "source_sha", "image_digest", "started_unix",
        "completed_unix", "duration_seconds", "run_id", "attempt", "method", "release_approval",
    }


@pytest.mark.parametrize("changes", [
    {"source_sha": "main"}, {"digest": "api:latest"}, {"completed": 999},
    {"started": True}, {"completed": 100000}, {"run_id": "https://token@example.test"},
    {"attempt": 0}, {"attempt": True}, {"role": "identity"},
])
def test_qualification_rejects_unpinned_or_invalid_identity(changes):
    with pytest.raises(ValueError):
        record(**changes)


def test_combined_build_evidence_cannot_mix_sources_runs_or_roles():
    roles = ("api", "web", "migrate", "postgres")
    manifest = {"source_sha": "a" * 40, "images": {
        role: {"ref": "ghcr.io/example/repo-" + role + "@sha256:" + "b" * 64}
        for role in roles
    }}
    records = {role: record(role) for role in roles}
    assert MODULE.combine(manifest, records, "123456789", 1)["release_approval"] is False
    for field, wrong in [("source_sha", "c" * 40), ("run_id", "9"), ("role", "web"),
                         ("image_digest", "sha256:" + "c" * 64), ("attempt", 2),
                         ("release_approval", True)]:
        broken = copy.deepcopy(records)
        broken["api"][field] = wrong
        with pytest.raises(ValueError):
            MODULE.combine(manifest, broken, "123456789", 1)


def test_observability_rules_and_build_workflow_preserve_aggregate_scope():
    data = yaml.safe_load((ROOT / "infra/observability/spec066.rules.yml").read_text())
    rules = data["groups"][0]["rules"]
    names = {r.get("record") for r in rules}
    assert {"ovg:http_latency:p95_5m", "ovg:http_latency:p99_5m",
            "ovg:tile_cache:hit_ratio_5m", "ovg:api:resident_memory_bytes"} <= names
    text = (ROOT / "infra/observability/spec066.rules.yml").read_text()
    for sensitive in ("actor", "organization", "project_id", "token", "feature_id", "route="):
        assert sensitive not in text
    workflow = yaml.safe_load((ROOT / ".github/workflows/release-images.yml").read_text())
    steps = workflow["jobs"]["build"]["steps"]
    builds = [s for s in steps if s.get("uses") == "docker/build-push-action@v6"]
    assert len(builds) == 1
    assert builds[0]["with"]["sbom"] is True
    assert builds[0]["with"]["provenance"] == "mode=max"
    assert next(s for s in steps if s.get("id") == "clock")["run"]
    assert any("infra/release/qualification.py" in s.get("run", "") for s in steps)
