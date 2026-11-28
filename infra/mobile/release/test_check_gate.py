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
 def test_duplicate_matrix_id(self):
  with tempfile.TemporaryDirectory() as d:
   p=pathlib.Path(d)/"matrix.md"
   source=(ROOT/"execution-matrix.md").read_text()
   p.write_text(source+"\n- [ ] T001.1 duplicate\n")
   with self.assertRaisesRegex(ValueError,"duplicate matrix ID"):matrix(p)
 def test_duplicate_even_if_first_record_invalid(self):
  r={"leaf":"T001.1","candidate_sha":"b"*40,"status":"PASS"}
  report=reconcile(ROOT,"a"*40,[r,r])
  self.assertEqual(len(report["invalid"]),2)
  self.assertEqual(report["invalid"][1]["reason"],"unknown or duplicate")
 def test_malformed_record(self):
  report=reconcile(ROOT,"a"*40,[None,{"leaf":[]}])
  self.assertEqual(len(report["invalid"]),2)
  self.assertFalse(report["android_evidence_complete"])
 def test_independent_ios_invalid_evidence(self):
  import hashlib
  with tempfile.TemporaryDirectory() as d:
   root=pathlib.Path(d)
   source=(ROOT/"execution-matrix.md").read_text().replace("- [ ] T","- [x] T")
   (root/"execution-matrix.md").write_text(source)
   (root/"artifact").write_bytes(b"test fixture")
   leaves,_=matrix(root/"execution-matrix.md")
   records=[]
   for leaf in sorted(leaves):
    if leaf.startswith("T077."):continue
    r={k:"fixture" for k in ["fr","sc","gate","command","expected","actual","reviewer","limitations","toolchain"]}
    r.update(leaf=leaf,candidate_sha="a"*40,status="PASS",artifact="artifact",artifact_sha256=hashlib.sha256(b"test fixture").hexdigest())
    records.append(r)
   records.append({"leaf":"T077.1","status":"FAIL"})
   report=reconcile(root,"a"*40,records)
   self.assertTrue(report["android_evidence_complete"])
   self.assertFalse(report["ios_evidence_complete"])
   self.assertFalse(report["release_authorized"])
if __name__=="__main__":unittest.main()
