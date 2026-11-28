"""Fail closed on missing QGIS parity acceptance records (spec 065)."""
import argparse
import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SPEC = ROOT / "specs/065-qgis-ux-parity"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--inventory-only", action="store_true",
                        help="Check coverage without claiming acceptance")
    args = parser.parse_args()
    matrix = re.findall(r"^\| ([A-Z]+-\d+) \|", (SPEC / "feature-matrix.md").read_text(), re.MULTILINE)
    tasks = {row: task for task, row in re.findall(
        r"\b(T\d+) \[US\d+\] \[([A-Z]+-\d+)\]", (SPEC / "tasks.md").read_text())}
    rows = json.loads((SPEC / "evidence/manifest.json").read_text())["rows"]
    ids = [row["id"] for row in rows]
    if len(ids) != len(set(ids)) or set(ids) != set(matrix) or set(ids) != set(tasks):
        raise SystemExit("FAIL: matrix/task/evidence coverage mismatch")
    required = ("candidate_commit", "qgis_version", "fixture_sha256",
                "steps", "expected", "actual", "test_ids", "paired_screenshots",
                "metrics", "reviewer")
    blocked = []
    for row in rows:
        if row["task"] != tasks[row["id"]]:
            raise SystemExit(f"FAIL: task mapping mismatch {row['id']}")
        if row["status"] not in ("verified", "partial", "unverified"):
            raise SystemExit(f"FAIL: invalid status {row['id']}")
        if row["status"] == "verified":
            if row.get("blocker") or not all(row.get(k) for k in required):
                raise SystemExit(f"FAIL: incomplete acceptance {row['id']}")
        else:
            if not row.get("blocker"):
                raise SystemExit(f"FAIL: missing blocker {row['id']}")
            blocked.append(row["id"])
    print(f"Inventory: {len(rows)} rows; {len(blocked)} not accepted")
    if blocked and not args.inventory_only:
        raise SystemExit("BLOCKED: " + ", ".join(blocked))


if __name__ == "__main__":
    main()
