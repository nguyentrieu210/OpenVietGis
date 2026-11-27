//! E10 Rust-native CAD/3D control plane, document vault, links, conversions, and COGO precision math.

use axum::body::{to_bytes, Body};
use axum::extract::rejection::QueryRejection;
use axum::extract::{Path, Query, Request, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use ovg_core::AppError;
use ovg_storage::cad_documents::{
    self, find_existing_georef_conversion, insert_dxf_export_conversion,
    insert_rejected_dwg_conversion, load_cad_document_version_blob, load_dxf_export_source,
    load_georef_draft_context, load_native_import_context, publish_cad_import,
    validate_geojson_ops_postgis, validate_wkts_postgis, CadDocError, LinkedFilter,
    PublishCadImportArgs, SaveCadVersionArgs,
};
use ovg_storage::catalog::ProjectReadPolicy;
use ovg_storage::editing::{EditError, Guard, Permission};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::io::AsyncReadExt;
use uuid::Uuid;

use crate::cad_worker::{
    run_export_georef_draft, run_parse_dxf, run_preview_native_import, run_write_dxf,
};
use crate::catalog::{project_read, CatalogError};
use crate::edit_auth::{
    self, authenticate_actor, authenticate_json, require_csrf as actor_csrf, Failure,
};
use crate::identity::{require_csrf, HumanSession};
use crate::tilesets::sha256_hex;
use crate::transfers::store::{multipart_parts, Store};
use crate::AppState;

const V8I_BASE: &str = "/organizations/{organization_id}/projects/{project_id}/integrations/v8i";
const CAD_DOCS: &str = "/organizations/{organization_id}/projects/{project_id}/cad/documents";
const CAD_BASE: &str = "/organizations/{organization_id}/projects/{project_id}/cad";
const MAX_CAD_BYTES: usize = 50 * 1024 * 1024;
const COGO_LIMIT: f64 = 1e9;

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route(
            &format!("{V8I_BASE}/capabilities"),
            get(read_v8i_capabilities),
        )
        .route(CAD_DOCS, get(list_documents).post(create_document))
        .route(&format!("{CAD_DOCS}/links"), get(feature_cad_links))
        .route(
            &format!("{CAD_DOCS}/{{document_id}}/versions"),
            get(list_document_versions).post(append_document_version),
        )
        .route(
            &format!("{CAD_DOCS}/{{document_id}}/versions/{{version}}/download"),
            get(download_document_version),
        )
        .route(
            &format!("{CAD_DOCS}/{{document_id}}/versions/{{version}}/links"),
            get(cad_feature_links),
        )
        .route(&format!("{CAD_BASE}/conversions"), get(list_conversions))
        .route(&format!("{CAD_BASE}/inspect"), post(inspect_cad_file))
        .route(&format!("{CAD_BASE}/import"), post(import_cad_file))
        .route(&format!("{CAD_BASE}/export"), post(export_cad_dxf))
        .route(
            &format!("{CAD_DOCS}/{{document_id}}/versions/{{version}}/inspect-georef"),
            post(inspect_georef),
        )
        .route(
            &format!("{CAD_DOCS}/{{document_id}}/versions/{{version}}/import-georef"),
            post(import_georef),
        )
        .route(
            &format!("{CAD_DOCS}/{{document_id}}/versions/{{version}}/export-georef-draft"),
            post(export_georef_draft),
        )
        .route(
            &format!("{CAD_DOCS}/{{document_id}}/versions/{{version}}/preview-native-import"),
            post(preview_native_import),
        )
        .route(
            &format!("{CAD_BASE}/precision/traverse"),
            post(cogo_traverse),
        )
        .route(
            &format!("{CAD_BASE}/precision/intersection"),
            post(cogo_intersection),
        )
        .route(&format!("{CAD_BASE}/precision/offset"), post(cogo_offset))
        .route(&format!("{CAD_BASE}/precision/snap"), post(cogo_snap))
}

fn capability_status() -> Value {
    json!({
        "connector": "microstation-v8i",
        "contract": "ovg.v8i.snapshot.v1",
        "status": "experimental",
        "connection_status": "not_paired",
        "windows_bridge_available": true,
        "com_probe_verified_on_windows": false,
        "geometry_scan_verified_on_windows": false,
        "cadastral_semantics_verified": false,
        "coordinate_units_verified": false,
        "capabilities": {
            "local_com_probe": "experimental",
            "local_snapshot_validation": "verified_fixture_only",
            "local_scan": "experimental_unverified",
            "remote_preview": "unavailable",
            "sync_to_openvietgis": "unavailable",
            "write_back_to_dgn": "unavailable"
        },
        "requires": [
            "licensed_running_microstation_v8i",
            "windows_pywin32",
            "explicit_crs",
            "cad_unit_control_point_verification",
            "authorized_device_pairing_before_remote_sync"
        ],
        "read_only": true,
        "live_connection": false
    })
}

fn validate_cad_upload(filename: &str, payload: &[u8]) -> Result<&'static str, CadReadError> {
    if filename.is_empty()
        || filename.chars().count() > 180
        || filename
            .bytes()
            .any(|b| b == b'/' || b == b'\\' || b < 0x20 || b == 0x7f)
    {
        return Err(CadReadError::Detail(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Invalid file name",
        ));
    }
    let Some((_, ext)) = filename.rsplit_once('.') else {
        return Err(CadReadError::Detail(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Only DWG/DXF supported",
        ));
    };
    let fmt = if ext.eq_ignore_ascii_case("dxf") {
        "dxf"
    } else if ext.eq_ignore_ascii_case("dwg") {
        "dwg"
    } else {
        return Err(CadReadError::Detail(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Only DWG/DXF supported",
        ));
    };
    if payload.is_empty() || payload.len() > MAX_CAD_BYTES {
        return Err(CadReadError::Detail(
            StatusCode::PAYLOAD_TOO_LARGE,
            "CAD file size limit",
        ));
    }
    if fmt == "dwg"
        && (payload.len() < 6
            || &payload[..4] != b"AC10"
            || !payload[4].is_ascii_digit()
            || !payload[5].is_ascii_digit())
    {
        return Err(CadReadError::Detail(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Invalid DWG header",
        ));
    }
    if fmt == "dxf" && !is_valid_dxf_header(payload) {
        return Err(CadReadError::Detail(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Invalid DXF header",
        ));
    }
    Ok(fmt)
}

fn is_valid_dxf_header(payload: &[u8]) -> bool {
    if payload.starts_with(b"AutoCAD Binary DXF") {
        return true;
    }
    let prefix = &payload[..payload.len().min(4096)];
    let lines: Vec<&[u8]> = prefix.split(|b| *b == b'\n').collect();
    for pair in lines.windows(2) {
        let first = pair[0].trim_ascii();
        let second = pair[1].trim_ascii();
        if first == b"0" && second == b"SECTION" {
            return true;
        }
    }
    false
}

