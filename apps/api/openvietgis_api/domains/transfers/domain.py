"""Pure R005 import/export and durable job domain model (zero adapter imports)."""

from __future__ import annotations

import hashlib
import json
import re
from collections.abc import Sequence
from dataclasses import dataclass
from datetime import datetime
from enum import StrEnum
from typing import Any
from uuid import UUID

from openvietgis_api.domains.spatial.crs import (
    get_coordinate_reference,
    get_transformation_profile,
)
from openvietgis_api.domains.spatial.domain import (
    AttributeValue,
    FieldType,
    GeometryDimensions,
    GeometryFamily,
    NativeGeometry,
    SourceField,
)
from openvietgis_api.domains.tenancy.domain import ActorType

MAX_UPLOAD_BYTES = 25 * 1024 * 1024
MAX_ZIP_EXPANDED_BYTES = 50 * 1024 * 1024
MAX_ZIP_ENTRIES = 64
MAX_TRANSFER_FEATURES = 50_000
MAX_TRANSFER_FIELDS = 100
MAX_STRING_LEN = 4096
ARTIFACT_TTL_HOURS = 24

SUPPORTED_CRS_CODES = frozenset({4326, 3857, 4756, 5899})
SUPPORTED_ENCODINGS = ("utf-8", "windows-1258", "cp1252", "latin-1")

_FIELD_ID_RE = re.compile(r"^[A-Za-z][A-Za-z0-9_]{0,63}$")
_SAFE_FILENAME_RE = re.compile(r"[^A-Za-z0-9._-]+")


class TransferFormat(StrEnum):
    GEOPACKAGE = "geopackage"
    GEOJSON = "geojson"
    SHAPEFILE_ZIP = "shapefile_zip"


class JobKind(StrEnum):
    IMPORT = "import"
    EXPORT = "export"


class JobStatus(StrEnum):
    ACCEPTED = "accepted"
    QUEUED = "queued"
    ACTIVE = "active"
    AWAITING_CONFIRMATION = "awaiting_confirmation"
    CANCEL_REQUESTED = "cancel_requested"
    RECOVERABLE_FAILED = "recoverable_failed"
    SUCCEEDED = "succeeded"
    FAILED = "failed"
    DENIED = "denied"
    CANCELLED = "cancelled"


class ArtifactState(StrEnum):
    READY = "ready"
    EXPIRED = "expired"
    UNAVAILABLE = "unavailable"


TERMINAL_JOB_STATUSES = frozenset(
    {JobStatus.SUCCEEDED, JobStatus.FAILED, JobStatus.DENIED, JobStatus.CANCELLED}
)

VALID_JOB_TRANSITIONS: dict[JobStatus, frozenset[JobStatus]] = {
    JobStatus.ACCEPTED: frozenset(
        {
            JobStatus.QUEUED,
            JobStatus.ACTIVE,
            JobStatus.AWAITING_CONFIRMATION,
            JobStatus.CANCEL_REQUESTED,
            JobStatus.CANCELLED,
            JobStatus.DENIED,
            JobStatus.FAILED,
        }
    ),
    JobStatus.QUEUED: frozenset(
        {JobStatus.ACTIVE, JobStatus.CANCEL_REQUESTED, JobStatus.CANCELLED, JobStatus.DENIED}
    ),
    JobStatus.AWAITING_CONFIRMATION: frozenset(
        {JobStatus.ACTIVE, JobStatus.CANCELLED, JobStatus.DENIED, JobStatus.FAILED}
    ),
    JobStatus.ACTIVE: frozenset(
        {
            JobStatus.SUCCEEDED,
            JobStatus.RECOVERABLE_FAILED,
            JobStatus.FAILED,
            JobStatus.DENIED,
            JobStatus.CANCEL_REQUESTED,
            JobStatus.CANCELLED,
        }
    ),
    JobStatus.CANCEL_REQUESTED: frozenset({JobStatus.CANCELLED, JobStatus.SUCCEEDED}),
    JobStatus.RECOVERABLE_FAILED: frozenset(
        {JobStatus.ACTIVE, JobStatus.CANCELLED, JobStatus.DENIED, JobStatus.FAILED}
    ),
    JobStatus.SUCCEEDED: frozenset(),
    JobStatus.FAILED: frozenset(),
    JobStatus.DENIED: frozenset(),
    JobStatus.CANCELLED: frozenset(),
}


