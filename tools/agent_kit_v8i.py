"""Safe, dependency-light Spec Kit worktree dispatcher for OpenVietGIS.

Prepares isolated worktrees and task contracts for five R025 agents; optional local
execution uses an already-authenticated CLI and never touches production.
No implicit deploy, push, merge, credential transfer, or destructive cleanup.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
from datetime import UTC, datetime
from pathlib import Path

TRACKS: dict[str, tuple[str, str]] = {
    "windows": ("01-windows-com.md", "V8i COM adapter and Windows capability matrix"),
    "backend": ("02-backend-security.md", "Device pairing, preview-only API, security and RLS"),
    "frontend": ("03-frontend.md", "Third-party connector UI and status"),
    "qa": ("04-qa.md", "Snapshot, CRS and cross-tenant QA"),
    "release": ("05-release.md", "Provenance, rollback, release acceptance"),
}
DEFAULT_BASE = "feature/025-microstation-v8i-bridge"
DEFAULT_ROOT = "/opt/openvietgis-v8i-agent-worktrees"
MANIFEST_NAME = "agentkit-manifest.json"


class AgentKitError(RuntimeError):
    """Fail closed without altering existing user work."""


def git(repo: Path, *args: str) -> str:
    process = subprocess.run(
        ["git", "-C", str(repo), *args], capture_output=True, text=True, check=False
    )
    if process.returncode:
        raise AgentKitError(f"git {' '.join(args)}: {process.stderr.strip()}")
    return process.stdout.strip()


def prepare(
    repo: Path, root: Path, base: str = DEFAULT_BASE, suffix: str = "20261123"
) -> dict:
    repo = repo.resolve()
    root = root.resolve()
    if root == repo or root.is_relative_to(repo) or repo.is_relative_to(root):
        raise AgentKitError("Worktree root and source repo must be separate.")
    if not (repo / "docs/agents").is_dir():
        raise AgentKitError("Missing docs/agents task contracts in selected repo.")
    if git(repo, "status", "--porcelain"):
        raise AgentKitError("Refusing to prepare from a dirty source worktree.")
    base_sha = git(repo, "rev-parse", "--verify", f"{base}^{{commit}}")
    root.mkdir(parents=True, exist_ok=True)
    manifest_file = root / MANIFEST_NAME
    if manifest_file.exists():
        manifest = json.loads(manifest_file.read_text(encoding="utf-8"))
        if manifest.get("base_sha") != base_sha or manifest.get("base") != base:
            raise AgentKitError(
                "Existing manifest targets a different base; no rewrite."
            )
    else:
        manifest = {
            "schema": 1,
            "base": base,
            "base_sha": base_sha,
            "source_repo": str(repo),
            "created_utc": datetime.now(UTC).isoformat(),
            "tracks": {},
        }

    for name, (prompt, title) in TRACKS.items():
        branch = f"agentkit/{name}-{suffix}"
        path = root / name
        if name in manifest["tracks"]:
            expected = manifest["tracks"][name]
            if expected["path"] != str(path) or expected["branch"] != branch:
                raise AgentKitError(f"Existing manifest conflict for {name}")
        if path.exists():
            if not (path / ".git").exists():
                raise AgentKitError(
                    f"Refusing to use existing non-worktree path: {path}"
                )
            active = git(path, "branch", "--show-current")
            if active != branch:
                raise AgentKitError(
                    f"Wrong branch in {path}: {active}, expected {branch}"
                )
        else:
            if git(repo, "branch", "--list", branch):
                raise AgentKitError(
                    f"Branch {branch} already exists; refusing to guess ownership."
                )
            git(repo, "worktree", "add", "-b", branch, str(path), base)
        task_file = path / "docs/agents" / prompt
        if not task_file.is_file():
            raise AgentKitError(f"Missing task contract: {task_file}")
        manifest["tracks"][name] = {
            "title": title,
            "branch": branch,
            "path": str(path),
            "task": f"docs/agents/{prompt}",
            "base_sha": base_sha,
        }
        # Save completed entries incrementally; safe to re-run after interrupted prep.
        manifest_file.write_text(
            json.dumps(manifest, ensure_ascii=False, indent=2) + "\n",
            encoding="utf-8",
        )
    return manifest


def describe(root: Path) -> dict:
    manifest_file = root.resolve() / MANIFEST_NAME
    if not manifest_file.is_file():
        raise AgentKitError(f"No agentkit manifest at {manifest_file}")
    manifest = json.loads(manifest_file.read_text(encoding="utf-8"))
    for track in manifest["tracks"].values():
        path = Path(track["path"])
        if not path.is_dir():
            track["status"] = "MISSING"
            continue
        try:
            track["head"] = git(path, "rev-parse", "--short=12", "HEAD")
            track["dirty_paths"] = len(git(path, "status", "--porcelain").splitlines())
            track["commits_ahead"] = int(
                git(path, "rev-list", "--count", f"{manifest['base_sha']}..HEAD")
            )
            track["status"] = (
                "HAS_CHANGES"
                if track["dirty_paths"] or track["commits_ahead"]
                else "READY"
            )
        except (AgentKitError, ValueError):
            track["status"] = "ERROR"
    return manifest


def run_agent(root: Path, track: str, engine: str, execute: bool, timeout: int) -> int:
    manifest = describe(root)
    if track not in manifest["tracks"]:
        raise AgentKitError(f"Unknown track {track}")
    item = manifest["tracks"][track]
    path = Path(item["path"])
    task = (path / item["task"]).read_text(encoding="utf-8")
    context = (
        "\n\nCRITICAL execution rules: operate only in this assigned worktree. "
        "Do not push, deploy, merge, reset, clean, delete branches, alter real "
        "databases, install dependencies or run heavy builds. "
        "No secrets; document findings with paths and verification status. "
        "Follow the task contract above and existing CLAUDE.md / Constitution. "
        "For code changes keep diffs small, focused, tested where lightweight.\n"
    )
    command: list[str]
    if engine == "codex":
        command = [
            "codex",
            "--no-daemon",
            "exec",
            "-C",
            str(path),
            "--sandbox",
            "workspace-write",
            "--ask-for-approval",
            "never",
            "--ephemeral",
            "-",
        ]
    else:
        command = [
            "claude",
            "-p",
            "--permission-mode",
            "acceptEdits",
            "--max-turns",
            "20",
            "--output-format",
            "text",
        ]
    if not execute:
        print(
            json.dumps(
                {
                    "track": track,
                    "engine": engine,
                    "cwd": str(path),
                    "command": command,
                    "execution": "DRY_RUN",
                    "task": item["task"],
                },
                ensure_ascii=False,
                indent=2,
            )
        )
        return 0
    if not shutil.which(command[0]):
        raise AgentKitError(
            f"{command[0]} not found; install and authenticate CLI locally. "
            "Do not copy credentials from another host."
        )
    lock = root.resolve() / ".agentkit-single-run.lock"
    try:
        fd = os.open(lock, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
    except FileExistsError as error:
        raise AgentKitError(
            "Another agent run is recorded; review lock owner."
        ) from error
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as stream:
            stream.write(f"track={track}, engine={engine}, pid={os.getpid()}\n")
        logs = root.resolve() / "logs"
        logs.mkdir(exist_ok=True)
        output = logs / f"{track}-{engine}.log"
        # The authenticated CLI is local to the executor. No tokens in arguments or logs.
        with output.open("w", encoding="utf-8") as stream:
            try:
                process = subprocess.run(
                    command,
                    cwd=path,
                    input=task + context,
                    text=True,
                    stdout=stream,
                    stderr=subprocess.STDOUT,
                    timeout=timeout,
                    check=False,
                )
            except subprocess.TimeoutExpired as error:
                raise AgentKitError(
                    f"Agent timed out after {timeout}s; inspect {output}"
                ) from error
        print(
            f"track={track}, engine={engine}, exit={process.returncode}, log={output}"
        )
        return process.returncode
    finally:
        # The lock is created exclusively by this process and removed only here.
        lock.unlink(missing_ok=True)


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__)
    sub = p.add_subparsers(dest="cmd", required=True)
    init = sub.add_parser(
        "prepare", help="Create five isolated R025 worktrees; do not launch models"
    )
    init.add_argument(
        "--repo", type=Path, default=Path("/opt/openvietgis-v8i-bridge")
    )
    init.add_argument("--root", type=Path, default=Path(DEFAULT_ROOT))
    init.add_argument("--base", default=DEFAULT_BASE)
    init.add_argument("--suffix", default="20261123")
    status = sub.add_parser("status", help="Read worktree state; no writes")
    status.add_argument("--root", type=Path, default=Path(DEFAULT_ROOT))
    run = sub.add_parser("run", help="Run a single authenticated agent; one at a time")
    run.add_argument("--root", type=Path, default=Path(DEFAULT_ROOT))
    run.add_argument("--track", choices=sorted(TRACKS), required=True)
    run.add_argument("--engine", choices=["codex", "claude"], default="codex")
    run.add_argument(
        "--execute", action="store_true", help="Actually invoke agent (default dry run)"
    )
    run.add_argument("--timeout", type=int, default=1200)
    args = p.parse_args()
    try:
        if args.cmd == "prepare":
            result = prepare(args.repo, args.root, args.base, args.suffix)
            for key, item in result["tracks"].items():
                print(f"PREPARED {key}: {item['path']} [{item['branch']}]")
        elif args.cmd == "status":
            print(json.dumps(describe(args.root), ensure_ascii=False, indent=2))
        else:
            return run_agent(
                args.root, args.track, args.engine, args.execute, args.timeout
            )
        return 0
    except AgentKitError as error:
        print(f"BLOCKED: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
