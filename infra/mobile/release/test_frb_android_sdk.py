import pathlib
import runpy
import unittest

PATCH = runpy.run_path(str(pathlib.Path(__file__).resolve().parents[1] / "patch-frb-android-sdk.py"))["patch"]

class AndroidSdkPatchTests(unittest.TestCase):
    def test_pinned_template(self):
        before = "android {\n    compileSdkVersion 33\n    defaultConfig { minSdkVersion 19 }\n}\n"
        after = PATCH(before)
        self.assertEqual(after, before.replace("compileSdkVersion 33", "compileSdkVersion 36"))
        self.assertEqual(PATCH(after), after)

    def test_unknown_template(self):
        for text in ("compileSdkVersion 32", "compileSdk = 33", "compileSdkVersion 33\ncompileSdkVersion 36"):
            with self.subTest(text=text), self.assertRaises(ValueError):
                PATCH(text)
