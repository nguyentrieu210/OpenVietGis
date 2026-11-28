//! Bounded format worker for E07 binary exports and E13 offline packaging/sync validation.
//! Rust owns the tenant transaction, CSRF/session checks, storage I/O, and changeset commits;
//! the worker receives no database credentials or network configuration.

use axum::http::StatusCode;
use serde_json::{json, Value};
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
        "openvietgis_api/domains/transfers/domain.py",
        include_str!("../../api/openvietgis_api/domains/transfers/domain.py"),
    ),
    (
        "openvietgis_api/domains/transfers/ports.py",
        include_str!("../../api/openvietgis_api/domains/transfers/ports.py"),
    ),
    (
        "openvietgis_api/adapters/formats/wkt_crs.py",
        include_str!("../../api/openvietgis_api/adapters/formats/wkt_crs.py"),
    ),
    (
        "openvietgis_api/adapters/formats/geopackage.py",
        include_str!("../../api/openvietgis_api/adapters/formats/geopackage.py"),
    ),
    (
        "openvietgis_api/adapters/formats/shapefile_zip.py",
        include_str!("../../api/openvietgis_api/adapters/formats/shapefile_zip.py"),
    ),
    (
        "openvietgis_api/adapters/formats/offline_snapshot.py",
        include_str!("../../api/openvietgis_api/adapters/formats/offline_snapshot.py"),
    ),
];

