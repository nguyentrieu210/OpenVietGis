from pathlib import Path
import sys
p=Path(sys.argv[1])
s=p.read_text()
marker="<application"
if s.count(marker)!=1:
    raise SystemExit("unexpected manifest template")
s=s.replace(marker,'<uses-permission android:name="android.permission.ACCESS_COARSE_LOCATION" />\n    <uses-permission android:name="android.permission.ACCESS_FINE_LOCATION" />\n    '+marker,1)
p.write_text(s)
