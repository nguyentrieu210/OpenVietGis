"""Fail if the integrated Flutter build resolves more than one shared engine."""
import json
import sys
from collections import defaultdict

with open(sys.argv[1], encoding="utf-8") as f:
    metadata = json.load(f)
names = defaultdict(list)
for package in metadata["packages"]:
    names[package["name"]].append(package)
required = (
    "ovg-project-store",
    "ovg-mobile-engine",
    "ovg-local-storage",
    "ovg-spatial",
    "ovg-core",
    "ovg-processing",
    "ovg-sync",
)
for name in required:
    packages = names[name]
    if len(packages) != 1:
        raise SystemExit(f"{name}: expected one package, got {[p['id'] for p in packages]}")
    if name != "ovg-project-store" and packages[0]["source"] is not None:
        raise SystemExit(f"{name}: expected shared checkout path, not {packages[0]['source']}")
print("Shared native graph: one instance of every required crate")
