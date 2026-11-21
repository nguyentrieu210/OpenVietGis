"""Pure domain rules for the official House Numbering plugin.

Inputs are already scoped to one route and measured in its declared projected
CRS/orientation. This library has no database, identity or Core dependencies.
It proposes values; persistence/authorization must revalidate at apply.
"""

from __future__ import annotations

import itertools
import math
import re
from collections.abc import Iterable
from dataclasses import dataclass


class DomainError(ValueError):
    """User-actionable invalid or conflicting domain input."""


_SIDES = {"left", "right"}
_STATES = {
    "old",
    "certificate",
    "cad",
    "survey",
    "proposed",
    "submitted",
    "approved",
    "reserved",
}
_LOCKED = {"certificate", "submitted", "approved", "reserved"}
_NUMBER = re.compile(r"([1-9][0-9]*)(?:([A-Z]+)|-([1-9][0-9]*))?")
_PREFIX = re.compile(r"[1-9][0-9]*(?:/[1-9][0-9]*)*")


def _finite(value: float, *, name: str) -> None:
    if (
        isinstance(value, bool)
        or not isinstance(value, (int, float))
        or not math.isfinite(value)
    ):
        raise DomainError(f"{name} must be a finite number")


@dataclass(frozen=True, slots=True)
class Rule:
    version: str
    left_odd: bool = True
    suffix_style: str = "letters"
    max_distance: float = 150.0
    authority: str | None = None

    def __post_init__(self) -> None:
        if not isinstance(self.version, str) or not self.version.strip():
            raise DomainError("rule version required")
        if not isinstance(self.left_odd, bool) or self.suffix_style not in {
            "letters",
            "dash",
        }:
            raise DomainError("unknown numbering policy")
        _finite(self.max_distance, name="max_distance")
        if self.max_distance <= 0:
            raise DomainError("max_distance must be positive")
        if self.authority is not None and (
            not isinstance(self.authority, str) or not self.authority.strip()
        ):
            raise DomainError("authority must be a nonempty reference")

    def parity(self, side: str) -> int:
        if side not in _SIDES:
            raise DomainError("side must be left or right")
        return int(self.left_odd if side == "left" else not self.left_odd)


@dataclass(frozen=True, slots=True)
class Address:
    id: str
    number: str
    measure: float
    side: str
    state: str = "proposed"

    def __post_init__(self) -> None:
        if not isinstance(self.id, str) or not self.id.strip():
            raise DomainError("address ID required")
        if not isinstance(self.number, str) or not self.number.strip():
            raise DomainError("number must preserve nonempty raw text")
        _finite(self.measure, name="measure")
        if self.measure < 0 or self.side not in _SIDES or self.state not in _STATES:
            raise DomainError("invalid address measure, side or state")


@dataclass(frozen=True, slots=True)
class Suggestion:
    number: str | None
    reason: str
    rule_version: str


def _letters_rank(suffix: str) -> int:
    n = 0
    for letter in suffix:
        n = n * 26 + ord(letter) - ord("A") + 1
    return n


def _rank_letters(rank: int) -> str:
    result = ""
    while rank:
        rank, rem = divmod(rank - 1, 26)
        result = chr(ord("A") + rem) + result
    return result


def _prefix(prefix: str) -> str:
    if prefix and not _PREFIX.fullmatch(prefix):
        raise DomainError("invalid alley prefix")
    return prefix + "/" if prefix else ""


def _parse(number: str, prefix: str, style: str) -> tuple[int, int] | None:
    if not number.startswith(prefix):
        return None
    match = _NUMBER.fullmatch(number[len(prefix) :])
    if not match:
        return None
    base, letters, dash = match.groups()
    if letters and style != "letters" or dash and style != "dash":
        return None
    # Bound parsed text so a hostile input cannot create arbitrarily large ints.
    if len(base) > 12 or dash and len(dash) > 9 or letters and len(letters) > 6:
        return None
    rank = _letters_rank(letters) if letters else int(dash) if dash else 0
    return int(base), rank


def _format(base: int, rank: int, prefix: str, style: str) -> str:
    suffix = (
        "" if rank == 0 else _rank_letters(rank) if style == "letters" else f"-{rank}"
    )
    return f"{prefix}{base}{suffix}"


