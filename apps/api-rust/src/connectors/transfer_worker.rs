//! Bounded Python worker for E12 layer export serialization and GeoPackage sync diffing.

use axum::http::StatusCode;
use serde_json::Value;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::sync::Semaphore;
use uuid::Uuid;

const MODULES: &[(&str, &str)] = &[
    (
        "openvietgis_api/domains/platform_errors.py",
        include_str!("../../../api/openvietgis_api/domains/platform_errors.py"),
    ),
    (
        "openvietgis_api/domains/tenancy/domain.py",
        include_str!("../../../api/openvietgis_api/domains/tenancy/domain.py"),
    ),
    (
        "openvietgis_api/domains/authorization/context.py",
        include_str!("../../../api/openvietgis_api/domains/authorization/context.py"),
    ),
    (
        "openvietgis_api/domains/spatial/crs.py",
        include_str!("../../../api/openvietgis_api/domains/spatial/crs.py"),
    ),
    (
        "openvietgis_api/domains/spatial/domain.py",
        include_str!("../../../api/openvietgis_api/domains/spatial/domain.py"),
    ),
    (
        "openvietgis_api/domains/editing/domain.py",
        include_str!("../../../api/openvietgis_api/domains/editing/domain.py"),
    ),
    (
        "openvietgis_api/domains/transfers/domain.py",
        include_str!("../../../api/openvietgis_api/domains/transfers/domain.py"),
    ),
    (
        "openvietgis_api/domains/transfers/ports.py",
        include_str!("../../../api/openvietgis_api/domains/transfers/ports.py"),
    ),
    (
        "openvietgis_api/domains/integration_hub/sync_diff.py",
        include_str!("../../../api/openvietgis_api/domains/integration_hub/sync_diff.py"),
    ),
    (
        "openvietgis_api/adapters/formats/wkt_crs.py",
        include_str!("../../../api/openvietgis_api/adapters/formats/wkt_crs.py"),
    ),
    (
        "openvietgis_api/adapters/formats/geojson.py",
        include_str!("../../../api/openvietgis_api/adapters/formats/geojson.py"),
    ),
    (
        "openvietgis_api/adapters/formats/geopackage.py",
        include_str!("../../../api/openvietgis_api/adapters/formats/geopackage.py"),
    ),
    (
        "openvietgis_api/adapters/formats/shapefile_zip.py",
        include_str!("../../../api/openvietgis_api/adapters/formats/shapefile_zip.py"),
    ),
    (
        "openvietgis_api/adapters/formats/postgis_sql.py",
        include_str!("../../../api/openvietgis_api/adapters/formats/postgis_sql.py"),
    ),
];

const PACKAGE_INITS: &[&str] = &[
    "openvietgis_api/__init__.py",
    "openvietgis_api/domains/__init__.py",
    "openvietgis_api/domains/tenancy/__init__.py",
    "openvietgis_api/domains/authorization/__init__.py",
    "openvietgis_api/domains/spatial/__init__.py",
    "openvietgis_api/domains/editing/__init__.py",
    "openvietgis_api/domains/transfers/__init__.py",
    "openvietgis_api/domains/integration_hub/__init__.py",
    "openvietgis_api/adapters/__init__.py",
    "openvietgis_api/adapters/formats/__init__.py",
];

const DRIVER: &str = r####"import hashlib
import json
import math
import os
import resource
import sqlite3
import sys
import tempfile
from decimal import Decimal, InvalidOperation
from pathlib import Path

resource.setrlimit(resource.RLIMIT_AS, (512 * 1024 * 1024, 512 * 1024 * 1024))
resource.setrlimit(resource.RLIMIT_CPU, (20, 24))

from openvietgis_api.adapters.formats.geojson import write_geojson
from openvietgis_api.adapters.formats.geopackage import (
    inspect_geopackage,
    parse_geopackage_member,
    write_geopackage,
)
from openvietgis_api.adapters.formats.postgis_sql import write_postgis_sql
from openvietgis_api.domains.integration_hub.sync_diff import (
    ServerFeatureSnapshot,
    UploadedFeatureSnapshot,
    compute_sync_diff,
)
from openvietgis_api.domains.platform_errors import PlatformValidationError
from openvietgis_api.domains.spatial.domain import FieldType, GeometryDimensions, NativeGeometry
from openvietgis_api.domains.transfers.domain import ExportFieldMapping, normalize_encoding
from openvietgis_api.domains.transfers.ports import TransferFormatError, TransferLimitExceededError


