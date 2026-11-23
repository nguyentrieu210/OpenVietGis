"""OpenVietGIS MicroStation Bridge proof-of-contract CLI (no remote connection)."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

from bridge_contract import (
    ContractError,
    diff_snapshots,
    stable_digest,
    validate_snapshot,
)
from windows_com import V8iProbeError, probe_running_v8i

MAX_BYTES = 4 * 1024 * 1024


def read_snapshot(filename: str) -> dict:
    path = Path(filename)
    if path.stat().st_size > MAX_BYTES:
        raise ContractError("snapshot exceeds 4 MiB local preview limit")
    with path.open(encoding="utf-8") as handle:
        return json.load(handle)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="OpenVietGIS V8i bridge: read-only diagnostic and fixture validation")
    cmd = parser.add_subparsers(dest="cmd", required=True)
    cmd.add_parser("probe", help="attach to an already-running V8i COM session (Windows only)")
    validate = cmd.add_parser("validate", help="validate source-CRS snapshot without network transfer")
    validate.add_argument("snapshot")
    compare = cmd.add_parser("diff", help="preview non-destructive revision changes")
    compare.add_argument("previous")
    compare.add_argument("current")
    args = parser.parse_args(argv)
    try:
        if args.cmd == "probe":
            result = probe_running_v8i()
        elif args.cmd == "validate":
            snapshot = validate_snapshot(read_snapshot(args.snapshot))
            result = {
                "status": "validated",
                "revision_digest": stable_digest(snapshot),
                "source_crs": snapshot["source_crs"],
                "element_count": len(snapshot["elements"]),
                "unsupported_count": snapshot["unsupported_count"],
                "cadastral_completeness_verified": False,
                "uploaded": False,
            }
        else:
            result = diff_snapshots(read_snapshot(args.previous), read_snapshot(args.current))
        print(json.dumps(result, ensure_ascii=False, sort_keys=True, indent=2))
        return 0
    except (ContractError, V8iProbeError, OSError, UnicodeError, json.JSONDecodeError) as exc:
        # Never print raw DGN data or full local paths in diagnostics.
        print(f"ERROR: {type(exc).__name__}: {str(exc) if isinstance(exc, (ContractError, V8iProbeError)) else 'cannot read snapshot'}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