def _rows(values: Iterable[Address]) -> tuple[Address, ...]:
    rows = tuple(values)
    if len({row.id for row in rows}) != len(rows):
        raise DomainError("duplicate address ID")
    return rows


def suggest_number(
    addresses: Iterable[Address],
    *,
    measure: float,
    side: str,
    rule: Rule,
    prefix: str = "",
    distance: float = 0.0,
    existing_id: str | None = None,
) -> Suggestion:
    """Propose a route-side number without modifying any input.

    Reserved slots occupy numbers but are not physical neighbor anchors.
    Ambiguous or conflicting anchor sequences require manual resolution.
    """
    parity = rule.parity(side)
    _finite(measure, name="measure")
    _finite(distance, name="distance")
    if measure < 0 or distance < 0:
        raise DomainError("measure and distance must be nonnegative")
    prefix_text = _prefix(prefix)
    rows = _rows(addresses)

    def result(number: str | None, reason: str) -> Suggestion:
        return Suggestion(number, reason, rule.version)

    if existing_id is not None:
        existing = next((row for row in rows if row.id == existing_id), None)
        if existing is None:
            raise DomainError("unknown existing address ID")
        return result(existing.number, "existing_number")
    if distance > rule.max_distance:
        return result(None, "outside_route_distance")

    anchors = sorted(
        (r for r in rows if r.side == side and r.state != "reserved"),
        key=lambda r: (r.measure, r.id),
    )
    parsed: list[tuple[Address, tuple[int, int]]] = []
    for row in anchors:
        value = _parse(row.number, prefix_text, rule.suffix_style)
        if value is None or value[0] % 2 != parity:
            return result(None, "ambiguous_or_inconsistent_anchor")
        parsed.append((row, value))
    for (a, av), (b, bv) in itertools.pairwise(parsed):
        if a.measure >= b.measure or av >= bv:
            return result(None, "nonmonotonic_or_duplicate_anchor")
    if any(row.measure == measure for row, _ in parsed):
        return result(None, "ambiguous_location")

    previous = next((v for r, v in reversed(parsed) if r.measure < measure), None)
    following = next((v for r, v in parsed if r.measure > measure), None)
    # Route-wide occupancy, even if a record has incorrectly assigned side.
    occupied = {r.number for r in rows}

    def free_base(start: int, stop: int | None = None) -> str | None:
        candidate = start
        # At most one occupied value per row, so bounded iterations suffice.
        for _ in range(len(occupied) + 1):
            if candidate <= 0 or stop is not None and candidate >= stop:
                return None
            number = _format(candidate, 0, prefix_text, rule.suffix_style)
            if number not in occupied:
                return number
            candidate += 2
        return None

    if previous is None and following is None:
        return result(free_base(1 if parity else 2), "empty_route_side")
    if previous is None:
        assert following is not None
        candidate = following[0] - 2
        while candidate > 0:
            number = _format(candidate, 0, prefix_text, rule.suffix_style)
            if number not in occupied:
                return result(number, "before_first")
            candidate -= 2
        return result(None, "no_number_before_first")
    if following is None:
        return result(free_base(previous[0] + 2), "after_last")

    gap = free_base(previous[0] + 2, following[0])
    if gap:
        return result(gap, "unused_gap")
    # Existing suffix can be followed by the next suffix on the same base only
    # when it remains strictly before the next physical anchor.
    rank = previous[1] + 1
    for _ in range(len(occupied) + 1):
        if (previous[0], rank) >= following:
            return result(None, "no_slot_between_anchors")
        number = _format(previous[0], rank, prefix_text, rule.suffix_style)
        if number not in occupied:
            return result(number, "subnumber")
        rank += 1
    return result(None, "no_free_slot")


