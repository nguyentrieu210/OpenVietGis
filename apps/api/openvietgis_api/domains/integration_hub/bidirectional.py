"""R027: deterministic, side-effect-free three-way GIS reconciliation.

This planner never performs I/O or grants write authority. Its callers MUST
supply complete, authorization-filtered and revision-pinned snapshots from a
verified binding, then obtain a new authorization/revision check before each
conditional R006 or external-agent write.
"""

from __future__ import annotations

import hashlib
import json
from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from typing import Any, Final, Literal

from openvietgis_api.domains.integration_hub.sync_diff import (
    _normalize_geometry,
    _values_equal,
)

Side = Literal["openvietgis", "external"]
MAX_RECONCILE_FEATURES: Final[int] = 25_000
MAX_RECONCILE_ACTIONS: Final[int] = 1_000


@dataclass(frozen=True, slots=True)
class FeatureSnapshot:
    feature_id: str
    attributes: Mapping[str, Any]
    geometry_geojson: Mapping[str, Any]


@dataclass(frozen=True, slots=True)
class ReconcileAction:
    target_side: Side
    feature_id: str
    action: Literal["create", "update"]
    # Authorized projection only. A receiver merges allowed fields with its
    # existing *unauthorized* fields, which must never be overwritten.
    attributes: dict[str, Any]
    geometry_geojson: dict[str, Any]
    changed_fields: tuple[str, ...]
    geometry_changed: bool


@dataclass(frozen=True, slots=True)
class ReconcileConflict:
    feature_id: str
    reason: Literal["concurrent_create", "concurrent_edit", "delete_candidate"]
    fields: tuple[str, ...] = ()
    geometry_conflict: bool = False


@dataclass(frozen=True, slots=True)
class ReconcilePlan:
    actions: tuple[ReconcileAction, ...]
    conflicts: tuple[ReconcileConflict, ...]
    unchanged_count: int
    digest: str


def _snapshot_index(
    snapshots: Sequence[FeatureSnapshot], allowed_fields: tuple[str, ...]
) -> dict[str, tuple[dict[str, Any], dict[str, Any]]]:
    if len(snapshots) > MAX_RECONCILE_FEATURES:
        raise ValueError("Too many features for a single reconciliation snapshot.")
    indexed: dict[str, tuple[dict[str, Any], dict[str, Any]]] = {}
    for item in snapshots:
        ident = item.feature_id
        if not isinstance(ident, str) or not ident or ident.strip() != ident or len(ident) > 128:
            raise ValueError("feature_id must be nonblank, trimmed and <=128 characters.")
        if ident in indexed:
            raise ValueError(f"Duplicate feature ID: {ident}")
        if not isinstance(item.attributes, Mapping):
            raise ValueError("Feature attributes must be a mapping.")
        # Distinguish null vs missing: otherwise a missing field silently
        # becomes an update to null on the receiving system.
        missing = set(allowed_fields) - item.attributes.keys()
        if missing:
            raise ValueError(f"Mapped fields missing for {ident}: {sorted(missing)}")
        props = {field: item.attributes[field] for field in allowed_fields}
        geometry = _normalize_geometry(item.geometry_geojson)
        # Reject unserializable and NaN/Infinity-valued attribute payloads.
        json.dumps(props, sort_keys=True, allow_nan=False)
        indexed[ident] = (props, geometry)
    return indexed


def _equal_props(a: Mapping[str, Any], b: Mapping[str, Any], fields: tuple[str, ...]) -> bool:
    return all(_values_equal(a[f], b[f]) for f in fields)