#[allow(clippy::too_many_arguments)]
async fn save_cad_version(
    state: AppState,
    auth: HumanSession,
    request_headers: HeaderMap,
    o: Uuid,
    p: Uuid,
    doc_id: Uuid,
    require_expected: bool,
    body: Body,
) -> Result<(StatusCode, Json<Value>), CadReadError> {
    require_csrf(&state, &auth, &request_headers)
        .map_err(|e| CadReadError::Access(CatalogError::Identity(e)))?;
    let mut edit = project_read(&state, &auth, o, p, ProjectReadPolicy::UnrestrictedEdit)
        .await
        .map_err(CadReadError::Access)?;

    let bytes = to_bytes(body, MAX_CAD_BYTES + 256 * 1024)
        .await
        .map_err(|_| CadReadError::Detail(StatusCode::PAYLOAD_TOO_LARGE, "CAD file size limit"))?;
    let content_type = request_headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let parts = multipart_parts(content_type, &bytes).ok_or(CadReadError::Detail(
        StatusCode::UNPROCESSABLE_ENTITY,
        "Invalid file name",
    ))?;

    let expected = if require_expected {
        parts
            .iter()
            .find_map(|(k, fn_opt, d)| {
                (k == "expected_version" && fn_opt.is_none())
                    .then(|| std::str::from_utf8(d).ok())
                    .flatten()
            })
            .and_then(|s| s.trim().parse::<i32>().ok())
            .filter(|v| *v >= 1)
            .ok_or(CadReadError::Detail(
                StatusCode::UNPROCESSABLE_ENTITY,
                "Invalid expected_version",
            ))?
    } else {
        0
    };

    let (filename, payload) = parts
        .iter()
        .find_map(|(k, fn_opt, d)| {
            (k == "file" && fn_opt.is_some()).then_some((fn_opt.unwrap_or(""), *d))
        })
        .ok_or(CadReadError::Detail(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Invalid file name",
        ))?;

    let fmt = validate_cad_upload(filename, payload)?;
    cad_documents::prepare_cad_save(&mut edit, doc_id, expected, payload.len(), fmt)
        .await
        .map_err(CadReadError::Doc)?;

    let key = format!("cad-doc/{o}/{p}/{doc_id}/{}.{fmt}", Uuid::new_v4());
    let store = Store::new(&state);
    let (digest, size) = store
        .put(&key, payload)
        .await
        .map_err(|e| CadReadError::Detail(StatusCode::INTERNAL_SERVER_ERROR, leak_safe_msg(e)))?;
    let expected_sha = sha256_hex(payload);
    if digest != expected_sha {
        store.delete(&key).await;
        return Err(CadReadError::Domain(AppError::internal(
            "CAD digest mismatch",
        )));
    }

    let created = match cad_documents::insert_cad_version(
        &mut edit,
        SaveCadVersionArgs {
            actor_id: auth.actor_id(),
            actor_display: auth.display_name(),
            doc_id,
            version: expected + 1,
            filename,
            format: fmt,
            storage_key: &key,
            sha256: &digest,
            byte_size: size,
        },
    )
    .await
    {
        Ok(val) => val,
        Err(err) => {
            store.delete(&key).await;
            return Err(CadReadError::Doc(err));
        }
    };

    if let Err(err) = edit.commit().await {
        store.delete(&key).await;
        return Err(CadReadError::Domain(err));
    }
    Ok((StatusCode::CREATED, Json(created)))
}

fn leak_safe_msg(_: ovg_storage::transfers::TransferError) -> &'static str {
    "CAD artifact storage unavailable"
}

async fn create_document(
    State(state): State<AppState>,
    auth: HumanSession,
    request_headers: HeaderMap,
    Path((o, p)): Path<(Uuid, Uuid)>,
    body: Body,
) -> Result<(StatusCode, Json<Value>), CadReadError> {
    save_cad_version(
        state,
        auth,
        request_headers,
        o,
        p,
        Uuid::new_v4(),
        false,
        body,
    )
    .await
}

async fn append_document_version(
    State(state): State<AppState>,
    auth: HumanSession,
    request_headers: HeaderMap,
    Path((o, p, doc_id)): Path<(Uuid, Uuid, Uuid)>,
    body: Body,
) -> Result<(StatusCode, Json<Value>), CadReadError> {
    save_cad_version(state, auth, request_headers, o, p, doc_id, true, body).await
}

async fn list_documents(
    State(state): State<AppState>,
    auth: HumanSession,
    Path((organization_id, project_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>, CatalogError> {
    let mut read = project_read(
        &state,
        &auth,
        organization_id,
        project_id,
        ProjectReadPolicy::Unrestricted,
    )
    .await?;
    let result = cad_documents::list_documents(&mut read)
        .await
        .map_err(CatalogError::Application)?;
    read.commit().await.map_err(CatalogError::Application)?;
    Ok(Json(result))
}

async fn list_document_versions(
    State(state): State<AppState>,
    auth: HumanSession,
    Path((organization_id, project_id, document_id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<Json<Value>, CadReadError> {
    let mut read = project_read(
        &state,
        &auth,
        organization_id,
        project_id,
        ProjectReadPolicy::Unrestricted,
    )
    .await
    .map_err(CadReadError::Access)?;
    let result = cad_documents::list_versions(&mut read, document_id)
        .await
        .map_err(CadReadError::Domain)?;
    read.commit().await.map_err(CadReadError::Domain)?;
    result.map(Json).ok_or(CadReadError::DocumentNotFound)
}

fn python_quote_filename(filename: &str) -> String {
    let mut out = String::with_capacity(filename.len());
    for byte in filename.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(*byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(*byte));
        } else {
            use std::fmt::Write;
            let _ = write!(&mut out, "%{byte:02X}");
        }
    }
    out
}

async fn download_document_version(
    State(state): State<AppState>,
    auth: HumanSession,
    Path((o, p, doc_id, version)): Path<(Uuid, Uuid, Uuid, i32)>,
) -> Result<Response, CadReadError> {
    let mut read = project_read(&state, &auth, o, p, ProjectReadPolicy::Unrestricted)
        .await
        .map_err(CadReadError::Access)?;
    let Some((filename, key, sha256)) =
        cad_documents::get_download_version(&mut read, doc_id, version)
            .await
            .map_err(CadReadError::Domain)?
    else {
        return Err(CadReadError::Detail(
            StatusCode::NOT_FOUND,
            "CAD version not found",
        ));
    };
    read.commit().await.map_err(CadReadError::Domain)?;

    let (mut file, _) = Store::new(&state)
        .open_verified(&key, &sha256)
        .await
        .map_err(|_| {
            CadReadError::Detail(StatusCode::CONFLICT, "CAD artifact integrity failure")
        })?;
    let mut payload = Vec::new();
    file.read_to_end(&mut payload)
        .await
        .map_err(|_| CadReadError::Domain(AppError::internal("CAD read failed")))?;

    let mut response = Response::new(Body::from(payload));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!(
            "attachment; filename*=UTF-8''{}",
            python_quote_filename(&filename)
        ))
        .unwrap_or_else(|_| HeaderValue::from_static("attachment")),
    );
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    if let Ok(hv) = HeaderValue::from_str(&sha256) {
        response.headers_mut().insert("x-content-sha256", hv);
    }
    Ok(response)
}

#[derive(Debug, Deserialize)]
struct FeatureLinksQuery {
    source_id: Uuid,
    feature_id: String,
}

#[derive(Debug, Deserialize)]
struct CadLinksQuery {
    handle: String,
}

async fn feature_cad_links(
    State(state): State<AppState>,
    auth: HumanSession,
    Path((o, p)): Path<(Uuid, Uuid)>,
    query: Result<Query<FeatureLinksQuery>, QueryRejection>,
) -> Result<Json<Value>, CadReadError> {
    let mut read = project_read(&state, &auth, o, p, ProjectReadPolicy::Unrestricted)
        .await
        .map_err(CadReadError::Access)?;
    let Query(q) = query.map_err(|_| {
        CadReadError::Detail(StatusCode::UNPROCESSABLE_ENTITY, "Invalid links query")
    })?;
    if q.feature_id.is_empty() || q.feature_id.chars().count() > 200 {
        return Err(CadReadError::Detail(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Invalid feature_id",
        ));
    }
    let org_id = read.organization_id();
    let proj_id = read.project_id();
    let items = cad_documents::linked_features(
        read.transaction(),
        org_id,
        proj_id,
        LinkedFilter::ByFeature {
            source_id: q.source_id,
            feature_id: &q.feature_id,
        },
    )
    .await
    .map_err(CadReadError::Domain)?;
    read.commit().await.map_err(CadReadError::Domain)?;
    Ok(Json(json!({ "items": items })))
}

async fn cad_feature_links(
    State(state): State<AppState>,
    auth: HumanSession,
    Path((o, p, doc_id, version)): Path<(Uuid, Uuid, Uuid, i32)>,
    query: Result<Query<CadLinksQuery>, QueryRejection>,
) -> Result<Json<Value>, CadReadError> {
    let mut read = project_read(&state, &auth, o, p, ProjectReadPolicy::Unrestricted)
        .await
        .map_err(CadReadError::Access)?;
    let Query(q) = query.map_err(|_| {
        CadReadError::Detail(StatusCode::UNPROCESSABLE_ENTITY, "Invalid handle query")
    })?;
    if q.handle.is_empty()
        || q.handle.len() > 32
        || !q.handle.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err(CadReadError::Detail(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Invalid handle",
        ));
    }
    let handle_upper = q.handle.to_ascii_uppercase();
    let org_id = read.organization_id();
    let proj_id = read.project_id();
    let items = cad_documents::linked_features(
        read.transaction(),
        org_id,
        proj_id,
        LinkedFilter::ByDocument {
            document_id: doc_id,
            version,
            handle: &handle_upper,
        },
    )
    .await
    .map_err(CadReadError::Domain)?;
    read.commit().await.map_err(CadReadError::Domain)?;
    Ok(Json(json!({ "items": items })))
}

async fn list_conversions(
    State(state): State<AppState>,
    Path((org, proj)): Path<(String, String)>,
    request: Request,
) -> Result<Json<Value>, Failure> {
    let (mut parts, _) = request.into_parts();
    let session = authenticate_actor(&state, &mut parts, &org).await?;
    let Some(mut scope) =
        edit_auth::open(&state, &session, &org, &proj, Guard::ReadScopedAllowed).await?
    else {
        return Err(EditError::InvalidRequest.into());
    };
    scope.authorize(Permission::Read).await?;
    let org_id = scope.organization_id();
    let proj_id = scope.project_id();
    let result = cad_documents::list_conversions(scope.tx_mut(), org_id, proj_id)
        .await
        .map_err(|_| EditError::Internal("cad conversions query failed".to_owned()))?;
    scope.commit().await?;
    Ok(Json(result))
}

const MAX_CAD_UPLOAD_BYTES: usize = 25 * 1024 * 1024;
const CAD_MULTIPART_CAP: usize = MAX_CAD_UPLOAD_BYTES + 256 * 1024;

fn is_hex64(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn parse_bool_field(raw: &str) -> Result<bool, EditError> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" | "" => Ok(false),
        _ => Err(EditError::InvalidRequest),
    }
}

