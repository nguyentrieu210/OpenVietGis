"""Narrow loopback transport policy for the explicit MVT route.

Never enable global cleartext loads. Generated host only; fail on conflict.
"""
from pathlib import Path
import plistlib
import sys
import xml.etree.ElementTree as ET

root=Path(sys.argv[1])
android=root/"android/app/src/main/AndroidManifest.xml"
if android.exists():
    ns="http://schemas.android.com/apk/res/android"
    ET.register_namespace("android",ns)
    tree=ET.parse(android)
    app=tree.getroot().find("application")
    if app is None:
        raise SystemExit("missing application")
    attr="{"+ns+"}networkSecurityConfig"
    old=app.get(attr)
    if old not in (None,"@xml/ovg_loopback_security"):
        raise SystemExit("existing network security policy requires review")
    if app.get("{"+ns+"}usesCleartextTraffic")=="true":
        raise SystemExit("global cleartext forbidden")
    app.set(attr,"@xml/ovg_loopback_security")
    dest=android.parent/"res/xml/ovg_loopback_security.xml"
    dest.parent.mkdir(parents=True,exist_ok=True)
    dest.write_text("""<?xml version="1.0" encoding="utf-8"?>
<network-security-config>
  <base-config cleartextTrafficPermitted="false" />
  <domain-config cleartextTrafficPermitted="true">
    <domain includeSubdomains="false">127.0.0.1</domain>
  </domain-config>
</network-security-config>
""")
    tree.write(android,encoding="utf-8",xml_declaration=True)
ios=root/"ios/Runner/Info.plist"
if ios.exists():
    data=plistlib.loads(ios.read_bytes())
    ats=data.setdefault("NSAppTransportSecurity",{})
    if ats.get("NSAllowsArbitraryLoads") is True:
        raise SystemExit("global ATS exception forbidden")
    # Apple's local networking exception covers local hosts/IP addresses.
    # Actual server still listens only on 127.0.0.1 with a per-route capability.
    ats["NSAllowsLocalNetworking"]=True
    ios.write_bytes(plistlib.dumps(data))
