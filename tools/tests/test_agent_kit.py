"""Offline safety tests for isolated Spec Kit agent worktrees."""

from __future__ import annotations

import subprocess
import tempfile
import unittest
from pathlib import Path

from tools.agent_kit import (
    DEFAULT_BASE,
    TRACKS,
    AgentKitError,
    describe,
    prepare,
    run_agent,
)


def run(*args: str, cwd: Path) -> None:
    subprocess.run(
        ["git", *args],
        cwd=cwd,
        text=True,
        capture_output=True,
        check=True,
    )


class AgentKitTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.parent = Path(self.tmp.name)
        self.repo = self.parent / "repo"
        self.repo.mkdir()
        run("init", "-b", "main", cwd=self.repo)
        run("config", "user.email", "unit-test@example.test", cwd=self.repo)
        run("config", "user.name", "AgentKit Tests", cwd=self.repo)
        prompt_folder = self.repo / "docs" / "agents"
        prompt_folder.mkdir(parents=True)
        for filename, _ in TRACKS.values():
            (prompt_folder / filename).write_text("# Test task\n", encoding="utf-8")
        run("add", ".", cwd=self.repo)
        run("commit", "-m", "fixture", cwd=self.repo)
        run("branch", DEFAULT_BASE, cwd=self.repo)
        self.root = self.parent / "worktrees"

    def test_create_five_isolated_worktrees_and_idempotence(self) -> None:
        result = prepare(self.repo, self.root)
        self.assertEqual(set(result["tracks"]), set(TRACKS))
        self.assertEqual(prepare(self.repo, self.root)["base_sha"], result["base_sha"])
        snap = describe(self.root)
        for track in snap["tracks"].values():
            self.assertEqual(track["status"], "READY")
            self.assertEqual(track["commits_ahead"], 0)
            self.assertTrue(Path(track["path"]).joinpath(track["task"]).is_file())
        self.assertEqual(
            run_agent(self.root, "frontend", "codex", execute=False, timeout=10), 0
        )

    def test_dirty_source_is_rejected(self) -> None:
        (self.repo / "untracked.txt").write_text("do not touch", encoding="utf-8")
        with self.assertRaisesRegex(AgentKitError, "dirty source"):
            prepare(self.repo, self.root)

    def test_nested_worktree_root_is_rejected(self) -> None:
        with self.assertRaisesRegex(AgentKitError, "separate"):
            prepare(self.repo, self.repo / "sub")

    def test_existing_unrelated_path_is_not_overwritten(self) -> None:
        self.root.mkdir()
        (self.root / "git").mkdir()
        (self.root / "git" / "personal.txt").write_text("keep", encoding="utf-8")
        with self.assertRaisesRegex(AgentKitError, "non-worktree"):
            prepare(self.repo, self.root)
        self.assertEqual(
            (self.root / "git" / "personal.txt").read_text(encoding="utf-8"),
            "keep",
        )


if __name__ == "__main__":
    unittest.main()