def preview_shift(
    addresses: Iterable[Address],
    *,
    start_measure: float,
    side: str,
    rule: Rule,
    prefix: str = "",
    steps: int = 1,
) -> dict[str, str]:
    """Preview a shift of proposed numbers only, fail atomically on conflicts."""
    parity = rule.parity(side)
    _finite(start_measure, name="start_measure")
    if (
        start_measure < 0
        or isinstance(steps, bool)
        or not isinstance(steps, int)
        or steps == 0
    ):
        raise DomainError("invalid shift operation")
    prefix_text = _prefix(prefix)
    rows = _rows(addresses)
    changed: dict[str, str] = {}
    for row in rows:
        if row.side != side or row.measure < start_measure or row.state != "proposed":
            continue
        parsed = _parse(row.number, prefix_text, rule.suffix_style)
        if parsed is None or parsed[0] % 2 != parity:
            raise DomainError("cannot shift inconsistent raw number")
        base, rank = parsed
        newbase = base + steps * 2
        if newbase <= 0:
            raise DomainError("shift crosses first house number")
        changed[row.id] = _format(newbase, rank, prefix_text, rule.suffix_style)
    effective = [changed.get(r.id, r.number) for r in rows]
    if len(effective) != len(set(effective)):
        raise DomainError("shift would duplicate or overwrite occupied number")
    previous: tuple[int, int] | None = None
    prevmeasure: float | None = None
    for row in sorted(
        (r for r in rows if r.side == side and r.state != "reserved"),
        key=lambda r: r.measure,
    ):
        value = _parse(changed.get(row.id, row.number), prefix_text, rule.suffix_style)
        if (
            value is None
            or value[0] % 2 != parity
            or (
                previous is not None
                and (value <= previous or row.measure == prevmeasure)
            )
        ):
            raise DomainError("shift would violate route anchor order")
        previous, prevmeasure = value, row.measure
    return changed


def classify_reconciliation(
    *,
    certificate: str | None,
    cad: str | None,
    survey: str | None,
    surveyed: bool,
    exists: bool = True,
    building_linked: bool = True,
) -> str:
    """Return one deterministic building category; unmatched cert is separate."""
    if not building_linked:
        if not certificate:
            raise DomainError("unmatched case requires certificate evidence")
        return "certificate_unmatched"
    if not surveyed and (survey is not None or not exists):
        raise DomainError("survey result requires a survey")
    if surveyed and not exists:
        return "no_longer_exists"
    if surveyed and not certificate and not cad:
        return "new_building"
    if certificate and cad and certificate != cad:
        return "cad_certificate_mismatch"
    if not surveyed:
        return "not_surveyed"
    if survey is None or survey != (certificate or cad):
        return "number_mismatch"
    return "matched"


def proposal_transition(
    state: str,
    action: str,
    *,
    reviewer: bool = False,
    comment: str | None = None,
    rule: Rule | None = None,
    eligible: bool = True,
) -> str:
    """Evaluate legal state transitions; adapter must enforce actor/revision/unique constraints."""
    if action == "submit" and state == "draft":
        if not eligible:
            raise DomainError("ineligible building cannot enter proposal")
        return "submitted"
    if state == "submitted" and action == "reject":
        if not reviewer or not comment or not comment.strip():
            raise DomainError("rejection requires reviewer and reason")
        return "draft"
    if state == "submitted" and action == "approve":
        if not reviewer or rule is None or not rule.authority or not eligible:
            raise DomainError(
                "approval requires reviewer, authoritative rule and eligibility"
            )
        return "approved"
    raise DomainError(f"invalid proposal transition: {state}/{action}")


@dataclass(frozen=True, slots=True)
class RouteLocation:
    measure: float
    distance: float
    side: str | None
    crs: str


def locate_on_route(
    point: tuple[float, float], vertices: Iterable[tuple[float, float]], *, crs: str
) -> RouteLocation:
    """Locate in a caller-verified projected metre CRS, not raw lat/lon.

    A declared CRS is preserved for provenance; verifying the actual CRS and
    transformation belongs to the Core adapter. Tied projections deliberately
    have no side. No nearest-route decision is made here.
    """
    if not isinstance(crs, str) or not crs.strip():
        raise DomainError("verified projected CRS required")
    points = tuple(vertices)
    if len(points) < 2:
        raise DomainError("route needs at least two vertices")
    for x, y in (point, *points):
        _finite(x, name="x")
        _finite(y, name="y")
    px, py = point
    best: RouteLocation | None = None
    cumulative = 0.0
    for (ax, ay), (bx, by) in itertools.pairwise(points):
        dx, dy = bx - ax, by - ay
        length = math.hypot(dx, dy)
        if length <= 0 or not math.isfinite(length):
            raise DomainError("route contains invalid or repeated vertices")
        unit_x, unit_y = dx / length, dy / length
        along = max(0.0, min(length, (px - ax) * unit_x + (py - ay) * unit_y))
        qx, qy = ax + along * unit_x, ay + along * unit_y
        distance = math.hypot(px - qx, py - qy)
        cross = unit_x * (py - ay) - unit_y * (px - ax)
        side = None if abs(cross) <= 1e-8 else "left" if cross > 0 else "right"
        candidate = RouteLocation(cumulative + along, distance, side, crs)
        if best is None or distance < best.distance - 1e-8:
            best = candidate
        elif abs(distance - best.distance) <= 1e-8 and (
            abs(candidate.measure - best.measure) > 1e-8 or candidate.side != best.side
        ):
            best = RouteLocation(best.measure, best.distance, None, crs)
        cumulative += length
    if best is None:
        raise DomainError("no usable route")
    return best


