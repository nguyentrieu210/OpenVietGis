"""Fail-closed evidence reconciliation; never authorizes a release by itself."""
import argparse,json,re,pathlib,hashlib
def matrix(path):
 s=path.read_text()
 parent_ids=re.findall(r"^### (T\d{3})\s",s,re.M)
 leaf_ids=re.findall(r"^- \[[ xX]\] (T\d{3}\.\d+)\s",s,re.M)
 parents=set(parent_ids)
 leaves=set(leaf_ids)
 if len(parent_ids)!=len(parents) or len(leaf_ids)!=len(leaves):raise ValueError("duplicate matrix ID")
 checked=set(re.findall(r"^- \[[xX]\] (T\d{3}\.\d+)\s",s,re.M))
 if len(parents)!=78 or len(leaves)!=238 or {x.split(".")[0] for x in leaves}!=parents:raise ValueError("matrix mismatch")
 return leaves,checked
def reconcile(root,sha,records):
 leaves,checked=matrix(root/"execution-matrix.md")
 valid=set();bad=[];reviewers=set();seen=set()
 for r in records:
  if not isinstance(r,dict):bad.append({"leaf":None,"reason":"invalid record"});continue
  leaf=r.get("leaf")
  if not isinstance(leaf,str) or leaf not in leaves or leaf in seen:bad.append({"leaf":leaf,"reason":"unknown or duplicate"});continue
  seen.add(leaf)
  required=["fr","sc","gate","command","expected","actual","reviewer","limitations","toolchain","artifact","artifact_sha256"]
  if r.get("candidate_sha")!=sha or r.get("status")!="PASS" or not all(isinstance(r.get(k),str) and r[k].strip() for k in required):
   bad.append({"leaf":leaf,"reason":"missing or non-PASS exact-candidate evidence"});continue
  if not re.fullmatch("[a-f0-9]{64}",r["artifact_sha256"]):bad.append({"leaf":leaf,"reason":"invalid artifact checksum"});continue
  artifact=root/r["artifact"]
  if not artifact.resolve().is_relative_to(root.resolve()) or not artifact.is_file():
   bad.append({"leaf":leaf,"reason":"missing artifact"});continue
  try:
   with artifact.open("rb") as f:digest=hashlib.file_digest(f,"sha256").hexdigest()
  except OSError:bad.append({"leaf":leaf,"reason":"unreadable artifact"});continue
  if digest!=r["artifact_sha256"]:bad.append({"leaf":leaf,"reason":"artifact checksum mismatch"});continue
  valid.add(leaf);reviewers.add(r["reviewer"])
 # iOS T077 independent; absence cannot block Android A4.
 android={l for l in leaves if not l.startswith("T077.")}
 missing=sorted(android-(valid&checked));ios=sorted((leaves-android)-(valid&checked))
 android_bad=[b for b in bad if not isinstance(b["leaf"],str) or not b["leaf"].startswith("T077.") or b["leaf"] not in leaves]
 ios_bad=[b for b in bad if not isinstance(b["leaf"],str) or b["leaf"] not in leaves or b["leaf"].startswith("T077.")]
 return {"candidate_sha":sha,"parents":78,"leaves":len(leaves),"checked":len(checked),"exact_evidence":len(valid),"reviewers":sorted(reviewers),"invalid":bad,"android_missing":missing,"ios_missing":ios,"android_evidence_complete":not missing and not android_bad,"ios_evidence_complete":not ios and not ios_bad,"release_authorized":False,"note":"Human independent review, signing, physical device and release approval still required. Optional rejections require documented review, not fake PASS."}
def main():
 p=argparse.ArgumentParser();p.add_argument("--root",type=pathlib.Path,required=True);p.add_argument("--sha",required=True);p.add_argument("--records",type=pathlib.Path);p.add_argument("--out",type=pathlib.Path,required=True);a=p.parse_args()
 if not re.fullmatch("[a-f0-9]{40}",a.sha):raise ValueError("invalid candidate SHA")
 result=reconcile(a.root,a.sha,json.loads(a.records.read_text()) if a.records else [])
 a.out.write_text(json.dumps(result,indent=2));print("Android missing:",len(result["android_missing"]),"iOS missing:",len(result["ios_missing"]))
 raise SystemExit(0 if result["android_evidence_complete"] else 2)
if __name__=="__main__":main()