fn valid_cad_unit(u: &str) -> bool {
    matches!(u, "meter" | "centimeter" | "millimeter")
}

async fn read_store_bytes(state: &AppState, key: &str, sha256: &str) -> Result<Vec<u8>, Failure> {
    let (mut file, _) = Store::new(state)
        .open_verified(key, sha256)
        .await
        .map_err(|_| EditError::detail(409, "CAD artifact integrity failure"))?;
    let mut payload = Vec::new();
    file.read_to_end(&mut payload)
        .await
        .map_err(|_| EditError::Internal("CAD store read failed".to_owned()))?;
    Ok(payload)
}

async fn inspect_cad_file(
    State(state): State<AppState>,
    Path((org, proj)): Path<(String, String)>,
    request: Request,
) -> Result<Json<Value>, Failure> {
    let (mut parts, body) = request.into_parts();
    let actor = authenticate_actor(&state, &mut parts, &org).await?;
    let content_type = parts
        .headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned();
    let raw = to_bytes(body, CAD_MULTIPART_CAP)
        .await
        .map_err(|_| EditError::validation("CAD_FILE_LIMIT", "CAD upload size unsupported"))?;
    let Some(mp) = multipart_parts(&content_type, &raw) else {
        return Err(EditError::InvalidRequest.into());
    };
    let mut file_info: Option<(String, &[u8])> = None;
    let mut units = String::new();
    let mut crs_code_raw = String::new();
    let mut tol_raw: Option<String> = None;
    for (k, fn_opt, content) in mp {
        if k == "file" && fn_opt.is_some() {
            file_info = Some((fn_opt.unwrap_or("drawing.dxf").to_owned(), content));
        } else if let Ok(v) = std::str::from_utf8(content) {
            match k.as_str() {
                "units" => units = v.to_owned(),
                "crs_code" => crs_code_raw = v.to_owned(),
                "curve_tolerance_m" if !v.trim().is_empty() => tol_raw = Some(v.to_owned()),
                _ => {}
            }
        }
    }
    let Some((filename, payload)) = file_info else {
        return Err(EditError::InvalidRequest.into());
    };
    if !valid_cad_unit(&units) {
        return Err(EditError::InvalidRequest.into());
    }
    let Ok(crs_code) = crs_code_raw.trim().parse::<i32>() else {
        return Err(EditError::InvalidRequest.into());
    };
    let curve_tolerance_m = match tol_raw {
        Some(t) => Some(
            t.trim()
                .parse::<f64>()
                .map_err(|_| EditError::InvalidRequest)?,
        ),
        None => None,
    };

    let Some(mut scope) = edit_auth::open(&state, &actor, &org, &proj, Guard::ScopedDenied).await?
    else {
        return Err(EditError::InvalidRequest.into());
    };
    actor_csrf(&state, &actor, &parts.headers)?;
    scope.authorize(Permission::Edit).await?;

    if !filename.to_ascii_lowercase().ends_with(".dxf") {
        return Err(EditError::validation(
            "CAD_FORMAT_UNSUPPORTED",
            "only 2D DXF can be inspected; DWG adapter unavailable",
        )
        .into());
    }
    if !matches!(crs_code, 3857 | 5899) {
        return Err(EditError::validation(
            "UNGEOREFERENCED_CAD_REJECTED",
            "explicit projected CRS required",
        )
        .into());
    }
    if payload.is_empty() || payload.len() > MAX_CAD_UPLOAD_BYTES {
        return Err(EditError::validation("CAD_FILE_LIMIT", "CAD upload size unsupported").into());
    }

    let parsed = run_parse_dxf(
        payload,
        json!({
            "units": units,
            "crs_code": crs_code,
            "curve_tolerance_m": curve_tolerance_m,
        }),
    )
    .await?;

    let wkts: Vec<String> = parsed["wkts"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(ToOwned::to_owned))
                .collect()
        })
        .unwrap_or_default();
    validate_wkts_postgis(&mut scope, &wkts).await?;
    scope.rollback().await?;

    let digest = parsed["sha256"].as_str().unwrap_or_default();
    let mut summary = parsed["entity_summary"].clone();
    if let Some(obj) = summary.as_object_mut() {
        obj.insert(
            "drawing_sha256".to_owned(),
            Value::String(digest.to_owned()),
        );
        obj.insert("crs_code".to_owned(), Value::from(crs_code));
    }

    Ok(Json(json!({
        "sha256": digest,
        "inspection_fingerprint": parsed["inspection_fingerprint"],
        "published": false,
        "geometry_valid": true,
        "entity_summary": summary,
        "loss_report": parsed["loss_report"],
    })))
}

