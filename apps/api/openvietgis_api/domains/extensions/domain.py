"""R014 Generic Internal Extension Framework domain (ADR-008: zero vertical knowledge)."""

from __future__ import annotations

import hashlib
import json
import math
import re
from dataclasses import dataclass, field
from datetime import datetime
from typing import Any
from uuid import UUID

from openvietgis_api.domains.extensions.external import EXTERNAL_PREFIX, validate_external_manifest
from openvietgis_api.domains.platform_errors import PlatformConflictError, PlatformValidationError

_EXT_KEY_RE = re.compile(r"^[a-z][a-z0-9_-]{1,99}$")
_SEMVER_RE = re.compile(r"^(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)$")
_NAME_RE = re.compile(r"^[A-Za-z][A-Za-z0-9_]{0,63}$")
PORTABLE_FORMAT = "openvietgis.extension.portable.v1"
MAX_RECORDS = 200
MAX_REVISIONS = 1000
MAX_BUNDLE_BYTES = 2 * 1024 * 1024
_SUPPORTED_API_RANGE = "1.x"


@dataclass(frozen=True, slots=True)
class ExtensionManifestRecord:
    id: UUID
    organization_id: UUID
    extension_key: str
    version: str
    api_compatibility: str
    display_name: str
    description: str
    permissions: list[str]
    commands: list[dict[str, Any]]
    capabilities: list[str]
    entity_schemas: dict[str, dict[str, Any]]
    enabled: bool
    registered_by_actor_id: UUID
    created_at: datetime
    ui_contributions: list[dict[str, Any]] = field(default_factory=list)


@dataclass(frozen=True, slots=True)
class ProjectActivationRecord:
    id: UUID
    organization_id: UUID
    project_id: UUID
    manifest_id: UUID
    extension_key: str
    status: str
    activated_by_actor_id: UUID
    activated_at: datetime
    updated_at: datetime
    revision: int = 1
    disabled_by_actor_id: UUID | None = None
    disabled_at: datetime | None = None


@dataclass(frozen=True, slots=True)
class ExtensionDataRecord:
    id: UUID
    organization_id: UUID
    project_id: UUID
    extension_key: str
    entity_name: str
    payload: dict[str, Any]
    linked_layer_id: UUID | None
    linked_feature_id: str | None
    created_by_actor_id: UUID
    created_at: datetime
    updated_at: datetime
    manifest_id: UUID
    revision: int
    link_context: dict[str, Any] | None
    operation_id: UUID | None = None
    updated_by_actor_id: UUID | None = None


