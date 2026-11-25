"""R007 exact typed, resource-bounded visual filter compiler (zero arbitrary SQL)."""

from __future__ import annotations

from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from datetime import datetime
from decimal import Decimal
from enum import StrEnum
from typing import Any
from uuid import UUID

from openvietgis_api.domains.platform_errors import PlatformValidationError
from openvietgis_api.domains.spatial.domain import (
    READABLE_CRS_CODES,
    CoordinateTree,
    FieldType,
    GeometryFamily,
    NativeGeometry,
    SourceField,
    SpatialValidationError,
    canonical_attribute_value,
    validate_ready_geometry,
)

MAX_QUERY_LIMIT = 500
MAX_QUERY_OFFSET = 50_000  # page through the whole 50k-feature layer (Spec 053)
MAX_QUERY_FEATURES = 50_000
MAX_QUERY_POSITIONS = 1_000_000
MAX_REFERENCE_FEATURES = 500
MAX_REFERENCE_POSITIONS = 100_000
MAX_PAGE_POSITIONS = 20_000
MAX_DRAWN_POSITIONS = 2048
MAX_AST_DEPTH = 6
MAX_AST_NODES = 100
MAX_AST_PARAMS = 1000
QUERY_STATEMENT_TIMEOUT = "5s"


class SpatialPredicate(StrEnum):
    INTERSECTS = "intersects"
    WITHIN = "within"
    CONTAINS = "contains"


@dataclass(frozen=True, slots=True)
class SavedQueryRecord:
    id: UUID
    organization_id: UUID
    project_id: UUID
    layer_id: UUID
    name: str
    description: str
    attribute_filter: dict[str, Any] | None
    spatial_filter: dict[str, Any] | None
    sort_field: str | None
    sort_direction: str
    created_by_actor_id: UUID
    created_at: datetime
    updated_at: datetime


@dataclass(frozen=True, slots=True)
class CompiledFilter:
    sql: str
    params: dict[str, Any]


def _invalid(message: str = "invalid typed filter") -> PlatformValidationError:
    return PlatformValidationError(message, code="INVALID_FILTER")


def typed_column_expr(json_text: str, field: SourceField) -> str:
    cast = {
        FieldType.INTEGER: "bigint",
        FieldType.DECIMAL: "numeric",
        FieldType.BOOLEAN: "boolean",
        FieldType.TIMESTAMP: "timestamptz",
    }.get(field.type)
    if cast:
        return f"CAST({json_text} AS {cast})"
    return f'({json_text} COLLATE "C")'


def _typed_value(value: Any, field: SourceField) -> Any:
    if value is None:
        raise _invalid("use is_null, not a null comparison value")
    try:
        canonical = canonical_attribute_value(field, value)
    except SpatialValidationError as exc:
        raise _invalid("value does not match declared field type or precision") from exc
    if field.type in (FieldType.DECIMAL, FieldType.TIMESTAMP):
        # Both logical types use canonical textual JSON representations.
        if not isinstance(canonical, str):
            raise _invalid("non-text canonical representation")
        if field.type == FieldType.DECIMAL:
            return Decimal(canonical)
        return datetime.fromisoformat(canonical)
    return canonical