async fn import_cad_file(
    State(state): State<AppState>,
    Path((org, proj)): Path<(String, String)>,
    request: Request,
) -> Result<(StatusCode, Json<Value>), Failure> {
    let (mut parts, body) = request.into_parts();
    let actor = authenticate_actor(&state, &mut parts, &org).await?;
    let content_type = parts
        .headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned();
    let raw = to_bytes(body, CAD_MULTIPART_CAP)
        .await
        .map_err(|_| EditError::validation("CAD_FILE_LIMIT", "CAD upload exceeds 25 MiB limit"))?;
    let Some(mp) = multipart_parts(&content_type, &raw) else {
        return Err(EditError::InvalidRequest.into());
    };
    let mut file_info: Option<(String, &[u8])> = None;
    let mut units = String::new();
    let mut crs_code_raw = String::new();
    let mut layer_name = "CAD Import".to_owned();
    let mut tol_raw: Option<String> = None;
    let mut ack_loss = false;
    let mut expected_sha256: Option<String> = None;
    let mut expected_fp: Option<String> = None;
    for (k, fn_opt, content) in mp {
        if k == "file" && fn_opt.is_some() {
            file_info = Some((fn_opt.unwrap_or("drawing.dxf").to_owned(), content));
        } else if let Ok(v) = std::str::from_utf8(content) {
            match k.as_str() {
                "units" => units = v.to_owned(),
                "crs_code" => crs_code_raw = v.to_owned(),
                "layer_name" => layer_name = v.to_owned(),
                "curve_tolerance_m" if !v.trim().is_empty() => tol_raw = Some(v.to_owned()),
                "acknowledge_loss" => ack_loss = parse_bool_field(v)?,
                "expected_sha256" if !v.is_empty() => expected_sha256 = Some(v.to_owned()),
                "expected_inspection_fingerprint" if !v.is_empty() => {
                    expected_fp = Some(v.to_owned())
                }
                _ => {}
            }
        }
    }
    let Some((filename, payload)) = file_info else {
        return Err(EditError::InvalidRequest.into());
    };
    if !valid_cad_unit(&units) {
        return Err(EditError::InvalidRequest.into());
    }
    let Ok(crs_code) = crs_code_raw.trim().parse::<i32>() else {
        return Err(EditError::InvalidRequest.into());
    };
    let curve_tolerance_m = match tol_raw {
        Some(t) => Some(
            t.trim()
                .parse::<f64>()
                .map_err(|_| EditError::InvalidRequest)?,
        ),
        None => None,
    };

    let Some(mut scope) = edit_auth::open(&state, &actor, &org, &proj, Guard::ScopedDenied).await?
    else {
        return Err(EditError::InvalidRequest.into());
    };
    actor_csrf(&state, &actor, &parts.headers)?;
    scope.authorize(Permission::Edit).await?;

    if !matches!(crs_code, 3857 | 5899) {
        return Err(EditError::validation(
            "UNGEOREFERENCED_CAD_REJECTED",
            "DXF length units require an explicit supported projected CRS (3857 or 5899); geographic degrees are not metres",
        )
        .into());
    }
    if payload.is_empty() || payload.len() > MAX_CAD_UPLOAD_BYTES {
        return Err(EditError::validation("CAD_FILE_LIMIT", "CAD upload size unsupported").into());
    }

    let lower = filename.to_ascii_lowercase();
    if !lower.ends_with(".dwg") && !lower.ends_with(".dxf") {
        return Err(
            EditError::validation("CAD_FORMAT_UNSUPPORTED", "only .dxf is supported").into(),
        );
    }

    let conv_id = Uuid::new_v4();
    let sha256_hex_val = sha256_hex(payload);
    if let Some(ref exp) = expected_sha256 {
        if exp != &sha256_hex_val {
            return Err(EditError::conflict(
                "CAD_INSPECTION_STALE",
                "DXF changed after inspection",
            )
            .into());
        }
    }

    let org_id = scope.organization_id();
    let proj_id = scope.project_id();
    let fmt = if lower.ends_with(".dwg") {
        "dwg"
    } else {
        "dxf"
    };
    let skey = format!("cad/{org_id}/{proj_id}/{conv_id}.{fmt}");

    if fmt == "dwg" {
        Store::new(&state)
            .put(&skey, payload)
            .await
            .map_err(|e| EditError::Internal(format!("CAD DWG store failed: {e:?}")))?;
        let rec_id = insert_rejected_dwg_conversion(
            &mut scope,
            conv_id,
            &filename,
            &units,
            crs_code,
            &skey,
            &sha256_hex_val,
            payload.len() as i64,
        )
        .await?;
        scope.commit().await?;
        return Err(EditError::validation(
            "DWG_ADAPTER_REQUIRED",
            "DWG requires a separately licensed adapter; original evidence preserved.",
        )
        .with_details(json!({ "conversion_id": rec_id.to_string(), "sha256": sha256_hex_val }))
        .into());
    }

    let parsed = run_parse_dxf(
        payload,
        json!({
            "units": units,
            "crs_code": crs_code,
            "curve_tolerance_m": curve_tolerance_m,
        }),
    )
    .await?;

    let fingerprint = parsed["inspection_fingerprint"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    if let Some(ref exp) = expected_fp {
        if exp != &fingerprint {
            return Err(EditError::conflict(
                "CAD_INSPECTION_STALE",
                "DXF inspection profile or drawing changed",
            )
            .into());
        }
    }
    let requires_ack = parsed["loss_report"]["requires_acknowledgement"]
        .as_bool()
        .unwrap_or(false);
    let mut summary = parsed["entity_summary"].clone();
    let mut loss_report = parsed["loss_report"].clone();
    if requires_ack && (!ack_loss || expected_fp.is_none()) {
        return Err(EditError::validation(
            "CAD_LOSS_ACK_REQUIRED",
            "Inspect the drawing and explicitly acknowledge CAD mapping loss before import",
        )
        .with_details(json!({
            "sha256": sha256_hex_val,
            "inspection_fingerprint": fingerprint,
            "entity_summary": summary,
            "loss_report": loss_report,
        }))
        .into());
    }

    if let Some(obj) = summary.as_object_mut() {
        obj.insert(
            "drawing_sha256".to_owned(),
            Value::String(sha256_hex_val.clone()),
        );
        obj.insert(
            "inspection_fingerprint".to_owned(),
            Value::String(fingerprint.clone()),
        );
        obj.insert("crs_code".to_owned(), Value::from(crs_code));
    }
    if let Some(obj) = loss_report.as_object_mut() {
        obj.insert("acknowledged".to_owned(), Value::from(ack_loss));
    }

    let features: Vec<Value> = parsed["features"].as_array().cloned().unwrap_or_default();
    let family = parsed["family"].as_str().unwrap_or("Point");
    let schema_rev = parsed["schema_rev"].as_str().unwrap_or_default();

    let conv = publish_cad_import(
        &mut scope,
        PublishCadImportArgs {
            conv_id,
            filename: &filename,
            layer_name: &layer_name,
            units: &units,
            crs_code,
            family,
            sha256: &sha256_hex_val,
            byte_size: payload.len() as i64,
            storage_key: &skey,
            schema_rev,
            inspection_fingerprint: &fingerprint,
            entity_summary: summary,
            loss_report,
            features: &features,
            doc_placement: None,
        },
    )
    .await?;

    Store::new(&state)
        .put(&skey, payload)
        .await
        .map_err(|e| EditError::Internal(format!("CAD store put failed: {e:?}")))?;
    scope.commit().await?;
    Ok((StatusCode::CREATED, Json(conv)))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExportDxfBody {
    layer_id: Uuid,
    units: String,
    expected_content_revision: String,
    #[serde(default)]
    acknowledge_attribute_loss: bool,
}

async fn export_cad_dxf(
    State(state): State<AppState>,
    Path((org, proj)): Path<(String, String)>,
    request: Request,
) -> Result<Response, Failure> {
    let (actor, headers, maybe_json) = authenticate_json(&state, request, &org).await?;
    let body: ExportDxfBody = maybe_json
        .and_then(|v| serde_json::from_value(v).ok())
        .ok_or(EditError::InvalidRequest)?;
    if !valid_cad_unit(&body.units) || !is_hex64(&body.expected_content_revision) {
        return Err(EditError::InvalidRequest.into());
    }
    let Some(mut scope) = edit_auth::open(&state, &actor, &org, &proj, Guard::Read).await? else {
        return Err(EditError::InvalidRequest.into());
    };
    actor_csrf(&state, &actor, &headers)?;
    scope.authorize(Permission::Read).await?;

    let src = load_dxf_export_source(
        &mut scope,
        body.layer_id,
        &body.expected_content_revision,
        body.acknowledge_attribute_loss,
    )
    .await?;

    let (dxf_bytes, dxf_sha, dxf_size) = run_write_dxf(json!({
        "features": src.features,
        "layer_name": src.layer_name,
        "units": body.units,
    }))
    .await?;

    let conv_id = Uuid::new_v4();
    let fname = format!("export-{conv_id}.dxf");
    let skey = format!(
        "cad/{}/{}/{fname}",
        scope.organization_id(),
        scope.project_id()
    );
    Store::new(&state)
        .put(&skey, &dxf_bytes)
        .await
        .map_err(|e| EditError::Internal(format!("CAD export store failed: {e:?}")))?;

    insert_dxf_export_conversion(
        &mut scope,
        conv_id,
        &fname,
        &body.units,
        src.stored_crs,
        &skey,
        &dxf_sha,
        dxf_size,
        src.source_id,
        body.layer_id,
        &src.content_revision,
        src.features.len(),
    )
    .await?;
    scope.commit().await?;

    let mut resp_headers = HeaderMap::new();
    resp_headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/dxf"),
    );
    if let Ok(hv) = HeaderValue::from_str(&format!("attachment; filename=\"{fname}\"")) {
        resp_headers.insert(header::CONTENT_DISPOSITION, hv);
    }
    if let Ok(hv) = HeaderValue::from_str(&dxf_sha) {
        resp_headers.insert("x-content-sha256", hv);
    }
    if let Ok(hv) = HeaderValue::from_str(&conv_id.to_string()) {
        resp_headers.insert("x-cad-conversion-id", hv);
    }
    Ok((StatusCode::OK, resp_headers, dxf_bytes).into_response())
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GeorefBody {
    crs_code: i32,
    #[serde(default)]
    web_controls: Option<Vec<[[f64; 2]; 2]>>,
    units: String,
    controls: Vec<[[f64; 2]; 2]>,
    tolerance_m: f64,
    max_scale_deviation: f64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GeorefImportBody {
    crs_code: i32,
    #[serde(default)]
    web_controls: Option<Vec<[[f64; 2]; 2]>>,
    units: String,
    controls: Vec<[[f64; 2]; 2]>,
    tolerance_m: f64,
    max_scale_deviation: f64,
    layer_name: String,
    expected_sha256: String,
    expected_inspection_fingerprint: String,
    acknowledge_loss: bool,
}

fn validate_georef_fields(
    crs_code: i32,
    units: &str,
    controls: &[[[f64; 2]; 2]],
    web_controls: Option<&Vec<[[f64; 2]; 2]>>,
    tolerance_m: f64,
    max_scale_deviation: f64,
) -> Result<(), EditError> {
    if !matches!(crs_code, 3857 | 5899)
        || !valid_cad_unit(units)
        || !(3..=100).contains(&controls.len())
        || web_controls.is_some_and(|wc| !(3..=100).contains(&wc.len()))
        || !tolerance_m.is_finite()
        || !(1e-6..=1000.0).contains(&tolerance_m)
        || !max_scale_deviation.is_finite()
        || !(0.0..=0.2).contains(&max_scale_deviation)
    {
        return Err(EditError::InvalidRequest);
    }
    Ok(())
}

async fn inspect_georef(
    State(state): State<AppState>,
    Path((org, proj, doc_id_raw, ver_raw)): Path<(String, String, String, String)>,
    request: Request,
) -> Result<Json<Value>, Failure> {
    let (actor, headers, maybe_json) = authenticate_json(&state, request, &org).await?;
    let body: GeorefBody = maybe_json
        .and_then(|v| serde_json::from_value(v).ok())
        .ok_or(EditError::InvalidRequest)?;
    validate_georef_fields(
        body.crs_code,
        &body.units,
        &body.controls,
        body.web_controls.as_ref(),
        body.tolerance_m,
        body.max_scale_deviation,
    )?;
    let (Ok(document_id), Ok(version)) = (Uuid::parse_str(&doc_id_raw), ver_raw.parse::<i32>())
    else {
        return Err(EditError::InvalidRequest.into());
    };
    let Some(mut scope) = edit_auth::open(&state, &actor, &org, &proj, Guard::ScopedDenied).await?
    else {
        return Err(EditError::InvalidRequest.into());
    };
    actor_csrf(&state, &actor, &headers)?;
    scope.authorize(Permission::Edit).await?;

    let (_, _, storage_key, sha256) =
        load_cad_document_version_blob(&mut scope, document_id, version).await?;
    let payload = read_store_bytes(&state, &storage_key, &sha256).await?;
    let parsed = run_parse_dxf(
        &payload,
        json!({
            "units": body.units,
            "crs_code": body.crs_code,
            "curve_tolerance_m": null,
            "document_id": document_id.to_string(),
            "version": version,
            "georef": {
                "crs_code": body.crs_code,
                "units": body.units,
                "controls": body.controls,
                "web_controls": body.web_controls,
                "tolerance_m": body.tolerance_m,
                "max_scale_deviation": body.max_scale_deviation,
            }
        }),
    )
    .await?;

    let wkts: Vec<String> = parsed["wkts"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(ToOwned::to_owned))
                .collect()
        })
        .unwrap_or_default();
    validate_wkts_postgis(&mut scope, &wkts).await?;
    scope.rollback().await?;

    let digest = parsed["sha256"].as_str().unwrap_or_default();
    let placed_crs = parsed["crs_code"].as_i64().unwrap_or(3857);
    let mut summary = parsed["entity_summary"].clone();
    if let Some(obj) = summary.as_object_mut() {
        obj.insert(
            "drawing_sha256".to_owned(),
            Value::String(digest.to_owned()),
        );
        obj.insert("crs_code".to_owned(), Value::from(placed_crs));
    }

    Ok(Json(json!({
        "sha256": digest,
        "inspection_fingerprint": parsed["inspection_fingerprint"],
        "published": false,
        "geometry_valid": true,
        "entity_summary": summary,
        "loss_report": parsed["loss_report"],
    })))
}

async fn import_georef(
    State(state): State<AppState>,
    Path((org, proj, doc_id_raw, ver_raw)): Path<(String, String, String, String)>,
    request: Request,
) -> Result<(StatusCode, Json<Value>), Failure> {
    let (actor, headers, maybe_json) = authenticate_json(&state, request, &org).await?;
    let body: GeorefImportBody = maybe_json
        .and_then(|v| serde_json::from_value(v).ok())
        .ok_or(EditError::InvalidRequest)?;
    validate_georef_fields(
        body.crs_code,
        &body.units,
        &body.controls,
        body.web_controls.as_ref(),
        body.tolerance_m,
        body.max_scale_deviation,
    )?;
    let layer_len = body.layer_name.chars().count();
    if !(1..=200).contains(&layer_len)
        || !is_hex64(&body.expected_sha256)
        || !is_hex64(&body.expected_inspection_fingerprint)
        || !body.acknowledge_loss
    {
        return Err(EditError::InvalidRequest.into());
    }
    let (Ok(document_id), Ok(version)) = (Uuid::parse_str(&doc_id_raw), ver_raw.parse::<i32>())
    else {
        return Err(EditError::InvalidRequest.into());
    };
    let Some(mut scope) = edit_auth::open(&state, &actor, &org, &proj, Guard::ScopedDenied).await?
    else {
        return Err(EditError::InvalidRequest.into());
    };
    actor_csrf(&state, &actor, &headers)?;
    scope.authorize(Permission::Edit).await?;

    let (filename, _, storage_key, doc_sha256) =
        load_cad_document_version_blob(&mut scope, document_id, version).await?;
    if body.expected_sha256 != doc_sha256 {
        return Err(
            EditError::conflict("CAD_INSPECTION_STALE", "DXF changed after inspection").into(),
        );
    }
    let payload = read_store_bytes(&state, &storage_key, &doc_sha256).await?;
    let parsed = run_parse_dxf(
        &payload,
        json!({
            "units": body.units,
            "crs_code": body.crs_code,
            "curve_tolerance_m": null,
            "document_id": document_id.to_string(),
            "version": version,
            "georef": {
                "crs_code": body.crs_code,
                "units": body.units,
                "controls": body.controls,
                "web_controls": body.web_controls,
                "tolerance_m": body.tolerance_m,
                "max_scale_deviation": body.max_scale_deviation,
            }
        }),
    )
    .await?;

    let fingerprint = parsed["inspection_fingerprint"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    if body.expected_inspection_fingerprint != fingerprint {
        return Err(EditError::conflict(
            "CAD_INSPECTION_STALE",
            "DXF inspection profile or drawing changed",
        )
        .into());
    }
    if let Some(prior) = find_existing_georef_conversion(&mut scope, &fingerprint).await? {
        scope.rollback().await?;
        return Ok((
            StatusCode::CREATED,
            Json(json!({
                "id": prior["id"],
                "layer_id": prior["layer_id"],
                "source_id": prior["source_id"],
                "sha256": prior["sha256"],
                "content_revision": prior["content_revision"],
                "entity_summary": prior["entity_summary"],
                "loss_report": prior["loss_report"],
            })),
        ));
    }

    let placed_crs = parsed["crs_code"].as_i64().unwrap_or(3857) as i32;
    let schema_rev = parsed["schema_rev"].as_str().unwrap_or_default();
    let family = parsed["family"].as_str().unwrap_or("Point");
    let features: Vec<Value> = parsed["features"].as_array().cloned().unwrap_or_default();
    let mut summary = parsed["entity_summary"].clone();
    if let Some(obj) = summary.as_object_mut() {
        obj.insert(
            "imported_schema_revision".to_owned(),
            Value::String(schema_rev.to_owned()),
        );
        obj.insert(
            "drawing_sha256".to_owned(),
            Value::String(doc_sha256.clone()),
        );
        obj.insert(
            "inspection_fingerprint".to_owned(),
            Value::String(fingerprint.clone()),
        );
        obj.insert("crs_code".to_owned(), Value::from(placed_crs));
    }
    let mut loss_report = parsed["loss_report"].clone();
    if let Some(obj) = loss_report.as_object_mut() {
        obj.insert("acknowledged".to_owned(), Value::from(true));
    }

    let conv_id = Uuid::new_v4();
    let conv_key = format!(
        "cad/{}/{}/{conv_id}.dxf",
        scope.organization_id(),
        scope.project_id()
    );
    let rec = publish_cad_import(
        &mut scope,
        PublishCadImportArgs {
            conv_id,
            filename: &filename,
            layer_name: &body.layer_name,
            units: &body.units,
            crs_code: placed_crs,
            family,
            sha256: &doc_sha256,
            byte_size: payload.len() as i64,
            storage_key: &conv_key,
            schema_rev,
            inspection_fingerprint: &fingerprint,
            entity_summary: summary,
            loss_report,
            features: &features,
            doc_placement: Some((document_id, version)),
        },
    )
    .await?;

    Store::new(&state)
        .put(&conv_key, &payload)
        .await
        .map_err(|e| EditError::Internal(format!("CAD georef store failed: {e:?}")))?;
    scope.commit().await?;

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id": rec["id"],
            "layer_id": rec["layer_id"],
            "source_id": rec["source_id"],
            "sha256": rec["sha256"],
            "content_revision": rec["content_revision"],
            "entity_summary": rec["entity_summary"],
            "loss_report": rec["loss_report"],
        })),
    ))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GeorefDraftBody {
    conversion_id: Uuid,
    expected_document_sha256: String,
    expected_content_revision: String,
    expected_schema_revision: String,
    #[serde(default)]
    expected_preview_fingerprint: Option<String>,
    #[serde(default)]
    acknowledge_loss: bool,
    #[serde(default = "default_draft_profile")]
    profile: String,
}

