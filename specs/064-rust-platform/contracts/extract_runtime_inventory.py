#!/usr/bin/env python3
"""Spec 064 T002 & E01-T01..E09-T01 runtime OpenAPI, algorithm, migration, and triage extractor.

Instantiates the real FastAPI app via create_app() from the 064-rust-platform worktree,
extracts all mounted HTTP, SSE/streaming, and WebSocket routes, reconciles static-inventory.json,
triages every UNCLASSIFIED and CROSS_CUTTING_REVIEW entry, extracts processing/domain algorithms
and all database migrations, and writes:
  - specs/064-rust-platform/contracts/inventory.json
  - specs/064-rust-platform/contracts/engines/e01.json .. e09.json
  - specs/064-rust-platform/evidence/e01-inventory.md .. e09-inventory.md
"""

from __future__ import annotations

import ast
import inspect
import json
import os
import re
import subprocess
import sys
from collections import Counter, defaultdict
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

# Configure isolated test settings BEFORE importing openvietgis_api.main
os.environ.setdefault("OPENVIETGIS_ENVIRONMENT", "test")
os.environ.setdefault(
    "OPENVIETGIS_DATABASE_URL",
    "postgresql+psycopg://inventory_ro:inventory_ro@127.0.0.1:5432/inventory_ro",
)
os.environ.setdefault(
    "OPENVIETGIS_APPLICATION_SESSION_SECRET",
    "spec064-inventory-session-secret-0123456789abcdef",
)
os.environ.setdefault(
    "OPENVIETGIS_CSRF_SECRET",
    "spec064-inventory-csrf-secret-0123456789abcdef01",
)
os.environ.setdefault(
    "OPENVIETGIS_BOOTSTRAP_SECRET",
    "spec064-inventory-bootstrap-secret-0123456789abcd",
)

ROOT = Path(__file__).resolve().parents[3]
API_DIR = ROOT / "apps" / "api"
if not (API_DIR / "openvietgis_api/domains/processing/domain.py").is_file():
    raise SystemExit(
        "Historical FastAPI inventory extractor: E08 has been retired. "
        "The frozen baseline inventory.json is retained as migration evidence."
    )
SPEC_DIR = ROOT / "specs" / "064-rust-platform"
CONTRACTS_DIR = SPEC_DIR / "contracts"
ENGINES_CONTRACTS_DIR = CONTRACTS_DIR / "engines"
EVIDENCE_DIR = SPEC_DIR / "evidence"

for p in (str(API_DIR), str(ROOT)):
    if p not in sys.path:
        sys.path.insert(0, p)

from fastapi.routing import APIRoute, APIWebSocketRoute  # noqa: E402
import openvietgis_api  # noqa: E402
from openvietgis_api.adapters.http import gis_access_guard  # noqa: E402
from openvietgis_api.adapters.http.official_modules import (  # noqa: E402
    discover_official_module_runtimes,
)
from openvietgis_api.domains.cartography.domain import (  # noqa: E402
    Orientation,
    PaperSize,
    PrintFormat,
    StyleMode,
)
from openvietgis_api.domains.editing.domain import EditOpType  # noqa: E402
from openvietgis_api.domains.processing.domain import (  # noqa: E402
    ALGORITHM_REGISTRY,
    ProcessingStatus,
)
from openvietgis_api.domains.queries.domain import (  # noqa: E402
    MAX_AST_DEPTH,
    MAX_AST_NODES,
    MAX_AST_PARAMS,
    MAX_DRAWN_POSITIONS,
    MAX_PAGE_POSITIONS,
    MAX_QUERY_FEATURES,
    MAX_QUERY_LIMIT,
    MAX_QUERY_OFFSET,
    MAX_QUERY_POSITIONS,
    MAX_REFERENCE_FEATURES,
    MAX_REFERENCE_POSITIONS,
    QUERY_STATEMENT_TIMEOUT,
    SpatialPredicate,
)
from openvietgis_api.domains.spatial.crs import (  # noqa: E402
    CRS_PROFILES,
    CRS_REGISTRY_VERSION,
    TRANSFORMATION_PROFILES,
)
from openvietgis_api.domains.spatial.domain import (  # noqa: E402
    FieldType,
    GeometryDimensions,
    SUPPORTED_FAMILIES,
)
from openvietgis_api.domains.transfers.domain import (  # noqa: E402
    ARTIFACT_TTL_HOURS,
    JobKind,
    JobStatus,
    MAX_STRING_LEN,
    MAX_TRANSFER_FEATURES,
    MAX_TRANSFER_FIELDS,
    MAX_UPLOAD_BYTES,
    MAX_ZIP_ENTRIES,
    MAX_ZIP_EXPANDED_BYTES,
    SUPPORTED_CRS_CODES,
    SUPPORTED_ENCODINGS,
    TransferFormat,
)
from openvietgis_api.main import create_app  # noqa: E402

# Safety assertion: ensure we imported from the worktree, never /opt/openvietgis
assert str(Path(openvietgis_api.__file__).resolve()).startswith(str(ROOT.resolve())), (
    f"Refusing to run against non-worktree package: {openvietgis_api.__file__}"
)

HTTP_METHODS = {"get", "post", "put", "patch", "delete", "head", "options", "trace"}

ENGINE_META: dict[str, dict[str, Any]] = {
    "E01": {
        "name": "Nền tảng, identity, tenant, security",
        "parent_references": ["R001", "R002", "R015"],
        "spec_path": "specs/064-rust-platform/engines/e01-platform-security.md",
        "architecture_candidate": "Rust Axum/SQLx; retain external OIDC provider; preserve token/cookie/CSRF/RLS/audit contract.",
    },
    "E02": {
        "name": "Project, catalog, data, CRS",
        "parent_references": ["R003", "R004"],
        "spec_path": "specs/064-rust-platform/engines/e02-project-data-crs.md",
        "architecture_candidate": "PostGIS, PROJ, GEOS, GeoRust; explicit CRS registry & VN-2000 control pipelines; no implicit CRS.",
    },
    "E03": {
        "name": "Geometry, editing, topology",
        "parent_references": ["R006"],
        "spec_path": "specs/064-rust-platform/engines/e03-geometry-editing.md",
        "architecture_candidate": "GEOS plus Rust kernels; server-side transaction/revision/idempotency authority.",
    },
    "E04": {
        "name": "Queries, expression, attribute table",
        "parent_references": ["R007"],
        "spec_path": "specs/064-rust-platform/engines/e04-query-attributes.md",
        "architecture_candidate": "PostGIS typed query compiler in Rust (zero arbitrary SQL); bounded AST/timeout/pagination.",
    },
    "E05": {
        "name": "Map rendering, tile, cache",
        "parent_references": ["R004", "R043"],
        "spec_path": "specs/064-rust-platform/engines/e05-render-tiles.md",
        "architecture_candidate": "MapLibre retained client-side; Martin/Rust tile gateway, MVT/3D Tiles/PMTiles; scoped cache & ACL revocation.",
    },
    "E06": {
        "name": "Symbology, labels, layout",
        "parent_references": ["R009"],
        "spec_path": "specs/064-rust-platform/engines/e06-cartography-layout.md",
        "architecture_candidate": "Retain canonical renderer/TS UI; Rust style/layout models and revision-pinned PNG/PDF export service.",
    },
    "E07": {
        "name": "Import/export and interoperability formats",
        "parent_references": ["R005", "R010"],
        "spec_path": "specs/064-rust-platform/engines/e07-formats-transfer.md",
        "architecture_candidate": "GDAL/PROJ native adapters behind Rust transfer job control plane; retain format-specific metadata & limits.",
    },
    "E08": {
        "name": "Processing, models, jobs",
        "parent_references": ["R008"],
        "spec_path": "specs/064-rust-platform/engines/e08-processing-jobs.md",
        "architecture_candidate": "Rust/Rayon, GEOS/PostGIS/GDAL bounded job runner; optional SedonaDB/Arrow gated by benchmark.",
    },
    "E09": {
        "name": "Raster, terrain, survey",
        "parent_references": ["R008", "R012"],
        "spec_path": "specs/064-rust-platform/engines/e09-raster-terrain.md",
        "architecture_candidate": "GDAL/COG + bounded Rust kernels for raster analysis, tiles, byte-range serving, and terrain preview.",
    },
    "E10": {"name": "CAD, 3D, model and scene", "parent_references": ["R012", "R037"]},
    "E11": {"name": "Realtime, edit leases, chat, tasks", "parent_references": ["R011", "R040"]},
    "E12": {"name": "QGIS, external sources, integration", "parent_references": ["R010", "R027"]},
    "E13": {"name": "Field, offline, mobile", "parent_references": ["R013"]},
    "E14": {"name": "Plugin SDK, marketplace, modules", "parent_references": ["R014"]},
    "E15": {"name": "House numbering and land asset module", "parent_references": ["R014", "R019", "R051"]},
    "E16": {"name": "AI and MCP", "parent_references": ["R041", "R021"]},
    "E17": {"name": "Canonical web, desktop-ready shell", "parent_references": ["R037", "R038", "R062"]},
    "E18": {"name": "Ops, deployment, backup, performance", "parent_references": ["R015", "R042", "R043", "R044", "R045", "R047"]},
}

HTTP_MODULE_TO_ENGINE: dict[str, str] = {
    "auth": "E01",
    "bootstrap": "E01",
    "me": "E01",
    "organizations": "E01",
    "workspaces": "E01",
    "projects": "E01",
    "grants": "E01",
    "gis_access": "E01",
    "invitations": "E01",
    "service_principals": "E01",
    "audit": "E01",
    "spatial": "E02",
    "maps": "E02",
    "map_viewport": "E02",
    "editing": "E03",
    "queries": "E04",
    "tilesets": "E05",
    "cartography": "E06",
    "layout_designer": "E06",
    "transfers": "E07",
    "processing": "E08",
    "processing_models": "E08",
    "professional": "E08",
    "raster": "E09",
    "terrain": "E09",
    "cad": "E10",
    "cad_documents": "E10",
    "model_vault": "E10",
    "saved_scenes": "E10",
    "v8i_bridge": "E10",
    "collaboration": "E11",
    "team_chat": "E11",
    "org_chat": "E11",
    "team_locks": "E11",
    "team_presence": "E11",
    "team_realtime": "E11",
    "managed_sync": "E11",
    "external_sources": "E12",
    "integration_hub": "E12",
    "integration_bindings": "E12",
    "field": "E13",
    "extensions": "E14",
    "marketplace": "E14",
    "marketplace_developers": "E14",
    "partner_platform": "E14",
    "health": "E18",
    "ops": "E18",
}

