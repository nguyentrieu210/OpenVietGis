import copy
import pathlib
import sys
import unittest
sys.path.insert(0, str(pathlib.Path(__file__).parent / "android"))
from compare_payload import compare

def fixture():
    return {"schema": 1, "candidate_sha": "a"*40, "profile": "debug-candidate",
            "package": "org.openvietgis.openvietgis_mobile", "version_code": "1",
            "version_name": "0.2.0", "payload": [
                {"path": "AndroidManifest.xml", "sha256": "b"*64, "bytes": 10},
                {"path": "lib/arm64-v8a/libflutter.so", "sha256": "c"*64, "bytes": 20}]}

class PayloadTests(unittest.TestCase):
    def test_valid_independent_payloads(self):
        x, y = fixture(), fixture()
        y["payload"].reverse()
        self.assertTrue(compare(x, y)["equal"])
        self.assertFalse(compare(x, y)["release_authorized"])

    def test_empty_payload_rejected(self):
        x = fixture()
        x["payload"] = []
        with self.assertRaisesRegex(ValueError, "incomplete"):
            compare(x, x)

    def test_duplicate_rejected(self):
        x = fixture()
        x["payload"].append(copy.deepcopy(x["payload"][0]))
        with self.assertRaisesRegex(ValueError, "duplicate"):
            compare(x, x)

    def test_invalid_digest_rejected(self):
        x = fixture()
        x["payload"][0]["sha256"] = "oops"
        with self.assertRaisesRegex(ValueError, "digest"):
            compare(x, x)

    def test_identity_mismatch(self):
        x, y = fixture(), fixture()
        y["version_code"] = "2"
        with self.assertRaisesRegex(ValueError, "identity"):
            compare(x, y)

    def test_payload_difference(self):
        x, y = fixture(), fixture()
        y["payload"][0]["bytes"] += 1
        self.assertEqual(compare(x, y)["different"], ["AndroidManifest.xml"])
