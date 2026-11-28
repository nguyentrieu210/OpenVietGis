"""Read-only static inventory of a pinned Git tree; not a runtime acceptance test."""

from __future__ import annotations

import argparse
import ast
import csv
import fnmatch
import json
import re
import subprocess
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SPEC = ROOT / "specs/031-product-inventory-audit"
METHODS = {
    "get",
    "post",
    "put",
    "patch",
    "delete",
    "head",
    "options",
    "websocket",
    "api_route",
}
UI_PATTERN = re.compile(
    r"<(?:button|Button|IconButton|MenuItem|input|select|textarea|a)\b|\bon(?:Click|Submit|Change|KeyDown)\s*="
)


def git(*args: str) -> str:
    return subprocess.check_output(["git", "-C", str(ROOT), *args], text=True)


def matches(path: str, patterns: list[str]) -> bool:
    return any(fnmatch.fnmatchcase(path, pattern) for pattern in patterns)


def is_test(path: str) -> bool:
    name = Path(path).name
    return path.endswith((".py", ".ts", ".tsx", ".js", ".mjs", ".cs")) and (
        "/tests/" in "/" + path
        or name.startswith("test_")
        or ".test." in name
        or ".spec." in name
        or name.endswith("_test.py")
    )


def python_candidates(source: str, path: str) -> list[dict[str, object]]:
    tree = ast.parse(source)
    rows = []
    for node in ast.walk(tree):
        if not isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
            continue
        for decorator in node.decorator_list:
            target = decorator.func if isinstance(decorator, ast.Call) else decorator
            if not isinstance(target, ast.Attribute):
                continue
            method = target.attr
            if path.startswith("apps/mcp/") and method == "tool":
                kind = "mcp_tool"
            elif path.startswith("apps/api/") and method in METHODS:
                kind = "http_route"
            else:
                continue
            route = (
                ast.unparse(decorator.args[0])
                if isinstance(decorator, ast.Call) and decorator.args
                else ""
            )
            rows.append(
                {
                    "kind": kind,
                    "path": path,
                    "line": decorator.lineno,
                    "name": node.name,
                    "detail": method + (" " + route if route else ""),
                }
            )
    # The remote workspace registers tools through a local wrapper and a literal registry.
    for node in ast.walk(tree):
        if (
            path == "apps/mcp/workspace_tools.py"
            and isinstance(node, ast.Call)
            and isinstance(node.func, ast.Name)
            and node.func.id == "tool"
            and node.args
        ):
            key = node.args[0]
            if isinstance(key, ast.Constant) and isinstance(key.value, str):
                rows.append(
                    {
                        "kind": "mcp_tool",
                        "path": path,
                        "line": node.lineno,
                        "name": key.value,
                        "detail": "static wrapper registration",
                    }
                )
        if (
            path == "apps/mcp/advanced_tools.py"
            and isinstance(node, ast.Assign)
            and any(isinstance(t, ast.Name) and t.id == "TOOLS" for t in node.targets)
            and isinstance(node.value, ast.Dict)
        ):
            for key in node.value.keys:
                if isinstance(key, ast.Constant) and isinstance(key.value, str):
                    rows.append(
                        {
                            "kind": "mcp_tool",
                            "path": path,
                            "line": key.lineno,
                            "name": key.value,
                            "detail": "static TOOLS registry",
                        }
                    )
    return sorted(rows, key=lambda row: (int(str(row["line"])), str(row["name"])))


def ui_candidates(source: str, path: str) -> list[dict[str, object]]:
    return [
        {
            "kind": "ui_candidate",
            "path": path,
            "line": index,
            "name": match.group(),
            "detail": "lexical match",
        }
        for index, line in enumerate(source.splitlines(), 1)
        for match in UI_PATTERN.finditer(line)
    ]


def generate(commit: str, out: Path) -> dict[str, object]:
    sha = git("rev-parse", "--verify", commit + "^{commit}").strip()
    paths = git("ls-tree", "-r", "--name-only", "-z", sha).rstrip("\0").split("\0")
    assert len(paths) == len(set(paths))
    registry = json.loads((SPEC / "capabilities.json").read_text())
    ids = [row["id"] for row in registry]
    if len(ids) != len(set(ids)):
        raise ValueError("duplicate capability IDs")
    capabilities = []
    for row in registry:
        sources = [
            p for p in paths if not is_test(p) and matches(p, row["source_patterns"])
        ]
        tests = [p for p in paths if is_test(p) and matches(p, row["test_patterns"])]
        capabilities.append(
            {
                **row,
                "source_candidates": sources,
                "test_candidates": tests,
                "status": "SOURCE_PRESENT" if sources else "PLANNED",
                "runtime_status": "NOT_RUN",
            }
        )
    candidates = []
    errors = []
    for path in paths:
        scan_py = (
            path.endswith(".py")
            and path.startswith(("apps/api/", "apps/mcp/"))
            and not is_test(path)
        )
        scan_ui = (
            path.endswith((".tsx", ".jsx"))
            and path.startswith(("apps/web-next/", "modules/"))
            and not is_test(path)
        )
        if not (scan_py or scan_ui):
            continue
        source = git("show", sha + ":" + path)
        try:
            candidates.extend(
                python_candidates(source, path)
                if scan_py
                else ui_candidates(source, path)
            )
        except SyntaxError as exc:
            errors.append({"path": path, "line": exc.lineno, "error": exc.msg})
    out.mkdir(parents=True, exist_ok=True)
    with (out / "files.csv").open("w", newline="") as handle:
        writer = csv.writer(handle, lineterminator="\n")
        writer.writerow(["path", "area", "test_candidate"])
        writer.writerows(
            (p, "/".join(p.split("/")[:2]), str(is_test(p)).lower()) for p in paths
        )
    with (out / "static-surfaces.csv").open("w", newline="") as handle:
        fields = ["kind", "path", "line", "name", "detail"]
        surface_writer = csv.DictWriter(handle, fieldnames=fields, lineterminator="\n")
        surface_writer.writeheader()
        surface_writer.writerows(candidates)
    summary = {
        "source_sha": sha,
        "files": len(paths),
        "test_files": sum(map(is_test, paths)),
        "area_counts": dict(sorted(Counter(p.split("/")[0] for p in paths).items())),
        "static_surfaces": dict(
            sorted(Counter(str(r["kind"]) for r in candidates).items())
        ),
        "capabilities": len(capabilities),
        "priority_counts": dict(Counter(r["priority"] for r in capabilities)),
        "source_present": sum(r["status"] == "SOURCE_PRESENT" for r in capabilities),
        "groups_without_test_candidates": [
            r["id"] for r in capabilities if not r["test_candidates"]
        ],
        "runtime_verified": 0,
        "parse_errors": errors,
    }
    for name, value in [("summary.json", summary), ("registry.json", capabilities)]:
        (out / name).write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n")
    return summary


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--commit", required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    summary = generate(args.commit, args.out)
    print(json.dumps(summary, indent=2))
    return int(bool(summary["parse_errors"]))


if __name__ == "__main__":
    raise SystemExit(main())
