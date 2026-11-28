"""Restrict the generated Android app's JNI packaging to the qualified ARM64 ABI."""
import sys
from pathlib import Path

BLOCK = """    packaging {
        jniLibs {
            excludes += setOf("lib/armeabi/**", "lib/armeabi-v7a/**", "lib/x86/**", "lib/x86_64/**", "lib/mips/**", "lib/mips64/**", "lib/riscv64/**")
        }
    }
"""
def patch(source):
    marker = "    defaultConfig {\n"
    if source.count(marker) != 1:
        raise ValueError("unexpected Android app template")
    if BLOCK in source:
        return source
    if "jniLibs {" in source or "packaging {" in source:
        raise ValueError("existing packaging configuration requires review")
    return source.replace(marker, BLOCK + "\n" + marker, 1)

if __name__ == "__main__":
    path = Path(sys.argv[1])
    path.write_text(patch(path.read_text()))
