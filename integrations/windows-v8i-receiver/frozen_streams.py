"""PyInstaller windowed runtime hook, before COM/PyWin32 runtime hooks.

Windows --windowed executables have no stdio streams. Some native import
hooks expect writable streams even when no user console exists.
"""
from __future__ import annotations

import os
import sys

if sys.stdout is None:
    sys.stdout = open(os.devnull, "w", encoding="utf-8")
if sys.stderr is None:
    sys.stderr = open(os.devnull, "w", encoding="utf-8")