import pathlib,tempfile,unittest,sys
sys.path.insert(0,str(pathlib.Path(__file__).parent))
from check_gate import reconcile,matrix
ROOT=pathlib.Path(__file__).resolve().parents[3]/"specs/073-native-mobile-platform"
class GateTests(unittest.TestCase):
 def test_real_matrix(self):
  leaves,checked=matrix(ROOT/"execution-matrix.md")
  self.assertEqual(len(leaves),238)
  report=reconcile(ROOT,"a"*40,[])
  self.assertEqual(len(report["android_missing"]),235)
  self.assertEqual(len(report["ios_missing"]),3)
  self.assertFalse(report["release_authorized"])
 def test_wrong_sha_and_missing_artifact(self):
  report=reconcile(ROOT,"a"*40,[{"leaf":"T001.1","candidate_sha":"b"*40,"status":"PASS"}])
  self.assertEqual(len(report["invalid"]),1)
  self.assertFalse(report["android_evidence_complete"])
if __name__=="__main__":unittest.main()