def _to_tuple_coords(coords):
    if isinstance(coords, list):
        return tuple(_to_tuple_coords(x) for x in coords)
    if isinstance(coords, tuple):
        return tuple(_to_tuple_coords(x) for x in coords)
    return float(coords)


def _to_list_coords(coords):
    if isinstance(coords, (tuple, list)):
        return [_to_list_coords(x) for x in coords]
    return float(coords)


def _norm_decimal(val):
    if val is None or isinstance(val, bool):
        return val
    try:
        dec = Decimal(str(val))
    except InvalidOperation:
        return val
    if not dec.is_finite():
        return val
    if dec.is_zero():
        return "0"
    rendered = format(dec, "f")
    if "." in rendered:
        rendered = rendered.rstrip("0").rstrip(".")
    return rendered or "0"


def _write_r2010_dxf(features, layer_name: str) -> bytes:
    max_coord = 1e9
    safe_layer = "".join(c if c.isalnum() or c in "_-" else "_" for c in layer_name)[:48] or "GIS"
    if not features or len(features) > 50_000:
        raise PlatformValidationError("DXF export requires 1–50000 features", code="CAD_FEATURE_LIMIT")
    entities = []
    handle = 0x100
    total_positions = 0

    def next_handle():
        nonlocal handle
        h = format(handle, "X")
        handle += 1
        return h

    def check_pt(p):
        x, y = float(p[0]), float(p[1])
        if not (math.isfinite(x) and math.isfinite(y) and abs(x) <= max_coord and abs(y) <= max_coord):
            raise PlatformValidationError("invalid or unbounded CAD coordinate", code="CAD_COORDINATE_LIMIT")
        return x, y

    for feat in features:
        geom = feat.get("geometry") or {}
        gtype = geom.get("type")
        coords = geom.get("coordinates")
        if gtype == "Point":
            x, y = check_pt(coords)
            total_positions += 1
            if total_positions > 200_000:
                raise PlatformValidationError("DXF export position limit exceeded", code="CAD_CURVE_LIMIT")
            entities.extend(["  0", "POINT", "  5", next_handle(), "  8", safe_layer, " 10", repr(x), " 20", repr(y), " 30", "0.0"])
        elif gtype in ("LineString", "Polygon"):
            if gtype == "Polygon":
                if not isinstance(coords, list) or len(coords) != 1:
                    raise PlatformValidationError("DXF simple polylines cannot preserve polygon holes", code="CAD_GEOMETRY_UNSUPPORTED")
                pts_raw = coords[0]
                closed_flag = "1"
            else:
                pts_raw = coords
                closed_flag = "0"
            if len(pts_raw) > 4096:
                raise PlatformValidationError("DXF export position limit exceeded", code="CAD_CURVE_LIMIT")
            pts = [check_pt(p) for p in pts_raw]
            total_positions += len(pts)
            if total_positions > 200_000:
                raise PlatformValidationError("DXF export position limit exceeded", code="CAD_CURVE_LIMIT")
            entities.extend([
                "  0", "LWPOLYLINE", "  5", next_handle(), "  8", safe_layer,
                " 90", str(len(pts)), " 70", closed_flag, " 43", "0.0",
            ])
            for x, y in pts:
                entities.extend([" 10", repr(x), " 20", repr(y)])
        else:
            raise PlatformValidationError(f"unsupported DXF geometry: {gtype}", code="CAD_GEOMETRY_UNSUPPORTED")

    lines = [
        "  0", "SECTION", "  2", "HEADER",
        "  9", "$ACADVER", "  1", "AC1024",
        "  9", "$INSUNITS", " 70", "6",
        "  0", "ENDSEC",
        "  0", "SECTION", "  2", "TABLES",
        "  0", "TABLE", "  2", "LAYER", " 70", "2",
        "  0", "LAYER", "  2", "0", " 70", "0", " 62", "7", "  6", "CONTINUOUS",
        "  0", "LAYER", "  2", safe_layer, " 70", "0", " 62", "7", "  6", "CONTINUOUS",
        "  0", "ENDTAB",
        "  0", "ENDSEC",
        "  0", "SECTION", "  2", "ENTITIES",
        *entities,
        "  0", "ENDSEC",
        "  0", "EOF",
        "",
    ]
    return "\n".join(lines).encode("utf-8")


