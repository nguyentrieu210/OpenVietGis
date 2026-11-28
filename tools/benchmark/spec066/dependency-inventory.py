"""Read-only source dependency and workflow inventory from an exact Git commit."""
import json
import posixpath
import re
import subprocess
import sys
import tomllib
from collections import defaultdict


def git(*args):
    return subprocess.check_output(["git", *args]).decode("utf-8")

ref = git("rev-parse", "--verify", (sys.argv[1] if len(sys.argv) > 1 else "HEAD") + "^{commit}").strip()
paths = set(git("ls-tree", "-r", "--name-only", ref).splitlines())

def read(path):
    return git("show", f"{ref}:{path}")

manifests = {}
for path in sorted(paths):
    if path.endswith("Cargo.toml"):
        data = tomllib.loads(read(path))
        manifests[path] = {
            "name": data.get("package", {}).get("name"),
            "workspace_members": data.get("workspace", {}).get("members", []),
            "local_dependencies": {
                name: dep for name, dep in data.get("dependencies", {}).items()
                if isinstance(dep, dict) and ("path" in dep or dep.get("workspace"))
            },
        }
    elif path.endswith("package.json"):
        data = json.loads(read(path))
        manifests[path] = {
            "name": data.get("name"),
            "local_dependencies": {
                name: dep for category in ("dependencies", "devDependencies", "peerDependencies")
                for name, dep in data.get(category, {}).items()
                if isinstance(dep, str) and dep.startswith(("workspace:", "file:", "link:"))
            },
            "script_names": sorted(data.get("scripts", {})),
        }
    elif path.endswith("pyproject.toml"):
        data = tomllib.loads(read(path))
        manifests[path] = {"name": data.get("project", {}).get("name"),
                           "dependencies": data.get("project", {}).get("dependencies", [])}

embedded = defaultdict(list)
for path in sorted(p for p in paths if p.endswith(".rs")):
    for match in re.finditer(r'include_(?:str|bytes)!\(\s*"([^"]+)"', read(path)):
        dest = posixpath.normpath(posixpath.join(posixpath.dirname(path), match.group(1)))
        embedded[dest].append(path)

workflow = {}
for path in sorted(p for p in paths if p.startswith(".github/workflows/") and p.endswith((".yaml", ".yml"))):
    source = read(path)
    jobs = source.split("\njobs:\n", 1)[-1]
    workflow[path] = {
        "name": re.search(r"^name:\s*(.*)$", source, re.MULTILINE).group(1),
        "job_ids": re.findall(r"^  ([\w-]+):\s*$", jobs, re.MULTILINE),
        "trigger_block": source.split("\non:\n", 1)[-1].split("\n\n", 1)[0].split("\npermissions:", 1)[0].split("\njobs:", 1)[0],
        "concurrency_present": bool(re.search(r"^concurrency:", source, re.MULTILINE)),
        "command_step_names": re.findall(r"^\s+- name:\s*(.*)$", jobs, re.MULTILINE),
    }

report = {
    "sha": ref, "manifests": manifests,
    "rust_embedded_files": [{"path": p, "tracked": p in paths, "consumers": c} for p, c in sorted(embedded.items())],
    "workflows": workflow,
    "limitations": ["Static manifest and literal include inventory; not a complete import/route graph.",
                    "Required branch rules and external deployment consumers must be verified separately.",
                    "No deletion decision follows automatically from absence in this report."],
}
print(json.dumps(report, indent=2, ensure_ascii=False))