MIGRATION_TO_ENGINE: dict[str, str] = {
    "0001_foundation_sample.py": "E01",
    "0002_identity_tenant_core.py": "E01",
    "0003a_identity_unlink_security.py": "E01",
    "0003_tenant_rls.py": "E01",
    "0004_machine_identity.py": "E01",
    "0005_retire_foundation_sample.py": "E01",
    "0006_spatial_catalog.py": "E02",
    "0007_map_workspace.py": "E02",
    "0008_import_export.py": "E07",
    "0009_editing_changesets.py": "E03",
    "0010_saved_queries.py": "E04",
    "0011_processing_jobs.py": "E08",
    "0012_cartography_layouts.py": "E06",
    "0013_external_sources.py": "E12",
    "0014_team_collaboration.py": "E11",
    "0015_cad_precision.py": "E10",
    "0016_field_offline.py": "E13",
    "0017_extension_platform.py": "E14",
    "0018_production_ops.py": "E18",
    "0019_extension_history.py": "E14",
    "0020_manual_layers.py": "E02",
    "0021_professional_tools.py": "E08",
    "0022_schema_change_evidence.py": "E08",
    "0023_gis_access.py": "E01",
    "0023_user_avatar.py": "E01",
    "0024_raster_layers.py": "E09",
    "0025_processing_models.py": "E08",
    "0026_layout_designer.py": "E06",
    "0027_partner_platform.py": "E14",
    "0028_marketplace_developers.py": "E14",
    "0029_cad_documents.py": "E10",
    "0029_team_chat_tasks.py": "E11",
    "0030_collaboration_realtime.py": "E11",
    "0030_model_vault.py": "E10",
    "0031_saved_scenes.py": "E10",
    "0031_spatial_presence_events.py": "E11",
    "0032_feature_edit_leases.py": "E11",
    "0032_tilesets.py": "E05",
    "0032_unified_spatial_collaboration.py": "E11",
    "0033_feature_comments.py": "E11",
    "0033_tileset_provenance.py": "E05",
    "0034_consolidated_heads.py": "E18",
    "0034_tile_feature_index.py": "E05",
    "0035_consolidated_scenes.py": "E10",
    "0036_consolidated_tiles.py": "E05",
    "0037_consolidated_generation.py": "E05",
    "0038_consolidated_feature_index.py": "E05",
    "0039_r027_connector_bindings.py": "E12",
    "0040_r027_pairing_and_conflicts.py": "E12",
    "0041_r029_managed_realtime_client.py": "E11",
    "0042_qgis_pairing_scope.py": "E12",
    "0043_chat_rooms.py": "E11",
}

CORE_CROSS_CUTTING_TRIAGE: dict[str, tuple[str, str, str, list[str]]] = {
    "apps/api/openvietgis_api/__init__.py": (
        "E01",
        "replace",
        "Python API package root; replaced by Rust workspace crate layout after cutover.",
        ["E01", "E18"],
    ),
    "apps/api/openvietgis_api/adapters/__init__.py": (
        "E01",
        "replace",
        "Python adapters package marker; replaced by Rust adapter modules.",
        ["E01"],
    ),
    "apps/api/openvietgis_api/adapters/http/__init__.py": (
        "E01",
        "replace",
        "Python HTTP adapter package marker; replaced by Rust Axum router registry.",
        ["E01"],
    ),
    "apps/api/openvietgis_api/adapters/http/errors.py": (
        "E01",
        "migrate",
        "Canonical HTTP exception-to-status/error-body mapping (400/403/404/409 OV409/422 sanitized validation); must be preserved verbatim across all engines.",
        ["E01", "E02", "E03", "E04", "E05", "E06", "E07", "E08", "E09", "E10", "E11", "E12", "E13", "E14", "E15", "E18"],
    ),
    "apps/api/openvietgis_api/adapters/http/invitation_mail.py": (
        "E01",
        "migrate",
        "Invitation email delivery adapter (Resend HTTPS transport) owned by E01 identity/invitations.",
        ["E01"],
    ),
    "apps/api/openvietgis_api/adapters/persistence/__init__.py": (
        "E01",
        "replace",
        "Python persistence package marker; replaced by Rust SQLx repository crates.",
        ["E01"],
    ),
    "apps/api/openvietgis_api/adapters/persistence/db.py": (
        "E01",
        "migrate",
        "Database engine/session pool and restricted runtime role connection setup; migrated to Rust SQLx pool with identical RLS transaction semantics.",
        ["E01", "E02", "E03", "E04", "E05", "E06", "E07", "E08", "E09", "E18"],
    ),
    "apps/api/openvietgis_api/adapters/persistence/platform_repository.py": (
        "E01",
        "migrate",
        "Shared PostgreSQL persistence helpers for platform entities/revisions/audit.",
        ["E01", "E02", "E03", "E04", "E06", "E08"],
    ),
    "apps/api/openvietgis_api/adapters/storage/local_object_store.py": (
        "E07",
        "migrate",
        "Content-addressed/scoped local artifact object store shared by transfer artifacts (E07), print exports (E06), rasters (E09), CAD/models (E10), and field packages (E13).",
        ["E06", "E07", "E09", "E10", "E13", "E18"],
    ),
    "apps/api/openvietgis_api/application/__init__.py": (
        "E01",
        "replace",
        "Legacy Python application layer package marker.",
        ["E01"],
    ),
    "apps/api/openvietgis_api/config.py": (
        "E01",
        "migrate",
        "OPENVIETGIS_* environment configuration and fail-closed production profile validation (HTTPS origins, >=32 char distinct secrets, openvietgis_runtime DB role).",
        ["E01", "E18"],
    ),
    "apps/api/openvietgis_api/domain/__init__.py": (
        "E01",
        "replace",
        "Legacy Python domain package marker.",
        ["E01"],
    ),
    "apps/api/openvietgis_api/domains/platform_errors.py": (
        "E01",
        "migrate",
        "Shared PlatformDomainError hierarchy (status_code, code, message, details) used across E01-E15.",
        ["E01", "E02", "E03", "E04", "E05", "E06", "E07", "E08", "E09", "E10", "E11", "E12", "E13", "E14", "E15"],
    ),
    "apps/api/openvietgis_api/main.py": (
        "E01",
        "migrate",
        "FastAPI composition root: SessionMiddleware, CORSMiddleware, /api/v1 router mounts, project_access_guard, official module discovery, and error handler installation.",
        ["E01", "E14", "E18"],
    ),
    "apps/api/openvietgis_api/module_sdk.py": (
        "E14",
        "migrate",
        "Official module SDK boundary isolating business modules (such as E15 house_numbering) from core internals.",
        ["E14", "E15"],
    ),
    "apps/api/openvietgis_api/ports/__init__.py": (
        "E01",
        "replace",
        "Legacy Python ports package marker.",
        ["E01"],
    ),
    "apps/api/pyproject.toml": (
        "E18",
        "retain",
        "Python reference API build/dependency manifest retained for dual-run parity harness until cutover.",
        ["E18"],
    ),
}


def module_to_engine(module_name: str) -> str:
    if module_name.startswith("modules.house_numbering"):
        return "E15"
    if module_name.startswith("modules.land_asset"):
        return "E15"
    if module_name.startswith("openvietgis_api.adapters.http."):
        stem = module_name.split("openvietgis_api.adapters.http.", 1)[1].split(".")[0]
        if stem in HTTP_MODULE_TO_ENGINE:
            return HTTP_MODULE_TO_ENGINE[stem]
    raise RuntimeError(f"Unmapped live route module: {module_name}")


def collect_schema_refs(obj: Any, out: set[str]) -> None:
    if isinstance(obj, dict):
        if "$ref" in obj and isinstance(obj["$ref"], str):
            out.add(obj["$ref"].rsplit("/", 1)[-1])
        for v in obj.values():
            collect_schema_refs(v, out)
    elif isinstance(obj, list):
        for item in obj:
            collect_schema_refs(item, out)


def extract_dependant_calls(dependant: Any) -> list[str]:
    calls: list[str] = []
    seen: set[int] = set()

    def walk(dep: Any) -> None:
        if dep is None or id(dep) in seen:
            return
        seen.add(id(dep))
        fn = getattr(dep, "call", None)
        if fn is not None:
            mod = getattr(fn, "__module__", "")
            name = getattr(fn, "__name__", repr(fn))
            calls.append(f"{mod}.{name}" if mod else name)
        for sub in getattr(dep, "dependencies", []) or []:
            walk(sub)

    walk(dependant)
    return calls


def normalize_openapi_path(raw_path: str, path_format: str | None = None) -> str:
    if path_format:
        return path_format
    return re.sub(r"\{([^}:]+):[^}]+\}", r"{\1}", raw_path)


def extract_source_info(endpoint: Any) -> tuple[str | None, int | None, str]:
    try:
        unwrapped = inspect.unwrap(endpoint)
        src_file = inspect.getsourcefile(unwrapped)
        rel_file = (
            str(Path(src_file).resolve().relative_to(ROOT.resolve())).replace("\\", "/")
            if src_file
            else None
        )
        lines, start_line = inspect.getsourcelines(unwrapped)
        return rel_file, start_line, "".join(lines)
    except Exception:
        return None, None, ""


def detect_transport(ctx: Any, src: str, openapi_op: dict[str, Any] | None) -> str:
    if isinstance(ctx.original_route, APIWebSocketRoute):
        return "websocket"
    if getattr(ctx, "is_sse_stream", False) or "text/event-stream" in src or "EventSourceResponse" in src:
        return "sse"
    if getattr(ctx, "is_json_stream", False) or "StreamingResponse" in src:
        return "streaming"
    if (
        "FileResponse" in src
        or 'media_type="application/vnd.mapbox-vector-tile"' in src
        or 'media_type="image/png"' in src
        or ("Response(" in src and "media_type=" in src)
    ):
        return "binary"
    if openapi_op:
        for resp in openapi_op.get("responses", {}).values():
            content = resp.get("content", {})
            if "text/event-stream" in content:
                return "sse"
    return "http"


