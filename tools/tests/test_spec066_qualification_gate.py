"""Sample counts, raw timings and candidate identity are hard lab gates."""
import copy
import importlib.util
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location(
    "qualification_gate", ROOT / "tools/benchmark/spec066/qualification_gate.py",
)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)
SHA = "a" * 40


def report():
    return {"sha": SHA, "failures": [], "raw": {
        key: [100] * 30 for key in (
            "cold_shell_ms", "cold_gis_ms", "warm_shell_ms", "gis_to_cad_ms", "cad_to_gis_ms",
            "gis_to_3d_ms",
        )
    }}


def test_lab_gate_requires_actual_raw_samples_and_expected_source():
    assert MODULE.evaluate(report(), SHA) == []
    bad = report()
    bad["sha"] = "b" * 40
    assert MODULE.evaluate(bad, SHA)
    bad = report()
    bad["raw"]["cold_gis_ms"] = [100] * 5
    assert any("cold_gis_ms" in f for f in MODULE.evaluate(bad, SHA))


@pytest.mark.parametrize("value", [None, True, -1, float("nan"), float("inf"), "100"])
def test_invalid_samples_cannot_be_a_pass(value):
    bad = report()
    bad["raw"]["warm_shell_ms"][0] = value
    assert MODULE.evaluate(bad, SHA)


def test_reported_summary_cannot_hide_slow_raw_samples():
    bad = report()
    bad["metrics"] = {"gis_to_cad_ms": {"p95_ms": 1, "count": 1000}}
    bad["raw"]["gis_to_cad_ms"][-3:] = [1500] * 3
    assert any("exceeds" in f for f in MODULE.evaluate(bad, SHA))


def test_soak_needs_a_full_continuous_interval():
    bad = report()
    assert MODULE.evaluate(bad, SHA, require_soak=True)
    bad["soak"] = {"requested_seconds": 3600, "elapsed_seconds": 3600,
                   "samples": [{"elapsed_seconds": n * 60} for n in range(60)]}
    assert MODULE.evaluate(bad, SHA, require_soak=True) == []
    broken = copy.deepcopy(bad)
    broken["soak"]["samples"][-1]["elapsed_seconds"] = 100
    assert MODULE.evaluate(broken, SHA, require_soak=True)


def test_soak_duplicate_timestamps_and_nonfinite_duration_do_not_prove_coverage():
    bad = report()
    bad["soak"] = {"requested_seconds": 3600, "elapsed_seconds": 3600,
                   "samples": [{"elapsed_seconds": 3540}] * 60}
    assert MODULE.evaluate(bad, SHA, require_soak=True)
    bad["soak"]["requested_seconds"] = float("nan")
    assert MODULE.evaluate(bad, SHA, require_soak=True)
    assert MODULE.evaluate(report(), "main")
