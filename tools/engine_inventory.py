"""Read-only source inventory for R031. Static discovery is not runtime certification."""
from __future__ import annotations

import argparse
import json
import re
import subprocess
from collections import Counter
from pathlib import Path

ALIAS = re.compile(r'\balias\s*:\s*["\']([^"\']+)["\']')
GROUP = re.compile(r'\bgroup\s*:\s*["\']([^"\']+)["\']')
ROUTE = re.compile(r'@(?:router|app)\.(get|post|put|patch|delete)\(\s*["\']([^"\']+)["\']')
SIGNALS = {
    "postgis": re.compile(r'\bST_[A-Z][A-Za-z0-9_]+\s*\('),
    "gdal_ogr": re.compile(r'\b(?:gdal|ogr|osr)\.\w+'),
    "cesium": re.compile(r'cesium3d|\bCesium\b'),
    "maplibre": re.compile(r'maplibre-gl|\bmaplibregl\b'),
    "opencad": re.compile(r'openCadEditor|ovg-cad-relay'),
}


def inventory(root: Path) -> dict:
    tracked = subprocess.check_output(
        ["git", "-C", str(root), "ls-files", "-z"], text=True
    ).split("\0")
    commands, routes, signals, manifests = [], [], [], {}
    for rel in sorted(filter(None, tracked)):
        path = root / rel
        if not path.is_file():
            continue
        if rel.endswith("package.json"):
            data = json.loads(path.read_text(encoding="utf-8"))
            manifests[rel] = data.get("dependencies", {})
        if not rel.startswith(("apps/", "packages/", "modules/", "integrations/")):
            continue
        if not rel.endswith((".ts", ".tsx", ".py")):
            continue
        if any(part in ("tests", "generated", "node_modules") for part in Path(rel).parts):
            continue
        content = path.read_text(encoding="utf-8")
        if rel.startswith("apps/web-next/src/"):
            for match in ALIAS.finditer(content):
                next_alias = ALIAS.search(content, match.end())
                segment = content[match.end():next_alias.start() if next_alias else match.end()+300]
                group = GROUP.search(segment)
                commands.append({"alias": match.group(1), "group": group.group(1) if group else None,
                                 "path": rel, "line": content.count("\n", 0, match.start())+1})
        if rel.endswith(".py"):
            for match in ROUTE.finditer(content):
                routes.append({"method": match.group(1).upper(), "route": match.group(2),
                               "path": rel, "line": content.count("\n", 0, match.start())+1})
        found = sorted(key for key, pattern in SIGNALS.items() if pattern.search(content))
        if found:
            signals.append({"path": rel, "engine_signals": found})
    counts = Counter(row["alias"] for row in commands)
    return {
        "source_sha": subprocess.check_output(["git", "-C", str(root), "rev-parse", "HEAD"], text=True).strip(),
        "scope": "literal web-next aliases, literal Python route decorators, engine source signals and direct JS manifests",
        "limitations": ["Dynamic registrations and runtime availability require separate reconciliation.",
                        "Signals are not proof of license, correct behavior or deployment."],
        "counts": {"literal_commands": len(commands), "unique_aliases": len(counts),
                   "literal_routes": len(routes), "engine_signal_files": len(signals)},
        "duplicate_aliases": sorted(k for k, v in counts.items() if v > 1),
        "commands": commands, "routes": routes, "engine_signals": signals,
        "manifests": manifests,
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    result = json.dumps(inventory(args.root), indent=2, ensure_ascii=False) + "\n"
    if args.output:
        args.output.write_text(result, encoding="utf-8")
    else:
        print(result, end="")


if __name__ == "__main__":
    main()
