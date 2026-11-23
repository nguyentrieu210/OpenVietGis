"""Shared non-lossy alley-prefix syntax for import, manual CRUD and numbering."""

from __future__ import annotations

import re

_SEGMENT = r"(?:[A-Z]+[1-9][0-9]*[A-Z]*|[1-9][0-9]*[A-Z]*)"
_PREFIX = re.compile(rf"{_SEGMENT}(?:/{_SEGMENT})*")


def canonical_prefix(value: str) -> str:
    """Preserve alphanumeric address components, canonicalize ASCII letters."""
    if not isinstance(value, str):
        raise TypeError("invalid_prefix")
    if not value.isascii():
        raise ValueError("invalid_prefix")
    value = value.strip().upper()
    if not value or len(value) > 100 or not _PREFIX.fullmatch(value):
        raise ValueError("invalid_prefix")
    return value