/// The production runtime intentionally uses Debian bookworm's system Python 3.11
/// for the isolated format worker. Canonical Python sources are developed on a
/// newer interpreter and may contain PEP 695 type aliases. The worker only
/// needs those names for postponed annotations, so generate a syntax-compatible
/// private copy without weakening the canonical source tree.
fn worker_python_311_source(source: &str) -> String {
    source
        .lines()
        .map(|line| {
            let Some(alias) = line.strip_prefix("type ") else {
                return line.to_owned();
            };
            let Some((name, rhs)) = alias.split_once(" = ") else {
                return line.to_owned();
            };
            if name == "CoordinateTree" {
                // Recursive alias syntax cannot be evaluated by Python 3.11;
                // every use in the embedded domain module is annotation-only.
                format!("{name} = tuple")
            } else {
                format!("{name} = {rhs}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

const PACKAGE_INITS: &[&str] = &[
    "openvietgis_api/__init__.py",
    "openvietgis_api/domains/__init__.py",
    "openvietgis_api/domains/tenancy/__init__.py",
    "openvietgis_api/domains/authorization/__init__.py",
    "openvietgis_api/domains/spatial/__init__.py",
    "openvietgis_api/domains/editing/__init__.py",
    "openvietgis_api/domains/transfers/__init__.py",
    "openvietgis_api/adapters/__init__.py",
    "openvietgis_api/adapters/formats/__init__.py",
];

const DRIVER: &str = r####"import hashlib
import json
import resource
import sys
from pathlib import Path

resource.setrlimit(resource.RLIMIT_AS, (512 * 1024 * 1024, 512 * 1024 * 1024))
resource.setrlimit(resource.RLIMIT_CPU, (15, 18))

from openvietgis_api.adapters.formats.geopackage import write_geopackage
from openvietgis_api.adapters.formats.offline_snapshot import (
    canonical_json,
    embed_snapshot,
    validate_offline_ops,
)
from openvietgis_api.domains.platform_errors import (
    PlatformDomainError,
    PlatformValidationError,
)
from openvietgis_api.domains.spatial.domain import FieldType, NativeGeometry
from openvietgis_api.domains.transfers.domain import ExportFieldMapping
from openvietgis_api.domains.transfers.ports import TransferError

try:
    req = json.loads(Path(sys.argv[1]).read_text("utf-8"))
    action = req["action"]
    if action == "build_package":
        snap = req["snapshot"]
        srid = int(req["srid"])
        mappings = [
            ExportFieldMapping(m["field_id"], m["name"], m["output"], FieldType(m["type"]))
            for m in req["mappings"]
        ]
        gpkg_rows = [
            (
                str(f["feature_id"]),
                dict(f["properties"]),
                NativeGeometry(
                    type=str(f["geometry"]["type"]),
                    coordinates=f["geometry"]["coordinates"],
                    srid=srid,
                ),
            )
            for f in snap["features"]
        ]
        gpkg_bytes, _, _ = write_geopackage(
            layer_name="field_layer",
            crs_code=srid,
            geometry_family=str(req["geometry_family"]),
            field_mappings=mappings,
            features=gpkg_rows,
        )
        for offset in range(0, len(gpkg_rows), 2000):
            validate_offline_ops(
                snap,
                [
                    {"op_type": "update", "feature_id": fid, "properties": attrs}
                    for fid, attrs, _ in gpkg_rows[offset : offset + 2000]
                ],
            )
        gpkg_bytes, snapshot_sha = embed_snapshot(gpkg_bytes, snap)
        if len(gpkg_bytes) > 75 * 1024 * 1024:
            raise PlatformValidationError("offline package byte limit exceeded; narrow AOI")
        Path(req["out_path"]).write_bytes(gpkg_bytes)
        print(
            json.dumps(
                {
                    "ok": True,
                    "snapshot_sha256": snapshot_sha,
                    "byte_size": len(gpkg_bytes),
                },
                allow_nan=False,
            )
        )
    elif action == "inspect_transfer":
        resource.setrlimit(resource.RLIMIT_FSIZE, (64 * 1024 * 1024, 64 * 1024 * 1024))
        from openvietgis_api.adapters.formats.geopackage import (
            inspect_geopackage, parse_geopackage_member,
        )
        data = Path(req["payload_path"]).read_bytes()
        encoding = req["declared_encoding"]
        source_crs = req["declared_source_crs"]
        if req["format"] == "geopackage":
            members = inspect_geopackage(data)
        elif req["format"] == "shapefile_zip":
            from openvietgis_api.adapters.formats.shapefile_zip import (
                inspect_shapefile_zip, parse_shapefile_member,
            )
            members = inspect_shapefile_zip(data, encoding=encoding)
        else:
            raise ValueError("unsupported binary import format")
        eligible = [m for m in members if m.eligible]
        selected = req["selected_member"]
        if selected is None and len(eligible) == 1:
            selected = eligible[0].name
        parsed = None
        if any(m.name == selected for m in eligible):
            if req["format"] == "geopackage":
                parsed = parse_geopackage_member(
                    data, member_name=selected, declared_crs=source_crs,
                )
            else:
                parsed = parse_shapefile_member(
                    data, member_name=selected, declared_crs=source_crs,
                    declared_encoding=encoding,
                )
        report = {"members": [m.to_wire() for m in members], "parsed": None}
        if parsed is not None:
            report["parsed"] = {
                "member_name": parsed.member_name,
                "geometry_family": parsed.geometry_family.value,
                "dimensions": parsed.dimensions.value,
                "detected_crs": parsed.detected_crs,
                "detected_encoding": parsed.detected_encoding,
                "requires_encoding_resolution": parsed.requires_encoding_resolution,
                "fields": [f.to_wire() for f in parsed.fields],
                "findings": [f.to_wire() for f in parsed.findings],
                "extent": parsed.extent,
                "features": [
                    {
                        "feature_id": feature.feature_id,
                        "attributes": feature.attributes,
                        "geometry": {
                            "type": feature.geometry.type,
                            "coordinates": feature.geometry.coordinates,
                        },
                    }
                    for feature in parsed.features
                ],
            }
        with Path(req["out_path"]).open("w", encoding="utf-8") as handle:
            json.dump(report, handle, allow_nan=False, ensure_ascii=False, separators=(",", ":"))
        print(json.dumps({"ok": True}, allow_nan=False))
    elif action == "export_transfer":
        resource.setrlimit(resource.RLIMIT_FSIZE, (75 * 1024 * 1024, 75 * 1024 * 1024))
        srid = int(req["srid"])
        mappings = [
            ExportFieldMapping(m["field_id"], m["source_name"], m["output_name"], FieldType(m["type"]))
            for m in req["mappings"]
        ]
        features = [
            (
                str(f["feature_id"]),
                dict(f["properties"]),
                NativeGeometry(
                    type=str(f["geometry"]["type"]),
                    coordinates=f["geometry"]["coordinates"],
                    srid=srid,
                ),
            )
            for f in req["features"]
        ]
        if req["format"] == "geopackage":
            writer = write_geopackage
        elif req["format"] == "shapefile_zip":
            from openvietgis_api.adapters.formats.shapefile_zip import write_shapefile_zip
            writer = write_shapefile_zip
        else:
            raise ValueError("unsupported binary export format")
        artifact, _, _ = writer(
            layer_name=req["layer_name"],
            crs_code=srid,
            geometry_family=req["geometry_family"],
            field_mappings=mappings,
            features=features,
        )
        if not artifact or len(artifact) > 75 * 1024 * 1024:
            raise PlatformValidationError("binary export exceeds format engine output limit")
        Path(req["out_path"]).write_bytes(artifact)
        print(json.dumps({"ok": True, "byte_size": len(artifact)}, allow_nan=False))
    elif action == "validate_ops":
        validate_offline_ops(req["snapshot"], req["operations"])
        scope_hash = hashlib.sha256(canonical_json(req["scope_tuple"])).hexdigest()[:48]
        payload_hash = hashlib.sha256(
            canonical_json([req["base_rev"], req["operations"]])
        ).hexdigest()
        print(
            json.dumps(
                {
                    "ok": True,
                    "scope_hash": scope_hash,
                    "payload_hash": payload_hash,
                },
                allow_nan=False,
            )
        )
    else:
        raise ValueError("unsupported action")
except PlatformDomainError as exc:
    err = {
        "ok": False,
        "status": exc.status_code,
        "code": exc.code,
        "message": exc.message,
    }
    if exc.details:
        err["details"] = exc.details
    print(json.dumps(err, allow_nan=False))
except TransferError as exc:
    print(json.dumps({
        "ok": False, "status": exc.status_code,
        "code": exc.code, "message": exc.message,
    }, allow_nan=False))
except Exception:
    print(
        json.dumps(
            {
                "ok": False,
                "status": 500,
                "code": "WORKER_FAILED",
                "message": "offline format worker failed",
            }
        )
    )
"####;

const MAX_PACKAGE_BYTES: usize = 75 * 1024 * 1024;
const MAX_REQUEST_BYTES: usize = 64 * 1024 * 1024;
static SLOTS: Semaphore = Semaphore::const_new(2);

#[derive(Debug)]
pub(crate) struct WorkerError {
    pub(crate) status: StatusCode,
    pub(crate) code: &'static str,
    pub(crate) message: String,
    pub(crate) details: Option<Value>,
}

fn unavailable(message: &str) -> WorkerError {
    WorkerError {
        status: StatusCode::SERVICE_UNAVAILABLE,
        code: "OFFLINE_ENGINE_UNAVAILABLE",
        message: message.to_owned(),
        details: None,
    }
}

fn static_code(raw: &str) -> &'static str {
    match raw {
        "SCHEMA_MISMATCH" => "SCHEMA_MISMATCH",
        "OUTSIDE_PACKAGE_SCOPE" => "OUTSIDE_PACKAGE_SCOPE",
        "INVALID_OFFLINE_OPERATION_ID" => "INVALID_OFFLINE_OPERATION_ID",
        "INVALID_FEATURE_ID" => "INVALID_FEATURE_ID",
        "PACKAGE_INVALID" => "PACKAGE_INVALID",
        "CONFLICT" => "CONFLICT",
        "FORBIDDEN" => "FORBIDDEN",
        "NOT_FOUND" => "NOT_FOUND",
        "ARCHIVE_BOMB_REJECTED" => "ARCHIVE_BOMB_REJECTED",
        "ARCHIVE_DUPLICATE_ENTRY" => "ARCHIVE_DUPLICATE_ENTRY",
        "ARCHIVE_ENCRYPTED" => "ARCHIVE_ENCRYPTED",
        "ARCHIVE_ENTRY_LIMIT_EXCEEDED" => "ARCHIVE_ENTRY_LIMIT_EXCEEDED",
        "ARCHIVE_PATH_TRAVERSAL" => "ARCHIVE_PATH_TRAVERSAL",
        "ARCHIVE_SIZE_EXCEEDED" => "ARCHIVE_SIZE_EXCEEDED",
        "ARCHIVE_SYMLINK_REJECTED" => "ARCHIVE_SYMLINK_REJECTED",
        "CRS_MISMATCH" => "CRS_MISMATCH",
        "ENCODING_CONFLICT" => "ENCODING_CONFLICT",
        "ENCODING_DECODE_FAILED" => "ENCODING_DECODE_FAILED",
        "FEATURE_LIMIT_EXCEEDED" => "FEATURE_LIMIT_EXCEEDED",
        "INVALID_ATTRIBUTE_VALUE" => "INVALID_ATTRIBUTE_VALUE",
        "INVALID_GEOMETRY" => "INVALID_GEOMETRY",
        "INVALID_GEOPACKAGE" => "INVALID_GEOPACKAGE",
        "INVALID_ZIP" => "INVALID_ZIP",
        "MISSING_SHAPEFILE_COMPONENTS" => "MISSING_SHAPEFILE_COMPONENTS",
        "MIXED_GEOMETRY_FAMILY" => "MIXED_GEOMETRY_FAMILY",
        "NULL_GEOMETRY" => "NULL_GEOMETRY",
        "TOO_MANY_FIELDS" => "TOO_MANY_FIELDS",
        "UNKNOWN_MEMBER" => "UNKNOWN_MEMBER",
        "UNSUPPORTED_DIMENSIONS" => "UNSUPPORTED_DIMENSIONS",
        "UNSUPPORTED_GEOMETRY" => "UNSUPPORTED_GEOMETRY",
        _ => "VALIDATION_ERROR",
    }
}

struct WorkerScratch(PathBuf);

impl Drop for WorkerScratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

async fn create_scratch() -> Result<WorkerScratch, WorkerError> {
    let path = std::env::temp_dir().join(format!("ovg-field-{}", Uuid::new_v4()));
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder
        .create(&path)
        .map_err(|_| unavailable("offline scratch unavailable"))?;
    let scratch = WorkerScratch(path);
    for init in PACKAGE_INITS {
        let file = scratch.0.join(init);
        if let Some(parent) = file.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|_| unavailable("offline scratch init failed"))?;
        }
        tokio::fs::write(&file, b"")
            .await
            .map_err(|_| unavailable("offline scratch init failed"))?;
    }
    for (name, source) in MODULES {
        let file = scratch.0.join(name);
        if let Some(parent) = file.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|_| unavailable("offline scratch write failed"))?;
        }
        tokio::fs::write(&file, worker_python_311_source(source))
            .await
            .map_err(|_| unavailable("offline scratch write failed"))?;
    }
    tokio::fs::write(scratch.0.join("driver.py"), DRIVER)
        .await
        .map_err(|_| unavailable("offline driver write failed"))?;
    Ok(scratch)
}