@dataclass(frozen=True, slots=True)
class EvidenceCandidate:
    building_id: str
    route_id: str
    old_number: str | None
    new_number: str | None


@dataclass(frozen=True, slots=True)
class EvidenceMatch:
    building_id: str | None
    reason: str


def match_certificate(
    candidates: Iterable[EvidenceCandidate],
    *,
    route_id: str,
    old_number: str | None,
    new_number: str | None,
) -> EvidenceMatch:
    """Match within an already authorized route; ambiguous old number never falls through."""
    if not route_id:
        raise DomainError("route required")
    rows = tuple(row for row in candidates if row.route_id == route_id)
    for field, number in (("old_number", old_number), ("new_number", new_number)):
        if not number:
            continue
        ids = {r.building_id for r in rows if getattr(r, field) == number}
        if len(ids) == 1:
            return EvidenceMatch(next(iter(ids)), f"unique_{field}")
        if len(ids) > 1:
            return EvidenceMatch(None, f"ambiguous_{field}")
    return EvidenceMatch(None, "no_match")


@dataclass(frozen=True, slots=True)
class SurveyResult:
    building_id: str
    condition: str
    state: str


def summarize_progress(
    surveys: Iterable[SurveyResult],
    *,
    target: int | None,
) -> dict[str, int | float | None]:
    """Aggregate accepted latest-per-building results; conflicting snapshots fail."""
    if target is not None and (
        isinstance(target, bool) or not isinstance(target, int) or target < 0
    ):
        raise DomainError("target must be a nonnegative integer or absent")
    accepted: dict[str, str] = {}
    for row in surveys:
        if not row.building_id or row.condition not in {"house", "vacant", "other"}:
            raise DomainError("invalid survey result")
        if row.state not in {"accepted", "draft", "submitted", "rejected"}:
            raise DomainError("invalid survey state")
        if row.state != "accepted":
            continue
        if row.building_id in accepted and accepted[row.building_id] != row.condition:
            raise DomainError(
                "conflicting accepted snapshots; resolve latest before metrics"
            )
        accepted[row.building_id] = row.condition
    values = tuple(accepted.values())
    return {
        "positions": len(values),
        "houses": values.count("house"),
        "vacant": values.count("vacant"),
        "other": values.count("other"),
        "completion": len(values) / target * 100 if target else None,
    }


def validate_import_counts(read: int, accepted: int, rejected: int) -> int:
    if any(
        isinstance(n, bool) or not isinstance(n, int) or n < 0
        for n in (read, accepted, rejected)
    ):
        raise DomainError("import counts must be nonnegative integers")
    if read != accepted + rejected:
        raise DomainError("import counts must reconcile")
    return read


_SAFE_EXPORT_FIELDS = frozenset(
    {
        "building_id",
        "house_number",
        "old_number",
        "proposed_number",
        "approved_number",
        "route_name",
        "area_name",
        "parcel_number",
        "sheet_number",
        "state",
        "crs",
    }
)


def public_export_record(record: dict[str, object]) -> dict[str, object]:
    """Allowlist flat fields only. Upstream provenance and output leak scan still required."""
    return {
        key: value
        for key, value in record.items()
        if key in _SAFE_EXPORT_FIELDS
        and (value is None or isinstance(value, str) or type(value) is int)
    }


def certificate_issuance(state: str, *, template_verified: bool, rule: Rule) -> str:
    """Pure status gate, not an authorization or document-generation operation."""
    if state != "approved":
        raise DomainError("certificate requires approved number")
    return "official" if template_verified and rule.authority else "draft"
