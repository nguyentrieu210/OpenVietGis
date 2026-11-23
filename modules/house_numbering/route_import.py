"""Preview and atomic import of GIS route hierarchies, without copying geometry."""

from __future__ import annotations

import hashlib
import json
from collections import Counter
from typing import Any
from uuid import UUID

from openvietgis_api.domains.platform_errors import (
    PlatformConflictError,
    PlatformValidationError,
)
from sqlalchemy import text

from modules.house_numbering.address_prefix import canonical_prefix
from modules.house_numbering.route_recovery import (
    address_label,
    recover_parent_evidence,
    selection_rows,
    survey_evidence,
)
from modules.house_numbering.route_topology import (
    append_parent_features,
    apply_merges,
    merge_options,
    parent_tasks,
)

_LIMIT = 5000


def _value(value: Any) -> str:
    return str(value).strip() if value is not None else ""


def _prefix(parent: str, value: str) -> str:
    value = canonical_prefix(value)
    if not parent:
        if "/" in value:
            raise ValueError("prefix_must_be_one_level")
        return value
    if "/" not in value:
        return f"{parent}/{value}"
    if value.rsplit("/", 1)[0] != parent:
        raise ValueError("prefix_does_not_follow_parent")
    return value


def group_routes(rows: list[dict[str, Any]]) -> list[dict[str, Any]]:
    """Opt-in grouping by logical code; never merge by name or proximity alone."""
    groups: dict[tuple[str, str], list[dict[str, Any]]] = {}
    raw_counts = Counter(r["code"] for r in rows)
    aliases: dict[str, str] = {}
    for row in rows:
        logical = row.get("logical_code") or ""
        logical_id = logical or row["code"]
        if raw_counts[row["code"]] != 1 or not row["code"]:
            row["errors"].append("duplicate_or_empty_code")
        aliases[row["code"]] = logical_id
        groups.setdefault(
            ("group", logical_id) if logical else ("single", row["key"]), []
        ).append(row)
    result = []
    for members in groups.values():
        if members[0].get("recovery_group"):
            members = sorted(members, key=lambda r: r["key"])
        first = dict(members[0])
        first["errors"] = sorted({e for row in members for e in row["errors"]})
        first["review_reason"] = "; ".join(
            sorted({r["review_reason"] for r in members if r.get("review_reason")})
        )
        group = first.get("logical_code")
        if group:
            first["code"] = group
            first["segments"] = [
                {
                    "source_id": r["source_id"],
                    "feature_id": r["feature_id"],
                    "code": r["code"],
                }
                for r in members
            ]
            first["segment_revisions"] = [r["content_revision"] for r in members]
            consistency = {
                (
                    r["name"],
                    r["route_type"],
                    aliases.get(r["parent_code"], r["parent_code"]),
                    r["prefix"],
                    r["direction_state"],
                )
                for r in members
            }
            if len(consistency) != 1:
                first["errors"].append("inconsistent_logical_route")
            if len(members) > 200:
                first["errors"].append("too_many_segments")
            if group in aliases and aliases[group] != group:
                first["errors"].append("duplicate_or_empty_code")
        first["parent_code"] = aliases.get(first["parent_code"], first["parent_code"])
        result.append(first)
    return result