async fn run_driver(scratch: &WorkerScratch, req: &Value) -> Result<Value, WorkerError> {
    if !crate::raster_compute::worker_nonroot() {
        return Err(unavailable(
            "offline format worker requires an unprivileged Linux runtime",
        ));
    }
    let _permit = SLOTS
        .acquire()
        .await
        .map_err(|_| unavailable("offline worker pool closed"))?;
    let req_bytes =
        serde_json::to_vec(req).map_err(|_| unavailable("invalid offline worker payload"))?;
    if req_bytes.len() > MAX_REQUEST_BYTES {
        return Err(WorkerError {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code: "VALIDATION_ERROR",
            message: "offline package request exceeds worker size limit".to_owned(),
            details: None,
        });
    }
    let req_path = scratch.0.join("request.json");
    tokio::fs::write(&req_path, &req_bytes)
        .await
        .map_err(|_| unavailable("offline worker request staging failed"))?;

    let mut command = tokio::process::Command::new("/usr/bin/python3");
    command
        .args(["-E", "-s", "-B"])
        .arg(scratch.0.join("driver.py"))
        .arg(&req_path)
        .current_dir(&scratch.0)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("LANG", "C.UTF-8")
        .env("HOME", &scratch.0)
        .env("TMPDIR", &scratch.0)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);

    let output = tokio::time::timeout(Duration::from_secs(20), command.output())
        .await
        .map_err(|_| unavailable("offline worker timed out"))?
        .map_err(|_| unavailable("offline worker launch failed"))?;
    if !output.status.success() {
        return Err(unavailable("offline format worker failed"));
    }
    let parsed: Value = serde_json::from_slice(&output.stdout)
        .map_err(|_| unavailable("offline worker returned invalid JSON"))?;
    if parsed.get("ok").and_then(Value::as_bool) == Some(true) {
        return Ok(parsed);
    }
    let status = parsed
        .get("status")
        .and_then(Value::as_u64)
        .and_then(|s| u16::try_from(s).ok())
        .and_then(|s| StatusCode::from_u16(s).ok())
        .unwrap_or(StatusCode::UNPROCESSABLE_ENTITY);
    if status == StatusCode::INTERNAL_SERVER_ERROR {
        return Err(unavailable("offline format engine failed"));
    }
    let code = parsed
        .get("code")
        .and_then(Value::as_str)
        .map_or("VALIDATION_ERROR", static_code);
    let message = parsed
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("offline validation failed")
        .to_owned();
    let details = parsed.get("details").cloned().filter(Value::is_object);
    Err(WorkerError {
        status,
        code,
        message,
        details,
    })
}

