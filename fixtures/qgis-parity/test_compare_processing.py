"""Fail-closed checks for independent planar and mixed-CRS processing comparison."""
import json
import tempfile
import unittest
from pathlib import Path

from compare_processing import compare


def union_rows():
    return [
        {"source_feature_id": "p1", "overlay_feature_id": None, "area_m2": 5000},
        {"source_feature_id": "p2", "overlay_feature_id": None, "area_m2": 5000},
        {"source_feature_id": "p3", "overlay_feature_id": None, "area_m2": 10000},
        {"source_feature_id": "p1", "overlay_feature_id": "clip", "area_m2": 5000},
        {"source_feature_id": "p2", "overlay_feature_id": "clip", "area_m2": 5000},
    ]


class ComparisonTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        root = Path(self.temp.name)
        self.qgis, self.rust = root / "qgis", root / "rust"
        (self.qgis / "reference").mkdir(parents=True)
        self.rust.mkdir()
        for directory in (self.qgis, self.rust):
            (directory / "candidate-sha.txt").write_text("a" * 40)
        self.q = {
            "qgis_version": "3.44.16", "crs": "EPSG:3857", "fixture_sha256": "b" * 64,
            "actual": {
                "buffer": {"features": 1, "area_m2": 312.1445152258},
                "clip": {"features": 2, "area_m2": 10000},
                "dissolve_zone": {"features": 2, "areas_m2": {"A": 20000, "B": 10000}},
                "mixed_crs_union": {"features": 5, "area_m2": 30000, "rows": union_rows()},
                "mixed_crs_spatial_join": {"features": 1, "rows": [
                    {"source_feature_id": "pt", "overlay_feature_id": "pt",
                     "source_code": "pt", "overlay_code": "pt"}]},
                "reverse_mixed_crs_spatial_join": {"features": 1, "rows": [
                    {"source_feature_id": "pt", "overlay_feature_id": "pt",
                     "source_code": "pt", "overlay_code": "pt"}]},
                "mixed_crs_reproject": {"crs": "EPSG:3857", "features": 1, "rows": [
                    {"source_feature_id": "pt", "overlay_feature_id": None,
                     "code": "pt", "x": 50.0, "y": 50.0}]},
            },
        }
        self.r = json.loads(json.dumps(self.q))
        del self.r["qgis_version"]

    def run_compare(self):
        (self.qgis / "reference/qgis-oracle.json").write_text(json.dumps(self.q))
        (self.rust / "processing.json").write_text(json.dumps(self.r))
        return compare(self.qgis, self.rust)

    def test_matching_measurements_and_lineage(self):
        self.assertEqual(len(self.run_compare()["checks"]), 8)

    def test_commit_mismatch(self):
        (self.rust / "candidate-sha.txt").write_text("c" * 40)
        with self.assertRaisesRegex(ValueError, "commit"):
            self.run_compare()

    def test_fixture_mismatch(self):
        self.r["fixture_sha256"] = "c" * 64
        with self.assertRaisesRegex(ValueError, "digest"):
            self.run_compare()

    def test_area_mismatch(self):
        self.r["actual"]["clip"]["area_m2"] += 0.01
        with self.assertRaisesRegex(ValueError, "Area mismatch"):
            self.run_compare()

    def test_nonfinite_area(self):
        self.r["actual"]["buffer"]["area_m2"] = float("nan")
        with self.assertRaisesRegex(ValueError, "Invalid area"):
            self.run_compare()

    def test_group_mismatch(self):
        self.r["actual"]["dissolve_zone"]["areas_m2"] = {"A": 30000}
        with self.assertRaisesRegex(ValueError, "group mismatch"):
            self.run_compare()

    def test_count_mismatch(self):
        self.r["actual"]["clip"]["features"] = 1
        with self.assertRaisesRegex(ValueError, "Feature count mismatch"):
            self.run_compare()

    def test_union_sliver_count_mismatch(self):
        self.r["actual"]["mixed_crs_union"]["features"] += 1
        self.r["actual"]["mixed_crs_union"]["rows"].append({
            "source_feature_id": None, "overlay_feature_id": "clip", "area_m2": 1e-12,
        })
        with self.assertRaisesRegex(ValueError, "mixed_crs_union"):
            self.run_compare()

    def test_union_lineage_mismatch(self):
        self.r["actual"]["mixed_crs_union"]["rows"][0]["source_feature_id"] = "wrong"
        with self.assertRaisesRegex(ValueError, "Union lineage mismatch"):
            self.run_compare()

    def test_spatial_join_attributes_mismatch(self):
        self.r["actual"]["mixed_crs_spatial_join"]["rows"][0]["overlay_code"] = "wrong"
        with self.assertRaisesRegex(ValueError, "Spatial Join attribute mismatch"):
            self.run_compare()

    def test_reproject_coordinate_mismatch(self):
        self.r["actual"]["mixed_crs_reproject"]["rows"][0]["x"] += 0.1
        with self.assertRaisesRegex(ValueError, "Reproject coordinate mismatch"):
            self.run_compare()


if __name__ == "__main__":
    unittest.main()