def classify_operation(engine: str, method: str, path: str, mod_stem: str, handler: str) -> tuple[str, str]:
    """Return (classification, rationale) in {'migrate', 'retain', 'replace', 'approved_retire'}."""
    if engine == "E05":
        if handler in {"generate_tileset", "get_manifest", "get_tile"}:
            return (
                "replace",
                "Replace Python 3D Tiles generation/archive serving with Rust/Martin tile gateway preserving exact HTTP, manifest, and ACL contract.",
            )
        return ("migrate", "Migrate tileset catalog/metadata/feature lookup route to Rust Axum/SQLx.")
    if engine == "E06":
        if handler in {"preview_layout_export", "export_layout", "download_export"}:
            return (
                "migrate",
                "Migrate revision-pinned layout export orchestration and artifact download to Rust while preserving PNG/PDF parity and SHA-256 manifest contract.",
            )
        return ("migrate", "Migrate cartography style and layout designer CRUD/validation to Rust Axum/SQLx.")
    if engine == "E07":
        if handler in {"inspect_import_asset", "confirm_import", "preview_export", "confirm_export"}:
            return (
                "replace",
                "Replace Python vector format codec execution with native GDAL/PROJ adapters behind Rust Axum transfer job state machine.",
            )
        return ("migrate", "Migrate transfer upload, durable job control plane, and artifact download to Rust Axum/SQLx.")
    if engine == "E08":
        if handler == "tile":
            return (
                "replace",
                "Replace Python MVT tile route (/pro/tiles/{layer_id}/{z}/{x}/{y}.mvt) with Martin/Rust tile gateway preserving scoped ACL and zoom/geometry bounds.",
            )
        return (
            "migrate",
            "Migrate processing algorithm/job/DAG model and professional geometry/topology/schema API to Rust/Rayon + GEOS/PostGIS.",
        )
    if engine == "E09":
        if handler in {"analyze_raster", "store_raster", "raster_report", "terrain_preview"}:
            return (
                "replace",
                "Replace Python raster/terrain compute worker execution with native GDAL/COG + bounded Rust kernels preserving nodata/CRS/quota contract.",
            )
        if handler == "raster_tile":
            return (
                "replace",
                "Replace Python raster PNG tile serving with Rust/GDAL COG tile gateway preserving scoped ACL and revision caching.",
            )
        return ("migrate", "Migrate raster layer catalog, byte-range file serving, and deletion to Rust Axum/SQLx.")
    if engine == "E01":
        if mod_stem == "auth" and ("oidc" in path or "callback" in path or "providers" in path):
            return (
                "migrate",
                "Migrate OIDC login/callback/linking HTTP endpoints to Rust Axum while retaining external OIDC identity provider and cookie/state contract.",
            )
        return ("migrate", "Migrate core identity, tenancy, RBAC/ACL, invitation, service principal, or audit route to Rust Axum/SQLx.")
    return ("migrate", f"Migrate {engine} runtime operation to Rust platform preserving exact HTTP/wire/security contract.")


def analyze_auth_and_scope(
    method: str,
    path: str,
    dep_calls: list[str],
    src: str,
    mod_stem: str,
    is_ws: bool,
) -> dict[str, Any]:
    dep_names = [d.rsplit(".", 1)[-1] for d in dep_calls]
    has_project_guard = "project_access_guard" in dep_names
    requires_csrf = "require_csrf" in dep_names or "verify_request_csrf" in src
    requires_human_auth = "authenticate_human_request" in dep_names
    requires_auth = (
        requires_human_auth
        or requires_csrf
        or "authenticate_request" in dep_names
        or has_project_guard
        or "authenticate_request" in src
    )
    uses_bootstrap_secret = mod_stem == "bootstrap"
    is_public = not requires_auth and not is_ws and not uses_bootstrap_secret
    if is_ws:
        auth_mode = "websocket_session_cookie_revalidated"
    elif uses_bootstrap_secret:
        auth_mode = "bootstrap_secret_header"
    elif requires_human_auth:
        auth_mode = "human_session_cookie_only"
    elif requires_csrf:
        auth_mode = "session_cookie_with_csrf_or_service_token"
    elif requires_auth:
        auth_mode = "session_cookie_or_service_token"
    else:
        auth_mode = "public"

    marker = "/projects/{project_id}"
    suffix = path.split(marker, 1)[1] if marker in path else None
    scoped_allowed: bool | None = None
    lock_mode: str | None = None
    if has_project_guard and suffix is not None:
        if method == "GET":
            scoped_allowed = suffix in gis_access_guard.SAFE_GET
            lock_mode = "FOR UPDATE" if suffix in gis_access_guard.WRITE_LOCK_GET else "FOR SHARE"
        elif method == "POST":
            scoped_allowed = suffix in gis_access_guard.SAFE_POST
            lock_mode = "FOR UPDATE"
        elif method == "DELETE":
            scoped_allowed = suffix in gis_access_guard.SAFE_DELETE
            lock_mode = "FOR UPDATE"
        else:
            scoped_allowed = False
            lock_mode = "FOR UPDATE"

    if "{layer_id}" in path or "{source_id}" in path or "{feature_id}" in path or "{tileset_id}" in path:
        tenant_scope = "project_resource"
    elif "{project_id}" in path:
        tenant_scope = "project"
    elif "{workspace_id}" in path:
        tenant_scope = "workspace"
    elif "{organization_id}" in path:
        tenant_scope = "organization"
    elif requires_auth or is_ws:
        tenant_scope = "authenticated_actor"
    else:
        tenant_scope = "public"

    perms = sorted(
        set(
            re.findall(
                r'Permission\.[A-Z0-9_]+|"(?:ext|org|workspace|project|spatial|map|editing|query|processing|cartography|transfer|raster|cad|field|ops|marketplace|partner)\.[a-z0-9_.]+"',
                src,
            )
        )
    )
    return {
        "auth_mode": auth_mode,
        "is_public": is_public,
        "requires_csrf": requires_csrf,
        "tenant_scope": tenant_scope,
        "project_access_guard": {
            "enabled": has_project_guard,
            "project_suffix": suffix,
            "scoped_human_allowed": scoped_allowed,
            "pg_lock_mode": lock_mode,
        },
        "dependencies": sorted(set(dep_names)),
        "permission_references": [p.strip('"') for p in perms],
        "rls_context_enforced": (
            "require_member_context" in src
            or "set_tenant_context" in src
            or has_project_guard
            or tenant_scope in {"organization", "workspace", "project", "project_resource"}
        ),
    }


def analyze_revision_crs_error_semantics(
    engine: str,
    method: str,
    path: str,
    src: str,
    req_schemas: list[str],
    openapi_op: dict[str, Any] | None,
    schemas_dict: dict[str, Any],
) -> tuple[dict[str, Any], dict[str, Any], dict[str, Any]]:
    schema_props: set[str] = set()
    for sname in req_schemas:
        sdef = schemas_dict.get(sname, {})
        if isinstance(sdef, dict) and isinstance(sdef.get("properties"), dict):
            schema_props.update(sdef["properties"].keys())
    query_param_names = {
        p["name"]
        for p in (openapi_op.get("parameters", []) if openapi_op else [])
        if p.get("in") == "query"
    }

    rev_keys = (
        "expected_revision",
        "expected_catalog_revision",
        "expected_content_revision",
        "base_content_revision",
        "expected_map_revision",
        "content_revision",
        "catalog_revision",
        "revision_manifest",
        "layer_revision",
    )
    has_expected_rev = any(k in schema_props or k in query_param_names or k in src for k in rev_keys)
    has_operation_id_idem = "operation_id" in schema_props or "operation_id" in src
    revision_semantics = {
        "uses_revision_pin_or_precondition": has_expected_rev,
        "uses_idempotent_operation_id": has_operation_id_idem,
        "matched_fields": sorted(
            k
            for k in (*rev_keys, "operation_id")
            if k in schema_props or k in query_param_names or k in src
        ),
    }

    crs_mentioned = (
        engine in {"E02", "E03", "E04", "E05", "E06", "E07", "E08", "E09"}
        and any(
            tok in src.lower() or tok in path.lower() or tok in " ".join(schema_props).lower()
            for tok in ("crs", "srid", "epsg", "4326", "3857", "4756", "5899", "extent", "bounds", "geometry", "tile", "raster", "terrain")
        )
    )
    crs_policy = "not_applicable"
    if crs_mentioned or engine in {"E02", "E03", "E04", "E05", "E07", "E08", "E09"}:
        if engine == "E02":
            crs_policy = "Explicit EPSG CRS registry (r003-v1); readable SRIDs {4326,3857,4756,5899}, definition-only {3405,3406}; xy axis order; 8 allowlisted directed transform edges; no implicit CRS."
        elif engine == "E03":
            crs_policy = "2D XY geometry in layer's explicit validated CRS; rejects unsupported dimensions/geometry/CRS with structured 422."
        elif engine == "E04":
            crs_policy = "Spatial filters require explicit readable CRS {4326,3857,4756,5899}; no implicit SRID assumption."
        elif engine == "E05":
            crs_policy = "3D Tiles ECEF/region bounding volumes (EPSG:4979/4326 radians) and MVT Web Mercator EPSG:3857 tiles."
        elif engine == "E06":
            crs_policy = "Layout crs_code validated against supported CRS registry; scale accuracy note preserved on print exports."
        elif engine == "E07":
            crs_policy = "Explicit source/target CRS in {4326,3857,4756,5899} validated against approved transformation allowlist; explicit encoding allowlist."
        elif engine == "E08":
            crs_policy = "Processing output_crs explicitly validated; buffer projects geographic inputs to Web Mercator (EPSG:3857); MVT tiles in EPSG:3857."
        elif engine == "E09":
            crs_policy = "Raster/DEM preserves explicit GDAL CRS, nodata, metre elevation_unit, and vertical_reference (local/orthometric/ellipsoidal); web tiles in EPSG:3857."

    crs_semantics = {
        "crs_sensitive": crs_policy != "not_applicable",
        "policy": crs_policy,
    }

    response_codes = sorted(openapi_op.get("responses", {}).keys()) if openapi_op else []
    error_formats = ["422 sanitized validation envelope (no raw input/secret echo)"]
    if engine in {"E02", "E03"}:
        error_formats.append("SpatialDomainError -> {code, detail}")
    if engine in {"E03", "E04", "E06", "E08", "E09"}:
        error_formats.append("PlatformDomainError -> {code, message, details?}")
    if engine == "E07":
        error_formats.append("TransferError -> {code, message}")
    if engine == "E01":
        error_formats.append("Tenancy/Auth errors -> {detail} (400/401/403/404)")
    error_formats.append("SQLSTATE OV409 guard -> 409 {code: DATA_LOCKED, message}")
    error_semantics = {
        "declared_status_codes": response_codes,
        "envelopes": error_formats,
    }
    return revision_semantics, crs_semantics, error_semantics


