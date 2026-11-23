"""Cross-platform V8i COM diagnostic mocks, no Bentley licensing or data."""
from __future__ import annotations

import sys
import unittest
from pathlib import Path
from types import SimpleNamespace

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from diagnostics import DiagnosticError, inspect_v8i  # noqa: E402


class Enumerator:
    def __init__(self, entities):
        self._values = iter(entities)
        self.Current = None

    def MoveNext(self):
        self.Current = next(self._values, None)
        return self.Current is not None


class DiagnosticTest(unittest.TestCase):
    def test_counts_supported_without_reading_geometry_or_modifying_model(self):
        model = SimpleNamespace(Name="Default", Scan=lambda: Enumerator([
            SimpleNamespace(Type=3), SimpleNamespace(Type=6), SimpleNamespace(Type=1),
        ]))
        app = SimpleNamespace(Version="8.11", ActiveModelReference=model)

        def attach(name):
            self.assertEqual(name, "MicroStationDGN.Application")
            return app

        report = inspect_v8i(attach)
        self.assertEqual(report["scanned"], 3)
        self.assertEqual(report["candidate_simple_entities"], 2)
        self.assertEqual(report["unsupported_or_unhandled"], 1)
        self.assertFalse(report["geometry_and_units_verified"])
        self.assertFalse(report["cad_write_enabled"])
        self.assertNotIn("drawing_path", report)

    def test_no_v8i_session_returns_clear_error(self):
        with self.assertRaisesRegex(DiagnosticError, "Cannot inspect"):
            inspect_v8i(lambda _: (_ for _ in ()).throw(RuntimeError("not running")))

    def test_scan_aborts_when_count_exceeds_bound(self):
        app = SimpleNamespace(ActiveModelReference=SimpleNamespace(
            Name="Default", Scan=lambda: Enumerator([SimpleNamespace(Type=3)] * 5001)
        ))
        with self.assertRaisesRegex(DiagnosticError, "exceeded 5000"):
            inspect_v8i(lambda _: app)


if __name__ == "__main__":
    unittest.main()