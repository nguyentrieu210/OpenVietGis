"""Run pinned, reproducible local quality checks; never certify production."""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import time
from pathlib import Path
from typing import Any

from tools.release_consolidation import LOCKS, artifact_hashes, git, hash_file

ROOT = Path(__file__).resolve().parents[1]
REQUIRED = (
    "ruff",
    "mypy",
    "python_tests",
    "architecture",
    "scope",
    "docs",
    "contracts",
    "sdk",
    "spatial",
    "mcp",
    "web_build",
    "browser",
    "artifact",
)


def commands(mcp_python: str) -> dict[str, tuple[list[str], Path]]:
    py = sys.executable
    root = ROOT
    return {
        "ruff": (
            [py, "-m", "ruff", "check", "apps/api", "tools", "tests", "modules"],
            root,
        ),
        "mypy": ([py, "-m", "mypy", "apps/api/openvietgis_api"], root),
        "python_tests": (
            [
                py,
                "-m",
                "pytest",
                "apps/api/tests/unit",
                "apps/api/tests/contract",
                "tools/tests",
                "tests/architecture",
                "-q",
                "-o",
                "addopts=",
            ],
            root,
        ),
        "architecture": ([py, "tools/check_architecture.py"], root),
        "scope": ([py, "tools/check_scope.py"], root),
        "docs": ([py, "tools/check_docs.py"], root),
        "contracts": ([py, "tools/check_contracts.py"], root),
        "sdk": (["pnpm", "--filter", "@openvietgis/client-sdk", "test"], root),
        "spatial": (
            ["pnpm", "--filter", "@openvietgis/spatial-workspace", "test"],
            root,
        ),
        "mcp": (
            [mcp_python, "-m", "pytest", "-q", "-o", "addopts="],
            root / "apps/mcp",
        ),
        "web_build": (["pnpm", "--filter", "@openvietgis/web-next", "build"], root),
        "browser": (["pnpm", "validate:canonical-smoke"], root),
    }


def local_failures(report: dict[str, Any], evidence: Path, candidate: str) -> list[str]:
    failures = []
    if report.get("source_sha") != candidate or report.get("clean_after") is not True:
        failures.append("Source pin or clean-tree check failed.")
    if report.get("production_approved") is not False:
        failures.append("Local evidence cannot approve production.")
    steps = report.get("steps")
    if not isinstance(steps, dict) or set(steps) != set(REQUIRED):
        return [*failures, "Required local check records missing or unexpected."]
    for name in REQUIRED:
        row = steps[name]
        log = evidence / (name + ".log")
        if (
            not isinstance(row, dict)
            or row.get("exit_code") != 0
            or not log.is_file()
            or log.is_symlink()
            or row.get("sha256") != hash_file(log)
        ):
            failures.append(f"{name}: failed or evidence missing/changed.")
    return failures


def run(output: Path, mcp_python: str) -> dict[str, Any]:
    if git(ROOT, "status", "--porcelain"):
        raise RuntimeError("Candidate checkout must be clean.")
    source = git(ROOT, "rev-parse", "HEAD")
    output = output.resolve()
    if output.is_relative_to(ROOT):
        raise RuntimeError("Keep evidence outside candidate checkout.")
    output.mkdir(parents=True, exist_ok=False)
    lockfiles = {p: hash_file(ROOT / p) for p in LOCKS}
    result: dict[str, Any] = {
        "source_sha": source,
        "lockfiles": lockfiles,
        "steps": {},
        "profile": "local_candidate_not_independent",
        "production_approved": False,
    }
    env = {**os.environ, "PYTHONPATH": str(ROOT / "apps/api") + os.pathsep + str(ROOT)}
    for name, (argv, cwd) in commands(mcp_python).items():
        path = output / (name + ".log")
        start = time.monotonic()
        with path.open("wb") as log:
            try:
                code = subprocess.run(
                    argv,
                    check=False,
                    cwd=cwd,
                    env=env,
                    stdout=log,
                    stderr=subprocess.STDOUT,
                    timeout=240,
                ).returncode
            except subprocess.TimeoutExpired:
                code = 124
            except OSError:
                code = 127
        result["steps"][name] = {
            "exit_code": code,
            "seconds": round(time.monotonic() - start, 3),
            "sha256": hash_file(path),
        }
        print(name, code, flush=True)
    path = output / "artifact.log"
    try:
        files = artifact_hashes(ROOT / "apps/web-next/dist")
        for suffix in ("", ".sha256"):
            relative = "downloads/openvietgis_connector-1.0.0.zip" + suffix
            if files[relative] != hash_file(ROOT / "apps/web-next/public" / relative):
                raise ValueError("Built connector differs from tracked source artifact")
        path.write_text(json.dumps(files, indent=2) + "\n")
        code = 0
    except (ValueError, OSError) as exc:
        path.write_text(str(exc) + "\n")
        code = 1
    result["steps"]["artifact"] = {"exit_code": code, "sha256": hash_file(path)}
    result["clean_after"] = (
        git(ROOT, "rev-parse", "HEAD") == source
        and not git(ROOT, "status", "--porcelain")
        and lockfiles == {p: hash_file(ROOT / p) for p in LOCKS}
    )
    result["failures"] = local_failures(result, output, source)
    (output / "report.json").write_text(json.dumps(result, indent=2) + "\n")
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--mcp-python", required=True)
    args = parser.parse_args()
    report = run(args.output, args.mcp_python)
    print(
        json.dumps(
            {
                "source_sha": report["source_sha"],
                "failures": report["failures"],
                "production_approved": False,
            }
        )
    )
    return int(bool(report["failures"]))


if __name__ == "__main__":
    raise SystemExit(main())
