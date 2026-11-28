"""Compare actual independently measured Rust and QGIS processing artifacts."""
import argparse
import json
import math
import re
from collections import Counter, defaultdict
from pathlib import Path


def _count(reference: dict, candidate: dict, algorithm: str) -> int:
    qgis_count = reference.get("features")
    rust_count = candidate.get("features")
    if (type(qgis_count) is not int or type(rust_count) is not int
            or qgis_count <= 0 or qgis_count != rust_count):
        raise ValueError(
            f"Feature count mismatch: {algorithm}: QGIS={qgis_count}, Rust={rust_count}"
        )
    return qgis_count


def _finite_area(value, algorithm: str) -> float:
    if type(value) not in (int, float) or not math.isfinite(value) or value < 0:
        raise ValueError(f"Invalid area: {algorithm}: {value!r}")
    return float(value)


def _rows(value: dict, algorithm: str) -> list[dict]:
    rows = value.get("rows")
    if not isinstance(rows, list) or len(rows) != value.get("features"):
        raise ValueError(f"Missing row level output evidence: {algorithm}")
    if any(not isinstance(row, dict) for row in rows):
        raise ValueError(f"Invalid row level output evidence: {algorithm}")
    return rows


def _lineage_counts(rows: list[dict]) -> Counter:
    return Counter((row.get("source_feature_id"), row.get("overlay_feature_id")) for row in rows)


