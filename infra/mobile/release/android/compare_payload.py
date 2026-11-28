"""Compare validated independently built APK payload reports; not release authorization."""
import argparse
import json
import pathlib
import re

IDENTITY = ("candidate_sha", "profile", "package", "version_code", "version_name")
REQUIRED = {"AndroidManifest.xml", "lib/arm64-v8a/libflutter.so"}

def payload(report):
    if not isinstance(report, dict) or report.get("schema") != 1:
        raise ValueError("invalid report schema")
    if not re.fullmatch(r"[a-f0-9]{40}", str(report.get("candidate_sha", ""))):
        raise ValueError("invalid candidate SHA")
    if report.get("profile") not in ("debug-candidate", "release-candidate"):
        raise ValueError("invalid profile")
    if report.get("package") != "org.openvietgis.openvietgis_mobile":
        raise ValueError("invalid package")
    for key in ("version_code", "version_name"):
        if not isinstance(report.get(key), str) or not report[key].strip():
            raise ValueError("missing version")
    items = report.get("payload")
    if not isinstance(items, list):
        raise ValueError("missing payload")
    result = {}
    for entry in items:
        if not isinstance(entry, dict):
            raise ValueError("invalid payload entry")
        name, digest, size = (entry.get(k) for k in ("path", "sha256", "bytes"))
        if (not isinstance(name, str) or not name or name in result
                or name.startswith(("/", "META-INF/")) or "\\" in name
                or ".." in pathlib.PurePosixPath(name).parts):
            raise ValueError("invalid or duplicate payload path")
        if (not isinstance(digest, str) or not re.fullmatch(r"[a-f0-9]{64}", digest)
                or type(size) is not int or size < 0):
            raise ValueError("invalid payload digest or size")
        result[name] = (digest, size)
    if not REQUIRED.issubset(result) or any(result[k][1] == 0 for k in REQUIRED):
        raise ValueError("incomplete APK payload")
    return result

def compare(first, second):
    x, y = payload(first), payload(second)
    if any(first[k] != second[k] for k in IDENTITY):
        raise ValueError("candidate identity mismatch")
    diff = sorted(k for k in x.keys() | y.keys() if x.get(k) != y.get(k))
    return {"equal": not diff, "different": diff, "release_authorized": False}

def main():
    p = argparse.ArgumentParser()
    p.add_argument("first", type=pathlib.Path)
    p.add_argument("second", type=pathlib.Path)
    a = p.parse_args()
    try:
        result = compare(json.loads(a.first.read_text()), json.loads(a.second.read_text()))
    except (ValueError, OSError) as e:
        p.exit(2, str(e) + "\n")
    print(json.dumps(result, indent=2))
    raise SystemExit(0 if result["equal"] else 1)

if __name__ == "__main__":
    main()