fn default_draft_profile() -> String {
    "derived-local-dxf-v1".to_owned()
}

async fn export_georef_draft(
    State(state): State<AppState>,
    Path((org, proj, doc_id_raw, ver_raw)): Path<(String, String, String, String)>,
    request: Request,
) -> Result<Response, Failure> {
    let (actor, headers, maybe_json) = authenticate_json(&state, request, &org).await?;
    let body: GeorefDraftBody = maybe_json
        .and_then(|v| serde_json::from_value(v).ok())
        .ok_or(EditError::InvalidRequest)?;
    if !is_hex64(&body.expected_document_sha256)
        || !is_hex64(&body.expected_content_revision)
        || !is_hex64(&body.expected_schema_revision)
        || body
            .expected_preview_fingerprint
            .as_deref()
            .is_some_and(|fp| !is_hex64(fp))
        || !matches!(
            body.profile.as_str(),
            "derived-local-dxf-v1" | "native-xy-dxf-v1"
        )
    {
        return Err(EditError::InvalidRequest.into());
    }
    let (Ok(document_id), Ok(version)) = (Uuid::parse_str(&doc_id_raw), ver_raw.parse::<i32>())
    else {
        return Err(EditError::InvalidRequest.into());
    };
    let Some(mut scope) = edit_auth::open(&state, &actor, &org, &proj, Guard::ScopedDenied).await?
    else {
        return Err(EditError::InvalidRequest.into());
    };
    actor_csrf(&state, &actor, &headers)?;
    scope.authorize(Permission::Edit).await?;

    let ctx = load_georef_draft_context(
        &mut scope,
        document_id,
        version,
        body.conversion_id,
        &body.expected_document_sha256,
        &body.expected_content_revision,
        &body.expected_schema_revision,
    )
    .await?;
    let original = read_store_bytes(&state, &ctx.storage_key, &ctx.sha256).await?;
    scope.rollback().await?;

    let (meta, payload) = run_export_georef_draft(
        &original,
        json!({
            "document_id": document_id.to_string(),
            "version": version,
            "document_sha256": ctx.sha256,
            "conversion_id": body.conversion_id.to_string(),
            "source_id": ctx.source_id.to_string(),
            "layer_id": ctx.layer_id.to_string(),
            "content_revision": ctx.content_revision,
            "schema_revision": ctx.schema_revision,
            "layer_name": ctx.layer_name,
            "entity_summary": ctx.entity_summary,
            "features": ctx.features,
            "profile": body.profile,
        }),
    )
    .await?;

    let fingerprint = meta["preview_fingerprint"].as_str().unwrap_or_default();
    if body.expected_preview_fingerprint.is_none() {
        if body.acknowledge_loss {
            return Err(EditError::detail(422, "Review preview before acknowledging loss").into());
        }
        let mut preview = meta["preview"].clone();
        if let Some(obj) = preview.as_object_mut() {
            obj.insert(
                "preview_fingerprint".to_owned(),
                Value::String(fingerprint.to_owned()),
            );
        }
        return Ok((
            StatusCode::OK,
            [(header::CACHE_CONTROL, "no-store")],
            Json(preview),
        )
            .into_response());
    }
    if body.expected_preview_fingerprint.as_deref() != Some(fingerprint) {
        return Err(EditError::detail(409, "Draft preview changed").into());
    }
    if !body.acknowledge_loss {
        return Err(
            EditError::detail(422, "Explicit derived DXF loss acknowledgement required").into(),
        );
    }

    let payload_sha = meta["payload_sha256"].as_str().unwrap_or_default();
    let mut resp_headers = HeaderMap::new();
    resp_headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/dxf"),
    );
    if let Ok(hv) = HeaderValue::from_str(&format!(
        "attachment; filename=\"derived-{document_id}-v{version}.dxf\""
    )) {
        resp_headers.insert(header::CONTENT_DISPOSITION, hv);
    }
    resp_headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    resp_headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    if let Ok(hv) = HeaderValue::from_str(payload_sha) {
        resp_headers.insert("x-content-sha256", hv);
    }
    if let Ok(hv) = HeaderValue::from_str(fingerprint) {
        resp_headers.insert("x-cad-preview-fingerprint", hv);
    }
    Ok((StatusCode::OK, resp_headers, payload).into_response())
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeImportBody {
    conversion_id: Uuid,
    expected_candidate_sha256: String,
}

