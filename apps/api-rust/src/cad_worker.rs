//! Bounded Python worker for E10 CAD/DXF parsing, serialization, georeference fitting, and native delta.

use axum::http::StatusCode;
use ovg_storage::editing::EditError;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::sync::Semaphore;
use uuid::Uuid;

const MODULES: &[(&str, &str)] = &[
    (
        "openvietgis_api/domains/platform_errors.py",
        include_str!("../../api/openvietgis_api/domains/platform_errors.py"),
    ),
    (
        "openvietgis_api/domains/tenancy/domain.py",
        include_str!("../../api/openvietgis_api/domains/tenancy/domain.py"),
    ),
    (
        "openvietgis_api/domains/authorization/context.py",
        include_str!("../../api/openvietgis_api/domains/authorization/context.py"),
    ),
    (
        "openvietgis_api/domains/spatial/crs.py",
        include_str!("../../api/openvietgis_api/domains/spatial/crs.py"),
    ),
    (
        "openvietgis_api/domains/spatial/domain.py",
        include_str!("../../api/openvietgis_api/domains/spatial/domain.py"),
    ),
    (
        "openvietgis_api/domains/editing/domain.py",
        include_str!("../../api/openvietgis_api/domains/editing/domain.py"),
    ),
    (
        "openvietgis_api/domains/cad/domain.py",
        include_str!("../../api/openvietgis_api/domains/cad/domain.py"),
    ),
    (
        "openvietgis_api/domains/cad/georeference.py",
        include_str!("../../api/openvietgis_api/domains/cad/georeference.py"),
    ),
    (
        "openvietgis_api/adapters/cad/dxf_codec.py",
        include_str!("../../api/openvietgis_api/adapters/cad/dxf_codec.py"),
    ),
    (
        "openvietgis_api/adapters/cad/native_delta.py",
        include_str!("../../api/openvietgis_api/adapters/cad/native_delta.py"),
    ),
];

const PACKAGE_INITS: &[&str] = &[
    "openvietgis_api/__init__.py",
    "openvietgis_api/domains/__init__.py",
    "openvietgis_api/domains/tenancy/__init__.py",
    "openvietgis_api/domains/authorization/__init__.py",
    "openvietgis_api/domains/spatial/__init__.py",
    "openvietgis_api/domains/editing/__init__.py",
    "openvietgis_api/domains/cad/__init__.py",
    "openvietgis_api/adapters/__init__.py",
    "openvietgis_api/adapters/cad/__init__.py",
];

const DRIVER: &str = r####"import hashlib
import json
import resource
import sys
from copy import deepcopy
from pathlib import Path
from uuid import UUID

resource.setrlimit(resource.RLIMIT_AS, (512 * 1024 * 1024, 512 * 1024 * 1024))
resource.setrlimit(resource.RLIMIT_CPU, (20, 24))

from openvietgis_api.adapters.cad.dxf_codec import parse_dxf_2d, write_dxf_2d
from openvietgis_api.adapters.cad.native_delta import cad_to_gis_preview, gis_to_cad_preview
from openvietgis_api.domains.cad.domain import CadUnit
from openvietgis_api.domains.cad.georeference import (
    CadPlacement,
    fit_web_placement,
    place_features,
    restore_features,
)
from openvietgis_api.domains.editing.domain import geometry_to_wkt, validate_geometry_payload
from openvietgis_api.domains.platform_errors import PlatformDomainError
from openvietgis_api.domains.spatial.domain import (
    FieldType,
    GeometryDimensions,
    GeometryFamily,
    SourceField,
    schema_revision_digest,
)

MAX_CAD_BYTES = 25 * 1024 * 1024


def _parse_controls(raw_controls):
    if raw_controls is None:
        return None
    return tuple(((float(a[0]), float(a[1])), (float(b[0]), float(b[1]))) for a, b in raw_controls)


def _fit_request_placement(doc_id_str, version, body):
    return fit_web_placement(
        document_id=UUID(doc_id_str),
        version=int(version),
        crs_code=int(body["crs_code"]),
        units=CadUnit(body["units"]),
        controls=_parse_controls(body["controls"]),
        web_controls=_parse_controls(body.get("web_controls")),
        tolerance_m=float(body["tolerance_m"]),
        max_scale_deviation=float(body["max_scale_deviation"]),
    )


