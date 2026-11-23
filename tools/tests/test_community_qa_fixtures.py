from __future__ import annotations

import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
FIXTURE_DIR = ROOT / "fixtures" / "community-qa"


def test_community_qa_fixtures_manifest_and_checksums() -> None:
    manifest_path = FIXTURE_DIR / "manifest.json"
    assert manifest_path.exists(), "fixtures/community-qa/manifest.json must exist"
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    assert manifest["contains_private_data"] is False
    artifacts = manifest["artifacts"]
    assert len(artifacts) == 6

    expected_files = {
        "synthetic_gis_4326.geojson",
        "synthetic_gis_3857.geojson",
        "synthetic_invalid_geometry.geojson",
        "synthetic_gis_4326.gpkg",
        "synthetic_gis_4326_shp.zip",
        "synthetic_cad_2d.dxf",
    }
    assert {item["file"] for item in artifacts} == expected_files

    for item in artifacts:
        fp = FIXTURE_DIR / item["file"]
        assert fp.exists(), f"Missing fixture file {item['file']}"
        actual = hashlib.sha256(fp.read_bytes()).hexdigest()
        assert actual == item["sha256"], f"Checksum mismatch for {item['file']}"


def test_community_qa_geojson_geometry_families() -> None:
    valid = json.loads((FIXTURE_DIR / "synthetic_gis_4326.geojson").read_text(encoding="utf-8"))
    types = [f["geometry"]["type"] for f in valid["features"]]
    assert types == ["Point", "LineString", "Polygon"]
    # Polygon has an exterior ring + 1 interior hole ring
    polygon_rings = valid["features"][2]["geometry"]["coordinates"]
    assert len(polygon_rings) == 2

    invalid = json.loads(
        (FIXTURE_DIR / "synthetic_invalid_geometry.geojson").read_text(encoding="utf-8")
    )
    assert len(invalid["features"]) == 2
