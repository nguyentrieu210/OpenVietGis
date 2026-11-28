"""Compatibility patch for pinned FRB 2.11.1 CargoKit on Gradle 9.
Fail on unexpected upstream changes instead of silently patching arbitrary code."""
from pathlib import Path
import sys
p=Path(sys.argv[1])
s=p.read_text()
marker="abstract class CargoKitBuildTask extends DefaultTask {"
if s.count(marker)!=1 or s.count("project.exec {")!=2:
    raise SystemExit("unexpected CargoKit source")
s=s.replace("import java.nio.file.Paths","import java.nio.file.Paths\nimport javax.inject.Inject\nimport org.gradle.process.ExecOperations",1)
s=s.replace(marker,marker+"\n    @Inject\n    abstract ExecOperations getExecOperations()\n",1)
s=s.replace("project.exec {","execOperations.exec {")
# Respect requested ARM64 build; no implicit x86 expansion.
s=s.replace('if (buildType == "debug") {\n                platforms.add("android-x86")\n                platforms.add("android-x64")\n            }','')
p.write_text(s)