def _reviewed_placement(summary, doc_id_str, version):
    meta = summary.get("placement")
    if not isinstance(meta, dict) or meta.get("algorithm") != "similarity-grid-2d-v1":
        raise ValueError("Missing reviewed CAD placement")
    native = meta.get("native_reference")
    crs_code = int(native["crs_code"] if native else meta["crs_code"])
    units = CadUnit(meta["units"])
    controls = _parse_controls(native["controls"] if native else meta["controls"])
    web_controls = _parse_controls(meta["controls"]) if native else None
    return fit_web_placement(
        document_id=UUID(doc_id_str),
        version=int(version),
        crs_code=crs_code,
        units=units,
        controls=controls,
        web_controls=web_controls,
        tolerance_m=float(meta["tolerance_grid_m"]),
        max_scale_deviation=float(meta["max_scale_deviation"]),
    )


def _inspection_fingerprint(digest, units, crs_code, tolerance, placement):
    profile = ["dxf-2d-bounded-v1", digest, units.value, crs_code, tolerance]
    if placement is not None:
        profile.append(placement.metadata())
    return hashlib.sha256(json.dumps(profile, allow_nan=False).encode()).hexdigest()


def _cad_schema_rev(family):
    nil = UUID("00000000-0000-0000-0000-000000000000")
    fields = [
        SourceField(
            organization_id=nil,
            project_id=nil,
            source_id=nil,
            field_id=fid,
            name=fname,
            ordinal=idx,
            type=FieldType.TEXT,
            nullable=False,
            schema_revision="0" * 64,
        )
        for idx, (fid, fname) in enumerate([("cad_layer", "CAD Layer"), ("cad_entity", "CAD Entity")])
    ]
    return schema_revision_digest(fields, GeometryFamily(family), GeometryDimensions.XY)