try:
    req = json.loads(Path(sys.argv[1]).read_text("utf-8"))
    action = req["action"]
    if action == "export":
        fmt = req["format"]
        crs_code = int(req["crs_code"])
        geometry_family = req["geometry_family"]
        layer_name = req["layer_name"]
        mappings = [
            ExportFieldMapping(
                field_id=m["field_id"],
                source_name=m["source_name"],
                output_name=m["output_name"],
                type=FieldType(m["field_type"]),
            )
            for m in req["field_mappings"]
        ]
        raw_features = req["features"]
        if fmt == "dxf":
            payload = _write_r2010_dxf(raw_features, layer_name)
            ext = "dxf"
            media_type = "application/dxf"
        else:
            features = [
                (
                    str(f["feature_id"]),
                    dict(f["attributes"]),
                    NativeGeometry(geometry_family, _to_tuple_coords(f["geometry"]["coordinates"]), crs_code),
                )
                for f in raw_features
            ]
            if fmt == "geopackage":
                payload, ext, media_type = write_geopackage(
                    layer_name=layer_name,
                    crs_code=crs_code,
                    geometry_family=geometry_family,
                    field_mappings=mappings,
                    features=features,
                )
            elif fmt == "shapefile_zip":
                from openvietgis_api.adapters.formats.shapefile_zip import write_shapefile_zip
                payload, ext, media_type = write_shapefile_zip(
                    layer_name=layer_name, crs_code=crs_code, geometry_family=geometry_family,
                    field_mappings=mappings, features=features,
                )
            elif fmt == "geojson":
                payload, ext, media_type = write_geojson(
                    field_mappings=mappings,
                    features=features,
                )
            else:
                payload, ext, media_type = write_postgis_sql(
                    layer_name=layer_name,
                    crs_code=crs_code,
                    geometry_family=geometry_family,
                    field_mappings=mappings,
                    features=features,
                )
        if req.get("bounded_transfer") and len(payload)>64*1024*1024:
            raise TransferLimitExceededError("ARTIFACT_SIZE_EXCEEDED","Binary export exceeds 64 MiB.")
        out_path = Path(req["output_path"])
        out_path.write_bytes(payload)
        print(json.dumps({
            "ok": True,
            "ext": ext,
            "media_type": media_type,
            "sha256": hashlib.sha256(payload).hexdigest(),
        }))
    elif action == "inspect_import":
        raw = Path(req["upload_path"]).read_bytes()
        fmt = req["format"]
        encoding = req.get("encoding")
        if encoding is not None:
            encoding = normalize_encoding(encoding)
            if encoding is None: raise ValueError("Unsupported encoding")
        if fmt == "geopackage":
            members = inspect_geopackage(raw)
        elif fmt == "shapefile_zip":
            from openvietgis_api.adapters.formats.shapefile_zip import inspect_shapefile_zip, parse_shapefile_member
            members = inspect_shapefile_zip(raw, encoding=encoding)
        else:
            raise ValueError("Unsupported import format")
        if len(members) > 64 or any(len(m.name) > 200 for m in members):
            raise ValueError("Too many or oversized dataset members")
        eligible = [m for m in members if m.eligible]
        chosen = req.get("member")
        if chosen is None and len(eligible) == 1:
            chosen = eligible[0].name
        member = next((m for m in members if m.name == chosen), None)
        parsed = None
        if member and member.eligible:
            # Never infer an unknown native CRS from coordinates or a user override.
            if member.detected_crs is not None:
                if fmt == "geopackage":
                    parsed = parse_geopackage_member(raw, member_name=member.name)
                else:
                    parsed = parse_shapefile_member(raw, member_name=member.name, declared_encoding=encoding)
        result = {"members":[m.to_wire() for m in members], "selected_member":chosen}
        if parsed is not None:
            result["parsed"] = {
                "family":parsed.geometry_family.value, "dimensions":parsed.dimensions.value,
                "crs":parsed.detected_crs, "encoding":parsed.detected_encoding,
                "requires_encoding":parsed.requires_encoding_resolution,
                "fields":[f.to_wire() for f in parsed.fields],
                "findings":[f.to_wire() for f in parsed.findings],
                "records":[{"id":r.feature_id,"attrs":r.attributes,"geometry":{"type":r.geometry.type,"coordinates":r.geometry.coordinates}} for r in parsed.features],
                "extent":parsed.extent,
            }
        # Output is bounded on disk rather than an unbounded subprocess stdout pipe.
        resource.setrlimit(resource.RLIMIT_FSIZE, (32*1024*1024,32*1024*1024))
        with open(req["output_path"],"w",encoding="utf-8") as out:
            json.dump(result,out,ensure_ascii=False,allow_nan=False,separators=(",",":"))
        print(json.dumps({"ok":True}))
    elif action == "sync_diff":
        raw = Path(req["upload_path"]).read_bytes()
        expected_crs_code = int(req["expected_crs_code"])
        expected_geometry_family = req["expected_geometry_family"]
        field_types = {k: FieldType(v) for k, v in req["field_types"].items()}
        members = inspect_geopackage(raw)
        vector_members = [m for m in members if m.geometry_family != "unknown"]
        if not vector_members:
            raise PlatformValidationError("Uploaded GeoPackage contains no vector feature layers.")
        if len(vector_members) > 1:
            raise PlatformValidationError("Uploaded GeoPackage must contain exactly one vector layer for sync.")
        member = vector_members[0]
        if not member.eligible:
            raise PlatformValidationError(member.reason or "Uploaded GeoPackage layer is not eligible for sync.")
        if member.detected_crs is None:
            raise PlatformValidationError("Uploaded GeoPackage is missing an explicit supported EPSG CRS.")
        if member.detected_crs != expected_crs_code:
            raise PlatformValidationError(
                f"Uploaded GeoPackage CRS EPSG:{member.detected_crs} does not match layer CRS EPSG:{expected_crs_code}."
            )
        if raw.startswith(b"SQLite format 3\x00"):
            quoted = '"' + member.name.replace('"', '""') + '"'
            conn = sqlite3.connect(f"file:{req['upload_path']}?mode=ro", uri=True)
            try:
                cols = {str(r[1]) for r in conn.execute(f"PRAGMA table_info({quoted})").fetchall()}
                if "ovg_feature_id" in cols:
                    dup = conn.execute(
                        f"SELECT ovg_feature_id FROM {quoted} WHERE ovg_feature_id IS NOT NULL "
                        "GROUP BY ovg_feature_id HAVING COUNT(*) > 1 LIMIT 1"
                    ).fetchone()
                    if dup is not None:
                        raise PlatformValidationError(f"Duplicate ovg_feature_id in uploaded GeoPackage: {dup[0]}")
            finally:
                conn.close()
        parsed = parse_geopackage_member(raw, member_name=member.name, declared_crs=expected_crs_code)
        errors = [f for f in parsed.findings if f.severity == "error"]
        if errors:
            first = errors[0]
            raise PlatformValidationError(f"Uploaded GeoPackage validation failed ({first.code}): {first.message}")
        if parsed.detected_crs != expected_crs_code:
            raise PlatformValidationError(f"Uploaded GeoPackage CRS does not match layer CRS EPSG:{expected_crs_code}.")
        if parsed.dimensions != GeometryDimensions.XY:
            raise PlatformValidationError("Only 2D XY GeoPackage layers are supported for sync.")
        if parsed.geometry_family.value != expected_geometry_family:
            raise PlatformValidationError(
                f"Uploaded GeoPackage geometry family '{parsed.geometry_family.value}' "
                f"does not match layer geometry family '{expected_geometry_family}'."
            )
        uploaded = []
        for rec in parsed.features:
            attrs = {}
            for k, v in rec.attributes.items():
                attrs[k] = _norm_decimal(v) if field_types.get(k) is FieldType.DECIMAL else v
            uploaded.append(
                UploadedFeatureSnapshot(
                    feature_id=rec.feature_id,
                    attributes=attrs,
                    geometry_geojson={
                        "type": rec.geometry.type,
                        "coordinates": _to_list_coords(rec.geometry.coordinates),
                    },
                )
            )
        server = [
            ServerFeatureSnapshot(
                feature_id=s["feature_id"],
                attributes=s["attributes"],
                geometry_geojson=s["geometry_geojson"],
            )
            for s in req["server_features"]
        ]
        plan = compute_sync_diff(
            server_features=server,
            uploaded_features=uploaded,
            allowed_field_ids=frozenset(field_types.keys()),
            base_content_revision=req["base_content_revision"],
            base_schema_revision=req["base_schema_revision"],
            allow_deletes=bool(req["allow_deletes"]),
        )
        print(json.dumps({
            "ok": True,
            "create_count": plan.create_count,
            "update_count": plan.update_count,
            "delete_count": plan.delete_count,
            "unchanged_count": plan.unchanged_count,
            "omitted_server_feature_ids": list(plan.omitted_server_feature_ids),
            "diff_sha256": plan.diff_sha256,
            "operations": [
                {
                    "op_type": op.op_type,
                    "feature_id": op.feature_id,
                    "geometry": op.geometry,
                    "properties": op.properties,
                    "changed_fields": list(op.changed_fields),
                    "geometry_changed": op.geometry_changed,
                }
                for op in plan.operations
            ],
        }))
    else:
        raise ValueError(f"unsupported action: {action}")
