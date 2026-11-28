import pathlib
import runpy
import unittest
P = runpy.run_path(str(pathlib.Path(__file__).resolve().parents[1] / "patch-arm64-packaging.py"))
class Arm64PackagingTests(unittest.TestCase):
    def test_patch_preserves_arm64_and_default_config(self):
        source = "android {\n    defaultConfig {\n        minSdk = flutter.minSdkVersion\n    }\n}\n"
        out = P["patch"](source)
        self.assertIn("lib/armeabi-v7a/**", out)
        self.assertIn("lib/x86_64/**", out)
        self.assertNotIn("lib/arm64-v8a/**", out)
        self.assertIn(source.split("\n", 1)[1], out)
        self.assertEqual(P["patch"](out), out)
    def test_reject_unknown_packaging(self):
        with self.assertRaises(ValueError):
            P["patch"]("android {\n    packaging {\n    }\n    defaultConfig {\n    }\n}")