def validate_manifest_contract(
    *,
    extension_key: str,
    version: str,
    api_compatibility: str,
    permissions: list[str],
    entity_schemas: dict[str, dict[str, Any]],
    commands: list[dict[str, Any]] | None = None,
    capabilities: list[str] | None = None,
    ui_contributions: list[dict[str, Any]] | None = None,
) -> None:
    """Validate versioned extension manifest & permission namespace isolation."""
    if not _EXT_KEY_RE.fullmatch(extension_key):
        raise PlatformValidationError(
            "extension_key must match ^[a-z][a-z0-9_-]{1,99}$",
            code="INVALID_EXTENSION_KEY",
        )
    if len(version) > 32 or not _SEMVER_RE.fullmatch(version):
        raise PlatformValidationError("version must be semver X.Y.Z", code="INVALID_VERSION")
    if api_compatibility != _SUPPORTED_API_RANGE:
        raise PlatformValidationError(
            f"unsupported api_compatibility {api_compatibility!r}; "
            f"expected {_SUPPORTED_API_RANGE!r}",
            code="INCOMPATIBLE_EXTENSION_API",
        )
    if extension_key.startswith(EXTERNAL_PREFIX):
        validate_external_manifest(
            extension_key=extension_key,
            permissions=permissions,
            commands=commands or [],
            capabilities=capabilities or [],
            entity_schemas=entity_schemas,
            ui_contributions=ui_contributions or [],
        )
        return
    if any(isinstance(ui, dict) and "runtime" in ui for ui in (ui_contributions or [])):
        raise PlatformValidationError("external runtime requires partner_ namespace")
    prefix = f"ext.{extension_key}."
    if len(permissions) > 64 or len(set(permissions)) != len(permissions):
        raise PlatformValidationError("duplicate or too many permission identifiers")
    for perm in permissions:
        if not perm.startswith(prefix) or not _NAME_RE.fullmatch(perm[len(prefix) :]):
            raise PlatformValidationError(
                f"permission {perm!r} must be namespaced under {prefix!r}",
                code="INVALID_PERMISSION_NAMESPACE",
            )
    if not entity_schemas or len(entity_schemas) > 100:
        raise PlatformValidationError(
            "manifest must declare one to 100 entity schemas", code="EMPTY_SCHEMAS"
        )
    for entity, schema in entity_schemas.items():
        if not _NAME_RE.fullmatch(entity) or set(schema) != {"fields"}:
            raise PlatformValidationError("invalid entity schema")
        fields = schema["fields"]
        if not isinstance(fields, dict) or not fields or len(fields) > 100:
            raise PlatformValidationError("entity fields must be a bounded mapping")
        for name, spec in fields.items():
            if (
                not _NAME_RE.fullmatch(name)
                or not isinstance(spec, dict)
                or set(spec) - {"type", "required"}
                or spec.get("type") not in ("text", "integer", "number", "boolean")
                or not isinstance(spec.get("required", False), bool)
            ):
                raise PlatformValidationError("invalid or unsupported field declaration")
    seen: set[str] = set()
    if commands is not None:
        if len(commands) > 64:
            raise PlatformValidationError("too many command declarations")
        for command in commands:
            identifier = command.get("id")
            label = command.get("label")
            if (
                set(command) != {"id", "label", "required_permission"}
                or not isinstance(identifier, str)
                or not _NAME_RE.fullmatch(identifier)
                or identifier in seen
                or not isinstance(label, str)
                or not label.strip()
                or len(label) > 200
                or command.get("required_permission") not in permissions
            ):
                raise PlatformValidationError("invalid or duplicate command declaration")
            seen.add(identifier)
    if capabilities is not None and (
        len(capabilities) > 64
        or len(set(capabilities)) != len(capabilities)
        or any(not _NAME_RE.fullmatch(capability) for capability in capabilities)
    ):
        raise PlatformValidationError("invalid or duplicate capability declaration")
    if ui_contributions is not None:
        if len(ui_contributions) > 64:
            raise PlatformValidationError("too many UI contributions", code="INVALID_MANIFEST")
        contribution_ids: set[str] = set()
        for contribution in ui_contributions:
            if not isinstance(contribution, dict):
                raise PlatformValidationError(
                    "UI contribution must be an object", code="INVALID_MANIFEST"
                )
            cid = contribution.get("id")
            slot = contribution.get("slot")
            label = contribution.get("label")
            route_suffix = contribution.get("route_suffix")
            if (
                not isinstance(cid, str)
                or not _NAME_RE.fullmatch(cid)
                or cid in contribution_ids
                or slot not in {"project_tool", "project_panel", "project_route"}
                or not isinstance(label, str)
                or not label.strip()
                or not isinstance(route_suffix, str)
                or not route_suffix.startswith("/")
                or ".." in route_suffix
            ):
                raise PlatformValidationError("invalid UI contribution", code="INVALID_MANIFEST")
            required = contribution.get("required_permission")
            if required is not None and required not in permissions:
                raise PlatformValidationError(
                    f"UI contribution {cid!r} references undeclared permission",
                    code="INVALID_MANIFEST",
                )
            contribution_ids.add(cid)
    canonical_bytes(
        {
            "permissions": permissions,
            "commands": commands,
            "capabilities": capabilities,
            "schemas": entity_schemas,
            "ui_contributions": ui_contributions or [],
        },
        limit=128 * 1024,
    )


def validate_record_against_schema(
    *,
    entity_name: str,
    payload: dict[str, Any],
    entity_schemas: dict[str, dict[str, Any]],
) -> dict[str, Any]:
    """Validate an extension record payload strictly against the manifest's declared fields."""
    schema = entity_schemas.get(entity_name)
    if schema is None:
        raise PlatformValidationError(
            f"unknown entity {entity_name!r} for this extension", code="UNKNOWN_ENTITY"
        )
    fields: dict[str, Any] = schema.get("fields", {})
    unknown = sorted(set(payload.keys()) - set(fields.keys()))
    if unknown:
        raise PlatformValidationError(
            f"unknown fields in extension record: {', '.join(unknown)}",
            code="SCHEMA_MISMATCH",
        )
    cleaned: dict[str, Any] = {}
    for fname, spec in fields.items():
        req = bool(spec.get("required", False))
        ftype = str(spec.get("type", "text"))
        val = payload.get(fname)
        if val is None:
            if req:
                raise PlatformValidationError(
                    f"missing required field {fname!r}", code="SCHEMA_MISMATCH"
                )
            cleaned[fname] = None
            continue
        if ftype == "integer" and (
            isinstance(val, bool) or not isinstance(val, int) or abs(val) > 9007199254740991
        ):
            raise PlatformValidationError(
                f"field {fname!r} expects integer", code="SCHEMA_MISMATCH"
            )
        if ftype == "number" and (
            isinstance(val, bool)
            or not isinstance(val, (int, float))
            or (isinstance(val, int) and abs(val) > 9007199254740991)
            or (isinstance(val, float) and not math.isfinite(val))
        ):
            raise PlatformValidationError(f"field {fname!r} expects number", code="SCHEMA_MISMATCH")
        if ftype == "boolean" and not isinstance(val, bool):
            raise PlatformValidationError(
                f"field {fname!r} expects boolean", code="SCHEMA_MISMATCH"
            )
        if ftype == "text" and (not isinstance(val, str) or len(val) > 4096 or "\0" in val):
            raise PlatformValidationError(f"field {fname!r} expects text", code="SCHEMA_MISMATCH")
        cleaned[fname] = val
    canonical_bytes(cleaned, limit=64 * 1024)
    return cleaned


