"""Package a locally built pinned CAD executable, never download arbitrary binary.

Run in trusted build runner. Manifest is an integrity check, not a signature.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess

PIN = "3e738a2447d0cccf196f3b0f81823cd6a3e1d62d"

def main():
    p = argparse.ArgumentParser()
    p.add_argument("--source", type=Path, required=True)
    p.add_argument("--binary", type=Path, required=True)
    p.add_argument("--resource-root", type=Path, required=True)
    a = p.parse_args()
    revision = subprocess.check_output(["git", "-C", str(a.source), "rev-parse", "HEAD"], text=True).strip()
    if revision != PIN:
        raise SystemExit("CAD revision mismatch")
    if subprocess.check_output(["git", "-C", str(a.source), "status", "--porcelain", "--untracked-files=no"], text=True).strip():
        raise SystemExit("CAD tracked source differs from pinned revision")
    if a.binary.is_symlink() or not a.binary.is_file() or not 0 < a.binary.stat().st_size <= 512 * 1024 * 1024:
        raise SystemExit("invalid CAD binary")
    out = a.resource_root / "native-cad"
    out.mkdir(parents=True, exist_ok=False)
    name = "OpenCADStudio.exe" if os.name == "nt" else "OpenCADStudio"
    binary = out / name
    shutil.copy2(a.binary, binary)
    with binary.open("rb") as f:
        digest = hashlib.file_digest(f, "sha256").hexdigest()
    (out / "manifest.json").write_text(json.dumps({"source_revision": PIN, "sha256": digest}) + "\n")
    shutil.copy2(a.source / "LICENSE", out / "LICENSE")
    (out / "SOURCE.txt").write_text("https://github.com/nguyentrieu210/OpenVietCAD/tree/" + PIN + "\n")

if __name__ == "__main__":
    main()