def can_transition(current: JobStatus, target: JobStatus) -> bool:
    return target in VALID_JOB_TRANSITIONS.get(current, frozenset())


def canonical_fingerprint(payload: dict[str, Any]) -> str:
    raw = json.dumps(payload, sort_keys=True, separators=(",", ":"), allow_nan=False)
    return hashlib.sha256(raw.encode("utf-8")).hexdigest()


def normalize_encoding(raw: str | None) -> str | None:
    if raw is None:
        return None
    cleaned = raw.strip().lower().replace("_", "-")
    aliases = {
        "utf8": "utf-8",
        "utf-8": "utf-8",
        "utf-8-sig": "utf-8",
        "windows-1258": "windows-1258",
        "cp1258": "windows-1258",
        "1258": "windows-1258",
        "windows-1252": "cp1252",
        "cp1252": "cp1252",
        "1252": "cp1252",
        "iso-8859-1": "latin-1",
        "latin1": "latin-1",
        "latin-1": "latin-1",
    }
    return aliases.get(cleaned)


def sanitize_filename(raw: str, fallback: str = "upload.bin") -> str:
    base = raw.replace("\\", "/").rsplit("/", 1)[-1].strip()
    cleaned = _SAFE_FILENAME_RE.sub("_", base).strip("._")
    if not cleaned:
        return fallback
    return cleaned[:200]


def sanitize_field_id(raw_name: str, used: set[str], ordinal: int) -> str:
    candidate = re.sub(r"[^A-Za-z0-9_]+", "_", raw_name.strip()).strip("_")
    if not candidate or not candidate[0].isalpha():
        candidate = f"fld_{candidate or ordinal}"
    candidate = candidate[:60]
    if not _FIELD_ID_RE.match(candidate):
        candidate = f"fld_{ordinal}"
    base = candidate
    suffix = 1
    while candidate in used:
        suffix += 1
        candidate = f"{base[:56]}_{suffix}"
    used.add(candidate)
    return candidate


@dataclass(frozen=True, slots=True)
class SourceAsset:
    id: UUID
    organization_id: UUID
    project_id: UUID
    filename: str
    detected_format: TransferFormat
    byte_size: int
    sha256: str
    storage_key: str
    uploaded_by_type: ActorType
    uploaded_by_id: UUID
    uploaded_by_display: str
    created_at: datetime


@dataclass(frozen=True, slots=True)
class ValidationFinding:
    severity: str  # "error" | "warning" | "info"
    code: str
    message: str
    feature_index: int | None = None
    field_name: str | None = None

    def to_wire(self) -> dict[str, Any]:
        return {
            "severity": self.severity,
            "code": self.code,
            "message": self.message,
            "feature_index": self.feature_index,
            "field_name": self.field_name,
        }


@dataclass(frozen=True, slots=True)
class InspectedField:
    field_id: str
    name: str
    type: FieldType
    nullable: bool
    ordinal: int
    source_name: str

    def to_wire(self) -> dict[str, Any]:
        return {
            "field_id": self.field_id,
            "name": self.name,
            "type": self.type.value,
            "nullable": self.nullable,
            "ordinal": self.ordinal,
            "source_name": self.source_name,
        }


@dataclass(frozen=True, slots=True)
class InspectedMember:
    name: str
    eligible: bool
    reason: str | None
    geometry_family: str
    dimensions: str
    feature_count: int
    detected_crs: int | None
    detected_encoding: str | None
    fields: tuple[InspectedField, ...]

    def to_wire(self) -> dict[str, Any]:
        return {
            "name": self.name,
            "eligible": self.eligible,
            "reason": self.reason,
            "geometry_family": self.geometry_family,
            "dimensions": self.dimensions,
            "feature_count": self.feature_count,
            "detected_crs": self.detected_crs,
            "detected_encoding": self.detected_encoding,
            "fields": [item.to_wire() for item in self.fields],
        }


