"""Align the pinned FRB Android library with the installed Android SDK.

Only change compileSdk, not minSdk, targetSdk, or dependency validation.
Reject a changed upstream template rather than guessing.
"""
import re
import sys
from pathlib import Path

def patch(source):
    pattern = r"(?m)^([ \t]*compileSdkVersion[ \t]+)(33|36)([ \t]*)$"
    matches = list(re.finditer(pattern, source))
    if len(matches) != 1:
        raise ValueError("unexpected FRB compileSdk declaration")
    return re.sub(pattern, r"\g<1>36\g<3>", source, count=1)

def main():
    path = Path(sys.argv[1])
    path.write_text(patch(path.read_text()))

if __name__ == "__main__":
    main()