def _canonical_value(value: Any) -> Any:
    # Browser JSON serialization may convert 12.0 to 12. Preserve semantic
    # JSON numbers, not incidental lexical form, within the exact integer range.
    if isinstance(value, bool) or value is None or isinstance(value, str):
        return value
    if isinstance(value, (int, float)):
        if abs(value) > 9007199254740991 or not math.isfinite(value):
            raise ValueError("number out of portable range")
        return int(value) if isinstance(value, float) and value.is_integer() else value
    if isinstance(value, dict):
        return {key: _canonical_value(item) for key, item in value.items()}
    if isinstance(value, list):
        return [_canonical_value(item) for item in value]
    raise ValueError("unsupported JSON value")


def canonical_bytes(value: Any, *, limit: int = MAX_BUNDLE_BYTES) -> bytes:
    """Deterministic transfer representation, not a signed provenance assertion."""
    try:
        encoded = json.dumps(
            _canonical_value(value),
            sort_keys=True,
            separators=(",", ":"),
            ensure_ascii=False,
            allow_nan=False,
        ).encode("utf-8")
    except (TypeError, ValueError, RecursionError, UnicodeError) as exc:
        raise PlatformValidationError("invalid portable JSON") from exc
    if len(encoded) > limit:
        raise PlatformValidationError(
            "portable data exceeds bounded transfer limit", code="EXTENSION_TRANSFER_LIMIT"
        )
    return encoded


def bundle_digest(bundle: dict[str, Any]) -> str:
    return hashlib.sha256(canonical_bytes(bundle)).hexdigest()


def manifest_contract(manifest: ExtensionManifestRecord) -> dict[str, Any]:
    """Only declared semantics, not installation approval or code execution."""
    return {
        "extension_key": manifest.extension_key,
        "version": manifest.version,
        "api_compatibility": manifest.api_compatibility,
        "permissions": manifest.permissions,
        "commands": manifest.commands,
        "capabilities": manifest.capabilities,
        "entity_schemas": manifest.entity_schemas,
    }


def matches_official_manifest(stored: dict[str, Any], shipped: dict[str, Any]) -> bool:
    """Exact declared runtime contract, independent of tenant-specific IDs and timestamps."""
    if stored.get("enabled") is not True:
        return False
    defaults: dict[str, Any] = {
        "permissions": [],
        "commands": [],
        "capabilities": [],
        "ui_contributions": [],
        "entity_schemas": {},
    }
    return all(
        stored.get(k, default) == shipped.get(k, default) for k, default in defaults.items()
    ) and all(
        stored.get(k) == shipped.get(k) for k in ("extension_key", "version", "api_compatibility")
    )


def validate_stored_manifest_for_mutation(manifest: ExtensionManifestRecord) -> None:
    """Recheck retained metadata before a new mutation, never execute a contribution.

    Older schema/registration versions preserved metadata that may not satisfy
    today's contract. Keep data/history readable and exportable, but do not
    activate or make new records under an unverified manifest.
    """
    try:
        validate_manifest_contract(
            extension_key=manifest.extension_key,
            version=manifest.version,
            api_compatibility=manifest.api_compatibility,
            permissions=manifest.permissions,
            commands=manifest.commands,
            capabilities=manifest.capabilities,
            entity_schemas=manifest.entity_schemas,
            ui_contributions=manifest.ui_contributions,
        )
    except (PlatformValidationError, TypeError, AttributeError) as exc:
        raise PlatformConflictError(
            "stored manifest no longer satisfies the metadata contract",
            code="EXTENSION_MANIFEST_REVALIDATION_REQUIRED",
        ) from exc


def verify_transition(current: ExtensionManifestRecord, target: ExtensionManifestRecord) -> None:
    """A deliberately narrow, data-preserving metadata-only forward transition."""
    if (
        current.extension_key != target.extension_key
        or not target.enabled
        or target.api_compatibility != _SUPPORTED_API_RANGE
        or current.api_compatibility != _SUPPORTED_API_RANGE
    ):
        raise PlatformConflictError(
            "incompatible extension transition", code="EXTENSION_TRANSITION_INCOMPATIBLE"
        )
    try:
        old = tuple(int(v) for v in current.version.split("."))
        new = tuple(int(v) for v in target.version.split("."))
        if len(old) != 3 or len(new) != 3 or old[0] != new[0] or new <= old:
            raise ValueError
    except ValueError as exc:
        raise PlatformConflictError(
            "only forward same-major versions are supported",
            code="EXTENSION_TRANSITION_INCOMPATIBLE",
        ) from exc
    old_contract, new_contract = manifest_contract(current), manifest_contract(target)
    del old_contract["version"], new_contract["version"]
    if canonical_bytes(old_contract) != canonical_bytes(new_contract):
        raise PlatformConflictError(
            "schema, permission or contribution changes require verified migration",
            code="EXTENSION_TRANSITION_MIGRATION_REQUIRED",
        )
