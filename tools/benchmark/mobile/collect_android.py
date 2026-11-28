"""Explicit disposable-device evidence collection; never claims full performance pass.
Requires installed candidate APK, USB debugging and a human-owned test device.
No reset, app-data clearing, network changes, or access to unrelated app data.
"""
import argparse,hashlib,json,pathlib,re,subprocess,time
p=argparse.ArgumentParser()
p.add_argument("--serial",required=True);p.add_argument("--apk",type=pathlib.Path,required=True)
p.add_argument("--report",type=pathlib.Path,required=True);p.add_argument("--out",type=pathlib.Path,required=True)
p.add_argument("--tier",choices=["low","mid"],required=True);p.add_argument("--fixture-sha",required=True)
p.add_argument("--fixture-features",type=int,choices=[10000,100000,1000000],required=True)
p.add_argument("--soak-minutes",type=int,default=30);a=p.parse_args()
if not 30<=a.soak_minutes<=120:raise SystemExit("soak minutes must be 30..120")
report=json.loads(a.report.read_text());pkg=report["package"]
with a.apk.open("rb") as f:sha=hashlib.file_digest(f,"sha256").hexdigest()
if sha!=report["apk_sha256"] or pkg!="org.openvietgis.openvietgis_mobile" or not re.fullmatch("[a-f0-9]{64}",a.fixture_sha):raise SystemExit("candidate/fixture mismatch")
a.out.mkdir(parents=True,exist_ok=False)
def adb(*args):
 return subprocess.check_output(["adb","-s",a.serial,*args],text=True,stderr=subprocess.STDOUT,timeout=45)
def save(name,*args):
 value=adb(*args);(a.out/name).write_text(value);return value
if adb("get-state").strip()!="device":raise SystemExit("device unavailable")
if adb("shell","getprop","ro.kernel.qemu").strip()=="1" or adb("shell","getprop","ro.boot.qemu").strip()=="1":raise SystemExit("physical device required")
abi=adb("shell","getprop","ro.product.cpu.abi").strip()
page=int(adb("shell","getconf","PAGE_SIZE").strip())
if abi!="arm64-v8a" or page not in [4096,16384]:raise SystemExit("wrong ABI/page size")
path=adb("shell","pm","path",pkg).strip().splitlines()
if len(path)!=1:raise SystemExit("expected single installed APK; split qualification separate")
# Verify installed package bytes rather than trusting a filename or version.
installed=path[0].removeprefix("package:")
device_hash=adb("shell","sha256sum",installed).split()[0]
if device_hash!=sha:raise SystemExit("installed APK digest mismatch")
model=adb("shell","getprop","ro.product.model").strip()
os_version=adb("shell","getprop","ro.build.version.release").strip()
save("display.txt","shell","dumpsys","display")
save("memory.txt","shell","cat","/proc/meminfo")
save("battery-before.txt","shell","dumpsys","battery")
save("thermal-before.txt","shell","dumpsys","thermalservice")
component=adb("shell","cmd","package","resolve-activity","--brief",pkg).strip().splitlines()[-1]
if not component.startswith(pkg+"/"):raise SystemExit("launcher not found")
launch=[]
for i in range(20):
 # Process-cold only: force-stop, not filesystem cold or freshly installed.
 adb("shell","am","force-stop",pkg)
 raw=save(f"launch-{i:02}.txt","shell","am","start","-W","-n",component)
 m=re.search(r"TotalTime:\s+(\d+)",raw)
 if not m or "Status: ok" not in raw:raise SystemExit("launch failed")
 launch.append(int(m[1]))
 time.sleep(1)
save("gfx-before.txt","shell","dumpsys","gfxinfo",pkg,"framestats")
input("On the disposable device, load the pinned fixture and begin the defined map/edit/track soak; press Enter when ready: ")
start=time.monotonic()
for i in range(a.soak_minutes+1):
 save(f"mem-{i:03}.txt","shell","dumpsys","meminfo",pkg)
 save(f"gfx-{i:03}.txt","shell","dumpsys","gfxinfo",pkg,"framestats")
 if i<a.soak_minutes:time.sleep(max(0,start+(i+1)*60-time.monotonic()))
save("battery-after.txt","shell","dumpsys","battery")
save("thermal-after.txt","shell","dumpsys","thermalservice")
summary={"candidate_sha":report["candidate_sha"],"apk_sha256":sha,"model":model,"os":os_version,"abi":abi,"page_size":page,"tier":a.tier,"fixture_sha256":a.fixture_sha,"fixture_features":a.fixture_features,"process_cold_launch_ms":launch,"soak_minutes":a.soak_minutes,"limitations":["am TotalTime is activity launch, not first usable map frame","gfxinfo may omit native MapLibre GPU frames","fixture load, gestures, snap, energy and RSS interpretation require independent review"],"status":"RAW_ONLY"}
(a.out/"capture.json").write_text(json.dumps(summary,indent=2))