except TransferLimitExceededError as exc:
    print(json.dumps({"ok":False,"status":413,"code":exc.code,"message":exc.message}))
    sys.exit(2)
except TransferFormatError as exc:
    print(json.dumps({"ok": False, "status": 422, "code": exc.code, "message": exc.message}))
    sys.exit(2)
except PlatformValidationError as exc:
    print(json.dumps({"ok": False, "status": 422, "message": exc.message}))
    sys.exit(2)
except ValueError as exc:
    print(json.dumps({"ok": False, "status": 422, "message": str(exc)}))
    sys.exit(2)
except Exception as exc:
    print(json.dumps({"ok": False, "status": 500, "message": str(exc)}))
    sys.exit(1)
"####;

const WORKERS: usize = 4;
const TIMEOUT: Duration = Duration::from_secs(20);
static SEM: Semaphore = Semaphore::const_new(WORKERS);

#[derive(Debug)]
pub(crate) struct WorkerError {
    pub(crate) status: StatusCode,
    pub(crate) message: String,
}

pub(crate) struct ExportArtifact {
    pub(crate) bytes: Vec<u8>,
    pub(crate) ext: String,
    pub(crate) media_type: String,
    pub(crate) sha256: String,
}

struct WorkDir(std::path::PathBuf);
impl Drop for WorkDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub(crate) async fn run_export(mut req: Value) -> Result<ExportArtifact, WorkerError> {
    let work = WorkDir(std::env::temp_dir().join(format!("ovg-e12-exp-{}", Uuid::new_v4())));
    let out_path = work.0.join("export.out");
    req["output_path"] = Value::String(out_path.to_string_lossy().into_owned());
    let meta = invoke(&work.0, req).await?;
    let bytes = tokio::fs::read(&out_path)
        .await
        .map_err(|err| WorkerError {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: format!("read export artifact failed: {err}"),
        })?;
    Ok(ExportArtifact {
        bytes,
        ext: meta["ext"].as_str().unwrap_or("bin").to_owned(),
        media_type: meta["media_type"]
            .as_str()
            .unwrap_or("application/octet-stream")
            .to_owned(),
        sha256: meta["sha256"].as_str().unwrap_or_default().to_owned(),
    })
}

