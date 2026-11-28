"""Aggregate build timings. Identification evidence, never release approval."""
from __future__ import annotations

import argparse
import json
import re
import time
from pathlib import Path


def build_record(*, role: str, source_sha: str, digest: str, started: int,
                 completed: int, run_id: str, attempt: int) -> dict:
    if role not in {"api", "web", "migrate", "postgres"}:
        raise ValueError("Unknown image role")
    if not re.fullmatch(r"[0-9a-f]{40}", source_sha):
        raise ValueError("Invalid source SHA")
    if not re.fullmatch(r"sha256:[0-9a-f]{64}", digest):
        raise ValueError("Invalid image digest")
    if (type(started) is not int or type(completed) is not int
            or started <= 0 or not 0 <= completed - started <= 86400):
        raise ValueError("Invalid build interval")
    if not re.fullmatch(r"[0-9]{1,24}", run_id) or type(attempt) is not int or attempt < 1:
        raise ValueError("Invalid run identity")
    return {
        "schema": 1, "phase": "build_publish", "role": role, "source_sha": source_sha,
        "image_digest": digest, "started_unix": started, "completed_unix": completed,
        "duration_seconds": completed - started, "run_id": run_id, "attempt": attempt,
        "method": "Wall time around the single BuildKit build-and-publish action; includes attestations.",
        "release_approval": False,
    }



def combine(manifest: dict, records: dict, run_id: str, attempt: int) -> dict:
    roles = {"api", "web", "migrate", "postgres"}
    if set(records) != roles:
        raise ValueError("Expected all role qualifications")
    for role, row in records.items():
        expected = build_record(role=role, source_sha=manifest["source_sha"],
                                digest=manifest["images"][role]["ref"].split("@", 1)[1],
                                started=row["started_unix"], completed=row["completed_unix"],
                                run_id=run_id, attempt=attempt)
        if row != expected:
            raise ValueError("Qualification does not match this source, role and run")
    return {"schema": 1, "source_sha": manifest["source_sha"], "roles": records,
            "release_approval": False}

def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("role", "source-sha", "digest", "run-id"):
        parser.add_argument("--" + name, required=True)
    parser.add_argument("--started", type=int, required=True)
    parser.add_argument("--attempt", type=int, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    record = build_record(role=args.role, source_sha=args.source_sha, digest=args.digest,
                          started=args.started, completed=int(time.time()),
                          run_id=args.run_id, attempt=args.attempt)
    args.output.write_text(json.dumps(record, indent=2) + "\n")


if __name__ == "__main__":
    main()