async fn preview_native_import(
    State(state): State<AppState>,
    Path((org, proj, doc_id_raw, ver_raw)): Path<(String, String, String, String)>,
    request: Request,
) -> Result<Response, Failure> {
    let (actor, headers, maybe_json) = authenticate_json(&state, request, &org).await?;
    let body: NativeImportBody = maybe_json
        .and_then(|v| serde_json::from_value(v).ok())
        .ok_or(EditError::InvalidRequest)?;
    if !is_hex64(&body.expected_candidate_sha256) {
        return Err(EditError::InvalidRequest.into());
    }
    let (Ok(document_id), Ok(version)) = (Uuid::parse_str(&doc_id_raw), ver_raw.parse::<i32>())
    else {
        return Err(EditError::InvalidRequest.into());
    };
    let Some(mut scope) = edit_auth::open(&state, &actor, &org, &proj, Guard::ScopedDenied).await?
    else {
        return Err(EditError::InvalidRequest.into());
    };
    actor_csrf(&state, &actor, &headers)?;
    scope.authorize(Permission::Edit).await?;

    let ctx = load_native_import_context(
        &mut scope,
        document_id,
        version,
        body.conversion_id,
        &body.expected_candidate_sha256,
    )
    .await?;
    let Some(imported_schema) = ctx
        .entity_summary
        .get("imported_schema_revision")
        .and_then(Value::as_str)
    else {
        return Err(
            EditError::detail(422, "CAD conversion lacks an immutable schema revision").into(),
        );
    };

    let original = read_store_bytes(&state, &ctx.original_key, &ctx.original_sha).await?;
    let candidate = read_store_bytes(&state, &ctx.candidate_key, &ctx.candidate_sha).await?;

    let preview = run_preview_native_import(
        &original,
        &candidate,
        json!({
            "document_id": document_id.to_string(),
            "version": version,
            "entity_summary": ctx.entity_summary,
            "original_sha": ctx.original_sha,
            "candidate_sha": ctx.candidate_sha,
            "imported_content_revision": ctx.imported_content_revision,
            "current_content_revision": ctx.content_revision,
            "imported_schema_revision": imported_schema,
            "current_schema_revision": ctx.schema_revision,
        }),
    )
    .await?;

    let ops = preview["operations"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    validate_geojson_ops_postgis(&mut scope, &ops).await?;
    scope.rollback().await?;

    Ok((
        StatusCode::OK,
        [(header::CACHE_CONTROL, "no-store")],
        Json(json!({
            "conversion_id": body.conversion_id.to_string(),
            "source_id": ctx.source_id.to_string(),
            "candidate_sha256": ctx.candidate_sha,
            "candidate_version": version + 1,
            "base_content_revision": ctx.content_revision,
            "base_schema_revision": ctx.schema_revision,
            "operations": ops,
            "published": false,
        })),
    )
        .into_response())
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CogoTraverseBody {
    origin: [f64; 2],
    distance: f64,
    azimuth_degrees: f64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CogoIntersectionBody {
    seg_a: [[f64; 2]; 2],
    seg_b: [[f64; 2]; 2],
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CogoOffsetBody {
    segment: [[f64; 2]; 2],
    offset_distance: f64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CogoSnapBody {
    target: [f64; 2],
    segments: Vec<[[f64; 2]; 2]>,
    tolerance: f64,
}

fn cogo_err(msg: &'static str) -> Failure {
    Failure::Edit(EditError::Platform {
        status: 422,
        code: "INVALID_COGO",
        message: msg.to_owned(),
        details: None,
    })
}

fn bounded(v: f64) -> bool {
    v.is_finite() && v.abs() <= COGO_LIMIT
}

fn validate_pt(pt: [f64; 2]) -> Result<(), Failure> {
    if !bounded(pt[0]) || !bounded(pt[1]) {
        return Err(cogo_err("finite bounded planar coordinates required"));
    }
    Ok(())
}

fn validate_seg(seg: [[f64; 2]; 2]) -> Result<f64, Failure> {
    validate_pt(seg[0])?;
    validate_pt(seg[1])?;
    Ok((seg[0][0] - seg[1][0]).hypot(seg[0][1] - seg[1][1]))
}

fn planar_preview(mut val: Value) -> Value {
    if let Some(obj) = val.as_object_mut() {
        obj.insert(
            "coordinate_reference".to_owned(),
            json!("unreferenced_planar_input"),
        );
        obj.insert("units".to_owned(), json!("input_coordinate_units"));
        obj.insert("survey_grade".to_owned(), json!(false));
        obj.insert("published".to_owned(), json!(false));
    }
    val
}

async fn authorize_cogo<T: serde::de::DeserializeOwned>(
    state: &AppState,
    org: &str,
    proj: &str,
    request: Request,
) -> Result<T, Failure> {
    let (session, headers, body) = authenticate_json(state, request, org).await?;
    actor_csrf(state, &session, &headers)?;
    let Some(mut scope) =
        edit_auth::open(state, &session, org, proj, Guard::ReadScopedAllowed).await?
    else {
        return Err(EditError::InvalidRequest.into());
    };
    scope.authorize(Permission::Read).await?;
    let parsed: T = edit_auth::decode(body)?;
    scope.commit().await?;
    Ok(parsed)
}

async fn cogo_traverse(
    State(state): State<AppState>,
    Path((org, proj)): Path<(String, String)>,
    request: Request,
) -> Result<Json<Value>, Failure> {
    let body: CogoTraverseBody = authorize_cogo(&state, &org, &proj, request).await?;
    validate_pt(body.origin)?;
    if body.distance <= 0.0 || !bounded(body.distance) {
        return Err(cogo_err("distance must be > 0"));
    }
    if !bounded(body.azimuth_degrees) {
        return Err(cogo_err("azimuth_degrees must be finite"));
    }
    let az = body.azimuth_degrees.rem_euclid(360.0);
    let rad = az.to_radians();
    let dx = body.distance * rad.sin();
    let dy = body.distance * rad.cos();
    let end = [body.origin[0] + dx, body.origin[1] + dy];
    validate_pt(end)?;
    Ok(Json(planar_preview(json!({
        "operation": "traverse",
        "origin": body.origin,
        "endpoint": end,
        "delta": [dx, dy],
        "distance": body.distance,
        "azimuth_degrees": az,
    }))))
}

async fn cogo_intersection(
    State(state): State<AppState>,
    Path((org, proj)): Path<(String, String)>,
    request: Request,
) -> Result<Json<Value>, Failure> {
    let body: CogoIntersectionBody = authorize_cogo(&state, &org, &proj, request).await?;
    let la = validate_seg(body.seg_a)?;
    let lb = validate_seg(body.seg_b)?;
    if la.min(lb) < 1e-9 {
        return Err(cogo_err("degenerate intersection segment"));
    }
    let [[x1, y1], [x2, y2]] = body.seg_a;
    let [[x3, y3], [x4, y4]] = body.seg_b;
    let denom = (x1 - x2) * (y3 - y4) - (y1 - y2) * (x3 - x4);
    if denom.abs() <= 1e-12 * la * lb {
        return Ok(Json(planar_preview(json!({
            "intersects": false,
            "parallel": true,
            "point": null,
            "within_segments": false,
        }))));
    }
    let t = ((x1 - x3) * (y3 - y4) - (y1 - y3) * (x3 - x4)) / denom;
    let u = -((x1 - x2) * (y1 - y3) - (y1 - y2) * (x1 - x3)) / denom;
    let px = x1 + t * (x2 - x1);
    let py = y1 + t * (y2 - y1);
    validate_pt([px, py])?;
    let within = (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u);
    Ok(Json(planar_preview(json!({
        "intersects": true,
        "parallel": false,
        "point": [px, py],
        "within_segments": within,
        "t": t,
        "u": u,
    }))))
}

async fn cogo_offset(
    State(state): State<AppState>,
    Path((org, proj)): Path<(String, String)>,
    request: Request,
) -> Result<Json<Value>, Failure> {
    let body: CogoOffsetBody = authorize_cogo(&state, &org, &proj, request).await?;
    let len = validate_seg(body.segment)?;
    if !bounded(body.offset_distance) {
        return Err(cogo_err("invalid offset distance"));
    }
    if len < 1e-9 {
        return Err(cogo_err("degenerate zero-length segment"));
    }
    let [[x1, y1], [x2, y2]] = body.segment;
    let dx = x2 - x1;
    let dy = y2 - y1;
    let ox = (dy / len) * body.offset_distance;
    let oy = (-dx / len) * body.offset_distance;
    validate_pt([x1 + ox, y1 + oy])?;
    validate_pt([x2 + ox, y2 + oy])?;
    Ok(Json(planar_preview(json!({
        "operation": "parallel_offset",
        "offset_distance": body.offset_distance,
        "segment": [[x1 + ox, y1 + oy], [x2 + ox, y2 + oy]],
    }))))
}

async fn cogo_snap(
    State(state): State<AppState>,
    Path((org, proj)): Path<(String, String)>,
    request: Request,
) -> Result<Json<Value>, Failure> {
    let body: CogoSnapBody = authorize_cogo(&state, &org, &proj, request).await?;
    validate_pt(body.target)?;
    if !bounded(body.tolerance) || body.tolerance < 0.0 || body.segments.len() > 10_000 {
        return Err(cogo_err("invalid snap tolerance or segment limit"));
    }
    for &seg in &body.segments {
        let _ = validate_seg(seg)?;
    }
    struct SnapBest {
        dist: f64,
        pt: [f64; 2],
        kind: &'static str,
    }
    impl SnapBest {
        fn consider(&mut self, tx: f64, ty: f64, px: f64, py: f64, label: &'static str) {
            let d = (tx - px).hypot(ty - py);
            if d < self.dist
                || (d == self.dist && (self.kind == "none" || (px, py) < (self.pt[0], self.pt[1])))
            {
                self.dist = d;
                self.pt = [px, py];
                self.kind = label;
            }
        }
    }

    let [tx, ty] = body.target;
    let mut best = SnapBest {
        dist: body.tolerance,
        pt: [tx, ty],
        kind: "none",
    };

    for &[a, b] in &body.segments {
        for [vx, vy] in [a, b] {
            best.consider(tx, ty, vx, vy, "vertex");
        }
    }
    if best.kind == "none" {
        for &[[ax, ay], [bx, by]] in &body.segments {
            best.consider(tx, ty, (ax + bx) / 2.0, (ay + by) / 2.0, "midpoint");
        }
    }
    if best.kind == "none" {
        for &[[ax, ay], [bx, by]] in &body.segments {
            let dx = bx - ax;
            let dy = by - ay;
            let lsq = dx * dx + dy * dy;
            if lsq <= 1e-18 {
                continue;
            }
            let t = (((tx - ax) * dx + (ty - ay) * dy) / lsq).clamp(0.0, 1.0);
            best.consider(tx, ty, ax + t * dx, ay + t * dy, "perpendicular");
        }
    }

    Ok(Json(planar_preview(json!({
        "snapped": best.kind != "none",
        "kind": best.kind,
        "point": best.pt,
        "distance": if best.kind != "none" { Some(best.dist) } else { None },
    }))))
}

enum CadReadError {
    Access(CatalogError),
    Domain(AppError),
    Doc(CadDocError),
    Detail(StatusCode, &'static str),
    DocumentNotFound,
}

impl IntoResponse for CadReadError {
    fn into_response(self) -> Response {
        match self {
            Self::Access(error) => error.into_response(),
            Self::Detail(status, msg) => (status, Json(json!({ "detail": msg }))).into_response(),
            Self::Doc(CadDocError::Detail(status, msg)) => {
                let st = StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
                (st, Json(json!({ "detail": msg }))).into_response()
            }
            Self::Doc(CadDocError::Internal(error)) | Self::Domain(error) => match error {
                AppError::Internal { internal_detail } => {
                    tracing::error!(detail = %internal_detail, "CAD request failed");
                    (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response()
                }
                other => {
                    let status = StatusCode::from_u16(other.status_code())
                        .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
                    (status, Json(other.to_envelope(None))).into_response()
                }
            },
            Self::DocumentNotFound => (
                StatusCode::NOT_FOUND,
                Json(json!({ "detail": "CAD document not found" })),
            )
                .into_response(),
        }
    }
}

async fn read_v8i_capabilities(
    State(state): State<AppState>,
    auth: HumanSession,
    Path((organization_id, project_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>, CatalogError> {
    let _scope = project_read(
        &state,
        &auth,
        organization_id,
        project_id,
        ProjectReadPolicy::Unrestricted,
    )
    .await?;
    Ok(Json(capability_status()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v8i_capability_contract_stays_fail_closed() {
        let body = capability_status();
        assert_eq!(body["connector"], "microstation-v8i");
        assert_eq!(body["contract"], "ovg.v8i.snapshot.v1");
        assert_eq!(body["read_only"], true);
        assert_eq!(body["live_connection"], false);
        assert_eq!(body["capabilities"]["remote_preview"], "unavailable");
        assert_eq!(body["capabilities"]["write_back_to_dgn"], "unavailable");
    }

    #[test]
    fn cad_header_checks() {
        assert_eq!(
            validate_cad_upload("a.dwg", b"AC1032abcdef").ok(),
            Some("dwg")
        );
        assert_eq!(
            validate_cad_upload("b.dxf", b"0\r\nSECTION\r\n2\r\nHEADER\r\n").ok(),
            Some("dxf")
        );
        assert!(validate_cad_upload("b.dwg", b"BAD123").is_err());
        assert!(validate_cad_upload("b.pdf", b"AC1032").is_err());
    }
}