try:
    req = json.loads(Path(sys.argv[1]).read_text("utf-8"))
    action = req["action"]

    if action == "parse_dxf":
        payload = Path(req["payload_path"]).read_bytes()
        units = CadUnit(req["units"])
        crs_code = int(req["crs_code"])
        tolerance = req.get("curve_tolerance_m")
        georef = req.get("georef")
        placement = (
            _fit_request_placement(req["document_id"], req["version"], georef)
            if georef is not None
            else None
        )
        if placement is not None:
            crs_code = placement.crs_code

        family, features, summary, report = parse_dxf_2d(
            payload, units=units, curve_tolerance_m=tolerance
        )
        if placement is None:
            summary["spatial_mapping"] = "caller_asserted_projected_grid_xy_scaled_to_meters"
        else:
            if placement.units != units or crs_code != 3857 or tolerance is not None:
                from openvietgis_api.domains.platform_errors import PlatformValidationError

                raise PlatformValidationError(
                    "Unsupported control-point placement profile", code="CAD_GEOREFERENCE_INVALID"
                )
            summary["spatial_mapping"] = "control_point_similarity_grid_metres"
            summary["cad_document_id"] = str(placement.document_id)
            summary["cad_document_version"] = placement.version
            summary["placement"] = placement.metadata()
            if placement.native_reference is not None:
                summary["native_placed_extent"] = place_features(
                    deepcopy(features), placement.native_reference
                )
            summary["placed_extent"] = place_features(features, placement)

        for feat in features:
            validate_geometry_payload(feat["geometry"], GeometryFamily(family))
        digest = hashlib.sha256(payload).hexdigest()
        fingerprint = _inspection_fingerprint(digest, units, crs_code, tolerance, placement)
        schema_rev = _cad_schema_rev(family)
        wkts = [geometry_to_wkt(feat["geometry"]) for feat in features]
        features_out = [
            {
                "feature_id": f["feature_id"],
                "attributes": f["attributes"],
                "wkt": wkt,
            }
            for f, wkt in zip(features, wkts, strict=True)
        ]
        print(
            json.dumps(
                {
                    "ok": True,
                    "family": family,
                    "crs_code": crs_code,
                    "sha256": digest,
                    "inspection_fingerprint": fingerprint,
                    "schema_rev": schema_rev,
                    "entity_summary": summary,
                    "loss_report": report,
                    "wkts": wkts,
                    "features": features_out,
                    "has_placement": placement is not None,
                },
                allow_nan=False,
            )
        )

    elif action == "write_dxf":
        dxf_bytes = write_dxf_2d(
            req["features"],
            layer_name=req["layer_name"],
            units=CadUnit(req["units"]),
        )
        Path(req["output_path"]).write_bytes(dxf_bytes)
        print(
            json.dumps(
                {
                    "ok": True,
                    "sha256": hashlib.sha256(dxf_bytes).hexdigest(),
                    "byte_size": len(dxf_bytes),
                }
            )
        )

    elif action == "export_georef_draft":
        original = Path(req["original_path"]).read_bytes()
        doc_id = UUID(req["document_id"])
        version = int(req["version"])
        placement = _reviewed_placement(req["entity_summary"], str(doc_id), version)
        features = req["features"]
        profile = req["profile"]
        changed_handles = ()
        if profile == "native-xy-dxf-v1":
            payload, changed_handles = gis_to_cad_preview(
                original,
                base_sha256=req["document_sha256"],
                placement=placement,
                mapping=req["entity_summary"]["entity_mapping"],
                features=features,
            )
            loss = {
                "derived_document": True,
                "original_document_unchanged": True,
                "attributes_omitted": True,
                "native_handles_not_preserved": False,
                "qualified_entity_properties_preserved": True,
                "other_native_metadata_not_certified": True,
                "geometry_only": True,
                "all_current_source_features_included": True,
                "survey_grade": False,
                "requires_acknowledgement": True,
            }
        else:
            local = restore_features(
                features, placement, document_id=doc_id, version=version, source_crs_code=3857
            )
            payload = write_dxf_2d(local, layer_name=req["layer_name"], units=placement.units)
            loss = {
                "derived_document": True,
                "original_document_unchanged": True,
                "attributes_omitted": True,
                "native_handles_not_preserved": True,
                "native_styles_layers_curves_blocks_text_and_unlinked_entities_not_preserved": True,
                "all_current_source_features_included": True,
                "survey_grade": False,
                "requires_acknowledgement": True,
            }
        if len(payload) > MAX_CAD_BYTES:
            print(json.dumps({"ok": False, "detail_status": 422, "message": "Derived DXF exceeds 25 MiB"}))
            sys.exit(2)
        preview = {
            "document_id": str(doc_id),
            "version": version,
            "document_sha256": req["document_sha256"],
            "conversion_id": req["conversion_id"],
            "source_id": req["source_id"],
            "layer_id": req["layer_id"],
            "content_revision": req["content_revision"],
            "schema_revision": req["schema_revision"],
            "format": "dxf",
            "units": placement.units.value,
            "feature_count": len(features),
            "loss_report": loss,
            "published": False,
            "profile": profile,
            "changed_handles": list(changed_handles),
        }
        fingerprint = hashlib.sha256(
            json.dumps(
                {
                    "profile": profile,
                    "preview": preview,
                    "placement": placement.metadata(),
                    "features": features,
                    "layer_name": req["layer_name"],
                },
                sort_keys=True,
                separators=(",", ":"),
                allow_nan=False,
            ).encode()
        ).hexdigest()
        Path(req["output_path"]).write_bytes(payload)
        print(
            json.dumps(
                {
                    "ok": True,
                    "preview": preview,
                    "preview_fingerprint": fingerprint,
                    "payload_sha256": hashlib.sha256(payload).hexdigest(),
                },
                allow_nan=False,
            )
        )

    elif action == "preview_native_import":
        original = Path(req["original_path"]).read_bytes()
        candidate = Path(req["candidate_path"]).read_bytes()
        doc_id = str(req["document_id"])
        version = int(req["version"])
        summary = req["entity_summary"]
        placement = _reviewed_placement(summary, doc_id, version)
        ops = cad_to_gis_preview(
            original,
            candidate,
            base_sha256=req["original_sha"],
            candidate_sha256=req["candidate_sha"],
            placement=placement,
            mapping=summary["entity_mapping"],
            imported_content_revision=req["imported_content_revision"],
            current_content_revision=req["current_content_revision"],
            imported_schema_revision=req["imported_schema_revision"],
            current_schema_revision=req["current_schema_revision"],
        )
        print(
            json.dumps(
                {
                    "ok": True,
                    "operations": [
                        {"op_type": op.op_type.value, "feature_id": op.feature_id, "geometry": op.geometry}
                        for op in ops
                    ],
                },
                allow_nan=False,
            )
        )

    else:
        raise ValueError(f"unsupported action: {action}")

