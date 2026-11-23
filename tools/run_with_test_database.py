from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path

from alembic import command
from alembic.config import Config

from tools.migrate_official_modules import migrate_official_modules
from tools.test_database import build_test_environment

ROOT = Path(__file__).resolve().parents[1]


def main() -> int:
    if len(sys.argv) < 2:
        raise RuntimeError("A test command is required")
    environment = build_test_environment(os.environ)
    os.environ.update(environment)
    command.upgrade(Config(str(ROOT / "apps" / "api" / "alembic.ini")), "head")
    migrate_official_modules()
    return subprocess.run(
        sys.argv[1:], cwd=ROOT, env=environment, check=False
    ).returncode


if __name__ == "__main__":
    raise SystemExit(main())