pub(crate) struct BuiltPackage {
    pub(crate) gpkg_bytes: Vec<u8>,
    pub(crate) snapshot_sha256: String,
}

pub(crate) async fn build_package(
    srid: i32,
    geometry_family: &str,
    mappings: &[Value],
    snapshot: &Value,
) -> Result<BuiltPackage, WorkerError> {
    let scratch = create_scratch().await?;
    let out_path = scratch.0.join("package.gpkg");
    let req = json!({
        "action": "build_package",
        "srid": srid,
        "geometry_family": geometry_family,
        "mappings": mappings,
        "snapshot": snapshot,
        "out_path": out_path.to_string_lossy(),
    });
    let res = run_driver(&scratch, &req).await?;
    let snapshot_sha256 = res
        .get("snapshot_sha256")
        .and_then(Value::as_str)
        .filter(|s| s.len() == 64)
        .ok_or_else(|| unavailable("missing snapshot digest"))?
        .to_owned();
    let gpkg_bytes = tokio::fs::read(&out_path)
        .await
        .map_err(|_| unavailable("failed to read built offline package"))?;
    if gpkg_bytes.is_empty() || gpkg_bytes.len() > MAX_PACKAGE_BYTES {
        return Err(WorkerError {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code: "VALIDATION_ERROR",
            message: "offline package byte limit exceeded; narrow AOI".to_owned(),
            details: None,
        });
    }
    let _ = Path::new(&out_path);
    Ok(BuiltPackage {
        gpkg_bytes,
        snapshot_sha256,
    })
}

