"""Compare actual independently measured Rust and QGIS processing artifacts."""
import argparse
import json
import math
import re
from pathlib import Path


def compare(qgis_root: Path, rust_root: Path) -> dict:
    qsha = (qgis_root / "candidate-sha.txt").read_text().strip()
    rsha = (rust_root / "candidate-sha.txt").read_text().strip()
    if not re.fullmatch(r"[a-f0-9]{40}", qsha) or qsha != rsha:
        raise ValueError("Evidence must refer to the same exact candidate commit")
    q = json.loads((qgis_root / "reference/qgis-oracle.json").read_text())
    r = json.loads((rust_root / "processing.json").read_text())
    if not str(q["qgis_version"]).startswith("3.44.") or q["crs"] != "EPSG:3857" or r["crs"] != q["crs"]:
        raise ValueError("QGIS version or CRS mismatch")
    checks = []
    for alg in ("buffer", "clip", "dissolve_zone"):
        a, b = q["actual"][alg], r["actual"][alg]
        if type(a["features"]) is not int or type(b["features"]) is not int or a["features"] != b["features"] or a["features"] <= 0:
            raise ValueError(f"Feature count mismatch: {alg}")
        fields = ["area_m2"] if alg != "dissolve_zone" else sorted(a["areas_m2"])
        if alg == "dissolve_zone" and (set(fields) != {"A", "B"} or set(fields) != set(b["areas_m2"])):
            raise ValueError("Dissolve group mismatch")
        for key in fields:
            x, y = (a[key], b[key]) if alg != "dissolve_zone" else (a["areas_m2"][key], b["areas_m2"][key])
            if type(x) not in (int, float) or type(y) not in (int, float) or not all(math.isfinite(z) and z > 0 for z in (x, y)):
                raise ValueError(f"Invalid area: {alg}/{key}")
            if not math.isclose(x, y, rel_tol=0, abs_tol=1e-6):
                raise ValueError(f"Area mismatch: {alg}/{key}: QGIS={x}, Rust={y}")
            checks.append({"algorithm": alg, "area": key, "qgis": x, "rust": y, "absolute_delta": abs(x-y)})
    return {"candidate_commit": qsha, "qgis_version": q["qgis_version"], "checks": checks,
            "scope": "Three planar vector algorithm fixtures, not full UI or QGIS feature parity"}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--qgis", required=True, type=Path)
    parser.add_argument("--rust", required=True, type=Path)
    args = parser.parse_args()
    print(json.dumps(compare(args.qgis, args.rust), indent=2))