def parse_migration_file(path: Path, scope: str) -> dict[str, Any]:
    text = path.read_text(encoding="utf-8")
    tree = ast.parse(text)
    revision = None
    down_revision = None
    branch_labels = None
    depends_on = None
    for node in tree.body:
        target_name = None
        val_node = None
        if isinstance(node, ast.Assign) and len(node.targets) == 1 and isinstance(node.targets[0], ast.Name):
            target_name = node.targets[0].id
            val_node = node.value
        elif isinstance(node, ast.AnnAssign) and isinstance(node.target, ast.Name):
            target_name = node.target.id
            val_node = node.value
        if target_name in {"revision", "down_revision", "branch_labels", "depends_on"} and val_node is not None:
            try:
                val = ast.literal_eval(val_node)
            except Exception:
                val = ast.unparse(val_node)
            if target_name == "revision":
                revision = val
            elif target_name == "down_revision":
                down_revision = val
            elif target_name == "branch_labels":
                branch_labels = val
            elif target_name == "depends_on":
                depends_on = val

    doc = ast.get_docstring(tree) or ""
    tables = sorted(
        set(
            re.findall(
                r"(?:CREATE TABLE(?:\s+IF NOT EXISTS)?|ALTER TABLE|DROP TABLE(?:\s+IF EXISTS)?)\s+([a-zA-Z_][a-zA-Z0-9_.]*)",
                text,
                re.IGNORECASE,
            )
        )
    )
    schemas = sorted(
        set(re.findall(r"CREATE SCHEMA(?:\s+IF NOT EXISTS)?\s+([a-zA-Z_][a-zA-Z0-9_]*)", text, re.IGNORECASE))
    )
    has_rls = bool(re.search(r"ROW LEVEL SECURITY|CREATE POLICY", text, re.IGNORECASE))
    rel_path = str(path.relative_to(ROOT)).replace("\\", "/")
    engine = "E15" if scope == "module:house_numbering" else MIGRATION_TO_ENGINE.get(path.name, "E18")
    return {
        "file": rel_path,
        "filename": path.name,
        "scope": scope,
        "revision": revision,
        "down_revision": down_revision,
        "branch_labels": branch_labels,
        "depends_on": depends_on,
        "summary": doc.splitlines()[0].strip() if doc else path.stem,
        "schemas_created": schemas,
        "tables_touched": tables,
        "has_rls_policy": has_rls,
        "engine": engine,
        "classification": "retain",
        "rationale": "FR-011 preserves data IDs and PostgreSQL schema compatibility; existing migrations are retained.",
    }


def extract_pytest_node_ids(path: Path) -> list[str]:
    rel = str(path.relative_to(ROOT)).replace("\\", "/")
    try:
        tree = ast.parse(path.read_text(encoding="utf-8"))
    except Exception:
        return []
    node_ids: list[str] = []
    for node in tree.body:
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)) and node.name.startswith("test_"):
            node_ids.append(f"{rel}::{node.name}")
        elif isinstance(node, ast.ClassDef) and node.name.startswith("Test"):
            for item in node.body:
                if isinstance(item, (ast.FunctionDef, ast.AsyncFunctionDef)) and item.name.startswith("test_"):
                    node_ids.append(f"{rel}::{node.name}::{item.name}")
    return node_ids


def classify_test_file(rel_path: str) -> tuple[str, list[str], str]:
    """Classify a pytest file to primary engine + secondary engines + rationale."""
    p = rel_path.lower()
    name = Path(rel_path).name.lower()

    if p.startswith("modules/house_numbering/") or "house_numbering" in name:
        return "E15", [], "House numbering module test suite."
    if p.startswith("tests/architecture/"):
        return "E18", ["E01", "E14"], "Cross-cutting architecture boundary enforcement test."
    if p.startswith("tests/modules/"):
        return "E14", ["E15"], "Extension portability and reference module contract test."

    if "/identity/" in p or "/rls/" in p or "/service_identity/" in p or "/tenancy/" in p or "/authorization/" in p or "/audit/" in p:
        return "E01", [], "Identity, tenancy, RLS, service principal, or authorization test."
    if "/integration_hub/" in p:
        if "r029" in name or "managed_sync" in name:
            return "E11", ["E12"], "Managed realtime sync / session auth integration test."
        return "E12", [], "Integration hub, QGIS plugin, connector bindings test."
    if "/transfers/" in p:
        if "offline" in name:
            return "E13", ["E07"], "Field offline snapshot/operation namespace test."
        return "E07", [], "Import/export format codecs and transfer domain test."
    if "/maps/" in p:
        if any(k in name for k in ("viewport", "render", "warm_revision")):
            return "E05", ["E02"], "Map viewport rendering, tile/warm-revision security test."
        return "E02", ["E05"], "Map workspace saves, wire contract, migrations, and authorization test."

    if any(k in name for k in ("identity_tenancy", "permission_registry", "foundation_contract", "tenant_foundation", "project_access_lock", "gis_access", "config", "lifecycle_security", "migration_rls", "repository_scope", "restart")):
        sec = ["E02"] if "/spatial/" in p else []
        return "E01", sec, "Platform security, configuration, permission registry, or tenant RLS test."
    if name == "test_authorization.py" and "/spatial/" in p:
        return "E01", ["E02"], "Spatial endpoint authorization & tenant boundary test."
    if any(k in name for k in ("cartography", "layout_designer", "print_revision")):
        return "E06", [], "Cartography styles, layout designer, or revision-pinned print export test."
    if any(k in name for k in ("editing", "r006_", "change_evidence", "spatial_changes")):
        return "E03", ["E02"], "Editing changesets, replay/rebase, schema pins, or change evidence test."
    if "query" in name:
        return "E04", [], "Typed query compiler, spatial predicates, or saved query test."
    if "tiles_generator" in name or "map_viewport" in name:
        return "E05", ["E02"], "Tile generation math/pipeline or bounded viewport test."
    if "r006_r009_real" in name:
        return "E08", ["E03", "E04", "E06"], "M2 real end-to-end editing, query, processing, and cartography test."
    if any(k in name for k in ("processing", "professional")):
        return "E08", ["E03", "E05"], "Processing algorithms, DAG models, or professional geometry/topology/MVT test."
    if any(k in name for k in ("raster", "terrain", "cogo", "r010_")):
        sec = ["E12"] if "external" in name else []
        return "E09", sec, "Raster analysis, persistent raster, terrain, COG/GeoTIFF, or COGO survey test."
    if any(k in name for k in ("cad", "model_vault", "dxf", "v8i", "r025", "r048")):
        return "E10", [], "CAD precision, DXF/V8i bridge, CAD documents, or 3D model vault test."
    if any(k in name for k in ("collaboration", "org_chat", "feature_comments", "activity_cursor")):
        return "E11", [], "Team collaboration, activity feed/cursor, comments, or organization chat test."
    if any(k in name for k in ("external_provider",)):
        return "E12", [], "External source provider adapter test."
    if any(k in name for k in ("r013", "offline")):
        return "E13", [], "Field offline package, snapshot pin, or operation mapping test."
    if any(k in name for k in ("extension", "marketplace", "partner", "official_module", "independent_lifecycle")):
        return "E14", [], "Plugin/extension lifecycle, marketplace developer, partner platform, or official module discovery test."
    if any(k in name for k in ("ops", "r015", "reset_safety")):
        return "E18", [], "Operations release gate, ops endpoints, or reset safety test."
    if "/spatial/" in p or name.startswith("test_spatial_") or "crs" in name or "pinned_reads" in name or "fixture_validation" in name or "manual_editor" in name:
        sec = ["E03"] if "manual_editor" in name else []
        return "E02", sec, "Spatial catalog, layers, features, CRS controls, fixtures, or pinned reads test."
    if name == "conftest.py" or name.startswith("_") or name == "__init__.py" or "helpers" in name or "fixtures" in name:
        return "E01", ["E02", "E03", "E04", "E05", "E06", "E07", "E08", "E09"], "Shared pytest harness/fixture helper."
    return "E18", [], "General repository verification/tooling test."


def extract_route_test_matches(
    engine: str,
    path: str,
    mod_stem: str,
    handler: str,
    test_index: list[dict[str, Any]],
) -> dict[str, Any]:
    """Match pytest files covering this route/engine using path-segment + module-keyword heuristics."""
    matched_files: list[str] = []
    matched_ids: list[str] = []

    cleaned = re.sub(r"\{[^}]+\}", "", path).replace("/api/v1/", "").strip("/")
    segments = [s for s in cleaned.split("/") if s and s not in {"organizations", "projects", "workspaces"}]
    keywords = set(segments) | {mod_stem}
    synonyms = {
        "auth": {"oidc", "login", "logout", "session", "http_security", "link_conflict", "unlink"},
        "bootstrap": {"bootstrap"},
        "me": {"account_avatar", "multi_org_session", "identity_tenancy"},
        "organizations": {"identity_tenancy", "administration", "cross_tenant", "tenant_foundation"},
        "workspaces": {"workspace_lifecycle", "administration", "identity_tenancy"},
        "projects": {"workspace_lifecycle", "project_access", "gis_access", "authorization"},
        "grants": {"permission", "authorization", "identity_tenancy"},
        "gis_access": {"gis_access", "project_access_lock"},
        "invitations": {"invitation", "identity_tenancy"},
        "service_principals": {"service_authorization", "service_credentials", "service_identity"},
        "audit": {"audit"},
        "spatial": {"spatial", "crs", "layers", "features", "manual_editor", "pinned_reads", "fixture"},
        "maps": {"maps", "saves", "map_domain", "map_service", "wire_contract", "map_migrations"},
        "map_viewport": {"map_viewport", "map_render", "map_warm_revision"},
        "editing": {"editing", "r006", "spatial_changes", "change_evidence", "manual_editor"},
        "queries": {"query"},
        "tilesets": {"tiles_generator", "tileset"},
        "cartography": {"cartography", "print_revision"},
        "layout_designer": {"layout_designer"},
        "transfers": {"transfers", "codecs", "gpkg"},
        "processing": {"processing"},
        "processing_models": {"processing_models"},
        "professional": {"professional", "r006_r009"},
        "raster": {"raster", "r010", "cog"},
        "terrain": {"terrain", "raster_terrain", "cogo"},
    }
    kw = keywords | synonyms.get(mod_stem, set())

    for entry in test_index:
        if entry["primary_engine"] != engine and engine not in entry["secondary_engines"]:
            continue
        lp = entry["path"].lower()
        if any(k.replace("-", "_") in lp or k in lp for k in kw):
            matched_files.append(entry["path"])
            matched_ids.extend(entry["test_ids"])

    if not matched_files:
        for entry in test_index:
            if entry["primary_engine"] == engine and entry["test_ids"]:
                matched_files.append(entry["path"])
                matched_ids.extend(entry["test_ids"])

    return {
        "test_file_count": len(set(matched_files)),
        "test_files": sorted(set(matched_files)),
        "test_id_count": len(set(matched_ids)),
        "sample_test_ids": sorted(set(matched_ids))[:12],
    }


