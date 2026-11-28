"""Record an operator-timed rollout only after read-only image/health verification.

No deployment, migration, network request, or shell command is initiated here
other than the existing local Docker inspection in verify_running.
"""
from __future__ import annotations

import argparse
import json
import re
import time
from pathlib import Path

from image_manifest import validate, verify_running


def record(manifest: dict, repository: str, candidate: str, project: str,
           started: int, completed: int) -> dict:
    images = validate(manifest, repository, candidate)
    if not re.fullmatch(r"[a-z0-9][a-z0-9_-]{0,62}", project):
        raise ValueError("Invalid trusted Compose project")
    if (type(started) is not int or type(completed) is not int
            or started <= 0 or not 0 <= completed - started <= 86400):
        raise ValueError("Invalid rollout observation interval")
    verify_running(images, candidate, project)
    return {
        "schema": 1,
        "phase": "deployment_observation",
        "source_sha": candidate,
        "images": {role: images[role] for role in ("api", "web")},
        "started_unix": started,
        "completed_unix": completed,
        "duration_seconds": completed - started,
        "method": "Operator-supplied rollout interval; local running digest and health verified.",
        "release_approval": False,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("manifest", type=Path)
    parser.add_argument("--repository", required=True)
    parser.add_argument("--candidate", required=True)
    parser.add_argument("--project", required=True)
    parser.add_argument("--started", type=int, required=True)
    parser.add_argument("--completed", type=int, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.completed > int(time.time()):
        parser.error("Rollout completion cannot be in the future")
    try:
        value = record(json.loads(args.manifest.read_text()), args.repository,
                       args.candidate, args.project, args.started, args.completed)
        # Do not replace a prior observation or the input manifest.
        with args.output.open("x", encoding="utf-8") as out:
            out.write(json.dumps(value, indent=2) + "\n")
    except (ValueError, OSError, TypeError) as error:
        parser.exit(1, f"Deployment observation failed: {error}\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
