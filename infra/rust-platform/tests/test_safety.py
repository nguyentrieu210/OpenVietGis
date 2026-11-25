"""Fail-closed rehearsal/benchmark tests: no real Docker or DB connections."""
import ast
import os
from pathlib import Path
import re
import subprocess
import tempfile
import unittest
from urllib.parse import urlparse

ROOT = Path(__file__).resolve().parents[3]
SCRIPT = ROOT / "infra/rust-platform/staging-backup-restore.sh"
RUNNER = ROOT / "tools/benchmark/rust-platform/benchmark_runner.py"


class SafetyTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.directory = Path(self.temp.name)
        self.log = self.directory / "docker.log"
        docker = self.directory / "docker"
        docker.write_text("""#!/bin/bash
set -eu
printf '%s\\n' "$*" >> "$MOCK_LOG"
if [[ "$1" == inspect ]]; then echo true; exit 0; fi
if [[ "$*" == *pg_database_size* ]]; then echo 1; exit 0; fi
if [[ "$*" == *pg_size_pretty* ]]; then echo '1 byte'; exit 0; fi
if [[ "$*" == *pg_dump* ]]; then echo mock-dump; exit 0; fi
if [[ "$*" == *psql* && "$*" == *' -d postgres '* ]]; then
  input="$(cat)"
  printf '%s\\n' "$input" >> "$MOCK_LOG"
  if [[ "$input" == *'CREATE DATABASE'* ]]; then exit 1; fi
fi
cat >/dev/null
""")
        docker.chmod(0o755)
        df = self.directory / "df"
        df.write_text("#!/bin/sh\nprintf 'Filesystem 1-blocks Used Available Capacity Mounted on\\nmock 9000000000 0 9000000000 0%% /\\n'\n")
        df.chmod(0o755)
        self.env = dict(os.environ, PATH=f"{self.directory}:{os.environ['PATH']}", MOCK_LOG=str(self.log))

    def run_backup(self, *args):
        return subprocess.run(
            ["bash", str(SCRIPT), "--backup-dir", str(self.directory), *args],
            env=self.env, text=True, capture_output=True, timeout=10,
        )

    def test_refuse_live_target_and_source(self):
        for args in (
            ("--target-container", "hn019-preview-postgres"),
            ("--source-container", "hn019-preview-postgres"),
            ("--target-db", "openvietgis_test"),
            ("--target-db", "ovg_staging_" + "a" * 70),
            ("--source-db", "ovg_staging_same", "--target-db", "ovg_staging_same"),
        ):
            with self.subTest(args=args):
                self.assertEqual(self.run_backup(*args).returncode, 10)
        self.assertFalse(self.log.exists())

    def test_collision_never_drops_existing_database(self):
        result = self.run_backup("--target-db", "ovg_rehearsal_collision")
        self.assertNotEqual(result.returncode, 0)
        calls = self.log.read_text()
        self.assertIn("CREATE DATABASE", calls)
        self.assertNotIn("DROP DATABASE", calls)
        self.assertNotIn("pg_terminate_backend", calls)

    def test_benchmark_guards(self):
        # Load only pure guard functions, without optional benchmark dependencies.
        tree = ast.parse(RUNNER.read_text())
        names = {"assert_safe_admin_url", "assert_safe_bench_db", "assert_safe_http_target"}
        module = ast.Module(body=[n for n in tree.body if isinstance(n, ast.FunctionDef) and n.name in names], type_ignores=[])
        env = {"urlparse": urlparse, "re": re}
        exec(compile(module, str(RUNNER), "exec"), env)
        env["assert_safe_admin_url"]("postgresql://localhost:54330/postgres")
        env["assert_safe_bench_db"]("postgresql://localhost:54330/openvietgis_bench_064")
        env["assert_safe_http_target"]("http://127.0.0.1:18900")
        for name, value in (
            ("assert_safe_admin_url", "postgresql://localhost:54340/postgres"),
            ("assert_safe_admin_url", "postgresql://localhost:54330/postgres?host=evil"),
            ("assert_safe_bench_db", "postgresql://localhost:54330/openvietgis_test"),
            ("assert_safe_bench_db", "postgresql://localhost:54330/openvietgis_bench_x%22"),
            ("assert_safe_http_target", "https://openvietgis.cloud"),
            ("assert_safe_http_target", "http://127.0.0.1:8019"),
        ):
            with self.subTest(value=value), self.assertRaises(RuntimeError):
                env[name](value)


if __name__ == "__main__":
    unittest.main()