def compare(qgis_root: Path, rust_root: Path) -> dict:
    qsha = (qgis_root / "candidate-sha.txt").read_text().strip()
    rsha = (rust_root / "candidate-sha.txt").read_text().strip()
    if not re.fullmatch(r"[a-f0-9]{40}", qsha) or qsha != rsha:
        raise ValueError("Evidence must refer to the same exact candidate commit")
    q = json.loads((qgis_root / "reference/qgis-oracle.json").read_text())
    r = json.loads((rust_root / "processing.json").read_text())
    if not str(q["qgis_version"]).startswith("3.44.") or q["crs"] != "EPSG:3857" or r["crs"] != q["crs"]:
        raise ValueError("QGIS version or CRS mismatch")
    fixture = q["fixture_sha256"]
    if not re.fullmatch(r"[a-f0-9]{64}", fixture) or fixture != r["fixture_sha256"]:
        raise ValueError("Fixture manifest digest mismatch")

    checks = []
    for alg in ("buffer", "clip", "dissolve_zone"):
        a, b = q["actual"][alg], r["actual"][alg]
        _count(a, b, alg)
        fields = ["area_m2"] if alg != "dissolve_zone" else sorted(a["areas_m2"])
        if alg == "dissolve_zone" and (set(fields) != {"A", "B"} or set(fields) != set(b["areas_m2"])):
            raise ValueError("Dissolve group mismatch")
        for key in fields:
            x, y = (a[key], b[key]) if alg != "dissolve_zone" else (a["areas_m2"][key], b["areas_m2"][key])
            x, y = _finite_area(x, f"{alg}/{key}"), _finite_area(y, f"{alg}/{key}")
            if not math.isclose(x, y, rel_tol=0, abs_tol=1e-6):
                raise ValueError(f"Area mismatch: {alg}/{key}: QGIS={x}, Rust={y}")
            checks.append({"algorithm": alg, "area": key, "qgis": x, "rust": y, "absolute_delta": abs(x-y)})

    qa, ra = q["actual"]["mixed_crs_union"], r["actual"]["mixed_crs_union"]
    _count(qa, ra, "mixed_crs_union")
    qrows, rrows = _rows(qa, "mixed_crs_union"), _rows(ra, "mixed_crs_union")
    if _lineage_counts(qrows) != _lineage_counts(rrows):
        raise ValueError(
            f"Union lineage mismatch: QGIS={_lineage_counts(qrows)}, Rust={_lineage_counts(rrows)}"
        )
    for row in qrows + rrows:
        _finite_area(row.get("area_m2"), "mixed_crs_union")
    qarea, rarea = _finite_area(qa.get("area_m2"), "mixed_crs_union"), _finite_area(ra.get("area_m2"), "mixed_crs_union")
    if not math.isclose(qarea, rarea, rel_tol=0, abs_tol=1e-6):
        raise ValueError(f"Area mismatch: mixed_crs_union: QGIS={qarea}, Rust={rarea}")
    # Compare piece count and area for every source/overlay lineage pair. This
    # catches numerical slivers whose aggregate area is below the area tolerance.
    def piece_summary(rows):
        counts, areas = Counter(), defaultdict(float)
        for row in rows:
            key = (row.get("source_feature_id"), row.get("overlay_feature_id"))
            counts[key] += 1
            areas[key] += _finite_area(row.get("area_m2"), "mixed_crs_union")
        return counts, areas
    qc, qby_area = piece_summary(qrows)
    rc, rby_area = piece_summary(rrows)
    if qc != rc:
        raise ValueError(f"Union pieces per lineage mismatch: QGIS={qc}, Rust={rc}")
    for key in qby_area:
        if not math.isclose(qby_area[key], rby_area[key], rel_tol=0, abs_tol=1e-6):
            raise ValueError(
                f"Union area mismatch for lineage {key}: QGIS={qby_area[key]}, Rust={rby_area[key]}"
            )
    checks.append({"algorithm": "mixed_crs_union", "features": len(qrows), "area_m2": qarea})

    for alg in ("mixed_crs_spatial_join", "reverse_mixed_crs_spatial_join"):
        a, b = q["actual"][alg], r["actual"][alg]
        _count(a, b, alg)
        qrows, rrows = _rows(a, alg), _rows(b, alg)
        qlineage, rlineage = _lineage_counts(qrows), _lineage_counts(rrows)
        if qlineage != rlineage:
            raise ValueError(f"Spatial Join lineage mismatch: {alg}: QGIS={qlineage}, Rust={rlineage}")
        for key in ("source_code", "overlay_code"):
            qvalues = Counter((row.get("source_feature_id"), row.get("overlay_feature_id"), row.get(key))
                              for row in qrows if key in row)
            rvalues = Counter((row.get("source_feature_id"), row.get("overlay_feature_id"), row.get(key))
                              for row in rrows if key in row)
            if qvalues and rvalues and qvalues != rvalues:
                raise ValueError(f"Spatial Join attribute mismatch: {alg}/{key}: QGIS={qvalues}, Rust={rvalues}")
        checks.append({"algorithm": alg, "features": len(qrows), "lineage": dict(qlineage)})

    qa, ra = q["actual"]["mixed_crs_reproject"], r["actual"]["mixed_crs_reproject"]
    if qa.get("crs") != ra.get("crs") or qa.get("crs") != "EPSG:3857":
        raise ValueError(f"Reproject CRS mismatch: QGIS={qa.get('crs')}, Rust={ra.get('crs')}")
    _count(qa, ra, "mixed_crs_reproject")
    qrows, rrows = _rows(qa, "mixed_crs_reproject"), _rows(ra, "mixed_crs_reproject")
    if _lineage_counts(qrows) != _lineage_counts(rrows):
        raise ValueError("Reproject feature lineage mismatch")
    for field in ("code",):
        if Counter(row.get(field) for row in qrows) != Counter(row.get(field) for row in rrows):
            raise ValueError(f"Reproject attribute mismatch: {field}")
    for key in ("x", "y"):
        qvalues = sorted(row.get(key) for row in qrows)
        rvalues = sorted(row.get(key) for row in rrows)
        if any(type(v) not in (int, float) or not math.isfinite(v) for v in qvalues + rvalues):
            raise ValueError(f"Invalid reproject coordinate: {key}")
        if any(not math.isclose(x, y, rel_tol=0, abs_tol=1e-6) for x, y in zip(qvalues, rvalues, strict=True)):
            raise ValueError(f"Reproject coordinate mismatch: {key}: QGIS={qvalues}, Rust={rvalues}")
    checks.append({"algorithm": "mixed_crs_reproject", "features": len(qrows), "crs": qa["crs"]})

    return {
        "candidate_commit": qsha, "qgis_version": q["qgis_version"],
        "fixture_sha256": fixture, "checks": checks,
        "scope": "Planar and mixed-CRS processing fixtures for Buffer, Clip, Dissolve, Union, Spatial Join and Reproject; not full UI or QGIS feature parity",
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--qgis", required=True, type=Path)
    parser.add_argument("--rust", required=True, type=Path)
    args = parser.parse_args()
    print(json.dumps(compare(args.qgis, args.rust), indent=2))
