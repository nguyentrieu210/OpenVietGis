"""Read-only repository topology and local candidate artifact integrity checks.

Neither an artifact manifest nor a successful check constitutes release approval.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
from pathlib import Path
from typing import Any

SHA = re.compile(r"[0-9a-f]{40}")
LOCKS = ("pnpm-lock.yaml", "apps/api/uv.lock", "apps/mcp/uv.lock")
PLUGIN = "downloads/openvietgis_connector-1.0.0.zip"


def git(repo: Path, *args: str) -> str:
    return subprocess.check_output(["git", "-C", str(repo), *args], text=True).strip()


def hash_file(path: Path) -> str:
    with path.open("rb") as handle:
        return hashlib.file_digest(handle, "sha256").hexdigest()


def artifact_hashes(root: Path) -> dict[str, str]:
    if not root.is_dir() or root.is_symlink():
        raise ValueError("Expected a real artifact directory")
    files = {}
    for path in sorted(root.rglob("*")):
        if path.is_symlink():
            raise ValueError(f"Unexpected artifact symlink: {path}")
        if path.is_file():
            files[path.relative_to(root).as_posix()] = hash_file(path)
    if not {"index.html", PLUGIN, PLUGIN + ".sha256"} <= files.keys():
        raise ValueError("Missing frontend entrypoint or connector artifact")
    checksum = (root / (PLUGIN + ".sha256")).read_text().split()
    if checksum != [files[PLUGIN], Path(PLUGIN).name]:
        raise ValueError("Connector checksum mismatch")
    return files


def snapshot(repo: Path, candidate: str) -> dict[str, Any]:
    candidate = git(repo, "rev-parse", "--verify", candidate + "^{commit}")
    refs = []
    for line in git(
        repo,
        "for-each-ref",
        "--format=%(refname) %(objectname)",
        "refs/heads",
        "refs/remotes",
    ).splitlines():
        ref, sha = line.split()
        if ref.endswith("/HEAD"):
            continue
        behind, ahead = map(
            int,
            git(
                repo, "rev-list", "--left-right", "--count", candidate + "..." + sha
            ).split(),
        )
        refs.append(
            {
                "ref": ref,
                "sha": sha,
                "behind_candidate": behind,
                "ahead_candidate": ahead,
                "disposition": "RETAIN_REVIEW" if ahead else "ANCESTRY_INCLUDED",
            }
        )
    worktrees = []
    for block in git(repo, "worktree", "list", "--porcelain").split("\n\n"):
        lines = block.splitlines()
        row = {
            "path": lines[0].removeprefix("worktree "),
            "head": next((x[5:] for x in lines if x.startswith("HEAD ")), None),
            "branch": next((x[7:] for x in lines if x.startswith("branch ")), None),
        }
        path = Path(row["path"])
        row["changed_paths"] = (
            len(git(path, "status", "--porcelain").splitlines())
            if path.is_dir()
            else None
        )
        worktrees.append(row)
    return {
        "candidate": candidate,
        "refs": refs,
        "worktrees": worktrees,
        "cleanup_authorized": False,
    }


def candidate_manifest(repo: Path, dist: Path, migration_head: str) -> dict[str, Any]:
    if git(repo, "status", "--porcelain"):
        raise ValueError("Candidate tracked/untracked tree must be clean")
    source = git(repo, "rev-parse", "HEAD")
    if not SHA.fullmatch(source) or not migration_head:
        raise ValueError("Invalid candidate or missing migration head")
    return {
        "source_sha": source,
        "frontend": "apps/web-next",
        "backend": "apps/api",
        "mcp": "apps/mcp",
        "migration_graph_head": migration_head,
        "lockfiles": {name: hash_file(repo / name) for name in LOCKS},
        "web_files": artifact_hashes(dist),
        "status": "LOCAL_ARTIFACT_INVENTORY_ONLY",
        "production_approved": False,
    }


def verify_manifest(repo: Path, dist: Path, manifest: dict[str, Any]) -> list[str]:
    errors = []
    if (
        manifest.get("status") != "LOCAL_ARTIFACT_INVENTORY_ONLY"
        or manifest.get("production_approved") is not False
    ):
        errors.append("Invalid non-production status")
    if manifest.get("source_sha") != git(repo, "rev-parse", "HEAD"):
        errors.append("Source SHA mismatch")
    if git(repo, "status", "--porcelain"):
        errors.append("Candidate tree is dirty")
    if manifest.get("lockfiles") != {name: hash_file(repo / name) for name in LOCKS}:
        errors.append("Dependency lock mismatch")
    try:
        if manifest.get("web_files") != artifact_hashes(dist):
            errors.append("Artifact set or hash mismatch")
    except (ValueError, OSError) as exc:
        errors.append(str(exc))
    return errors


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, required=True)
    sub = parser.add_subparsers(dest="action", required=True)
    refs = sub.add_parser("refs")
    refs.add_argument("--candidate", required=True)
    for action in ("manifest", "verify"):
        cmd = sub.add_parser(action)
        cmd.add_argument("--dist", required=True, type=Path)
        cmd.add_argument("--record", required=True, type=Path)
        if action == "manifest":
            cmd.add_argument("--migration-head", required=True)
    args = parser.parse_args()
    if args.action == "refs":
        print(json.dumps(snapshot(args.repo, args.candidate), indent=2))
    elif args.action == "manifest":
        result = candidate_manifest(args.repo, args.dist, args.migration_head)
        # Refuse to replace existing evidence.
        with args.record.open("x") as handle:
            json.dump(result, handle, indent=2)
            handle.write("\n")
    else:
        errors = verify_manifest(
            args.repo, args.dist, json.loads(args.record.read_text())
        )
        print(json.dumps({"integrity_errors": errors, "production_approved": False}))
        return int(bool(errors))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