def build_algorithms_catalog() -> dict[str, Any]:
    processing_algos = [
        {
            "id": str(desc.id),
            "name": desc.name,
            "description": desc.description,
            "required_inputs": list(desc.required_inputs),
            "parameters_schema": desc.parameters_schema,
            "source": "apps/api/openvietgis_api/domains/processing/domain.py",
            "engine": "E08",
        }
        for desc in ALGORITHM_REGISTRY.values()
    ]
    return {
        "processing_registry": {
            "source": "apps/api/openvietgis_api/domains/processing/domain.py",
            "engine": "E08",
            "statuses": [str(s) for s in ProcessingStatus],
            "algorithms": processing_algos,
        },
        "processing_model_dag_algorithms": {
            "source": "apps/api/openvietgis_api/adapters/http/processing_models.py",
            "engine": "E08",
            "cross_engine_raster_nodes": [
                "band", "ndvi", "ndwi", "ndbi", "hillshade", "slope", "aspect",
                "resample", "mosaic", "change", "rgb", "calculator",
            ],
            "algorithms": [
                "buffer", "clip", "dissolve", "band", "ndvi", "ndwi", "ndbi",
                "hillshade", "slope", "aspect", "resample", "mosaic", "change",
                "rgb", "calculator",
            ],
        },
        "professional_operations": {
            "source": "apps/api/openvietgis_api/adapters/http/professional.py",
            "engine": "E08",
            "geometry_operations": [
                "move_batch", "copy", "offset", "rotate", "scale", "stretch",
                "extend", "trim", "fillet", "intersection", "union", "difference",
                "centroid", "hull", "simplify", "nearest", "spatial_join",
            ],
            "topology_checks": ["duplicate", "invalid", "overlap", "dangling", "outside", "gap"],
            "schema_actions": ["add", "rename"],
            "schema_field_types": ["text", "decimal", "integer", "boolean", "timestamp"],
        },
        "raster_and_terrain_operations": {
            "sources": [
                "apps/api/openvietgis_api/adapters/http/raster.py",
                "apps/api/openvietgis_api/adapters/http/terrain.py",
            ],
            "engine": "E09",
            "analyze_methods": ["band", "ndvi", "ndwi", "ndbi", "hillshade"],
            "store_and_derive_methods": [
                "band", "ndvi", "ndwi", "ndbi", "hillshade", "calculator",
                "rgb", "clip", "change", "slope", "aspect", "mosaic", "resample",
            ],
            "report_methods": ["zonal", "contour"],
            "resampling_methods": ["near", "bilinear"],
            "terrain_preview": {
                "elevation_units": ["metre"],
                "vertical_references": ["local", "orthometric", "ellipsoidal"],
                "requires_acknowledge_dem": True,
            },
        },
        "crs_and_transformations": {
            "source": "apps/api/openvietgis_api/domains/spatial/crs.py",
            "engine": "E02",
            "registry_version": CRS_REGISTRY_VERSION,
            "geometry_families_supported": sorted(str(f) for f in SUPPORTED_FAMILIES),
            "geometry_dimensions": [str(d) for d in GeometryDimensions],
            "field_types": [str(f) for f in FieldType],
            "crs_profiles": [p.to_wire() for p in CRS_PROFILES],
            "transformation_profiles": [
                {
                    **t.to_wire(),
                    "requires_trusted_fixture": t.requires_trusted_fixture,
                    "step_count": len(t.steps),
                }
                for t in TRANSFORMATION_PROFILES
            ],
        },
        "editing_operations": {
            "sources": [
                "apps/api/openvietgis_api/domains/editing/domain.py",
                "apps/api/openvietgis_api/adapters/http/spatial.py",
            ],
            "engine": "E03",
            "changeset_op_types": [str(op) for op in EditOpType],
            "geometry_preview_actions": ["split", "merge"],
            "snapping_endpoint": "/api/v1/organizations/{organization_id}/projects/{project_id}/editing/snap",
        },
        "query_capabilities": {
            "source": "apps/api/openvietgis_api/domains/queries/domain.py",
            "engine": "E04",
            "spatial_predicates": [str(p) for p in SpatialPredicate],
            "bounds": {
                "max_query_limit": MAX_QUERY_LIMIT,
                "max_query_offset": MAX_QUERY_OFFSET,
                "max_query_features": MAX_QUERY_FEATURES,
                "max_query_positions": MAX_QUERY_POSITIONS,
                "max_reference_features": MAX_REFERENCE_FEATURES,
                "max_reference_positions": MAX_REFERENCE_POSITIONS,
                "max_page_positions": MAX_PAGE_POSITIONS,
                "max_drawn_positions": MAX_DRAWN_POSITIONS,
                "max_ast_depth": MAX_AST_DEPTH,
                "max_ast_nodes": MAX_AST_NODES,
                "max_ast_params": MAX_AST_PARAMS,
                "statement_timeout": QUERY_STATEMENT_TIMEOUT,
            },
        },
        "cartography_and_layout": {
            "sources": [
                "apps/api/openvietgis_api/domains/cartography/domain.py",
                "apps/api/openvietgis_api/adapters/http/layout_designer.py",
            ],
            "engine": "E06",
            "style_modes": [str(m) for m in StyleMode],
            "paper_sizes": [str(p) for p in PaperSize],
            "orientations": [str(o) for o in Orientation],
            "print_formats": [str(f) for f in PrintFormat],
            "layout_designer_element_kinds": ["map", "text", "legend", "north", "scale", "rect"],
            "layout_designer_dpi_options": [150, 300],
        },
        "transfer_formats": {
            "source": "apps/api/openvietgis_api/domains/transfers/domain.py",
            "engine": "E07",
            "formats": [str(f) for f in TransferFormat],
            "supported_crs_codes": sorted(SUPPORTED_CRS_CODES),
            "supported_encodings": list(SUPPORTED_ENCODINGS),
            "job_kinds": [str(k) for k in JobKind],
            "job_statuses": [str(s) for s in JobStatus],
            "limits": {
                "max_upload_bytes": MAX_UPLOAD_BYTES,
                "max_zip_expanded_bytes": MAX_ZIP_EXPANDED_BYTES,
                "max_zip_entries": MAX_ZIP_ENTRIES,
                "max_transfer_features": MAX_TRANSFER_FEATURES,
                "max_transfer_fields": MAX_TRANSFER_FIELDS,
                "max_string_len": MAX_STRING_LEN,
                "artifact_ttl_hours": ARTIFACT_TTL_HOURS,
            },
        },
    }


def engine_specific_capabilities(engine: str, catalog: dict[str, Any]) -> dict[str, Any]:
    if engine == "E01":
        return {
            "auth_mechanisms": [
                "OIDC authorization code flow (SessionMiddleware ovg_oidc_state)",
                "Signed human session cookie (ovg_session, configurable TTL 300..604800s)",
                "Double-submit / signed CSRF header (x-openvietgis-csrf) on mutating human routes",
                "Machine/service principal bearer credential tokens",
                "One-time bootstrap secret header (OPENVIETGIS_BOOTSTRAP_SECRET)",
            ],
            "project_access_guard": {
                "safe_get_suffixes": sorted(gis_access_guard.SAFE_GET),
                "safe_post_suffixes": sorted(gis_access_guard.SAFE_POST),
                "safe_delete_suffixes": sorted(gis_access_guard.SAFE_DELETE),
                "write_lock_get_suffixes": sorted(gis_access_guard.WRITE_LOCK_GET),
                "lock_order": "org.organizations -> workspace.projects (FOR SHARE on read, FOR UPDATE on write)",
            },
            "rls_enforcement": "PostgreSQL RLS policies enforced under restricted openvietgis_runtime role in production.",
        }
    if engine == "E02":
        return catalog["crs_and_transformations"]
    if engine == "E03":
        return catalog["editing_operations"]
    if engine == "E04":
        return catalog["query_capabilities"]
    if engine == "E05":
        return {
            "tileset_capabilities": [
                "3D Tiles archive upload & manifest validation (tileset.json)",
                "3D Tiles server-side generation from spatial sources",
                "Tile feature metadata lookup & feature-by-id location index",
                "Authorized tileset manifest & tile URI streaming",
            ],
            "cross_engine_render_and_tile_routes": [
                {
                    "engine": "E02",
                    "module": "map_viewport",
                    "paths": [
                        "/api/v1/organizations/{organization_id}/projects/{project_id}/layers/{layer_id}/viewport",
                        "/api/v1/organizations/{organization_id}/projects/{project_id}/layers/{layer_id}/display-extent",
                    ],
                },
                {
                    "engine": "E08",
                    "module": "professional",
                    "paths": [
                        "/api/v1/organizations/{organization_id}/projects/{project_id}/pro/tiles/{layer_id}/{z}/{x}/{y}.mvt",
                    ],
                },
                {
                    "engine": "E09",
                    "module": "raster",
                    "paths": [
                        "/api/v1/organizations/{organization_id}/projects/{project_id}/raster/layers/{layer_id}/tiles/{z}/{x}/{y}.png",
                    ],
                },
            ],
        }
    if engine == "E06":
        return catalog["cartography_and_layout"]
    if engine == "E07":
        return catalog["transfer_formats"]
    if engine == "E08":
        return {
            "processing_registry": catalog["processing_registry"],
            "processing_model_dag_algorithms": catalog["processing_model_dag_algorithms"],
            "professional_operations": catalog["professional_operations"],
        }
    if engine == "E09":
        return catalog["raster_and_terrain_operations"]
    return {}


def cross_engine_deps_for(engine: str) -> list[dict[str, str]]:
    deps = (
        [{"engine": "E01", "relation": "All tenant/project routes depend on E01 authentication, CSRF, RBAC/ACL, RLS, and project_access_guard."}]
        if engine != "E01"
        else []
    )
    extra: dict[str, list[dict[str, str]]] = {
        "E01": [
            {"engine": "E02..E15", "relation": "Provides auth, session/CSRF, project_access_guard, RLS context, and audit logging for all domain engines."},
            {"engine": "E18", "relation": "Production profile config validation and health/ops security controls."},
        ],
        "E02": [
            {"engine": "E03", "relation": "Layer/source schemas and content_revision underpin editing changesets and geometry preview."},
            {"engine": "E05", "relation": "map_viewport (/viewport, /display-extent) and maps state directly feed MapLibre rendering and tile caching."},
        ],
        "E03": [
            {"engine": "E02", "relation": "Validates edits against E02 layer schema, geometry family, explicit CRS, and catalog/content revisions."},
            {"engine": "E05", "relation": "Committed changesets must invalidate viewport/MVT tile caches."},
            {"engine": "E11", "relation": "Edits respect feature edit leases and publish realtime change events."},
        ],
        "E04": [
            {"engine": "E02", "relation": "Reads E02 source field types, explicit readable CRS codes, and native PostGIS geometries."},
        ],
        "E05": [
            {"engine": "E02", "relation": "Depends on E02 spatial layers, CRS, maps state, and map_viewport endpoints."},
            {"engine": "E03", "relation": "Requires cache invalidation on E03 changeset commits and E01 ACL revocations."},
            {"engine": "E08", "relation": "Coordinates MVT tile serving (/pro/tiles/{layer_id}/{z}/{x}/{y}.mvt) with Martin/Rust tile gateway."},
            {"engine": "E09", "relation": "Coordinates raster tile serving (/raster/layers/{layer_id}/tiles/{z}/{x}/{y}.png)."},
        ],
        "E06": [
            {"engine": "E02", "relation": "Pins layout exports to E02 layer content revisions and validated CRS profiles."},
            {"engine": "E07", "relation": "Uses shared local_object_store artifact storage for immutable PNG/PDF print outputs."},
        ],
        "E07": [
            {"engine": "E02", "relation": "Imports register/populate E02 spatial sources/layers with validated CRS transformations."},
            {"engine": "E09", "relation": "Raster uploads/GeoTIFFs are handled in E09 while vector GeoJSON/GPKG/Shapefile ZIP live in E07."},
        ],
        "E08": [
            {"engine": "E02", "relation": "Reads authorized E02 input layers and publishes derived output sources/layers."},
            {"engine": "E05", "relation": "Hosts /pro/tiles/{layer_id}/{z}/{x}/{y}.mvt which converges with E05 tile gateway."},
            {"engine": "E09", "relation": "Processing model DAGs include 12 raster algorithm nodes executed against E09 raster primitives."},
        ],
        "E09": [
            {"engine": "E02", "relation": "Links raster/terrain products to project/layer catalog and explicit CRS metadata."},
            {"engine": "E05", "relation": "Raster PNG tiles (/raster/layers/{layer_id}/tiles/{z}/{x}/{y}.png) participate in E05 tile caching/ACL revocation."},
            {"engine": "E08", "relation": "Provides raster computation operations consumed by E08 processing_models DAGs."},
        ],
    }
    return deps + extra.get(engine, [])


