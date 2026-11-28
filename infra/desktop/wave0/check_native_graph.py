"""Reject server and Flutter adapter crates in resolved native dependency metadata."""
import json
import sys
from pathlib import Path

DENIED = {"axum", "sqlx", "sqlx-core", "sqlx-postgres", "openvietgis-api-rust",
          "ovg-storage", "ovg-mobile-ffi", "flutter_rust_bridge"}

def violations(data):
    resolve = data.get("resolve")
    if not isinstance(resolve, dict) or not resolve.get("nodes"):
        raise ValueError("missing resolved dependency graph")
    packages = {p["id"]: p["name"] for p in data["packages"]}
    active = {node["id"] for node in resolve["nodes"]}
    if not active <= packages.keys():
        raise ValueError("unresolved package IDs")
    return sorted({packages[p] for p in active if packages[p] in DENIED})

if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("usage: check_native_graph.py metadata.json")
    failures = violations(json.loads(Path(sys.argv[1]).read_text(encoding="utf-8")))
    if failures:
        raise SystemExit("Forbidden native dependencies: " + ", ".join(failures))
    print("Resolved graph excludes the listed server and Flutter adapter crates")