@dataclass(frozen=True, slots=True)
class ConversionDisclosure:
    code: str
    message: str
    requires_approval: bool
    affected_fields: tuple[str, ...] = ()

    def to_wire(self) -> dict[str, Any]:
        return {
            "code": self.code,
            "message": self.message,
            "requires_approval": self.requires_approval,
            "affected_fields": list(self.affected_fields),
        }


@dataclass(frozen=True, slots=True)
class ParsedFeatureRecord:
    feature_id: str
    attributes: dict[str, AttributeValue]
    geometry: NativeGeometry


@dataclass(frozen=True, slots=True)
class ParsedMemberData:
    member_name: str
    geometry_family: GeometryFamily
    dimensions: GeometryDimensions
    detected_crs: int | None
    detected_encoding: str | None
    requires_encoding_resolution: bool
    fields: tuple[InspectedField, ...]
    features: tuple[ParsedFeatureRecord, ...]
    findings: tuple[ValidationFinding, ...]
    extent: dict[str, float | int] | None


@dataclass(frozen=True, slots=True)
class ImportInspectionReport:
    asset_sha256: str
    detected_format: TransferFormat
    members: tuple[InspectedMember, ...]
    selected_member: str | None
    requires_member_selection: bool
    requires_crs_resolution: bool
    requires_encoding_resolution: bool
    resolved_source_crs: int | None
    target_crs: int | None
    resolved_encoding: str | None
    sample_only: bool
    feature_count: int
    geometry_family: str | None
    dimensions: str | None
    extent: dict[str, float | int] | None
    fields: tuple[InspectedField, ...]
    findings: tuple[ValidationFinding, ...]
    conversions: tuple[ConversionDisclosure, ...]
    confirmable: bool
    inspection_fingerprint: str

    def to_wire(self) -> dict[str, Any]:
        return {
            "asset_sha256": self.asset_sha256,
            "detected_format": self.detected_format.value,
            "members": [m.to_wire() for m in self.members],
            "selected_member": self.selected_member,
            "requires_member_selection": self.requires_member_selection,
            "requires_crs_resolution": self.requires_crs_resolution,
            "requires_encoding_resolution": self.requires_encoding_resolution,
            "resolved_source_crs": self.resolved_source_crs,
            "target_crs": self.target_crs,
            "resolved_encoding": self.resolved_encoding,
            "sample_only": self.sample_only,
            "feature_count": self.feature_count,
            "geometry_family": self.geometry_family,
            "dimensions": self.dimensions,
            "extent": self.extent,
            "fields": [f.to_wire() for f in self.fields],
            "findings": [f.to_wire() for f in self.findings],
            "conversions": [c.to_wire() for c in self.conversions],
            "confirmable": self.confirmable,
            "inspection_fingerprint": self.inspection_fingerprint,
        }


@dataclass(frozen=True, slots=True)
class ExportFieldMapping:
    field_id: str
    source_name: str
    output_name: str
    type: FieldType

    def to_wire(self) -> dict[str, Any]:
        return {
            "field_id": self.field_id,
            "source_name": self.source_name,
            "output_name": self.output_name,
            "type": self.type.value,
        }


@dataclass(frozen=True, slots=True)
class ExportPreviewReport:
    layer_id: UUID
    source_id: UUID
    content_revision: str
    schema_revision: str
    output_format: TransferFormat
    source_crs: int
    output_crs: int
    transformation_operation_id: str | None
    feature_count: int
    geometry_family: str
    field_mappings: tuple[ExportFieldMapping, ...]
    disclosures: tuple[ConversionDisclosure, ...]
    confirmable: bool
    preview_fingerprint: str

    def to_wire(self) -> dict[str, Any]:
        return {
            "layer_id": str(self.layer_id),
            "source_id": str(self.source_id),
            "content_revision": self.content_revision,
            "schema_revision": self.schema_revision,
            "output_format": self.output_format.value,
            "source_crs": self.source_crs,
            "output_crs": self.output_crs,
            "transformation_operation_id": self.transformation_operation_id,
            "feature_count": self.feature_count,
            "geometry_family": self.geometry_family,
            "field_mappings": [m.to_wire() for m in self.field_mappings],
            "disclosures": [d.to_wire() for d in self.disclosures],
            "confirmable": self.confirmable,
            "preview_fingerprint": self.preview_fingerprint,
        }


