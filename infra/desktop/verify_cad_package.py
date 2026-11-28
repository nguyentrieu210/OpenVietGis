"""Check packaged native engine identity and real stdio ready handshake."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

PIN = "3e738a2447d0cccf196f3b0f81823cd6a3e1d62d"

def main():
    p = argparse.ArgumentParser()
    p.add_argument("--resource-root", type=Path, required=True)
    a = p.parse_args()
    root = a.resource_root / "native-cad"
    manifest = json.loads((root / "manifest.json").read_text())
    binary = (root / ("OpenCADStudio.exe" if os.name == "nt" else "OpenCADStudio")).resolve(strict=True)
    assert manifest["source_revision"] == PIN
    with binary.open("rb") as f:
        assert hashlib.file_digest(f, "sha256").hexdigest() == manifest["sha256"]
    assert (root / "LICENSE").stat().st_size > 0
    with tempfile.TemporaryDirectory() as tmp:
        env = {k: os.environ[k] for k in ("PATH", "SystemRoot", "WINDIR", "COMSPEC", "LANG", "LC_ALL") if k in os.environ}
        env.update({k: tmp for k in ("HOME","USERPROFILE","XDG_CONFIG_HOME","XDG_CACHE_HOME","APPDATA","LOCALAPPDATA","TEMP","TMP","TMPDIR")})
        # Bounded known read on a fresh blank drawing; EOF shuts down server.
        r = subprocess.run([str(binary), "--serve"], input=b'{"protocol":1,"op":"state"}\n',
                           cwd=tmp, env=env, capture_output=True, timeout=60, check=True)
        assert len(r.stdout) <= 4*1024*1024
        lines = r.stdout.splitlines()
        assert len(lines) == 2
        ready, state = map(json.loads, lines)
        assert ready["ok"] is True and ready["ready"] is True
        assert state["ok"] is True and isinstance(state["document_id"], int)
    print("Pinned packaged engine digest and stdio handshake PASS")

if __name__ == "__main__":
    main()
