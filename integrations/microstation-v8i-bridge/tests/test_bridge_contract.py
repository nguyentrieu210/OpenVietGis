"""R025 portable snapshot and revision safety tests; no Bentley installation required."""

import json
import sys
import unittest
from pathlib import Path

BRIDGE_ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(BRIDGE_ROOT))
from bridge import main
from bridge_contract import (
    ContractError,
    diff_snapshots,
    stable_digest,
    validate_snapshot,
)


def fixture(name="sample-snapshot.json"):
    return json.loads((BRIDGE_ROOT / "fixtures" / name).read_text(encoding="utf-8"))


class SnapshotContractTests(unittest.TestCase):
    def test_validate_explicit_crs_and_projected_coordinates(self):
        result = validate_snapshot(fixture())
        self.assertEqual(result["source_crs"], "EPSG:3405")
        self.assertEqual(len(result["elements"]), 2)
        self.assertGreater(result["elements"][0]["geometry"]["coordinates"][0][0][0], 180)

    def test_stable_digest_independent_of_element_and_property_order(self):
        first = fixture()
        second = fixture()
        second["elements"].reverse()
        second["elements"][0]["properties"] = dict(reversed(list(second["elements"][0]["properties"].items())))
        self.assertEqual(stable_digest(first), stable_digest(second))

    def test_missing_crs_rejected(self):
        bad = fixture()
        del bad["source_crs"]
        with self.assertRaises(ContractError):
            validate_snapshot(bad)

    def test_implicit_crs_rejected(self):
        bad = fixture()
        bad["source_crs"] = "VN2000"
        with self.assertRaisesRegex(ContractError, "explicit EPSG"):
            validate_snapshot(bad)

    def test_unrecognized_fields_rejected(self):
        bad = fixture()
        bad["drawing_full_path"] = "C:/private/client.dgn"
        with self.assertRaises(ContractError):
            validate_snapshot(bad)

    def test_duplicate_element_id_rejected(self):
        bad = fixture()
        bad["elements"][1]["element_id"] = bad["elements"][0]["element_id"]
        with self.assertRaisesRegex(ContractError, "duplicate"):
            validate_snapshot(bad)

    def test_open_polygon_ring_rejected(self):
        bad = fixture()
        bad["elements"][0]["geometry"]["coordinates"][0].pop()
        with self.assertRaisesRegex(ContractError, "not closed"):
            validate_snapshot(bad)

    def test_unreadable_geometry_type_rejected(self):
        bad = fixture()
        bad["elements"][0]["geometry"]["type"] = "Curve"
        with self.assertRaisesRegex(ContractError, "only Point"):
            validate_snapshot(bad)

    def test_properties_non_scalar_rejected(self):
        bad = fixture()
        bad["elements"][0]["properties"]["metadata"] = {"secret": "not_allowed"}
        with self.assertRaisesRegex(ContractError, "scalar"):
            validate_snapshot(bad)

    def test_different_drawing_or_crs_refused(self):
        before = fixture()
        after = fixture("sample-snapshot-v2.json")
        after["drawing_id"] = "different-drawing"
        with self.assertRaisesRegex(ContractError, "drawing_id changed"):
            diff_snapshots(before, after)
        after["drawing_id"] = before["drawing_id"]
        after["source_crs"] = "EPSG:4326"
        with self.assertRaisesRegex(ContractError, "source_crs changed"):
            diff_snapshots(before, after)

    def test_unsupported_cad_entities_block_diff(self):
        before = fixture()
        after = fixture("sample-snapshot-v2.json")
        after["unsupported_count"] = 1
        with self.assertRaisesRegex(ContractError, "unsupported CAD entities"):
            diff_snapshots(before, after)

    def test_preview_delta_never_applies_delete(self):
        delta = diff_snapshots(fixture(), fixture("sample-snapshot-v2.json"))
        ops = {c["element_id"]: c["operation"] for c in delta["changes"]}
        self.assertEqual(ops, {"parcel-A": "upsert", "parcel-B": "delete_candidate", "parcel-C": "upsert"})
        self.assertIs(delta["applied"], False)
        self.assertNotIn("delete", ops.values())

    def test_no_change(self):
        result = diff_snapshots(fixture(), fixture())
        self.assertEqual({e["operation"] for e in result["changes"]}, {"unchanged"})

    def test_cli_validate(self):
        self.assertEqual(main(["validate", str(BRIDGE_ROOT / "fixtures" / "sample-snapshot.json")]), 0)

    def test_cli_probe_fails_safely_off_windows(self):
        if sys.platform != "win32":
            self.assertEqual(main(["probe"]), 2)


if __name__ == "__main__":
    unittest.main()