@dataclass(frozen=True, slots=True)
class JobAttempt:
    id: UUID
    organization_id: UUID
    project_id: UUID
    job_id: UUID
    attempt_number: int
    status: str
    actor_type: ActorType
    actor_id: UUID
    credential_id: UUID | None
    started_at: datetime
    finished_at: datetime | None
    error_code: str | None
    error_message: str | None


@dataclass(frozen=True, slots=True)
class DerivedArtifact:
    id: UUID
    organization_id: UUID
    project_id: UUID
    job_id: UUID
    source_id: UUID
    layer_id: UUID
    source_content_revision: str
    source_schema_revision: str
    output_format: TransferFormat
    output_crs: int
    filename: str
    content_type: str
    byte_size: int
    sha256: str
    storage_key: str
    comparison_report: dict[str, Any]
    created_by_type: ActorType
    created_by_id: UUID
    created_by_display: str
    created_at: datetime
    expires_at: datetime
    state: ArtifactState


@dataclass(frozen=True, slots=True)
class TransferJob:
    id: UUID
    organization_id: UUID
    project_id: UUID
    operation_id: UUID
    kind: JobKind
    request_fingerprint: str
    status: JobStatus
    actor_type: ActorType
    actor_id: UUID
    actor_display_snapshot: str
    credential_id: UUID | None
    source_asset_id: UUID | None
    target_source_id: UUID | None
    target_layer_id: UUID | None
    source_content_revision: str | None
    source_schema_revision: str | None
    artifact_id: UUID | None
    attempt_count: int
    options_json: dict[str, Any]
    inspection_report: dict[str, Any] | None
    result_report: dict[str, Any] | None
    error_code: str | None
    error_message: str | None
    created_at: datetime
    updated_at: datetime
    completed_at: datetime | None
    attempts: tuple[JobAttempt, ...] = ()
    artifact: DerivedArtifact | None = None


def resolve_crs_transform(source_crs: int, target_crs: int) -> str | None:
    """Return approved R003 transformation operation_id, or None if same CRS."""
    if source_crs not in SUPPORTED_CRS_CODES or target_crs not in SUPPORTED_CRS_CODES:
        raise ValueError("Unsupported CRS code")
    if get_coordinate_reference(source_crs) is None or get_coordinate_reference(target_crs) is None:
        raise ValueError("CRS code not readable in approved R003 registry")
    if source_crs == target_crs:
        return None
    profile = get_transformation_profile(source_crs, target_crs)
    if profile is None:
        raise ValueError(
            f"No approved coordinate operation from EPSG:{source_crs} to EPSG:{target_crs}"
        )
    return profile.operation_id