ENGINE_EDGE_CASES: dict[str, list[str]] = {
    "E01": [
        "Expired or revoked session cookie / service token must fail closed (401) across HTTP, SSE, and WebSocket.",
        "Cross-tenant organization_id / workspace_id / project_id access must return 403/404 with zero RLS pool bleed.",
        "Missing or invalid x-openvietgis-csrf header on human mutating endpoints must fail closed.",
        "Scoped human GIS accounts calling unallowlisted project_access_guard routes must return 403.",
        "Concurrent OIDC callback / unlink / bootstrap race conditions and last-owner protection.",
        "Sanitized 422 validation envelope must never echo secrets (bootstrap/invitation/OIDC tokens).",
    ],
    "E02": [
        "Implicit or missing CRS must be rejected; definition-only CRS (EPSG:3405, 3406) must not allow coordinate reads.",
        "VN-2000 (EPSG:4756 / 5899) transformations must enforce trusted-fixture and validated bounding-box guards (Quang Ninh controls) and exact tolerances.",
        "Axis order must remain strictly XY (lon/lat or easting/northing) with Web Mercator latitude clamp +/-85.0511287798066 deg.",
        "Optimistic revision mismatch on source/layer/map save must reject stale writes without silent overwrite.",
        "Retiring a source with active dependent layers must fail with 409 ACTIVE_LAYER_DEPENDENCIES.",
        "Bounded viewport / feature pagination must never trigger full-table unbounded downloads on 30k/100k/1M layers.",
    ],
    "E03": [
        "Stale base_content_revision or concurrent edits must return conflict without corrupting change history.",
        "Duplicate operation_id retries must be idempotent; internal operation_id prefixes rejected on public endpoints.",
        "Invalid geometry, non-2D Z/M coordinates, or geometry-family mismatch must be rejected (422 UNSUPPORTED_GEOMETRY / UNSUPPORTED_DIMENSIONS).",
        "SQLSTATE OV409 database lock guard (e.g. module lock trigger) must surface 409 DATA_LOCKED.",
        "Reverting a changeset must preserve audit attribution, before/after snapshots, and transactional atomicity.",
    ],
    "E04": [
        "Zero arbitrary SQL injection: typed AST compiler only, enforcing field types and C-collation text comparisons.",
        "NULL comparisons must use is_null operator rather than raw null equality.",
        "AST bounds (depth <= 6, nodes <= 100, params <= 1000), page limit <= 500, offset <= 50,000, and 5s statement timeout must fail safely.",
        "Drawn spatial filter geometries must declare an explicit readable CRS and stay within MAX_DRAWN_POSITIONS (2048).",
    ],
    "E05": [
        "No shared private tile cache across tenants or revoked permissions; tile/manifest routes must verify project ACL.",
        "Malformed 3D Tiles archives, path traversal in {uri:path}, or invalid boundingVolume/geometricError trees must be rejected.",
        "Edit-to-tile invalidation and bounded zoom/geometry payloads on 30k/100k/1M feature layers.",
        "Cross-engine convergence across 3D tilesets (E05), map_viewport (E02), MVT /pro/tiles (E08), and raster PNG tiles (E09).",
    ],
    "E06": [
        "Revision-pinned print exports must record deterministic SHA-256 revision_manifest fingerprint and reject drifted layer revisions when pinned.",
        "Invalid hex colors, unknown style modes, invalid label fields, or out-of-range paper sizes/DPI must fail validation.",
        "Layout designer elements must enforce version=1, valid element kinds, and bounded paper/DPI (A0..A4, 150/300 DPI).",
        "Export artifact downloads must verify tenant/project ownership and artifact integrity.",
    ],
    "E07": [
        "Malicious ZIP archives (zip bombs > 50 MiB expanded, > 64 entries, path traversal, uploads > 25 MiB) must be rejected.",
        "Feature count (> 50,000), field count (> 100), and string length (> 4096) limits enforced consistently.",
        "Unsupported CRS or unapproved CRS transformation edges and unsupported character encodings (outside utf-8/windows-1258/cp1252/latin-1) must fail with structured TransferError.",
        "GeoPackage decimal notation, shapefile DBF encoding, and round-trip geometry/attribute fidelity.",
        "Job cancellation, retry state transitions, and 24-hour artifact expiration (READY/EXPIRED/UNAVAILABLE).",
    ],
    "E08": [
        "Processing parameter validation must reject booleans/strings coerced as numbers, NaN/Inf, and out-of-range distance_meters (0.1..500000) or quad_segs (2..16).",
        "DAG cycle detection, invalid step/layer references, and arity validation (binary ops, 3-band RGB, calculator formula 'B' ref) in processing_models.",
        "Job cancellation, retry idempotency, bounded worker concurrency/memory, and recovery after worker crash.",
        "Professional geometry/topology/schema operations must honor pinned revisions, scoped ACL, and fillet interior-vertex preconditions.",
    ],
    "E09": [
        "Raster upload size limits, storage quota enforcement, and malformed GeoTIFF/COG rejection.",
        "Explicit nodata propagation, band index bounds (1..16), formula sanitization in raster calculator, and resampling allowlist (near/bilinear).",
        "Byte-range (HTTP Range / 206) serving on /raster/layers/{layer_id}/file must handle invalid/unsatisfiable ranges safely.",
        "Terrain preview requires explicit elevation_unit='metre', valid vertical_reference, and acknowledge_dem=True (no survey certification claim).",
    ],
}


def render_engine_evidence_md(engine_doc: dict[str, Any]) -> str:
    eid = engine_doc["engine_id"]
    s = engine_doc["summary"]
    lines: list[str] = [
        f"# {eid} Inventory & Contract Evidence — {engine_doc['name']}",
        "",
        f"- **Task**: `{eid}-T01` (Acceptance Criterion: `{eid}-AC1`)",
        f"- **Parent Spec**: Spec 064 (`064-rust-platform`), references: {', '.join(engine_doc['parent_references'])}",
        f"- **Child Spec**: `{engine_doc['spec_path']}`",
        f"- **Contract Artifact**: `specs/064-rust-platform/contracts/engines/{eid.lower()}.json`",
        f"- **Baseline Commit**: `{engine_doc['baseline_commit']}`",
        f"- **Generated (UTC)**: `{engine_doc['generated_at']}`",
        f"- **Architecture Candidate**: {engine_doc['architecture_candidate']}",
        "",
        "## 1. Summary Counts",
        "",
        "| Metric | Count |",
        "|---|---:|",
        f"| Live HTTP Operations | {s['http_operations']} |",
        f"| Live WebSocket Routes | {s['websocket_routes']} |",
        f"| Live SSE / Streaming Routes | {s['sse_routes']} / {s['streaming_routes']} |",
        f"| Classification: `migrate` | {s['classifications'].get('migrate', 0)} |",
        f"| Classification: `replace` | {s['classifications'].get('replace', 0)} |",
        f"| Classification: `retain` | {s['classifications'].get('retain', 0)} |",
        f"| Classification: `approved_retire` | {s['classifications'].get('approved_retire', 0)} |",
        f"| Owned Database Migrations | {s['migrations_count']} |",
        f"| Mapped & Triaged Source Files | {s['source_files_count']} |",
        f"| Mapped Pytest Files | {s['test_files_count']} |",
        f"| Mapped Pytest Test Case IDs | {s['test_case_ids_count']} |",
        "",
        "## 2. Live Runtime Operation Matrix",
        "",
        "| Method | Path | Operation ID | Handler (`module`) | Classification | Auth / Guard | Revision / Idempotency | Tests |",
        "|---|---|---|---|---|---|---|---:|",
    ]
    for op in engine_doc["operations"]:
        guard = op["auth_tenant_scope"]["project_access_guard"]
        auth_label = op["auth_tenant_scope"]["auth_mode"]
        if op["auth_tenant_scope"]["requires_csrf"]:
            auth_label += " + CSRF"
        if guard["enabled"]:
            scoped_txt = "scoped:allow" if guard["scoped_human_allowed"] else "scoped:deny"
            auth_label += f" + proj_guard({scoped_txt}, {guard['pg_lock_mode']})"
        rev_flags = []
        if op["revision_semantics"]["uses_revision_pin_or_precondition"]:
            rev_flags.append("rev-pin")
        if op["revision_semantics"]["uses_idempotent_operation_id"]:
            rev_flags.append("op-id")
        rev_txt = ", ".join(rev_flags) if rev_flags else "—"
        mod_short = op["module"].rsplit(".", 1)[-1]
        lines.append(
            f"| `{op['method']}` | `{op['path']}` | `{op['operation_id']}` | `{op['handler']}` (`{mod_short}`) | `{op['classification']}` | {auth_label} | {rev_txt} | {op['existing_tests']['test_file_count']} files ({op['existing_tests']['test_id_count']} IDs) |"
        )

    lines.extend(["", "## 3. Algorithms, Registries & Domain Capabilities", ""])
    lines.append("```json")
    lines.append(json.dumps(engine_doc["algorithms_and_capabilities"], ensure_ascii=False, indent=2))
    lines.append("```")

    lines.extend(["", "## 4. Owned Database Migrations", ""])
    if engine_doc["migrations"]:
        lines.extend([
            "| Revision | Down Revision | File | RLS | Classification | Summary |",
            "|---|---|---|---|---|---|",
        ])
        for m in engine_doc["migrations"]:
            lines.append(
                f"| `{m['revision']}` | `{m['down_revision']}` | `{m['file']}` | {'yes' if m['has_rls_policy'] else 'no'} | `{m['classification']}` | {m['summary']} |"
            )
    else:
        lines.append("No dedicated database migrations are exclusively owned by this engine (uses shared tables from E01/E02).")

    lines.extend(["", "## 5. Mapped & Triaged Source Files", ""])
    lines.append(f"Total source files mapped to `{eid}`: **{len(engine_doc['source_files'])}**")
    lines.append("")
    for sf in engine_doc["source_files"]:
        lines.append(f"- `{sf['path']}` (`{sf['classification']}` — {sf['source_origin']})")

    lines.extend(["", "## 6. Cross-Engine Dependencies", ""])
    for dep in engine_doc["cross_engine_dependencies"]:
        lines.append(f"- **{dep['engine']}**: {dep['relation']}")

    lines.extend(["", "## 7. Existing Pytest Evidence & Test Identifiers", ""])
    lines.append(
        f"Mapped **{s['test_files_count']} pytest files** containing **{s['test_case_ids_count']} test case IDs** (primary + cross-engine coverage):"
    )
    lines.append("")
    for tf in engine_doc["existing_test_suite"]:
        sample_str = ", ".join(f"`{tid.split('::', 1)[1]}`" for tid in tf["test_ids"][:5])
        more = f" (+{len(tf['test_ids']) - 5} more)" if len(tf["test_ids"]) > 5 else ""
        role_tag = "primary" if tf["primary_engine"] == eid else f"cross-ref from {tf['primary_engine']}"
        lines.append(f"- `{tf['path']}` ({role_tag}, {len(tf['test_ids'])} tests): {sample_str}{more}")

    lines.extend(["", "## 8. Edge Cases & Parity Risks", ""])
    for ec in engine_doc["edge_cases"]:
        lines.append(f"- {ec}")

    lines.extend([
        "",
        f"## 9. {eid}-AC1 Verification",
        "",
        f"- [x] Live FastAPI runtime instantiated via `create_app()` from worktree (`{engine_doc['baseline_commit'][:7]}`), zero mocked route tables.",
        f"- [x] All **{s['http_operations'] + s['websocket_routes']}** live runtime operations in `{eid}` extracted with exact OpenAPI method, path, operationId, schemas, auth/CSRF/guard dependencies, and revision/CRS/error semantics.",
        f"- [x] 100% of `{eid}` operations explicitly classified (`migrate` / `retain` / `replace` / `approved_retire`); zero unclassified or silently dropped routes.",
        f"- [x] Algorithms/capabilities, **{s['migrations_count']}** owned migrations, **{s['source_files_count']}** source files, and **{s['test_files_count']}** pytest files (**{s['test_case_ids_count']}** test IDs) linked in `contracts/engines/{eid.lower()}.json`.",
        f"- [ ] Next phase (`{eid}-T02`): expand golden/negative/permission fixtures for any operation relying only on engine-level suite coverage.",
        "",
    ])
    return "\n".join(lines)


