"""Inspect a real APK. No production signing or release authorization."""
import argparse, hashlib, json, pathlib, re, subprocess, sys, zipfile
sys.path.insert(0,str(pathlib.Path(__file__).resolve().parents[2]/"native"))
from verify_elf import inspect
def sha(path):
    with path.open("rb") as f:return hashlib.file_digest(f,"sha256").hexdigest()
def cmd(*args):
    return subprocess.check_output([str(a) for a in args],text=True,stderr=subprocess.STDOUT)
def inventory(apk):
    native=[];payload=[];names=set();total=0
    with zipfile.ZipFile(apk) as z:
        for i in z.infolist():
            if i.filename in names or i.filename.startswith("/") or ".." in pathlib.PurePosixPath(i.filename).parts or "\\" in i.filename:raise ValueError("unsafe APK path")
            names.add(i.filename)
            if i.is_dir():continue
            total+=i.file_size
            if total>512*1024*1024 or i.file_size>256*1024*1024:raise ValueError("unpacked APK limit")
            with z.open(i) as f:
                digest=hashlib.file_digest(f,"sha256").hexdigest()
            if not i.filename.startswith("META-INF/"):payload.append({"path":i.filename,"sha256":digest,"bytes":i.file_size})
            if i.filename.startswith("lib/"):
                if not re.fullmatch(r"lib/arm64-v8a/[^/]+\.so",i.filename):raise ValueError("unexpected native ABI")
                native.append({"path":i.filename,"sha256":digest,"loads":inspect(z.read(i))})
    if not native or "AndroidManifest.xml" not in names or not any(x["path"]=="lib/arm64-v8a/libflutter.so" for x in native):raise ValueError("incomplete Flutter APK")
    return {"native":sorted(native,key=lambda x:x["path"]),"payload":sorted(payload,key=lambda x:x["path"]),"unpacked_bytes":total}
def main():
    p=argparse.ArgumentParser();p.add_argument("--apk",type=pathlib.Path,required=True);p.add_argument("--tools",type=pathlib.Path,required=True);p.add_argument("--commit",required=True);p.add_argument("--out",type=pathlib.Path,required=True);p.add_argument("--kind",choices=["debug-candidate","release-candidate"],required=True);p.add_argument("--expected-cert");a=p.parse_args()
    if not re.fullmatch("[a-f0-9]{40}",a.commit):raise ValueError("commit")
    a.out.mkdir(parents=True,exist_ok=True)
    alignment=cmd(a.tools/"zipalign","-c","-P","16","-v","4",a.apk)
    signature=cmd(a.tools/"apksigner","verify","--verbose","--print-certs",a.apk)
    badging=cmd(a.tools/"aapt","dump","badging",a.apk)
    package=re.search(r"package: name='([^']+)' versionCode='([^']+)' versionName='([^']+)'",badging)
    if not package or package[1]!="org.openvietgis.openvietgis_mobile":raise ValueError("unexpected application ID")
    certs=re.findall(r"certificate SHA-256 digest: ([0-9a-fA-F]+)",signature)
    if not certs:raise ValueError("missing signer digest")
    if a.kind=="release-candidate" and (not a.expected_cert or [a.expected_cert.lower()]!=[c.lower() for c in certs] or "application-debuggable" in badging):raise ValueError("release signer/debug mismatch")
    report={"schema":1,"candidate_sha":a.commit,"profile":a.kind,"release_authorized":False,"apk_sha256":sha(a.apk),"apk_bytes":a.apk.stat().st_size,"package":package[1],"version_code":package[2],"version_name":package[3],"signer_sha256":certs,"zipalign_16k":"PASS","elf_load_alignment":"PASS","runtime_4k_16k":"NOT_TESTED",**inventory(a.apk)}
    for name,body in {"apk-report.json":json.dumps(report,indent=2),"signature.txt":signature,"zipalign.txt":alignment,"badging.txt":badging,"SHA256SUMS":report["apk_sha256"]+"  "+a.apk.name+"\n"}.items():(a.out/name).write_text(body)
if __name__=="__main__":main()