def plan_export_mapping(
    fields: Sequence[SourceField],
    output_format: TransferFormat,
    source_crs: int,
    output_crs: int,
    *,
    has_long_text: bool = False,
) -> tuple[tuple[ExportFieldMapping, ...], tuple[ConversionDisclosure, ...], str | None]:
    """Compute deterministic field mapping and required representational loss disclosures."""
    op_id = resolve_crs_transform(source_crs, output_crs)
    if output_format == TransferFormat.GEOJSON and output_crs != 4326:
        raise ValueError("RFC 7946 GeoJSON export requires EPSG:4326 output CRS")

    disclosures: list[ConversionDisclosure] = []
    if op_id is not None:
        disclosures.append(
            ConversionDisclosure(
                code="CRS_TRANSFORMATION",
                message=(
                    f"Coordinates will be transformed from EPSG:{source_crs} to "
                    f"EPSG:{output_crs} via approved operation {op_id}."
                ),
                requires_approval=False,
            )
        )

    if output_format == TransferFormat.GEOPACKAGE:
        mappings = tuple(ExportFieldMapping(f.field_id, f.name, f.field_id, f.type) for f in fields)
        return mappings, tuple(disclosures), op_id

    if output_format == TransferFormat.GEOJSON:
        mappings = tuple(ExportFieldMapping(f.field_id, f.name, f.field_id, f.type) for f in fields)
        stringified = tuple(
            f.field_id for f in fields if f.type in (FieldType.DECIMAL, FieldType.TIMESTAMP)
        )
        if stringified:
            disclosures.append(
                ConversionDisclosure(
                    code="GEOJSON_TYPED_STRING_ENCODING",
                    message=(
                        "RFC 7946 GeoJSON encodes decimal and timestamp fields as "
                        "exact JSON strings rather than distinct native types."
                    ),
                    requires_approval=True,
                    affected_fields=stringified,
                )
            )
        return mappings, tuple(disclosures), op_id

    # ZIP Shapefile (DBF 10-char limit + 254-char text limit + timestamp/boolean mapping)
    used_dbf: set[str] = {"OVG_FID"}
    dbf_mappings: list[ExportFieldMapping] = []
    truncated_names: list[str] = []
    timestamp_fields: list[str] = []
    decimal_fields: list[str] = []

    for f in fields:
        clean = re.sub(r"[^A-Za-z0-9_]", "_", f.field_id).upper()
        if not clean or not clean[0].isalpha():
            clean = f"F_{clean}"
        candidate = clean[:10]
        idx = 1
        while candidate in used_dbf:
            suffix = f"_{idx}"
            candidate = clean[: 10 - len(suffix)] + suffix
            idx += 1
        used_dbf.add(candidate)
        if candidate.lower() != f.field_id.lower():
            truncated_names.append(f"{f.field_id}->{candidate}")
        if f.type == FieldType.TIMESTAMP:
            timestamp_fields.append(f.field_id)
        if f.type == FieldType.DECIMAL:
            decimal_fields.append(f.field_id)
        dbf_mappings.append(ExportFieldMapping(f.field_id, f.name, candidate, f.type))

    if truncated_names:
        disclosures.append(
            ConversionDisclosure(
                code="DBF_FIELD_NAME_TRUNCATED",
                message=(
                    "Shapefile DBF limits field names to 10 ASCII characters; "
                    "deterministic mapping will be applied."
                ),
                requires_approval=True,
                affected_fields=tuple(truncated_names),
            )
        )
    if timestamp_fields:
        disclosures.append(
            ConversionDisclosure(
                code="DBF_TIMESTAMP_AS_TEXT",
                message=(
                    "Shapefile DBF has no ISO-8601 UTC timestamp type; "
                    "timestamps will be written as UTC ISO-8601 strings."
                ),
                requires_approval=True,
                affected_fields=tuple(timestamp_fields),
            )
        )
    if decimal_fields:
        disclosures.append(
            ConversionDisclosure(
                code="DBF_DECIMAL_FIXED_PRECISION",
                message=(
                    "Shapefile DBF encodes decimals as fixed-width numeric fields "
                    "(up to 6 decimal places)."
                ),
                requires_approval=True,
                affected_fields=tuple(decimal_fields),
            )
        )
    text_fields = tuple(f.field_id for f in fields if f.type == FieldType.TEXT)
    if text_fields:
        disclosures.append(
            ConversionDisclosure(
                code="DBF_TEXT_NULL_EMPTY",
                message=(
                    "DBF character fields cannot preserve the distinction between "
                    "NULL, empty and padded text."
                ),
                requires_approval=True,
                affected_fields=text_fields,
            )
        )
    if has_long_text:
        disclosures.append(
            ConversionDisclosure(
                code="DBF_TEXT_TRUNCATED_254",
                message="Shapefile DBF truncates character values exceeding 254 bytes.",
                requires_approval=True,
                affected_fields=tuple(f.field_id for f in fields if f.type == FieldType.TEXT),
            )
        )
    return tuple(dbf_mappings), tuple(disclosures), op_id
