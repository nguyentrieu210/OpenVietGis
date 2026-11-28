"""Candidate dependency inventory from actual resolved Rust metadata and Dart pub deps.
Not a complete SBOM for Gradle, native SDKs or license compliance."""
import argparse,json,pathlib,hashlib,tomllib
def main():
 p=argparse.ArgumentParser();p.add_argument("--cargo",type=pathlib.Path,required=True);p.add_argument("--cargo-lock",type=pathlib.Path,required=True);p.add_argument("--dart",type=pathlib.Path,required=True);p.add_argument("--pub-lock",type=pathlib.Path,required=True);p.add_argument("--commit",required=True);p.add_argument("--out",type=pathlib.Path,required=True);a=p.parse_args()
 meta=json.loads(a.cargo.read_text());dart=json.loads(a.dart.read_text());lock=tomllib.loads(a.cargo_lock.read_text())
 checks={(x["name"],x["version"],x.get("source")):x.get("checksum") for x in lock["package"]}
 components=[];seen=set()
 for x in meta["packages"]:
  ref="cargo:"+x["id"]
  if ref in seen:raise ValueError("duplicate Rust id")
  seen.add(ref);c={"type":"library","name":x["name"],"version":x["version"],"bom-ref":ref,"properties":[{"name":"ovg:ecosystem","value":"cargo"}]}
  if x.get("license"):c["licenses"]=[{"license":{"name":x["license"]}}]
  h=checks.get((x["name"],x["version"],x.get("source")))
  if h:c["hashes"]=[{"alg":"SHA-256","content":h}]
  components.append(c)
 for x in dart["packages"]:
  ref="pub:"+x["name"]+"@"+x["version"]
  if ref in seen:raise ValueError("duplicate Dart id")
  seen.add(ref);components.append({"type":"library","name":x["name"],"version":x["version"],"bom-ref":ref,"properties":[{"name":"ovg:ecosystem","value":"pub"}]})
 dependencies=[{"ref":"cargo:"+n["id"],"dependsOn":["cargo:"+d for d in n["dependencies"]]} for n in (meta.get("resolve") or {}).get("nodes",[])]
 pubrefs={x["name"]:"pub:"+x["name"]+"@"+x["version"] for x in dart["packages"]}
 for x in dart["packages"]:
  dependencies.append({"ref":pubrefs[x["name"]],"dependsOn":[pubrefs[d] for d in x.get("dependencies",[]) if d in pubrefs]})
 out={"bomFormat":"CycloneDX","specVersion":"1.5","version":1,"metadata":{"component":{"type":"application","name":"OpenVietGIS mobile candidate","version":a.commit},"properties":[{"name":"ovg:coverage","value":"partial; resolved cargo and pub only; native/Gradle/license audit pending"},{"name":"ovg:cargo-lock-sha256","value":hashlib.sha256(a.cargo_lock.read_bytes()).hexdigest()},{"name":"ovg:pub-lock-sha256","value":hashlib.sha256(a.pub_lock.read_bytes()).hexdigest()}]},"components":components,"dependencies":dependencies}
 a.out.write_text(json.dumps(out,indent=2))
if __name__=="__main__":main()
