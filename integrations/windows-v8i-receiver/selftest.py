"""Headless local companion smoke test (does not require Tk, V8i, network)."""
from __future__ import annotations

import json
import tempfile
from pathlib import Path

import bridge_contract  # noqa: F401
import diagnostics  # noqa: F401
import outbound  # noqa: F401
import receiver  # noqa: F401
import sync_transport  # noqa: F401
import windows_com  # noqa: F401


def run() -> int:
    with tempfile.TemporaryDirectory() as temporary:
        output = Path(temporary) / "ovg-smoke.json"
        output.write_text(json.dumps({"selftest": "ok"}), encoding="utf-8")
        if json.loads(output.read_text(encoding="utf-8")) != {"selftest": "ok"}:
            raise RuntimeError("Cannot read/write companion temporary files")
    return 0


if __name__ == "__main__":
    raise SystemExit(run())