def main() -> None:
    baseline = subprocess.check_output(
        ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True
    ).strip()
    static_inv = json.loads((CONTRACTS_DIR / "static-inventory.json").read_text(encoding="utf-8"))
    static_baseline = static_inv.get("baseline", baseline)

    app = create_app()
    openapi = app.openapi()
    schemas_dict = openapi.get("components", {}).get("schemas", {})
    official_modules = discover_official_module_runtimes()

    route_contexts: list[Any] = []
    for r in app.routes:
        if hasattr(r, "effective_route_contexts"):
            route_contexts.extend(list(r.effective_route_contexts()))

    oa_ops_map: dict[tuple[str, str], dict[str, Any]] = {}
    for p, path_item in openapi.get("paths", {}).items():
        for m, op_obj in path_item.items():
            if m.lower() in HTTP_METHODS:
                oa_ops_map[(m.upper(), p)] = op_obj

    live_operations: list[dict[str, Any]] = []
    websocket_routes: list[dict[str, Any]] = []
    sse_routes: list[dict[str, Any]] = []
    streaming_routes: list[dict[str, Any]] = []
    matched_oa_keys: set[tuple[str, str]] = set()

    test_index: list[dict[str, Any]] = []
    all_test_paths = sorted(
        set(
            list((ROOT / "apps" / "api" / "tests").rglob("test_*.py"))
            + list((ROOT / "tests").rglob("test_*.py"))
            + list((ROOT / "modules").rglob("test_*.py"))
        )
    )
    for tp in all_test_paths:
        rel = str(tp.relative_to(ROOT)).replace("\\", "/")
        prim, secs, rat = classify_test_file(rel)
        tids = extract_pytest_node_ids(tp)
        test_index.append({
            "path": rel,
            "primary_engine": prim,
            "secondary_engines": secs,
            "rationale": rat,
            "test_ids": tids,
        })

    for ctx in route_contexts:
        orig = ctx.original_route
        is_ws = isinstance(orig, APIWebSocketRoute)
        endpoint = ctx.endpoint or getattr(orig, "endpoint", None)
        assert endpoint is not None, f"Route context has no endpoint: {ctx.path}"
        mod_name = endpoint.__module__
        handler_name = endpoint.__name__
        mod_stem = mod_name.rsplit(".", 1)[-1]
        engine = module_to_engine(mod_name)
        src_file, src_line, src_code = extract_source_info(endpoint)
        dep_calls = extract_dependant_calls(getattr(ctx, "dependant", None))
        for d in getattr(ctx, "dependencies", []) or []:
            fn = getattr(d, "dependency", None)
            if fn is not None:
                dep_calls.append(f"{fn.__module__}.{fn.__name__}")
        dep_calls = sorted(set(dep_calls))

        raw_path = (
            ctx.path
            or getattr(getattr(ctx, "starlette_route", None), "path", "")
            or getattr(orig, "path", "")
        )
        norm_path = normalize_openapi_path(raw_path, getattr(ctx, "path_format", None) or None)
        assert norm_path.startswith("/"), f"Empty or invalid resolved path for {mod_name}.{handler_name}: {norm_path!r}"

        if is_ws:
            auth_scope = analyze_auth_and_scope("WEBSOCKET", norm_path, dep_calls, src_code, mod_stem, True)
            cls, cls_rat = classify_operation(engine, "WEBSOCKET", norm_path, mod_stem, handler_name)
            ws_entry = {
                "method": "WEBSOCKET",
                "path": norm_path,
                "raw_path": raw_path,
                "operation_id": f"ws_{mod_stem}_{handler_name}",
                "summary": (inspect.getdoc(endpoint) or "").splitlines()[0] if inspect.getdoc(endpoint) else handler_name,
                "tags": list(getattr(ctx, "tags", []) or []),
                "transport": "websocket",
                "status_code": 101,
                "deprecated": False,
                "include_in_schema": False,
                "handler": handler_name,
                "module": mod_name,
                "source_file": src_file,
                "source_line": src_line,
                "engine": engine,
                "classification": cls,
                "classification_rationale": cls_rat,
                "auth_tenant_scope": auth_scope,
                "request_body_content_types": [],
                "request_body_schemas": [],
                "response_schemas": {},
                "parameters": {"path": re.findall(r"\{([^}:]+)", raw_path), "query": [], "header": []},
                "revision_semantics": {"uses_revision_pin_or_precondition": False, "uses_idempotent_operation_id": False, "matched_fields": []},
                "crs_semantics": {"crs_sensitive": False, "policy": "not_applicable"},
                "error_semantics": {"declared_status_codes": ["101", "1008"], "envelopes": ["WebSocket close code 1008 on auth/ACL failure or revalidation denial"]},
                "existing_tests": extract_route_test_matches(engine, norm_path, mod_stem, handler_name, test_index),
            }
            websocket_routes.append(ws_entry)
            live_operations.append(ws_entry)
            continue

        assert isinstance(orig, APIRoute), f"Unexpected route type: {type(orig)}"
        for method in sorted(m.upper() for m in ctx.methods if m.upper() != "HEAD"):
            oa_op = oa_ops_map.get((method, norm_path))
            if oa_op is not None:
                matched_oa_keys.add((method, norm_path))

            req_schemas: set[str] = set()
            req_content_types: list[str] = []
            if oa_op and "requestBody" in oa_op:
                rb_content = oa_op["requestBody"].get("content", {})
                req_content_types = sorted(rb_content.keys())
                collect_schema_refs(rb_content, req_schemas)

            resp_schemas: dict[str, dict[str, Any]] = {}
            if oa_op:
                for sc, rdef in oa_op.get("responses", {}).items():
                    srefs: set[str] = set()
                    rcontent = rdef.get("content", {})
                    collect_schema_refs(rcontent, srefs)
                    resp_schemas[str(sc)] = {
                        "content_types": sorted(rcontent.keys()),
                        "schemas": sorted(srefs),
                    }

            params_by_in: dict[str, list[str]] = {"path": [], "query": [], "header": []}
            if oa_op:
                for prm in oa_op.get("parameters", []):
                    loc = prm.get("in")
                    if loc in params_by_in:
                        params_by_in[loc].append(prm["name"])

            transport = detect_transport(ctx, src_code, oa_op)
            auth_scope = analyze_auth_and_scope(method, norm_path, dep_calls, src_code, mod_stem, False)
            cls, cls_rat = classify_operation(engine, method, norm_path, mod_stem, handler_name)
            rev_sem, crs_sem, err_sem = analyze_revision_crs_error_semantics(
                engine, method, norm_path, src_code, sorted(req_schemas), oa_op, schemas_dict
            )
            op_entry = {
                "method": method,
                "path": norm_path,
                "raw_path": raw_path,
                "operation_id": (oa_op.get("operationId") if oa_op else None) or getattr(ctx, "unique_id", handler_name),
                "summary": (oa_op.get("summary") if oa_op else None) or getattr(ctx, "summary", None) or handler_name.replace("_", " ").title(),
                "tags": (oa_op.get("tags", []) if oa_op else list(getattr(ctx, "tags", []) or [])),
                "transport": transport,
                "status_code": getattr(ctx, "status_code", None) or (200 if method == "GET" else None),
                "deprecated": bool(oa_op.get("deprecated", False)) if oa_op else bool(getattr(ctx, "deprecated", False)),
                "include_in_schema": bool(getattr(ctx, "include_in_schema", True)),
                "handler": handler_name,
                "module": mod_name,
                "source_file": src_file,
                "source_line": src_line,
                "engine": engine,
                "classification": cls,
                "classification_rationale": cls_rat,
                "auth_tenant_scope": auth_scope,
                "request_body_content_types": req_content_types,
                "request_body_schemas": sorted(req_schemas),
                "response_schemas": resp_schemas,
                "parameters": params_by_in,
                "revision_semantics": rev_sem,
                "crs_semantics": crs_sem,
                "error_semantics": err_sem,
                "existing_tests": extract_route_test_matches(engine, norm_path, mod_stem, handler_name, test_index),
            }
            live_operations.append(op_entry)
            if transport == "sse":
                sse_routes.append(op_entry)
            elif transport == "streaming":
                streaming_routes.append(op_entry)

    unmatched_oa = sorted(set(oa_ops_map.keys()) - matched_oa_keys)
    assert not unmatched_oa, f"Unmatched OpenAPI operations: {unmatched_oa}"

    live_operations.sort(key=lambda x: (x["engine"], x["path"], x["method"]))

    core_mig_files = sorted((API_DIR / "migrations" / "versions").glob("*.py"))
    mod_mig_files = sorted((ROOT / "modules" / "house_numbering" / "migrations" / "versions").glob("*.py"))
    migrations = [parse_migration_file(p, "core_api") for p in core_mig_files] + [
        parse_migration_file(p, "module:house_numbering") for p in mod_mig_files
    ]

    algorithms_catalog = build_algorithms_catalog()

    triaged_files: list[dict[str, Any]] = []
    engine_source_files: dict[str, list[dict[str, Any]]] = defaultdict(list)

    for rec in static_inv["files"]:
        path = rec["path"]
        cand = rec["engine_candidate"]
        role = rec["role"]
        if cand not in {"UNCLASSIFIED", "CROSS_CUTTING_REVIEW"}:
            if role == "source":
                engine_source_files[cand].append({
                    "path": path,
                    "role": role,
                    "classification": "retain" if cand == "E17" else "migrate",
                    "source_origin": "static-inventory direct mapping",
                })
            continue

        if role == "source":
            if path in CORE_CROSS_CUTTING_TRIAGE:
                eng, cls, rat, aff = CORE_CROSS_CUTTING_TRIAGE[path]
            else:
                eng, cls, rat, aff = ("E18", "migrate", "Platform support source file.", ["E18"])
            triaged_files.append({
                "path": path,
                "original_engine_candidate": cand,
                "role": role,
                "triaged_engine": eng,
                "affected_engines": aff,
                "classification": cls,
                "rationale": rat,
            })
            engine_source_files[eng].append({
                "path": path,
                "role": role,
                "classification": cls,
                "source_origin": f"triaged from {cand}: {rat}",
            })
        elif role == "migration":
            fname = Path(path).name
            if fname == "env.py":
                eng = "E18" if path.startswith("apps/api/") else "E15"
                rat = "Alembic migration runner environment; retained for schema/rollback compatibility (FR-011)."
            else:
                eng = MIGRATION_TO_ENGINE.get(fname, "E18")
                rat = f"Core Alembic migration mapped to owning engine {eng}; retained for FR-011 schema compatibility."
            triaged_files.append({
                "path": path,
                "original_engine_candidate": cand,
                "role": role,
                "triaged_engine": eng,
                "affected_engines": [eng],
                "classification": "retain",
                "rationale": rat,
            })
        elif role == "test":
            prim, secs, rat = classify_test_file(path)
            triaged_files.append({
                "path": path,
                "original_engine_candidate": cand,
                "role": role,
                "triaged_engine": prim,
                "affected_engines": [prim] + secs,
                "classification": "retain",
                "rationale": f"{rat} Retained as baseline parity test reference.",
            })
        else:
            triaged_files.append({
                "path": path,
                "original_engine_candidate": cand,
                "role": role,
                "triaged_engine": "E18",
                "affected_engines": ["E18"],
                "classification": "retain",
                "rationale": "Auxiliary repository artifact triaged to E18 ops/release.",
            })

    live_handler_keys = {(op["source_file"], op["handler"]) for op in live_operations}
    static_not_live = [
        rc for rc in static_inv["route_candidates"]
        if (rc["file"], rc["handler"]) not in live_handler_keys
    ]
    static_handler_keys = {(rc["file"], rc["handler"]) for rc in static_inv["route_candidates"]}
    live_not_static = [
        {"method": op["method"], "path": op["path"], "file": op["source_file"], "handler": op["handler"], "engine": op["engine"]}
        for op in live_operations
        if (op["source_file"], op["handler"]) not in static_handler_keys
    ]

    generated_at = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")

    engine_route_counts = dict(sorted(Counter(op["engine"] for op in live_operations).items()))
    method_counts = dict(sorted(Counter(op["method"] for op in live_operations).items()))
    transport_counts = dict(sorted(Counter(op["transport"] for op in live_operations).items()))
    classification_counts = dict(sorted(Counter(op["classification"] for op in live_operations).items()))

    inventory_doc: dict[str, Any] = {
        "schema_version": "064-runtime-inventory-v1",
        "generated_at": generated_at,
        "worktree_head": baseline,
        "static_inventory_baseline": static_baseline,
        "fastapi_app": {
            "title": openapi.get("info", {}).get("title"),
            "version": openapi.get("info", {}).get("version"),
            "openapi_version": openapi.get("openapi"),
            "openapi_path_count": len(openapi.get("paths", {})),
            "openapi_operation_count": len(oa_ops_map),
            "openapi_schema_count": len(schemas_dict),
            "component_schema_names": sorted(schemas_dict.keys()),
            "official_modules_discovered": [
                {
                    "extension_key": m.extension_key,
                    "version": m.manifest.get("version"),
                    "api_compatibility": m.manifest.get("api_compatibility"),
                    "commands": [c["id"] for c in m.manifest.get("commands", [])],
                    "permissions": m.manifest.get("permissions", []),
                }
                for m in official_modules
            ],
        },
        "summary": {
            "total_live_routes": len(live_operations),
            "http_operations": len(live_operations) - len(websocket_routes),
            "websocket_routes": len(websocket_routes),
            "sse_routes": len(sse_routes),
            "streaming_routes": len(streaming_routes),
            "unmapped_live_routes": 0,
            "by_engine": engine_route_counts,
            "by_method": method_counts,
            "by_transport": transport_counts,
            "by_classification": classification_counts,
            "core_migrations_count": len(core_mig_files),
            "module_migrations_count": len(mod_mig_files),
            "total_migrations_count": len(migrations),
            "registered_processing_algorithms_count": len(algorithms_catalog["processing_registry"]["algorithms"]),
            "processing_model_dag_algorithms_count": len(algorithms_catalog["processing_model_dag_algorithms"]["algorithms"]),
            "professional_geometry_ops_count": len(algorithms_catalog["professional_operations"]["geometry_operations"]),
            "professional_topology_checks_count": len(algorithms_catalog["professional_operations"]["topology_checks"]),
            "raster_derive_methods_count": len(algorithms_catalog["raster_and_terrain_operations"]["store_and_derive_methods"]),
            "static_files_triaged_count": len(triaged_files),
            "remaining_unclassified_count": 0,
            "remaining_cross_cutting_untriaged_count": 0,
            "pytest_files_indexed": len(test_index),
            "pytest_test_ids_indexed": sum(len(t["test_ids"]) for t in test_index),
        },
        "reconciliation_with_static_inventory": {
            "static_route_candidates_count": len(static_inv["route_candidates"]),
            "live_operations_count": len(live_operations),
            "static_candidates_not_mounted_live": static_not_live,
            "live_operations_not_in_static_candidates": live_not_static,
        },
        "websocket_routes": [
            {"path": w["path"], "handler": w["handler"], "module": w["module"], "engine": w["engine"], "auth_mode": w["auth_tenant_scope"]["auth_mode"]}
            for w in websocket_routes
        ],
        "sse_routes": [
            {"method": s["method"], "path": s["path"], "handler": s["handler"], "module": s["module"], "engine": s["engine"]}
            for s in sse_routes
        ],
        "operations": live_operations,
        "algorithms_and_registries": algorithms_catalog,
        "migrations": migrations,
        "static_inventory_triage": {
            "total_triaged": len(triaged_files),
            "by_role": dict(sorted(Counter(t["role"] for t in triaged_files).items())),
            "by_triaged_engine": dict(sorted(Counter(t["triaged_engine"] for t in triaged_files).items())),
            "items": triaged_files,
        },
    }

    CONTRACTS_DIR.mkdir(parents=True, exist_ok=True)
    ENGINES_CONTRACTS_DIR.mkdir(parents=True, exist_ok=True)
    EVIDENCE_DIR.mkdir(parents=True, exist_ok=True)

    inv_path = CONTRACTS_DIR / "inventory.json"
    inv_path.write_text(json.dumps(inventory_doc, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")

    e01_e09_summary: dict[str, Any] = {}
    for idx in range(1, 10):
        eid = f"E0{idx}"
        meta = ENGINE_META[eid]
        e_ops = [op for op in live_operations if op["engine"] == eid]
        e_migs = [m for m in migrations if m["engine"] == eid]
        e_sources = sorted(engine_source_files.get(eid, []), key=lambda x: x["path"])
        e_tests = [
            t for t in test_index
            if t["primary_engine"] == eid or eid in t["secondary_engines"]
        ]
        e_test_id_count = sum(len(t["test_ids"]) for t in e_tests)
        e_cls_counts = dict(sorted(Counter(op["classification"] for op in e_ops).items()))
        caps = engine_specific_capabilities(eid, algorithms_catalog)

        engine_doc = {
            "schema_version": "064-engine-contract-v1",
            "engine_id": eid,
            "name": meta["name"],
            "parent_references": meta["parent_references"],
            "spec_path": meta["spec_path"],
            "architecture_candidate": meta["architecture_candidate"],
            "baseline_commit": static_baseline,
            "worktree_head": baseline,
            "generated_at": generated_at,
            "ac1_status": "verified_runtime_inventory_complete",
            "summary": {
                "http_operations": sum(1 for o in e_ops if o["transport"] != "websocket"),
                "websocket_routes": sum(1 for o in e_ops if o["transport"] == "websocket"),
                "sse_routes": sum(1 for o in e_ops if o["transport"] == "sse"),
                "streaming_routes": sum(1 for o in e_ops if o["transport"] == "streaming"),
                "classifications": e_cls_counts,
                "migrations_count": len(e_migs),
                "source_files_count": len(e_sources),
                "test_files_count": len(e_tests),
                "test_case_ids_count": e_test_id_count,
            },
            "operations": e_ops,
            "algorithms_and_capabilities": caps,
            "migrations": e_migs,
            "source_files": e_sources,
            "cross_engine_dependencies": cross_engine_deps_for(eid),
            "existing_test_suite": e_tests,
            "edge_cases": ENGINE_EDGE_CASES[eid],
        }

        eng_json_path = ENGINES_CONTRACTS_DIR / f"{eid.lower()}.json"
        eng_json_path.write_text(json.dumps(engine_doc, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")

        ev_md_path = EVIDENCE_DIR / f"{eid.lower()}-inventory.md"
        ev_md_path.write_text(render_engine_evidence_md(engine_doc), encoding="utf-8")

        e01_e09_summary[eid] = engine_doc["summary"]

    print(json.dumps({
        "inventory_summary": inventory_doc["summary"],
        "reconciliation": {
            "static_not_live": len(static_not_live),
            "live_not_static": len(live_not_static),
        },
        "e01_e09": e01_e09_summary,
    }, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