def plan_routes(
    rows: list[dict[str, Any]], existing: list[dict[str, Any]]
) -> list[dict[str, Any]]:
    """Resolve dependencies; no row is ever written by the planner."""
    counts = Counter(r["code"] for r in rows)
    by_code = {r["code"]: r for r in rows if counts[r["code"]] == 1}
    by_id = {str(r["id"]): r for r in existing}
    by_saved_code: dict[str, list[dict[str, Any]]] = {}
    for r in existing:
        label = r.get("source_label", "")
        if label.startswith("route-tree-import:"):
            by_saved_code.setdefault(
                label.removeprefix("route-tree-import:"), []
            ).append(r)
    by_feature: dict[tuple[str, str], list[dict[str, Any]]] = {}
    for r in existing:
        by_feature.setdefault(
            (str(r["linked_source_id"]), str(r["linked_feature_id"])), []
        ).append(r)
        for segment in r.get("segments") or []:
            segment_key = (str(segment["source_id"]), str(segment["feature_id"]))
            if segment_key != (str(r["linked_source_id"]), str(r["linked_feature_id"])):
                by_feature.setdefault(segment_key, []).append(r)
            code = segment.get("code")
            if code and all(
                old["id"] != r["id"] for old in by_saved_code.get(code, [])
            ):
                by_saved_code.setdefault(code, []).append(r)
    states: dict[str, int] = {}
    stack: list[dict[str, Any]] = []
    result: list[dict[str, Any]] = []

    def resolve(r: dict[str, Any]) -> None:
        key = r["key"]
        if states.get(key) == 2:
            return
        if states.get(key) == 1:
            for member in stack:
                if "cycle" not in member["errors"]:
                    member["errors"].append("cycle")
            return
        states[key] = 1
        stack.append(r)
        errors = r["errors"]
        if not r["code"] or len(r["code"]) > 160 or counts[r["code"]] > 1:
            errors.append("duplicate_or_empty_code")
        if not r["name"] or len(r["name"]) > 200:
            errors.append("invalid_name")
        refs = r.get("segments") or [
            {"source_id": r["source_id"], "feature_id": r["feature_id"]}
        ]
        found = list(
            {
                str(old["id"]): old
                for ref in refs
                for old in by_feature.get((ref["source_id"], ref["feature_id"]), [])
            }.values()
        )
        if len(found) > 1:
            errors.append("multiple_existing_routes")
        if len(found) == 1:
            r["existing_id"] = str(found[0]["id"])
        aliases = [r["code"]] + [
            s["code"] for s in r.get("segments") or [] if s.get("code")
        ]
        if any(
            str(old["id"]) != r.get("existing_id")
            for alias in aliases
            for old in by_saved_code.get(alias, [])
        ):
            errors.append("duplicate_or_empty_code")
        if r["route_type"] not in {"street", "alley"}:
            errors.append("invalid_route_type")
        ref = r["parent_code"]
        parent = None
        parent_id = None
        parent_prefix = ""
        parent_type = None
        if ref:
            if ref in by_code:
                parent = by_code[ref]
                resolve(parent)
                if parent["errors"]:
                    errors.append("parent_has_errors")
                parent_id = parent.get("existing_id")
                parent_prefix = parent.get("resolved_prefix") or ""
                parent_type = parent["route_type"]
                r["parent_key"] = parent["key"]
            elif ref in by_id or len(by_saved_code.get(ref, [])) == 1:
                p = by_id[ref] if ref in by_id else by_saved_code[ref][0]
                parent_id = str(p["id"])
                parent_prefix = p["prefix"] or ""
                parent_type = p["route_type"]
                r["parent_key"] = None
                r["parent_revision"] = p["revision"]
            else:
                errors.append("parent_not_found")
        if r["route_type"] == "street":
            if ref or r["prefix"]:
                errors.append("street_cannot_have_parent_or_prefix")
            r["resolved_prefix"] = None
        elif r["route_type"] == "alley":
            if not ref:
                errors.append("parent_required")
            if parent_type == "alley" and not parent_prefix:
                errors.append("parent_prefix_missing")
            try:
                r["resolved_prefix"] = _prefix(parent_prefix, r["prefix"])
                if len(r["resolved_prefix"]) > 100:
                    errors.append("prefix_too_long")
            except ValueError as e:
                errors.append(str(e))
        r["parent_id"] = parent_id
        if len(found) == 1:
            old = found[0]
            if (
                old["name"] != r["name"]
                or old["route_type"] != r["route_type"]
                or (old["prefix"] or None) != r.get("resolved_prefix")
                or (str(old["parent_route_id"]) if old["parent_route_id"] else None)
                != parent_id
                or old["direction_state"] != r["direction_state"]
                or (old.get("segments") or []) != (r.get("segments") or [])
            ):
                errors.append("existing_route_conflict")
            r["existing_revision"] = old["revision"]
        stack.pop()
        states[key] = 2
        result.append(r)

    for row in rows:
        try:
            resolve(row)
        except RecursionError as exc:
            raise PlatformValidationError(
                "Cây tuyến quá sâu", code="ROUTE_DEPTH_LIMIT"
            ) from exc

    occupied: dict[tuple[str, str], str] = {}
    new_occupied: dict[tuple[str, str], list[dict[str, Any]]] = {}
    for old in existing:
        if old["prefix"] and old["parent_route_id"]:
            occupied[(str(old["parent_route_id"]), old["prefix"])] = str(old["id"])
    for r in result:
        if r["errors"] or r.get("existing_id") or not r.get("resolved_prefix"):
            continue
        parent_ref = r.get("parent_id") or r.get("parent_key")
        key = (parent_ref, r["resolved_prefix"])
        if key in occupied:
            r["errors"].append("duplicate_sibling_prefix")
        else:
            new_occupied.setdefault(key, []).append(r)
    for duplicates in new_occupied.values():
        if len(duplicates) > 1:
            for duplicate in duplicates:
                duplicate["errors"].append("duplicate_sibling_prefix")
    by_key = {r["key"]: r for r in result}
    for r in result:
        parent_key = r.get("parent_key")
        if parent_key:
            parent = by_key.get(parent_key)
            if parent and parent["errors"] and "parent_has_errors" not in r["errors"]:
                r["errors"].append("parent_has_errors")
        r["state"] = (
            "error" if r["errors"] else "existing" if r.get("existing_id") else "ready"
        )
    return result