/// Decode format bytes in an unprivileged process. The returned metadata is
/// independently checked by the Rust inspection and storage policy.
pub(crate) async fn inspect_transfer_payload(
    format: &str,
    payload: &[u8],
    selected_member: Option<&str>,
    declared_source_crs: Option<i64>,
    declared_encoding: Option<&str>,
) -> Result<Value, WorkerError> {
    let scratch = create_scratch().await?;
    let input = scratch.0.join("source.bin");
    let output = scratch.0.join("inspection.json");
    tokio::fs::write(&input, payload)
        .await
        .map_err(|_| unavailable("transfer input staging failed"))?;
    let request = json!({
        "action": "inspect_transfer",
        "format": format,
        "payload_path": input.to_string_lossy(),
        "out_path": output.to_string_lossy(),
        "selected_member": selected_member,
        "declared_source_crs": declared_source_crs,
        "declared_encoding": declared_encoding,
    });
    run_driver(&scratch, &request).await?;
    let metadata = tokio::fs::metadata(&output)
        .await
        .map_err(|_| unavailable("missing transfer inspection"))?;
    if metadata.len() == 0 || metadata.len() > MAX_REQUEST_BYTES as u64 {
        return Err(unavailable("invalid transfer inspection size"));
    }
    let bytes = tokio::fs::read(&output)
        .await
        .map_err(|_| unavailable("transfer inspection read failed"))?;
    serde_json::from_slice(&bytes).map_err(|_| unavailable("invalid transfer inspection data"))
}

/// Encode already-authorized, revision-pinned rows. The adapter receives no
/// identity, session, database credentials, or permission to publish artifacts.
pub(crate) async fn build_transfer_artifact(
    format: &str,
    layer_name: &str,
    srid: i32,
    geometry_family: &str,
    mappings: &[Value],
    features: Vec<Value>,
) -> Result<Vec<u8>, WorkerError> {
    let scratch = create_scratch().await?;
    let out_path = scratch.0.join("transfer.bin");
    let request = json!({
        "action": "export_transfer",
        "format": format,
        "layer_name": layer_name,
        "srid": srid,
        "geometry_family": geometry_family,
        "mappings": mappings,
        "features": features,
        "out_path": out_path.to_string_lossy(),
    });
    let report = run_driver(&scratch, &request).await?;
    let metadata = tokio::fs::metadata(&out_path)
        .await
        .map_err(|_| unavailable("missing binary export artifact"))?;
    if metadata.len() == 0
        || metadata.len() > MAX_PACKAGE_BYTES as u64
        || report.get("byte_size").and_then(Value::as_u64) != Some(metadata.len())
    {
        return Err(unavailable("invalid binary export artifact size"));
    }
    tokio::fs::read(&out_path)
        .await
        .map_err(|_| unavailable("unable to read binary export artifact"))
}

pub(crate) struct ValidatedOpsHashes {
    pub(crate) scope_hash: String,
    pub(crate) payload_hash: String,
}