def reconcile_three_way(
    *,
    baseline: Sequence[FeatureSnapshot],
    openvietgis: Sequence[FeatureSnapshot],
    external: Sequence[FeatureSnapshot],
    allowed_fields: frozenset[str],
    baseline_revision: str,
    openvietgis_revision: str,
    external_revision: str,
    max_actions: int = MAX_RECONCILE_ACTIONS,
) -> ReconcilePlan:
    """Preview safe synchronization from a shared acknowledged baseline.

    Deletions, concurrent creates and overlapping edits are *never* emitted
    as writes. Independent field edits may produce two conditional actions,
    one per target. The snapshots MUST be complete for the accessible scope;
    do not treat a paged/filtered/partial source as a deletion candidate.
    """
    if not allowed_fields or any(
        not isinstance(f, str) or not f or f.strip() != f or len(f) > 128
        for f in allowed_fields
    ):
        raise ValueError("Allowed fields must be a nonempty set of valid field IDs.")
    if max_actions < 1 or max_actions > MAX_RECONCILE_ACTIONS:
        raise ValueError("Invalid max_actions.")
    for rev in (baseline_revision, openvietgis_revision, external_revision):
        if not isinstance(rev, str) or not rev or len(rev) > 256:
            raise ValueError("All snapshot revisions must be explicit and nonempty.")

    fields = tuple(sorted(allowed_fields))
    base = _snapshot_index(baseline, fields)
    local = _snapshot_index(openvietgis, fields)
    remote = _snapshot_index(external, fields)

    actions: list[ReconcileAction] = []
    conflicts: list[ReconcileConflict] = []
    unchanged = 0
    for feature_id in sorted(set(base) | set(local) | set(remote)):
        prior = base.get(feature_id)
        ovg = local.get(feature_id)
        ext = remote.get(feature_id)

        if prior is None:
            if ovg is not None and ext is None:
                actions.append(
                    ReconcileAction("external", feature_id, "create", ovg[0], ovg[1], fields, True)
                )
            elif ext is not None and ovg is None:
                actions.append(
                    ReconcileAction(
                        "openvietgis", feature_id, "create", ext[0], ext[1], fields, True
                    )
                )
            elif ovg is not None and ext is not None:
                # Never guess that independently created IDs refer to the same
                # feature, even if current attributes and geometry happen to match.
                conflicts.append(ReconcileConflict(feature_id, "concurrent_create"))
            continue

        if ovg is None or ext is None:
            # Missing from either side is a potential tombstone, never an
            # implicit delete or a silent re-create of previously deleted data.
            conflicts.append(ReconcileConflict(feature_id, "delete_candidate"))
            continue

        bprops, bgeom = prior
        lprops, lgeom = ovg
        rprops, rgeom = ext
        conflicting_fields = tuple(
            field for field in fields
            if not _values_equal(lprops[field], bprops[field])
            and not _values_equal(rprops[field], bprops[field])
            and not _values_equal(lprops[field], rprops[field])
        )
        lgeom_changed = lgeom != bgeom
        rgeom_changed = rgeom != bgeom
        geom_conflict = lgeom_changed and rgeom_changed and lgeom != rgeom
        if conflicting_fields or geom_conflict:
            conflicts.append(
                ReconcileConflict(feature_id, "concurrent_edit", conflicting_fields, geom_conflict)
            )
            continue

        # A field changed on only one side since the last common checkpoint:
        # push only that field to the other side. Preserve the target's other
        # fields, including fields outside the authorized projection.
        l_to_r = tuple(
            f for f in fields
            if not _values_equal(lprops[f], bprops[f])
            and not _values_equal(lprops[f], rprops[f])
        )
        r_to_l = tuple(
            f for f in fields
            if not _values_equal(rprops[f], bprops[f])
            and not _values_equal(lprops[f], rprops[f])
        )
        geom_to_r = lgeom_changed and lgeom != rgeom
        geom_to_l = rgeom_changed and lgeom != rgeom
        if l_to_r or geom_to_r:
            projected = {**rprops, **{f: lprops[f] for f in l_to_r}}
            actions.append(
                ReconcileAction("external", feature_id, "update", projected,
                                lgeom if geom_to_r else rgeom, l_to_r, geom_to_r)
            )
        if r_to_l or geom_to_l:
            projected = {**lprops, **{f: rprops[f] for f in r_to_l}}
            actions.append(
                ReconcileAction("openvietgis", feature_id, "update", projected,
                                rgeom if geom_to_l else lgeom, r_to_l, geom_to_l)
            )
        if not l_to_r and not r_to_l and not geom_to_r and not geom_to_l:
            unchanged += 1

    if len(actions) > max_actions:
        raise ValueError(f"Reconciliation exceeds {max_actions} proposed actions.")
    canonical = {
        "baseline_revision": baseline_revision,
        "openvietgis_revision": openvietgis_revision,
        "external_revision": external_revision,
        "allowed_fields": fields,
        "actions": [
            {"side": a.target_side, "id": a.feature_id, "action": a.action,
             "attrs": a.attributes, "geometry": a.geometry_geojson}
            for a in actions
        ],
        "conflicts": [
            {"id": c.feature_id, "reason": c.reason, "fields": c.fields,
             "geometry_conflict": c.geometry_conflict}
            for c in conflicts
        ],
    }
    digest = hashlib.sha256(
        json.dumps(canonical, sort_keys=True, separators=(",", ":"), allow_nan=False).encode()
    ).hexdigest()
    return ReconcilePlan(tuple(actions), tuple(conflicts), unchanged, digest)


def is_echo_event(*, origin_chain: Sequence[str], target_binding_id: str) -> bool:
    """Suppress loops only for explicitly known origin hops, not timestamps."""
    if not target_binding_id or len(origin_chain) > 16 or any(
        not isinstance(hop, str) or not hop or len(hop) > 128 for hop in origin_chain
    ):
        raise ValueError("Invalid origin chain.")
    return target_binding_id in origin_chain