"""Prepare a pinned synthetic-test relay directory; never publish it automatically."""

import argparse
import hashlib
import json
import re
import shutil
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument("source", type=Path)
parser.add_argument("target", type=Path)
parser.add_argument("--parent-origin", required=True)
args = parser.parse_args()
here = Path(__file__).resolve().parent
if not re.fullmatch(r"http://127\.0\.0\.1:[1-9][0-9]{0,4}", args.parent_origin):
    raise SystemExit("Synthetic test staging requires a loopback parent")
if args.target.exists():
    raise SystemExit("Refuse to overwrite an existing staging target")
manifest = {}
for line in (here / "release.sha256").read_text().splitlines():
    digest, name = line.split("  ", 1)
    if (
        not re.fullmatch("[a-f0-9]{64}", digest)
        or not name.startswith("./")
        or ".." in Path(name).parts
    ):
        raise SystemExit("Invalid manifest")
    manifest[name[2:]] = digest
actual = {}
for file in args.source.rglob("*"):
    if file.is_symlink():
        raise SystemExit("Unexpected symlink")
    if file.is_file():
        with file.open("rb") as f:
            actual[file.relative_to(args.source).as_posix()] = hashlib.file_digest(
                f, "sha256"
            ).hexdigest()
if actual != manifest:
    raise SystemExit("Release hash or file inventory differs")
shutil.copytree(args.source, args.target / "app")
html = args.target / "app/index.html"
source = html.read_text()
source, count = re.subn(
    r'<script id="openaec-analytics-loader" src="https://open-aec\.com/shared/analytics\.js\?v=20261026" defer></script>',
    "",
    source,
)
if count != 1:
    raise SystemExit("Unexpected analytics loader")
source = source.replace('<script src="ocs-control.js" defer></script>', "")
html.write_text(source)
for name in ("relay.html", "relay.css", "relay.js"):
    shutil.copy2(here / name, args.target / name)
(args.target / "relay-config.js").write_text(
    "window.OVG_CAD_PARENT_ORIGIN = " + json.dumps(args.parent_origin) + ";\n"
)
print("Pinned synthetic relay prepared:", args.target)
