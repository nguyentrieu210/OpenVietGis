"""Install reviewed native key provider into generated FRB host (Android/iOS)."""
import re
import shutil
import sys
import tomllib
from pathlib import Path

repo = Path(sys.argv[1]).resolve()
host = Path(sys.argv[2]).resolve()
rust = host / "rust"
manifest = rust / "Cargo.toml"
text = manifest.read_text()
if "ovg-project-store" not in text:
    raise SystemExit("shared store dependency missing")
for dep in ('zeroize = "=1.8.1"', 'chacha20poly1305 = "=0.10.1"', 'x25519-dalek = { version = "=2.0.1", features = ["static_secrets"] }', 'hkdf = "=0.12.4"', 'sha2 = "0.10"'):
    if dep not in text:
        text = text.replace("[dependencies]", "[dependencies]\n" + dep, 1)
if "[target.'cfg(target_os = \"android\")'.dependencies]" not in text:
    text += "\n[target.'cfg(target_os = \"android\")'.dependencies]\njni = \"=0.21.1\"\n"
manifest.write_text(text)
shutil.copyfile(repo / "crates/ovg-mobile-ffi/src/media_keys.rs", rust / "src/media_keys.rs")
shutil.copyfile(repo / "crates/ovg-mobile-ffi/src/media_transfer.rs", rust / "src/media_transfer.rs")
lib = rust / "src/lib.rs"
s = lib.read_text()
for module in ("media_keys", "media_transfer"):
    marker=f"mod {module};"
    if marker not in s:
        s=marker+"\n"+s
lib.write_text(s)
config = tomllib.loads(text)
name = config.get("lib", {}).get("name", config["package"]["name"].replace("-", "_"))
if not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", name):
    raise SystemExit("unexpected native library name")
src = host / "android/app/src/main/kotlin"
if src.exists():
    activities = list(src.rglob("MainActivity.kt"))
    if len(activities) != 1:
        raise SystemExit("unexpected activity template")
    p = activities[0]
    source = p.read_text()
    package = re.search(r"(?m)^package ([A-Za-z0-9_.]+)\s*$", source)
    if not package or not re.search(r"class MainActivity\s*:\s*FlutterActivity\(\)\s*\{\s*\}", source):
        raise SystemExit("activity template changed")
    p.write_text(f"""package {package.group(1)}

import android.content.Intent
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import org.openvietgis.secure.MediaKeys

class MainActivity : FlutterActivity() {{
    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {{
        super.configureFlutterEngine(flutterEngine)
        MediaKeys.attach(this)
        try {{
            System.loadLibrary("{name}")
            MediaKeys(applicationContext).install()
        }} catch (_: LinkageError) {{
            // Provider remains unavailable, never a fallback key.
        }} catch (_: Exception) {{
            // Provider remains unavailable.
        }}
    }}
    override fun onStop() {{
        MediaKeys.dismissPreview()
        super.onStop()
    }}
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {{
        if (!MediaKeys.cameraResult(requestCode, resultCode, data)) super.onActivityResult(requestCode, resultCode, data)
    }}
}}
""")
    dst = src / "org/openvietgis/secure/MediaKeys.kt"
    dst.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(repo / "infra/mobile/native/media/MediaKeys.kt", dst)
    shutil.copyfile(repo / "infra/mobile/native/media/CameraCaptureActivity.kt", dst.parent / "CameraCaptureActivity.kt")
    import xml.etree.ElementTree as ET
    ns="http://schemas.android.com/apk/res/android"
    ET.register_namespace("android",ns)
    m=host/"android/app/src/main/AndroidManifest.xml"
    tree=ET.parse(m); manifest=tree.getroot(); app=manifest.find("application")
    attr=lambda key: "{"+ns+"}"+key
    if app is None: raise SystemExit("application missing")
    if not any(x.get(attr("name"))=="android.permission.CAMERA" for x in manifest.findall("uses-permission")):
        ET.SubElement(manifest,"uses-permission",{attr("name"):"android.permission.CAMERA"})
    ET.SubElement(manifest,"uses-feature",{attr("name"):"android.hardware.camera",attr("required"):"false"})
    ET.SubElement(app,"activity",{attr("name"):"org.openvietgis.secure.CameraCaptureActivity",attr("exported"):"false",attr("screenOrientation"):"portrait"})
    tree.write(m,encoding="utf-8",xml_declaration=True)
    gradle=host/"android/app/build.gradle.kts"
    if not gradle.is_file(): raise SystemExit("expected Kotlin Gradle template")
    deps=["androidx.activity:activity:1.9.3"]+["androidx.camera:camera-"+part+":1.4.2" for part in ("core","camera2","lifecycle","view")]
    with gradle.open("a") as f:
        f.write("\n dependencies {\n"+"\n".join(f\'    implementation("{d}")\' for d in deps)+"\n}\n")
# JNI method is name-bound; keep in minified Android release.
rules = host / "android/app/proguard-rules.pro"
if rules.parent.exists():
    with rules.open("a") as f:
        f.write("\n-keep class org.openvietgis.secure.MediaKeys { *; }\n")