class RouteTreeImporter:
    def __init__(self, repo: Any) -> None:
        self.repo = repo
        self.session = repo.session

    def preview(
        self,
        layers: list[dict[str, Any]],
        overrides: dict[str, dict[str, Any]],
        merge_groups: list[str] | None = None,
        parent_features: list[dict[str, Any]] | None = None,
    ) -> dict[str, Any]:
        self.repo.prepare("edit")
        params = {"org": self.repo.org_id, "project": self.repo.project_id}
        rows: list[dict[str, Any]] = []
        seen_sources: set[str] = set()
        for layer in layers:
            source = str(layer["source_id"])
            if source in seen_sources:
                raise PlatformValidationError(
                    "Một lớp chỉ được chọn một lần", code="DUPLICATE_SOURCE"
                )
            seen_sources.add(source)
            entries = (
                self.session.execute(
                    text("""
                SELECT f.feature_id, f.attributes, f.content_revision, f.geometry_state,
                       ST_GeometryType(f.geometry) AS geometry_type, ST_SRID(f.geometry) AS srid
                FROM spatial.data_sources s JOIN spatial.reference_features f
                  ON f.organization_id=s.organization_id AND f.project_id=s.project_id AND f.source_id=s.id
                WHERE s.organization_id=:org AND s.project_id=:project AND s.id=:source
                  AND s.lifecycle='active' AND s.validation_state='ready'
                ORDER BY f.feature_id LIMIT :limit
            """),
                    {**params, "source": source, "limit": _LIMIT + 1},
                )
                .mappings()
                .all()
            )
            if not entries:
                raise PlatformValidationError(
                    "Lớp không có đối tượng hoặc chưa sẵn sàng", code="SOURCE_NOT_READY"
                )
            if len(rows) + len(entries) > _LIMIT:
                raise PlatformValidationError(
                    f"Tối đa {_LIMIT} tuyến mỗi đợt", code="ROUTE_IMPORT_LIMIT"
                )
            mapping = layer["mapping"]
            for f in entries:
                attrs = f["attributes"]
                feature = str(f["feature_id"])
                key = f"{source}:{feature}"

                def read(
                    field: str, default: str = "", mapping=mapping, attrs=attrs
                ) -> str:
                    name = mapping.get(field)
                    return _value(attrs.get(name)) if name else default

                code = read("code", feature)
                parent = read("parent_code")
                prefix = read("prefix")
                kind = read("route_type", layer["default_type"]).lower()
                aliases = {
                    "đường": "street",
                    "duong": "street",
                    "hẻm": "alley",
                    "hem": "alley",
                    "ngách": "alley",
                    "ngach": "alley",
                    "nhánh": "alley",
                    "nhanh": "alley",
                }
                kind = aliases.get(kind, kind)
                errors = []
                if (
                    f["geometry_state"] != "present"
                    or f["geometry_type"] not in {"ST_LineString", "ST_MultiLineString"}
                    or not f["srid"]
                ):
                    errors.append("invalid_line_geometry")
                rows.append(
                    {
                        "key": key,
                        "source_id": source,
                        "feature_id": feature,
                        "content_revision": f["content_revision"],
                        "code": code,
                        "logical_code": read("logical_code"),
                        "name": read("name"),
                        "geometry_type": f["geometry_type"],
                        "parent_code": parent,
                        "prefix": prefix,
                        "route_type": kind,
                        "direction_state": layer["direction_state"],
                        "errors": errors,
                        "review_reason": _value(attrs.get("review_reason"))
                        or (
                            "review_required"
                            if attrs.get("import_state") == "review_required"
                            else ""
                        ),
                    }
                )
        existing = [dict(r) for r in self.repo.list_routes()]
        merges = merge_options(self.session, params, rows, existing)
        apply_merges(rows, existing, merges, merge_groups or [])
        if parent_features:
            if len(rows) + len(parent_features) > _LIMIT:
                raise PlatformValidationError(
                    "Quá giới hạn tuyến mỗi đợt", code="ROUTE_IMPORT_LIMIT"
                )
            append_parent_features(
                self.session, params, rows, existing, parent_features
            )
        grouped = group_routes(rows)
        aliases = {row["code"]: row.get("logical_code") or row["code"] for row in rows}
        for row in grouped:
            override = overrides.get(row["key"], {})
            parent = override.get("parent_code", row["parent_code"])
            row["parent_code"] = aliases.get(parent, parent)
            locked_identity = bool(
                row.get("manual_parent") or row.get("recovery_group")
            )
            if locked_identity and any(
                override.get(field) is not None and override[field] != row[field]
                for field in ("name", "prefix", "route_type")
            ):
                raise PlatformValidationError(
                    "Không đổi đường dẫn của nhóm ghép hoặc tuyến cha bổ sung; hãy bỏ nhóm và xem lại",
                    code="RECOVERY_IDENTITY_LOCKED",
                )
            row["prefix"] = override.get("prefix", row["prefix"])
            for field in ("name", "route_type"):
                if override.get(field) is not None:
                    row[field] = override[field]
            row["reviewed"] = bool(override.get("reviewed", False))
        # A previously recovered review feature is already linked. Its saved
        # route is authoritative, even when the legacy attributes are obsolete.
        # Only exact link sets qualify; do not hide conflicting grouped links.
        existing_by_feature = {}
        for saved in existing:
            key = (str(saved["linked_source_id"]), str(saved["linked_feature_id"]))
            existing_by_feature.setdefault(key, []).append(saved)
        for row in grouped:
            if not row.get("review_reason"):
                continue
            matches = existing_by_feature.get((row["source_id"], row["feature_id"]), [])
            if len(matches) != 1:
                continue
            old = matches[0]
            if (old.get("segments") or []) != (row.get("segments") or []):
                continue
            row.update(
                name=old["name"],
                route_type=old["route_type"],
                parent_code=str(old["parent_route_id"])
                if old["parent_route_id"]
                else "",
                prefix=old["prefix"] or "",
                direction_state=old["direction_state"],
                reviewed=True,
            )
        planned = plan_routes(grouped, existing)
        for row in planned:
            if row.get("review_reason") and not row.get("existing_id"):
                if not row["reviewed"]:
                    row["errors"].append("review_required")
                if row["route_type"] == "street" and address_label(row["name"]):
                    row["errors"].append("review_alley_type_required")
                if row["errors"]:
                    row["state"] = "error"
        for row in planned:
            # Explicit recovery groups already passed the batched merge geometry check;
            # create_route repeats membership and topology checks transactionally on apply.
            if (
                not row.get("existing_id")
                and not row.get("recovery_group")
                and (
                    row.get("segments")
                    or row.get("manual_parent")
                    or row.get("geometry_type") == "ST_MultiLineString"
                )
            ):
                try:
                    self.repo.validate_route_segments(
                        UUID(row["source_id"]), row["feature_id"], row.get("segments")
                    )
                except (PlatformValidationError, PlatformConflictError):
                    row["errors"].append("invalid_route_segments")
                    row["state"] = "error"
        # Evidence is advisory until the operator explicitly confirms the row.
        recover_parent_evidence(self.session, params, planned, existing)
        survey_evidence(self.session, params, planned, existing)
        from modules.house_numbering.address_verification import apply_staged_reviews

        apply_staged_reviews(self.repo, planned)
        # Propagate all parent errors (including post-plan geometry and review failures).
        by_key = {row["key"]: row for row in planned}
        for row in planned:
            parent = by_key.get(row.get("parent_key"))
            if parent and parent["state"] == "error":
                if "parent_has_errors" not in row["errors"]:
                    row["errors"].append("parent_has_errors")
                row["state"] = "error"
        from modules.house_numbering.geometry_review import stage_eligible

        for row in planned:
            row["geometry_stage_eligible"] = stage_eligible(row)
        tasks = parent_tasks(planned, existing)
        digest = hashlib.sha256(
            json.dumps(
                {"items": planned, "merge_options": merges, "missing_parents": tasks},
                sort_keys=True,
                ensure_ascii=False,
                separators=(",", ":"),
            ).encode()
        ).hexdigest()
        return {
            "preview_token": digest,
            "items": planned,
            "merge_options": merges,
            "missing_parents": tasks,
            "ready": sum(r["state"] == "ready" for r in planned),
            "existing": sum(r["state"] == "existing" for r in planned),
            "errors": sum(r["state"] == "error" for r in planned),
        }

    def apply(
        self,
        layers: list[dict[str, Any]],
        overrides: dict[str, dict[str, Any]],
        token: str,
        selected_keys: list[str] | None = None,
        merge_groups: list[str] | None = None,
        parent_features: list[dict[str, Any]] | None = None,
    ) -> dict[str, Any]:
        self.repo.prepare("edit")
        self.session.execute(
            text("SELECT pg_advisory_xact_lock(hashtextextended(:key, 0))"),
            {"key": f"house-numbering-route-import:{self.repo.project_id}"},
        )
        plan = self.preview(layers, overrides, merge_groups, parent_features)
        if plan["preview_token"] != token:
            raise PlatformConflictError(
                "Dữ liệu đã thay đổi, hãy xem trước lại", code="STALE_ROUTE_IMPORT"
            )
        selected = selection_rows(plan, selected_keys)
        ids: dict[str, UUID] = {}
        created_count = 0
        for r in selected:
            if r.get("existing_id"):
                ids[r["key"]] = UUID(r["existing_id"])
                continue
            parent = (
                ids.get(r.get("parent_key"))
                if r.get("parent_key")
                else UUID(r["parent_id"])
                if r.get("parent_id")
                else None
            )
            created = self.repo.create_route(
                name=r["name"],
                route_type=r["route_type"],
                parent_route_id=parent,
                prefix=r["resolved_prefix"],
                source_id=UUID(r["source_id"]),
                feature_id=r["feature_id"],
                direction_state=r["direction_state"],
                source_label=f"route-tree-import:{r['code']}",
                status="draft",
                segments=r.get("segments"),
            )
            ids[r["key"]] = created["id"]
            created_count += 1
        self.session.commit()
        return {
            "created": created_count,
            "existing": plan["existing"],
            "remaining": plan["ready"] + plan["errors"] - created_count,
        }