except PlatformDomainError as exc:
    print(
        json.dumps(
            {
                "ok": False,
                "status": exc.status_code,
                "code": exc.code,
                "message": exc.message,
                "details": exc.details,
            },
            allow_nan=False,
        )
    )
    sys.exit(2)
except ValueError as exc:
    print(json.dumps({"ok": False, "detail_status": 422, "message": str(exc)}))
    sys.exit(2)
except Exception as exc:
    print(json.dumps({"ok": False, "detail_status": 500, "message": str(exc)}))
    sys.exit(1)
"####;

const WORKERS: usize = 4;
const TIMEOUT: Duration = Duration::from_secs(20);
static SEM: Semaphore = Semaphore::const_new(WORKERS);

struct WorkDir(PathBuf);
impl Drop for WorkDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn python_executable() -> PathBuf {
    if let Ok(explicit) = std::env::var("OPENVIETGIS_CAD_PYTHON") {
        let p = PathBuf::from(explicit);
        if p.is_file() {
            return p;
        }
    }
    if let Ok(venv) = std::env::var("VIRTUAL_ENV") {
        let candidate = PathBuf::from(venv).join("bin").join("python3");
        if candidate.is_file() {
            return candidate;
        }
    }
    PathBuf::from("/usr/bin/python3")
}

pub(crate) async fn run_parse_dxf(payload: &[u8], mut req: Value) -> Result<Value, EditError> {
    let work = WorkDir(std::env::temp_dir().join(format!("ovg-e10-parse-{}", Uuid::new_v4())));
    tokio::fs::create_dir_all(&work.0)
        .await
        .map_err(|err| EditError::Internal(format!("create cad work dir: {err}")))?;
    let payload_path = work.0.join("input.dxf");
    tokio::fs::write(&payload_path, payload)
        .await
        .map_err(|err| EditError::Internal(format!("write cad input: {err}")))?;
    req["action"] = Value::String("parse_dxf".to_owned());
    req["payload_path"] = Value::String(payload_path.to_string_lossy().into_owned());
    invoke(&work.0, req).await
}

pub(crate) async fn run_write_dxf(mut req: Value) -> Result<(Vec<u8>, String, i64), EditError> {
    let work = WorkDir(std::env::temp_dir().join(format!("ovg-e10-write-{}", Uuid::new_v4())));
    tokio::fs::create_dir_all(&work.0)
        .await
        .map_err(|err| EditError::Internal(format!("create cad work dir: {err}")))?;
    let out_path = work.0.join("output.dxf");
    req["action"] = Value::String("write_dxf".to_owned());
    req["output_path"] = Value::String(out_path.to_string_lossy().into_owned());
    let meta = invoke(&work.0, req).await?;
    let bytes = tokio::fs::read(&out_path)
        .await
        .map_err(|err| EditError::Internal(format!("read dxf output: {err}")))?;
    let sha256 = meta["sha256"].as_str().unwrap_or_default().to_owned();
    let byte_size = meta["byte_size"].as_i64().unwrap_or(bytes.len() as i64);
    Ok((bytes, sha256, byte_size))
}

