"""Fake COM unit tests. These do not assert Windows/V8i compatibility."""

import sys
import unittest
from pathlib import Path
from types import SimpleNamespace

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from windows_com import V8iProbeError, scan_model_preview


class Enumerator:
    def __init__(self, elements):
        self.elements = elements
        self.index = -1

    def MoveNext(self):
        self.index += 1
        return self.index < len(self.elements)

    @property
    def Current(self):
        return self.elements[self.index]


class Model:
    def __init__(self, elements):
        self.elements = elements

    def Scan(self):
        return Enumerator(self.elements)


def pt(x, y):
    return SimpleNamespace(X=x, Y=y, Z=0)


def line(identity=10):
    p = SimpleNamespace(StartPoint=pt(1, 2), EndPoint=pt(5, 6))
    return SimpleNamespace(ID=identity, Type=3, AsLineElement=p)


def shape(identity=11):
    verts = [pt(1, 2), pt(7, 2), pt(7, 8), pt(1, 8)]
    return SimpleNamespace(ID=identity, Type=6, AsVertexList=SimpleNamespace(GetVertices=lambda: verts))


def text_element(identity=13):
    return SimpleNamespace(ID=identity, Type=17)


def scan(elements, **kw):
    return scan_model_preview(
        Model(elements), drawing_id="drawing1", model_id="model1",
        source_crs="EPSG:3405", **kw,
    )


class SafeScanContract(unittest.TestCase):
    def test_extract_line_shape_skip_text(self):
        result = scan([line(), shape(), text_element()])
        snap = result["snapshot"]
        self.assertEqual(len(snap["elements"]), 2)
        self.assertEqual(snap["unsupported_count"], 1)
        self.assertEqual(result["scan_stats"]["cad_type_counts"], {"17": 1, "3": 1, "6": 1})
        self.assertFalse(result["scan_stats"]["coordinate_units_verified"])
        self.assertFalse(result["scan_stats"]["reference_attachments_scanned"])
        geom = next(e["geometry"] for e in snap["elements"] if e["element_id"] == "11")
        self.assertEqual(geom["type"], "Polygon")
        self.assertEqual(geom["coordinates"][0][0], geom["coordinates"][0][-1])

    def test_reject_unknown_dlong_identifier(self):
        item = line(SimpleNamespace(High=1, Low=2))
        result = scan([item])
        self.assertEqual(result["snapshot"]["elements"][0]["element_id"], "00000001:00000002")

    def test_stable_id_must_be_unique(self):
        with self.assertRaisesRegex(V8iProbeError, "Duplicate"):
            scan([line(3), shape(3)])

    def test_known_but_malformed_geometry_aborts(self):
        bad = line()
        bad.AsLineElement.EndPoint = pt(float("nan"), 7)
        with self.assertRaisesRegex(V8iProbeError, "Snapshot failed"):
            scan([bad])

    def test_scanner_limit_never_returns_partial(self):
        with self.assertRaisesRegex(V8iProbeError, "Scan limit reached"):
            scan([line(), shape()], max_elements=1)

    def test_missing_crs_aborts(self):
        with self.assertRaisesRegex(V8iProbeError, "Snapshot failed"):
            scan_model_preview(Model([line()]), drawing_id="drawing1", model_id="model1", source_crs="VN2000")

    def test_empty_model_is_not_parcel_success(self):
        result = scan([])
        self.assertEqual(len(result["snapshot"]["elements"]), 0)
        self.assertFalse(result["scan_stats"]["cadastral_attributes_verified"])

    def test_unsupported_entities_block_complete_delta(self):
        from bridge_contract import ContractError, diff_snapshots
        current = scan([line(), text_element()])["snapshot"]
        with self.assertRaisesRegex(ContractError, "unsupported CAD entities"):
            diff_snapshots(current, current)


if __name__ == "__main__":
    unittest.main()
