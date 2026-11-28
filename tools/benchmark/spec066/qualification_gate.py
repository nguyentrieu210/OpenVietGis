"""Fail-closed checks for browser lab samples; not release certification."""
from __future__ import annotations

import argparse
import json
import math
import re
from itertools import pairwise
from pathlib import Path


def evaluate(report: dict, candidate: str, *, require_soak: bool = False) -> list[str]:
    failures = []
    if not isinstance(candidate, str) or not re.fullmatch(r"[0-9a-f]{40}", candidate):
        failures.append("Expected an exact 40-character candidate SHA")
    if not isinstance(report, dict):
        return [*failures, "Malformed browser report"]
    if report.get("sha") != candidate:
        failures.append("Browser samples are not pinned to the expected candidate")
    if report.get("failures") != []:
        failures.append("Browser workload contains failures or lacks a failure inventory")
    raw = report.get("raw")
    if not isinstance(raw, dict):
        return [*failures, "Raw samples are missing"]
    limits = {"warm_shell_ms": 1000, "gis_to_cad_ms": 300,
              "cad_to_gis_ms": 300, "gis_to_3d_ms": 300}
    for metric in ("cold_shell_ms", "cold_gis_ms", *limits):
        values = raw.get(metric)
        if (not isinstance(values, list) or len(values) < 30
                or any(type(x) not in (int, float) or not math.isfinite(x) or x < 0 for x in values)):
            failures.append(f"{metric}: at least 30 finite independent/declared samples required")
            continue
        p95 = sorted(values)[math.ceil(len(values) * .95) - 1]
        if metric in limits and p95 > limits[metric]:
            failures.append(f"{metric}: p95 {p95:.3f} ms exceeds {limits[metric]} ms")
    if require_soak:
        soak = report.get("soak")
        if not isinstance(soak, dict):
            return [*failures, "A continuous 60-minute browser soak is missing"]
        requested, elapsed = soak.get("requested_seconds"), soak.get("elapsed_seconds")
        samples = soak.get("samples")
        if (type(requested) not in (int, float) or not math.isfinite(requested) or requested < 3600
                or type(elapsed) not in (int, float) or not math.isfinite(elapsed) or elapsed < 3600
                or not isinstance(samples, list) or len(samples) < 60):
            failures.append("A continuous 60-minute browser soak is missing")
        else:
            times = [s.get("elapsed_seconds") if isinstance(s, dict) else None for s in samples]
            if (any(type(t) not in (int, float) or not math.isfinite(t) or t < 0 for t in times)
                    or times[0] > 30 or times[-1] < 3540 or times[-1] > elapsed
                    or any(not 0 < b - a <= 90 for a, b in pairwise(times))):
                failures.append("Soak sampling continuity is not proven")
    return failures


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("report", type=Path)
    parser.add_argument("--candidate", required=True)
    parser.add_argument("--require-soak", action="store_true")
    args = parser.parse_args()
    failures = evaluate(json.loads(args.report.read_text()), args.candidate,
                        require_soak=args.require_soak)
    print(json.dumps({"scope": "browser lab timing only; no process/GPU/RUM or release approval",
                      "candidate": args.candidate, "passed": not failures, "failures": failures}, indent=2))
    return int(bool(failures))


if __name__ == "__main__":
    raise SystemExit(main())