pub(crate) async fn run_export_georef_draft(
    original: &[u8],
    mut req: Value,
) -> Result<(Value, Vec<u8>), EditError> {
    let work = WorkDir(std::env::temp_dir().join(format!("ovg-e10-draft-{}", Uuid::new_v4())));
    tokio::fs::create_dir_all(&work.0)
        .await
        .map_err(|err| EditError::Internal(format!("create cad work dir: {err}")))?;
    let orig_path = work.0.join("original.dxf");
    let out_path = work.0.join("draft.dxf");
    tokio::fs::write(&orig_path, original)
        .await
        .map_err(|err| EditError::Internal(format!("write cad original: {err}")))?;
    req["action"] = Value::String("export_georef_draft".to_owned());
    req["original_path"] = Value::String(orig_path.to_string_lossy().into_owned());
    req["output_path"] = Value::String(out_path.to_string_lossy().into_owned());
    let meta = invoke(&work.0, req).await?;
    let bytes = tokio::fs::read(&out_path)
        .await
        .map_err(|err| EditError::Internal(format!("read draft dxf: {err}")))?;
    Ok((meta, bytes))
}

pub(crate) async fn run_preview_native_import(
    original: &[u8],
    candidate: &[u8],
    mut req: Value,
) -> Result<Value, EditError> {
    let work = WorkDir(std::env::temp_dir().join(format!("ovg-e10-delta-{}", Uuid::new_v4())));
    tokio::fs::create_dir_all(&work.0)
        .await
        .map_err(|err| EditError::Internal(format!("create cad work dir: {err}")))?;
    let orig_path = work.0.join("original.dxf");
    let cand_path = work.0.join("candidate.dxf");
    tokio::fs::write(&orig_path, original)
        .await
        .map_err(|err| EditError::Internal(format!("write cad original: {err}")))?;
    tokio::fs::write(&cand_path, candidate)
        .await
        .map_err(|err| EditError::Internal(format!("write cad candidate: {err}")))?;
    req["action"] = Value::String("preview_native_import".to_owned());
    req["original_path"] = Value::String(orig_path.to_string_lossy().into_owned());
    req["candidate_path"] = Value::String(cand_path.to_string_lossy().into_owned());
    invoke(&work.0, req).await
}

async fn invoke(work_dir: &Path, request: Value) -> Result<Value, EditError> {
    let _permit = SEM
        .acquire()
        .await
        .map_err(|_| EditError::detail(503, "CAD worker unavailable"))?;

    for init in PACKAGE_INITS {
        let p = work_dir.join(init);
        if let Some(parent) = p.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|err| EditError::Internal(format!("create cad pkg dir: {err}")))?;
        }
        tokio::fs::write(&p, b"")
            .await
            .map_err(|err| EditError::Internal(format!("write cad pkg init: {err}")))?;
    }
    for (rel, body) in MODULES {
        let p = work_dir.join(rel);
        if let Some(parent) = p.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|err| EditError::Internal(format!("create cad mod dir: {err}")))?;
        }
        tokio::fs::write(&p, body)
            .await
            .map_err(|err| EditError::Internal(format!("write cad module: {err}")))?;
    }

    let req_path = work_dir.join("req.json");
    let drv_path = work_dir.join("driver.py");
    let req_bytes = serde_json::to_vec(&request)
        .map_err(|err| EditError::Internal(format!("serialize cad worker req: {err}")))?;
    tokio::fs::write(&req_path, req_bytes)
        .await
        .map_err(|err| EditError::Internal(format!("write cad req.json: {err}")))?;
    tokio::fs::write(&drv_path, DRIVER)
        .await
        .map_err(|err| EditError::Internal(format!("write cad driver.py: {err}")))?;

    let py = python_executable();
    let mut cmd = tokio::process::Command::new(py);
    cmd.arg("-B")
        .arg(&drv_path)
        .arg(&req_path)
        .env_clear()
        .env("PYTHONPATH", work_dir)
        .env("PATH", "/usr/bin:/bin")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let out = tokio::time::timeout(TIMEOUT, cmd.output())
        .await
        .map_err(|_| EditError::detail(503, "CAD worker timed out"))?
        .map_err(|err| EditError::Internal(format!("spawn cad worker: {err}")))?;

    let stdout = String::from_utf8_lossy(&out.stdout);
    if let Some(last_line) = stdout.lines().rfind(|l| !l.trim().is_empty()) {
        if let Ok(val) = serde_json::from_str::<Value>(last_line) {
            if val.get("ok").and_then(Value::as_bool) == Some(true) {
                return Ok(val);
            }
            if let Some(detail_status) = val.get("detail_status").and_then(Value::as_u64) {
                let msg = val
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("CAD request rejected");
                if detail_status == 500 {
                    return Err(EditError::Internal(msg.to_owned()));
                }
                return Err(EditError::detail(detail_status as u16, msg));
            }
            let status = val.get("status").and_then(Value::as_u64).unwrap_or(422) as u16;
            let code = val
                .get("code")
                .and_then(Value::as_str)
                .unwrap_or("VALIDATION_ERROR");
            let message = val
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("CAD validation failed")
                .to_owned();
            let details = val.get("details").cloned().filter(|v| !v.is_null());
            return Err(EditError::Platform {
                status,
                code: leak_static_code(code),
                message,
                details,
            });
        }
    }

    let stderr = String::from_utf8_lossy(&out.stderr);
    let _ = StatusCode::INTERNAL_SERVER_ERROR;
    Err(EditError::Internal(format!(
        "CAD worker failed: {stderr}"
    )))
}