def compile_attribute_filter(
    node: Mapping[str, Any] | None, fields: Sequence[SourceField]
) -> CompiledFilter:
    """Compile literal, exact typed values and SQL three-valued Boolean semantics."""
    if node is None:
        return CompiledFilter(sql="TRUE", params={})
    field_map = {field.field_id: field for field in fields}
    params: dict[str, Any] = {}
    nodes = 0

    def bind(value: Any) -> str:
        if len(params) >= MAX_AST_PARAMS:
            raise PlatformValidationError("filter value budget exceeded", code="QUERY_TOO_COMPLEX")
        key = f"qf_{len(params) + 1}"
        params[key] = value
        return f":{key}"

    def compile_node(expr: Any, depth: int) -> str:
        nonlocal nodes
        nodes += 1
        if depth > MAX_AST_DEPTH or nodes > MAX_AST_NODES:
            raise PlatformValidationError("filter complexity exceeded", code="QUERY_TOO_COMPLEX")
        if not isinstance(expr, Mapping) or not isinstance(expr.get("op"), str):
            raise _invalid()
        op = expr["op"]
        if op in ("and", "or"):
            if set(expr) != {"op", "conditions"}:
                raise _invalid()
            children = expr["conditions"]
            if not isinstance(children, list) or not children or len(children) > MAX_AST_NODES:
                raise _invalid("logical group requires bounded non-empty conditions")
            return "(" + f" {op.upper()} ".join(compile_node(c, depth + 1) for c in children) + ")"
        if op == "not":
            if set(expr) != {"op", "condition"}:
                raise _invalid()
            return f"(NOT ({compile_node(expr['condition'], depth + 1)}))"
        if op == "is_null":
            if set(expr) - {"op", "field", "value"}:
                raise _invalid()
        elif set(expr) != {"op", "field", "value"}:
            raise _invalid()
        field_id = expr.get("field")
        if not isinstance(field_id, str) or field_id not in field_map:
            raise PlatformValidationError("unknown query field", code="UNKNOWN_QUERY_FIELD")
        field = field_map[field_id]  # validated stable SourceField identifier
        column = f"(rf.attributes ->> '{field.field_id}')"
        typed = typed_column_expr(column, field)
        value = expr.get("value", True) if op == "is_null" else expr["value"]
        if op == "is_null":
            if type(value) is not bool:
                raise _invalid("is_null requires a Boolean value")
            return f"({column} IS {'NULL' if value else 'NOT NULL'})"
        if op in ("contains", "prefix"):
            if field.type != FieldType.TEXT:
                raise PlatformValidationError(
                    "literal text operator requires text", code="INVALID_OPERATOR"
                )
            parameter = bind(_typed_value(value, field))
            # No LIKE wildcards, escape syntax, accent folding or case folding.
            fn = "strpos" if op == "contains" else "starts_with"
            comparison = " > 0" if op == "contains" else ""
            return f"({fn}({typed}, {parameter}){comparison})"
        if op == "in":
            if not isinstance(value, list) or not 1 <= len(value) <= 100:
                raise _invalid("membership requires 1..100 typed values")
            return f"({typed} IN ({', '.join(bind(_typed_value(v, field)) for v in value)}))"
        if op == "between":
            if field.type == FieldType.BOOLEAN or not isinstance(value, list) or len(value) != 2:
                raise _invalid("range requires two ordered non-Boolean values")
            start, end = (_typed_value(v, field) for v in value)
            if start > end:
                raise _invalid("range endpoints are reversed")
            return f"({typed} BETWEEN {bind(start)} AND {bind(end)})"
        comparators = {"eq": "=", "neq": "!=", "gt": ">", "gte": ">=", "lt": "<", "lte": "<="}
        if op not in comparators or (op not in ("eq", "neq") and field.type == FieldType.BOOLEAN):
            raise PlatformValidationError("unsupported typed operator", code="INVALID_OPERATOR")
        return f"({typed} {comparators[op]} {bind(_typed_value(value, field))})"

    return CompiledFilter(sql=compile_node(node, 0), params=params)


def compile_sort(fields: Sequence[SourceField], sort_field: str | None, sort_direction: str) -> str:
    if sort_direction not in ("asc", "desc"):
        raise PlatformValidationError("sort direction must be asc or desc", code="INVALID_SORT")
    direction = sort_direction.upper()
    if sort_field is None:
        return f"rf.feature_id {direction}"
    field = next((f for f in fields if f.field_id == sort_field), None)
    if field is None:
        raise PlatformValidationError("unknown sort field", code="UNKNOWN_SORT_FIELD")
    typed = typed_column_expr(f"(rf.attributes ->> '{field.field_id}')", field)
    return f"{typed} {direction} NULLS LAST, rf.feature_id ASC"


def parse_drawn_area(value: Any, crs_code: Any) -> NativeGeometry:
    """Accept only bounded native XY Polygon with explicitly declared supported CRS."""
    if type(crs_code) is not int or crs_code not in READABLE_CRS_CODES:
        raise PlatformValidationError(
            "drawn area requires explicit supported CRS", code="UNSUPPORTED_CRS"
        )
    if (
        not isinstance(value, Mapping)
        or set(value) != {"type", "coordinates"}
        or value["type"] != "Polygon"
    ):
        raise PlatformValidationError(
            "drawn geometry must be a Polygon", code="INVALID_SPATIAL_FILTER"
        )
    rings = value["coordinates"]
    if not isinstance(rings, list) or not 1 <= len(rings) <= 128:
        raise PlatformValidationError("invalid drawn polygon rings", code="INVALID_SPATIAL_FILTER")
    total = 0
    for ring in rings:
        if not isinstance(ring, list) or len(ring) < 4:
            raise PlatformValidationError(
                "invalid drawn polygon ring", code="INVALID_SPATIAL_FILTER"
            )
        total += len(ring)
        if total > MAX_DRAWN_POSITIONS:
            raise PlatformValidationError(
                "drawn polygon exceeds position limit", code="QUERY_TOO_COMPLEX"
            )
        if any(not isinstance(pos, list) or len(pos) != 2 for pos in ring):
            raise PlatformValidationError(
                "drawn geometry requires XY coordinates", code="INVALID_SPATIAL_FILTER"
            )
    try:
        coordinates: CoordinateTree = tuple(
            tuple(tuple(position) for position in ring) for ring in rings
        )
        geometry = NativeGeometry("Polygon", coordinates, crs_code)
        validate_ready_geometry(geometry, GeometryFamily.POLYGON, crs_code)
    except SpatialValidationError as exc:
        raise PlatformValidationError(
            "invalid drawn polygon or CRS domain", code="INVALID_SPATIAL_FILTER"
        ) from exc
    return geometry