pub(crate) async fn run_sync_diff(
    upload_bytes: &[u8],
    mut req: Value,
) -> Result<Value, WorkerError> {
    let work = WorkDir(std::env::temp_dir().join(format!("ovg-e12-diff-{}", Uuid::new_v4())));
    std::fs::create_dir_all(&work.0).map_err(|err| WorkerError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        message: format!("create worker dir failed: {err}"),
    })?;
    let upload_path = work.0.join("upload.gpkg");
    tokio::fs::write(&upload_path, upload_bytes)
        .await
        .map_err(|err| WorkerError {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: format!("write upload artifact failed: {err}"),
        })?;
    req["upload_path"] = Value::String(upload_path.to_string_lossy().into_owned());
    invoke(&work.0, req).await
}

async fn invoke(work_dir: &Path, request: Value) -> Result<Value, WorkerError> {
    let _permit = SEM.acquire().await.map_err(|_| WorkerError {
        status: StatusCode::SERVICE_UNAVAILABLE,
        message: "worker semaphore closed".to_owned(),
    })?;

    tokio::fs::create_dir_all(work_dir)
        .await
        .map_err(|err| WorkerError {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: format!("create worker root: {err}"),
        })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(work_dir, std::fs::Permissions::from_mode(0o700))
            .await
            .map_err(|_| WorkerError {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                message: "secure worker directory failed".into(),
            })?;
    }
    for init in PACKAGE_INITS {
        let p = work_dir.join(init);
        if let Some(parent) = p.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|err| WorkerError {
                    status: StatusCode::INTERNAL_SERVER_ERROR,
                    message: format!("create pkg dir: {err}"),
                })?;
        }
        tokio::fs::write(&p, b"").await.map_err(|err| WorkerError {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: format!("write pkg init: {err}"),
        })?;
    }
    for (rel, body) in MODULES {
        let p = work_dir.join(rel);
        if let Some(parent) = p.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|err| WorkerError {
                    status: StatusCode::INTERNAL_SERVER_ERROR,
                    message: format!("create mod dir: {err}"),
                })?;
        }
        tokio::fs::write(&p, body)
            .await
            .map_err(|err| WorkerError {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                message: format!("write module: {err}"),
            })?;
    }

    let req_path = work_dir.join("req.json");
    let drv_path = work_dir.join("driver.py");
    let req_bytes = serde_json::to_vec(&request).map_err(|err| WorkerError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        message: format!("serialize worker request: {err}"),
    })?;
    tokio::fs::write(&req_path, req_bytes)
        .await
        .map_err(|err| WorkerError {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: format!("write req.json: {err}"),
        })?;
    tokio::fs::write(&drv_path, DRIVER)
        .await
        .map_err(|err| WorkerError {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: format!("write driver.py: {err}"),
        })?;

    let python = if request["action"] == "inspect_import" || request["format"] == "shapefile_zip" {
        std::env::var("OPENVIETGIS_VECTOR_CODEC_PYTHON")
            .unwrap_or_else(|_| "/usr/bin/python3".into())
    } else {
        "/usr/bin/python3".into()
    };
    let mut cmd = tokio::process::Command::new(python);
    cmd.args(["-E", "-s", "-B"])
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
        .map_err(|_| WorkerError {
            status: StatusCode::SERVICE_UNAVAILABLE,
            message: "transfer worker timed out".to_owned(),
        })?
        .map_err(|err| WorkerError {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: format!("spawn transfer worker: {err}"),
        })?;

    let stdout = String::from_utf8_lossy(&out.stdout);
    if let Some(last_line) = stdout.lines().rfind(|l| !l.trim().is_empty()) {
        if let Ok(val) = serde_json::from_str::<Value>(last_line) {
            if val.get("ok").and_then(Value::as_bool) == Some(true) && out.status.success() {
                return Ok(val);
            }
            let status = match val.get("status").and_then(Value::as_u64) {
                Some(422) => StatusCode::UNPROCESSABLE_ENTITY,
                Some(413) => StatusCode::PAYLOAD_TOO_LARGE,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            let msg = val
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("transfer worker failed")
                .to_owned();
            return Err(WorkerError {
                status,
                message: msg,
            });
        }
    }

    let stderr = String::from_utf8_lossy(&out.stderr);
    Err(WorkerError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        message: format!("transfer worker error: {stderr}"),
    })
}