pub(crate) async fn validate_ops(
    snapshot: &Value,
    operations: &[Value],
    base_rev: &str,
    scope_tuple: [&str; 5],
) -> Result<ValidatedOpsHashes, WorkerError> {
    let scratch = create_scratch().await?;
    let req = json!({
        "action": "validate_ops",
        "snapshot": snapshot,
        "operations": operations,
        "base_rev": base_rev,
        "scope_tuple": scope_tuple,
    });
    let res = run_driver(&scratch, &req).await?;
    let scope_hash = res
        .get("scope_hash")
        .and_then(Value::as_str)
        .filter(|s| s.len() == 48)
        .ok_or_else(|| unavailable("missing scope hash"))?
        .to_owned();
    let payload_hash = res
        .get("payload_hash")
        .and_then(Value::as_str)
        .filter(|s| s.len() == 64)
        .ok_or_else(|| unavailable("missing payload hash"))?
        .to_owned();
    Ok(ValidatedOpsHashes {
        scope_hash,
        payload_hash,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_sources_are_python_311_syntax_compatible() {
        for (_, source) in MODULES {
            let compatible = worker_python_311_source(source);
            assert!(!compatible.lines().any(|line| line.starts_with("type ")));
            if source.contains("type CoordinateTree =") {
                assert!(compatible
                    .lines()
                    .any(|line| line == "CoordinateTree = tuple"));
            }
        }
    }

    #[tokio::test]
    async fn build_and_validate_offline_package_round_trip() {
        let org_id = Uuid::new_v4().to_string();
        let proj_id = Uuid::new_v4().to_string();
        let layer_id = Uuid::new_v4().to_string();
        let source_id = Uuid::new_v4().to_string();
        let pkg_id = Uuid::new_v4().to_string();
        let actor_id = Uuid::new_v4().to_string();
        let schema_rev = "a".repeat(64);
        let content_rev = "b".repeat(64);
        let snapshot = json!({
            "version": 1,
            "package_id": pkg_id,
            "actor_id": actor_id,
            "organization_id": org_id,
            "project_id": proj_id,
            "layer_id": layer_id,
            "layer_name": "Points",
            "source_id": source_id,
            "content_revision": content_rev,
            "schema_revision": schema_rev,
            "crs_code": 4326,
            "geometry_family": "Point",
            "aoi_bbox": {"min_x": 106.0, "min_y": 10.0, "max_x": 107.0, "max_y": 11.0},
            "created_at": "2026-11-27T00:00:00+00:00",
            "expires_at": "2026-11-28T00:00:00+00:00",
            "fields": [{"field_id": "name", "name": "Name", "type": "text", "nullable": false}],
            "features": [{
                "feature_id": "pt_1",
                "properties": {"name": "Alpha"},
                "geometry": {"type": "Point", "coordinates": [106.5, 10.5]},
            }],
        });
        let mappings = vec![json!({
            "field_id": "name",
            "name": "Name",
            "output": "name",
            "type": "text",
        })];
        let built = build_package(4326, "Point", &mappings, &snapshot)
            .await
            .expect("build_package succeeds");
        assert!(built.gpkg_bytes.starts_with(b"SQLite format 3\0"));
        assert_eq!(built.snapshot_sha256.len(), 64);

        let ops = vec![json!({
            "op_type": "create",
            "feature_id": "local_1",
            "client_operation_id": Uuid::new_v4().to_string(),
            "geometry": {"type": "Point", "coordinates": [106.6, 10.6]},
            "properties": {"name": "Beta"},
        })];
        let hashes = validate_ops(
            &snapshot,
            &ops,
            &content_rev,
            [&org_id, &proj_id, &actor_id, &pkg_id, "op-1"],
        )
        .await
        .expect("validate_ops succeeds");
        assert_eq!(hashes.scope_hash.len(), 48);
        assert_eq!(hashes.payload_hash.len(), 64);

        let oob = vec![json!({
            "op_type": "create",
            "feature_id": "local_2",
            "geometry": {"type": "Point", "coordinates": [108.0, 10.6]},
            "properties": {"name": "Outside"},
        })];
        let err = validate_ops(
            &snapshot,
            &oob,
            &content_rev,
            [&org_id, &proj_id, &actor_id, &pkg_id, "op-2"],
        )
        .await
        .err()
        .expect("out-of-AOI point rejected");
        assert_eq!(err.code, "OUTSIDE_PACKAGE_SCOPE");
    }
}
