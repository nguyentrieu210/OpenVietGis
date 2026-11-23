"""Non-destructive, route-scoped audit of numbering proposals and existing evidence.

An unused number is a gap to review, never a directive to renumber or invent a house.
Route prefix and direction are supplied by the same snapshot as batch numbering.
"""

from __future__ import annotations

from collections import defaultdict
from typing import Any

from modules.house_numbering.domain import Rule, _parse, _prefix, locate_on_route

MAX_DETAILS = 40


def audit_route_numbering(
    *,
    route: dict[str, Any],
    vertices: list[tuple[float, float]],
    rule: Rule,
    points: list[dict[str, Any]],
    numbers: list[dict[str, Any]],
    changes: list[dict[str, Any]],
) -> dict[str, Any]:
    prefix = _prefix(str(route.get("prefix") or ""))
    route_id = str(route["id"])
    occupied: dict[str, list[dict[str, str | None]]] = defaultdict(list)
    anchors: dict[str, list[dict[str, Any]]] = {"left": [], "right": []}
    parity_errors: list[dict[str, Any]] = []
    invalid_formats: list[dict[str, Any]] = []
    ordering_errors: list[dict[str, Any]] = []
    gaps: list[dict[str, Any]] = []
    number_ids = {str(n["id"]): n for n in numbers}

    def check(
        number: str, side: str | None, origin: str, bid: str | None
    ) -> tuple[int, int] | None:
        parsed = _parse(number, prefix, rule.suffix_style)
        if parsed is None:
            invalid_formats.append(
                {"number": number, "building_id": bid, "origin": origin}
            )
        elif side in ("left", "right") and parsed[0] % 2 != rule.parity(side):
            parity_errors.append(
                {"number": number, "building_id": bid, "side": side, "origin": origin}
            )
        return parsed

    for n in numbers:
        number = str(n["number_text"])
        bid = str(n["building_id"]) if n.get("building_id") else None
        occupied[number].append(
            {"origin": "existing", "building_id": bid, "number_id": str(n["id"])}
        )
        # Reservations without a physical house have no trustworthy left/right geometry.
        if n.get("kind") != "reserved":
            check(number, n.get("side"), "existing", bid)

    primary: set[str] = set()
    for point in points:
        number_id = str(point.get("number_id") or "")
        if number_id not in number_ids or point.get("number_text") is None:
            continue
        n = number_ids[number_id]
        if n.get("kind") == "reserved":
            continue
        loc = locate_on_route(
            (float(point["x"]), float(point["y"])),
            vertices,
            crs=f"EPSG:{route['srid']}",
        )
        if loc.side not in anchors or loc.distance > rule.max_distance:
            continue
        # Use the actual route projection; stored measure_m may be obsolete.
        number = str(n["number_text"])
        parsed = _parse(number, prefix, rule.suffix_style)
        if parsed:
            if parsed[0] % 2 != rule.parity(loc.side) and not any(
                x["number"] == number and x["building_id"] == str(point["id"])
                for x in parity_errors
            ):
                parity_errors.append(
                    {
                        "number": number,
                        "building_id": str(point["id"]),
                        "side": loc.side,
                        "origin": "geometry",
                    }
                )
            anchors[loc.side].append(
                {
                    "number": number,
                    "building_id": str(point["id"]),
                    "measure": loc.measure,
                    "parsed": parsed,
                    "origin": "existing",
                }
            )
        primary.add(number_id)

    for change in changes:
        number = str(change["number"])
        bid = str(change["building_id"])
        # Historical evidence is not a new proposal and must never become one.
        if change.get("reason") == "existing_number":
            continue
        occupied[number].append(
            {"origin": "proposal", "building_id": bid, "number_id": None}
        )
        parsed = check(number, change.get("side"), "proposal", bid)
        if parsed and change.get("side") in anchors:
            anchors[change["side"]].append(
                {
                    "number": number,
                    "building_id": bid,
                    "measure": float(change["measure"]),
                    "parsed": parsed,
                    "origin": "proposal",
                }
            )

    duplicates = [
        {"number": number, "occupants": entries}
        for number, entries in sorted(occupied.items())
        if len(entries) > 1
    ]

    # A vacant *numeric slot* is informational: it may be reserved for an entrance,
    # parcel split or existing unregistered house. Never fill it automatically.
    occupied_bases = {
        parsed[0]
        for number in occupied
        if (parsed := _parse(number, prefix, rule.suffix_style))
    }
    gap_total = 0
    for side, values in anchors.items():
        values.sort(key=lambda x: (x["measure"], x["number"], x["building_id"]))
        prev: dict[str, Any] | None = None
        parity = rule.parity(side)
        for anchor in values:
            if prev is not None:
                if (
                    prev["measure"] >= anchor["measure"] - 1e-6
                    or prev["parsed"] >= anchor["parsed"]
                ):
                    ordering_errors.append(
                        {
                            "side": side,
                            "before": prev["number"],
                            "after": anchor["number"],
                            "before_building_id": prev["building_id"],
                            "after_building_id": anchor["building_id"],
                        }
                    )
                else:
                    left, right = prev["parsed"][0], anchor["parsed"][0]
                    count = max(0, (right - left) // 2 - 1)
                    used = sorted(
                        v
                        for v in occupied_bases
                        if v % 2 == parity and left < v < right
                    )
                    count = max(0, count - len(used))
                    if count:
                        gap_total += count
                        gaps.append(
                            {
                                "side": side,
                                "from_number": prev["number"],
                                "to_number": anchor["number"],
                                "unused_slots": count,
                            }
                        )
            prev = anchor
    # Existing evidence with inconsistent parity/sequence must not be renumbered.
    blockers = (
        len(duplicates)
        + len(parity_errors)
        + len(invalid_formats)
        + len(ordering_errors)
    )
    return {
        "route_id": route_id,
        "existing": len(numbers),
        "proposed": len(changes),
        "duplicate_count": len(duplicates),
        "parity_error_count": len(parity_errors),
        "invalid_format_count": len(invalid_formats),
        "ordering_error_count": len(ordering_errors),
        "gap_count": gap_total,
        "gap_ranges": len(gaps),
        "blocking_issues": blockers,
        "duplicates": duplicates[:MAX_DETAILS],
        "parity_errors": parity_errors[:MAX_DETAILS],
        "invalid_formats": invalid_formats[:MAX_DETAILS],
        "ordering_errors": ordering_errors[:MAX_DETAILS],
        "gaps": gaps[:MAX_DETAILS],
    }


def summarize_audit(rows: list[dict[str, Any]]) -> dict[str, Any]:
    keys = (
        "existing",
        "proposed",
        "duplicate_count",
        "parity_error_count",
        "invalid_format_count",
        "ordering_error_count",
        "gap_count",
        "gap_ranges",
        "blocking_issues",
    )
    summary = {k: sum(row[k] for row in rows) for k in keys}
    summary["routes"] = len(rows)
    summary["details"] = rows
    summary["can_save"] = summary["blocking_issues"] == 0
    return summary


def find_tree_collisions(
    parent_by_id: dict[str, str | None], occupants: list[dict[str, Any]]
) -> list[dict[str, Any]]:
    """Same full number on different logical routes under one street root.

    The same number on unrelated streets is valid. Every row has route_id,
    number_text, building_id and origin; the caller supplies full tree ancestry.
    """
    roots: dict[str, str] = {}

    def root(rid: str) -> str:
        if rid in roots:
            return roots[rid]
        seen: set[str] = set()
        current = rid
        while parent_by_id.get(current) is not None:
            if current in seen:
                return rid  # invalid graph is handled by route-tree validation
            seen.add(current)
            current = str(parent_by_id[current])
        roots[rid] = current
        return current

    groups: dict[tuple[str, str], list[dict[str, Any]]] = defaultdict(list)
    for x in occupants:
        rid, number = str(x["route_id"]), str(x["number_text"])
        groups[(root(rid), number)].append(
            {
                "route_id": rid,
                "building_id": str(x["building_id"]) if x.get("building_id") else None,
                "origin": x["origin"],
            }
        )
    return [
        {"root_route_id": scope, "number": number, "occupants": values}
        for (scope, number), values in sorted(groups.items())
        if len({v["route_id"] for v in values}) > 1
    ]
