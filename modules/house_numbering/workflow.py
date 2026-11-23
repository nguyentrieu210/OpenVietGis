"""Independent House Numbering reference workflow (strictly outside Core)."""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

MANIFEST_PATH = Path(__file__).with_name("manifest.json")


def load_manifest() -> dict[str, Any]:
    return dict(json.loads(MANIFEST_PATH.read_text(encoding="utf-8")))


def assign_even_odd_sequence(
    *,
    street_name: str,
    side: str,
    start_number: int,
    count: int,
    ward_code: str = "W01",
) -> list[dict[str, Any]]:
    """Deterministic street-side odd/even address plate generator."""
    step = 2
    num = start_number
    if (side == "left" and num % 2 == 0) or (side == "right" and num % 2 == 1):
        num += 1
    plates: list[dict[str, Any]] = []
    for i in range(count):
        plates.append(
            {
                "street_name": street_name,
                "house_number": str(num + i * step),
                "side": side,
                "ward_code": ward_code,
                "verified": True,
            }
        )
    return plates
