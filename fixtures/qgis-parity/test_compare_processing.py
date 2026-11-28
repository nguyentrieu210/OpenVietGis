"""Fail-closed checks for the independent processing comparison."""
import json
import tempfile
import unittest
from pathlib import Path

from compare_processing import compare


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
            },
        }
        self.r = json.loads(json.dumps(self.q))
        del self.r["qgis_version"]

    def run_compare(self):
        (self.qgis / "reference/qgis-oracle.json").write_text(json.dumps(self.q))
        (self.rust / "processing.json").write_text(json.dumps(self.r))
        return compare(self.qgis, self.rust)

    def test_matching_measurements(self):
        self.assertEqual(len(self.run_compare()["checks"]), 4)

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
        with self.assertRaisesRegex(ValueError, "count mismatch"):
            self.run_compare()


if __name__ == "__main__":
    unittest.main()