fn leak_static_code(code: &str) -> &'static str {
    match code {
        "CAD_FORMAT_UNSUPPORTED" => "CAD_FORMAT_UNSUPPORTED",
        "UNGEOREFERENCED_CAD_REJECTED" => "UNGEOREFERENCED_CAD_REJECTED",
        "CAD_FILE_LIMIT" => "CAD_FILE_LIMIT",
        "CAD_VERSION_UNSUPPORTED" => "CAD_VERSION_UNSUPPORTED",
        "CAD_UNIT_MISMATCH" => "CAD_UNIT_MISMATCH",
        "CAD_ENTITY_LIMIT" => "CAD_ENTITY_LIMIT",
        "CAD_COORDINATE_LIMIT" => "CAD_COORDINATE_LIMIT",
        "CAD_3D_UNSUPPORTED" => "CAD_3D_UNSUPPORTED",
        "CAD_OCS_UNSUPPORTED" => "CAD_OCS_UNSUPPORTED",
        "CAD_CURVE_LIMIT" => "CAD_CURVE_LIMIT",
        "CAD_WIDTH_UNSUPPORTED" => "CAD_WIDTH_UNSUPPORTED",
        "CAD_BULGE_UNSUPPORTED" => "CAD_BULGE_UNSUPPORTED",
        "CAD_NO_GEOMETRY" => "CAD_NO_GEOMETRY",
        "CAD_MIXED_FAMILIES" => "CAD_MIXED_FAMILIES",
        "CAD_GEOMETRY_UNSUPPORTED" => "CAD_GEOMETRY_UNSUPPORTED",
        "CAD_GEOREFERENCE_INVALID" => "CAD_GEOREFERENCE_INVALID",
        "CAD_GEOREFERENCE_CONFIRM_REQUIRED" => "CAD_GEOREFERENCE_CONFIRM_REQUIRED",
        "CAD_DELTA_UNSUPPORTED" => "CAD_DELTA_UNSUPPORTED",
        "STALE_CAD_REVISION" => "STALE_CAD_REVISION",
        "STALE_SOURCE_REVISION" => "STALE_SOURCE_REVISION",
        "INVALID_DXF" => "INVALID_DXF",
        "INVALID_GEOMETRY" => "INVALID_GEOMETRY",
        "CAD_INSPECTION_STALE" => "CAD_INSPECTION_STALE",
        "CAD_LOSS_ACK_REQUIRED" => "CAD_LOSS_ACK_REQUIRED",
        "DWG_ADAPTER_REQUIRED" => "DWG_ADAPTER_REQUIRED",
        "CAD_PROJECTED_CRS_REQUIRED" => "CAD_PROJECTED_CRS_REQUIRED",
        "CAD_FEATURE_LIMIT" => "CAD_FEATURE_LIMIT",
        "CAD_SPLINE_UNSUPPORTED" => "CAD_SPLINE_UNSUPPORTED",
        "CAD_CURVE_UNSUPPORTED" => "CAD_CURVE_UNSUPPORTED",
        _ => "VALIDATION_ERROR",
    }
}
