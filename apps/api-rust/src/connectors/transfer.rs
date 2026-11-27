//! E12 integration-hub layer export and two-step revision-pinned GeoPackage sync routes.

use std::collections::{BTreeMap, HashMap};

use axum::body::to_bytes;
use axum::extract::{Path, Request, State};
use axum::http::{header, HeaderMap, HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use ovg_core::{lookup_readable_crs, CrsUnit};
use ovg_storage::editing::domain::{EditOperation, OpType};
use ovg_storage::editing::{CommitRequest, EditError, Guard, Permission};
use ovg_storage::integration_hub::{
    ascii_fold, is_source_allowed, load_active_layer_by_id, load_export_features,
    load_source_fields, load_sync_server_features, record_layer_export_audit, resolve_export_scope,
    utm_code_for_source, ExportScopeSpec,
};
use serde_json::{json, Value};
use uuid::Uuid;

use super::catalog::get_adapter;
use super::transfer_worker::{run_export, run_sync_diff, WorkerError};
use crate::edit_auth::{self, authenticate_actor, require_csrf, Failure};
use crate::transfers::store::multipart_parts;
use crate::AppState;

const BASE: &str = "/organizations/{organization_id}/projects/{project_id}/integration-hub";
const MAX_DXF_EXPORT_FEATURES: usize = 50_000;
const MAX_SYNC_UPLOAD_BYTES: usize = 16 * 1024 * 1024;
const MULTIPART_BODY_CAP: usize = MAX_SYNC_UPLOAD_BYTES + 256 * 1024;
const MAX_SYNC_PREVIEW_ITEMS: usize = 100;

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route(
            &format!("{BASE}/adapters/{{adapter_key}}/layers/{{layer_id}}/export"),
            get(export_integration_layer),
        )
        .route(
            &format!("{BASE}/adapters/{{adapter_key}}/layers/{{layer_id}}/sync-preview"),
            post(preview_integration_layer_sync),
        )
        .route(
            &format!("{BASE}/adapters/{{adapter_key}}/layers/{{layer_id}}/sync-push"),
            post(push_integration_layer_sync),
        )
}

fn export_enabled(key: &str) -> bool {
    matches!(key, "qgis" | "postgis" | "autocad")
}

fn sync_push_enabled(key: &str) -> bool {
    key == "qgis"
}

fn is_explicit_readable_crs(stored_crs_code: Option<i32>, validation_state: &str) -> bool {
    stored_crs_code
        .and_then(|c| u32::try_from(c).ok())
        .is_some_and(|c| validation_state == "ready" && lookup_readable_crs(c).is_some())
}

fn is_supported_export_geometry(family: &str) -> bool {
    matches!(family, "Point" | "LineString" | "Polygon")
}

fn is_wgs84_utm(code: i32) -> bool {
    (32601..=32660).contains(&code) || (32701..=32760).contains(&code)
}

fn safe_filename_slug(name: &str) -> String {
    let lower = ascii_fold(name.trim()).to_ascii_lowercase();
    let mut slug = String::with_capacity(lower.len());
    let mut last_dash = false;
    for ch in lower.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            slug.push(ch);
            last_dash = false;
        } else if !last_dash && !slug.is_empty() {
            slug.push('-');
            last_dash = true;
        }
    }
    let trimmed = slug.trim_matches(|c| c == '-' || c == '_');
    if trimmed.is_empty() {
        "layer".to_owned()
    } else {
        trimmed.chars().take(64).collect()
    }
}

fn is_hex64(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn format_distance_g(val: f64) -> String {
    if (val - val.trunc()).abs() < 1e-9 && val.abs() < 1e12 {
        format!("{}", val.trunc() as i64)
    } else {
        format!("{val}")
    }
}

fn worker_err(err: WorkerError) -> Failure {
    if err.status == StatusCode::UNPROCESSABLE_ENTITY {
        EditError::detail(422, err.message).into()
    } else if err.status == StatusCode::SERVICE_UNAVAILABLE {
        EditError::detail(503, err.message).into()
    } else {
        EditError::Internal(err.message).into()
    }
}

async fn export_integration_layer(
    State(state): State<AppState>,
    Path((org, proj, adapter_key, layer_id_raw)): Path<(String, String, String, String)>,
    request: Request,
) -> Result<Response, Failure> {
    let (mut parts, _) = request.into_parts();
    let actor = authenticate_actor(&state, &mut parts, &org).await?;

    let mut format = "geopackage".to_owned();
    let mut scope_area = Vec::<String>::new();
    let mut scope_route = Vec::<String>::new();
    let mut route_distance_m = 30.0_f64;

    for (k, v) in url::form_urlencoded::parse(parts.uri.query().unwrap_or("").as_bytes()) {
        match k.as_ref() {
            "format" => format = v.into_owned(),
            "scope_area" => scope_area.push(v.into_owned()),
            "scope_route" => scope_route.push(v.into_owned()),
            "route_distance_m" => {
                route_distance_m = v.parse::<f64>().map_err(|_| EditError::InvalidRequest)?;
            }
            _ => {}
        }
    }

    if !matches!(
        format.as_str(),
        "geopackage" | "geojson" | "postgis_sql" | "dxf"
    ) || scope_area.len() > 200
        || scope_route.len() > 500
        || !route_distance_m.is_finite()
        || !(1.0..=200.0).contains(&route_distance_m)
    {
        return Err(EditError::InvalidRequest.into());
    }

    let Ok(layer_id) = Uuid::parse_str(&layer_id_raw) else {
        return Err(EditError::InvalidRequest.into());
    };

    let Some(mut scope) =
        edit_auth::open(&state, &actor, &org, &proj, Guard::ReadScopedAllowed).await?
    else {
        return Err(EditError::InvalidRequest.into());
    };
    scope.authorize(Permission::Read).await?;

    let scope_filter: Option<ExportScopeSpec> = if !scope_area.is_empty() || !scope_route.is_empty()
    {
        if scope_area
            .iter()
            .chain(scope_route.iter())
            .any(|v| v.is_empty() || v.chars().count() > 128)
        {
            return Err(EditError::detail(422, "Invalid export scope identifier.").into());
        }
        Some(resolve_export_scope(&mut scope, &scope_area, &scope_route, route_distance_m).await?)
    } else {
        None
    };

    let Some(adapter) = get_adapter(&adapter_key) else {
        return Err(EditError::detail(404, "Integration adapter not found.").into());
    };
    if !export_enabled(adapter.key) {
        return Err(EditError::detail(
            409,
            "Read-only hub export is not enabled for this adapter.",
        )
        .into());
    }
    if adapter.key == "autocad" && format != "dxf" {
        return Err(EditError::detail(422, "AutoCAD adapter supports format=dxf only.").into());
    }

    let Some(layer_row) = load_active_layer_by_id(&mut scope, layer_id).await? else {
        return Err(EditError::detail(404, "Layer not found.").into());
    };

    if scope.is_scoped() {
        if !is_source_allowed(&mut scope, layer_row.source_id, "read").await? {
            return Err(EditError::detail(404, "Layer not found.").into());
        }
        if !is_source_allowed(&mut scope, layer_row.source_id, "export").await? {
            return Err(EditError::detail(403, "Export not permitted for this layer.").into());
        }
    }

    if !is_explicit_readable_crs(layer_row.stored_crs_code, &layer_row.validation_state) {
        return Err(EditError::detail(
            422,
            "Layer CRS is missing, unverified, or definition-only; explicit readable EPSG CRS is required.",
        )
        .into());
    }
    let stored_crs_code = layer_row
        .stored_crs_code
        .ok_or_else(|| EditError::detail(422, "Layer CRS is missing"))?;

    if !is_supported_export_geometry(&layer_row.geometry_family) {
        return Err(EditError::detail(
            422,
            "Only single-part 2D Point, LineString, and Polygon layers are supported for export.",
        )
        .into());
    }

    let mut output_crs_code = stored_crs_code;
    if format == "geojson" && stored_crs_code == 3857 {
        output_crs_code = 4326;
    }
    if format == "dxf" && stored_crs_code == 4326 {
        output_crs_code = utm_code_for_source(&mut scope, layer_row.source_id).await?;
    }

    if format == "geojson" && output_crs_code != 4326 {
        return Err(EditError::detail(
            422,
            "GeoJSON export requires EPSG:4326 or EPSG:3857; use geopackage or postgis_sql to preserve native EPSG coordinates without unverified reprojection.",
        )
        .into());
    }

    if format == "dxf" && !is_wgs84_utm(output_crs_code) {
        let is_metre = u32::try_from(output_crs_code)
            .ok()
            .and_then(lookup_readable_crs)
            .is_some_and(|p| p.unit == CrsUnit::Metre);
        if !is_metre {
            return Err(EditError::detail(
                422,
                "DXF export requires a projected metre CRS (EPSG:5899 or EPSG:3857); geographic degree CRS is not supported.",
            )
            .into());
        }
    }

    let field_specs = load_source_fields(&mut scope, layer_row.source_id).await?;
    let has_field = |name: &str| {
        field_specs
            .iter()
            .any(|f| f.source_name == name || f.field_id == name)
    };

    let area_mode: Option<&'static str> = scope_filter.as_ref().and_then(|sf| {
        if sf.area_fids.is_empty() {
            return None;
        }
        if sf.commune_sids.contains(&layer_row.source_id)
            && (!sf.area_codes.is_empty() || !sf.area_fids.is_empty())
        {
            Some("commune")
        } else if sf.hamlet_sids.contains(&layer_row.source_id) && !sf.area_names.is_empty() {
            Some("own_name")
        } else if !sf.area_sids.contains(&layer_row.source_id)
            && has_field("ap")
            && !sf.area_names.is_empty()
        {
            Some("attribute")
        } else {
            Some("spatial")
        }
    });

    let route_mode: Option<&'static str> = scope_filter.as_ref().and_then(|sf| {
        if sf.route_ids.is_empty() {
            return None;
        }
        if sf.route_sids.contains(&layer_row.source_id) {
            Some("self")
        } else if has_field("tuyen_id") {
            Some("attribute")
        } else {
            Some("distance")
        }
    });

    let features = load_export_features(
        &mut scope,
        layer_row.source_id,
        output_crs_code,
        stored_crs_code,
        &layer_row.geometry_family,
        scope_filter.as_ref(),
        area_mode,
        route_mode,
    )
    .await?;

    if scope_filter.is_some() && features.is_empty() {
        return Err(EditError::detail(422, "No features in the selected scope.").into());
    }
    if format == "dxf" && (features.is_empty() || features.len() > MAX_DXF_EXPORT_FEATURES) {
        return Err(
            EditError::detail(422, "DXF export requires between 1 and 50,000 features.").into(),
        );
    }

    let mappings_json: Vec<Value> = field_specs
        .iter()
        .map(|f| {
            json!({
                "field_id": f.field_id,
                "source_name": f.source_name,
                "output_name": f.output_name,
                "field_type": f.field_type,
            })
        })
        .collect();
    let feature_count = features.len();
    let artifact = run_export(json!({
        "action": "export",
        "format": format,
        "crs_code": output_crs_code,
        "geometry_family": layer_row.geometry_family,
        "layer_name": layer_row.name,
        "field_mappings": mappings_json,
        "features": features,
    }))
    .await
    .map_err(worker_err)?;

    record_layer_export_audit(
        &mut scope,
        layer_id,
        adapter.key,
        &format,
        output_crs_code,
        feature_count,
        &layer_row.content_revision,
        &layer_row.schema_revision,
        &artifact.sha256,
    )
    .await?;
    scope.commit().await?;

    let filename = format!(
        "{}-{}.{}",
        safe_filename_slug(&layer_row.name),
        adapter.key,
        artifact.ext
    );
    let mut headers = HeaderMap::new();
    let insert_hdr = |headers: &mut HeaderMap, name: HeaderName, val: &str| {
        if let Ok(hv) = HeaderValue::from_str(val) {
            headers.insert(name, hv);
        }
    };
    insert_hdr(&mut headers, header::CONTENT_TYPE, &artifact.media_type);
    insert_hdr(&mut headers, header::CACHE_CONTROL, "no-store");
    insert_hdr(
        &mut headers,
        header::CONTENT_DISPOSITION,
        &format!("attachment; filename=\"{filename}\""),
    );
    insert_hdr(
        &mut headers,
        HeaderName::from_static("x-content-sha256"),
        &artifact.sha256,
    );
    insert_hdr(
        &mut headers,
        HeaderName::from_static("x-openvietgis-crs"),
        &format!("EPSG:{output_crs_code}"),
    );
    insert_hdr(
        &mut headers,
        HeaderName::from_static("x-openvietgis-content-revision"),
        &layer_row.content_revision,
    );
    insert_hdr(
        &mut headers,
        HeaderName::from_static("x-openvietgis-schema-revision"),
        &layer_row.schema_revision,
    );
    insert_hdr(
        &mut headers,
        HeaderName::from_static("x-openvietgis-feature-count"),
        &feature_count.to_string(),
    );
    if format == "dxf" {
        insert_hdr(
            &mut headers,
            HeaderName::from_static("x-openvietgis-attribute-loss"),
            "true",
        );
    }
    if let Some(sf) = scope_filter {
        let mut parts = Vec::new();
        if let Some(m) = area_mode {
            parts.push(format!("area-{m}"));
        }
        if route_mode == Some("distance") {
            parts.push(format!(
                "route-distance-{}m",
                format_distance_g(sf.route_distance_m)
            ));
        } else if let Some(m) = route_mode.filter(|m| matches!(*m, "self" | "attribute")) {
            parts.push(format!("route-{m}"));
        }
        insert_hdr(
            &mut headers,
            HeaderName::from_static("x-openvietgis-scope"),
            &parts.join(","),
        );
    }

    Ok((StatusCode::OK, headers, artifact.bytes).into_response())
}

struct SyncMultipart<'a> {
    file: &'a [u8],
    base_content_revision: String,
    base_schema_revision: String,
    allow_deletes: bool,
    operation_id: Option<String>,
    expected_diff_sha256: Option<String>,
    summary: String,
}

fn parse_bool_field(raw: &str) -> Result<bool, EditError> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" | "" => Ok(false),
        _ => Err(EditError::InvalidRequest),
    }
}

fn parse_sync_multipart<'a>(
    content_type: &str,
    body: &'a [u8],
    require_push_fields: bool,
) -> Result<SyncMultipart<'a>, EditError> {
    let Some(parts) = multipart_parts(content_type, body) else {
        return Err(EditError::InvalidRequest);
    };
    let mut file_bytes: Option<&'a [u8]> = None;
    let mut fields: HashMap<String, String> = HashMap::new();
    for (name, filename, content) in parts {
        if name == "file" && filename.is_some() {
            file_bytes = Some(content);
        } else if let Ok(text) = std::str::from_utf8(content) {
            fields.insert(name, text.to_owned());
        } else {
            return Err(EditError::InvalidRequest);
        }
    }
    let Some(file) = file_bytes else {
        return Err(EditError::InvalidRequest);
    };
    if file.len() > MAX_SYNC_UPLOAD_BYTES {
        return Err(EditError::detail(
            413,
            "Uploaded GeoPackage exceeds maximum size of 16 MiB.",
        ));
    }
    if file.is_empty() {
        return Err(EditError::detail(422, "Uploaded GeoPackage file is empty."));
    }

    let base_content_revision = fields
        .remove("base_content_revision")
        .filter(|v| is_hex64(v))
        .ok_or(EditError::InvalidRequest)?;
    let base_schema_revision = fields
        .remove("base_schema_revision")
        .filter(|v| is_hex64(v))
        .ok_or(EditError::InvalidRequest)?;
    let allow_deletes = fields
        .remove("allow_deletes")
        .as_deref()
        .map(parse_bool_field)
        .transpose()?
        .unwrap_or(false);

    let summary = fields
        .remove("summary")
        .unwrap_or_else(|| "QGIS GeoPackage sync push".to_owned());
    if summary.chars().count() > 500 {
        return Err(EditError::InvalidRequest);
    }

    let (operation_id, expected_diff_sha256) = if require_push_fields {
        let op_id = fields
            .remove("operation_id")
            .filter(|v| !v.is_empty() && v.chars().count() <= 128)
            .ok_or(EditError::InvalidRequest)?;
        let diff_sha = fields
            .remove("expected_diff_sha256")
            .filter(|v| is_hex64(v))
            .ok_or(EditError::InvalidRequest)?;
        (Some(op_id), Some(diff_sha))
    } else {
        (None, None)
    };

    Ok(SyncMultipart {
        file,
        base_content_revision,
        base_schema_revision,
        allow_deletes,
        operation_id,
        expected_diff_sha256,
        summary,
    })
}

struct PreparedSync {
    adapter_key: String,
    layer_id: Uuid,
    source_id: Uuid,
    stored_crs_code: i32,
    geometry_family: String,
    current_content_revision: String,
    current_schema_revision: String,
    stale_revision: bool,
    diff: Value,
}

async fn prepare_sync_plan(
    scope: &mut ovg_storage::editing::EditScope<'_>,
    adapter_key: &str,
    layer_id: Uuid,
    parsed: &SyncMultipart<'_>,
) -> Result<PreparedSync, Failure> {
    let Some(adapter) = get_adapter(adapter_key) else {
        return Err(EditError::detail(404, "Integration adapter not found.").into());
    };
    if !sync_push_enabled(adapter.key) {
        return Err(EditError::detail(
            409,
            "GeoPackage sync push is not enabled for this adapter.",
        )
        .into());
    }

    let Some(layer_row) = load_active_layer_by_id(scope, layer_id).await? else {
        return Err(EditError::detail(404, "Layer not found.").into());
    };
    if scope.is_scoped() && !is_source_allowed(scope, layer_row.source_id, "read").await? {
        return Err(EditError::detail(404, "Layer not found.").into());
    }
    if !is_explicit_readable_crs(layer_row.stored_crs_code, &layer_row.validation_state) {
        return Err(EditError::detail(
            422,
            "Layer CRS is missing, unverified, or definition-only; explicit readable EPSG CRS is required.",
        )
        .into());
    }
    let stored_crs_code = layer_row
        .stored_crs_code
        .ok_or_else(|| EditError::detail(422, "Layer CRS is missing"))?;

    if !is_supported_export_geometry(&layer_row.geometry_family) {
        return Err(EditError::detail(
            422,
            "Only single-part 2D Point, LineString, and Polygon layers are supported for sync.",
        )
        .into());
    }

    let field_specs = load_source_fields(scope, layer_row.source_id).await?;
    let field_types: BTreeMap<String, String> = field_specs
        .into_iter()
        .map(|f| (f.field_id, f.field_type))
        .collect();
    let server_features = load_sync_server_features(scope, layer_row.source_id).await?;

    let diff = run_sync_diff(
        parsed.file,
        json!({
            "action": "sync_diff",
            "expected_crs_code": stored_crs_code,
            "expected_geometry_family": layer_row.geometry_family,
            "field_types": field_types,
            "server_features": server_features,
            "base_content_revision": parsed.base_content_revision,
            "base_schema_revision": parsed.base_schema_revision,
            "allow_deletes": parsed.allow_deletes,
        }),
    )
    .await
    .map_err(worker_err)?;

    let stale_revision = parsed.base_content_revision != layer_row.content_revision
        || parsed.base_schema_revision != layer_row.schema_revision;

    Ok(PreparedSync {
        adapter_key: adapter.key.to_owned(),
        layer_id,
        source_id: layer_row.source_id,
        stored_crs_code,
        geometry_family: layer_row.geometry_family,
        current_content_revision: layer_row.content_revision,
        current_schema_revision: layer_row.schema_revision,
        stale_revision,
        diff,
    })
}

async fn preview_integration_layer_sync(
    State(state): State<AppState>,
    Path((org, proj, adapter_key, layer_id_raw)): Path<(String, String, String, String)>,
    request: Request,
) -> Result<Response, Failure> {
    let (mut parts, body) = request.into_parts();
    let actor = authenticate_actor(&state, &mut parts, &org).await?;
    let content_type = parts
        .headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned();
    let raw = to_bytes(body, MULTIPART_BODY_CAP).await.map_err(|_| {
        EditError::detail(413, "Uploaded GeoPackage exceeds maximum size of 16 MiB.")
    })?;
    let parsed = parse_sync_multipart(&content_type, &raw, false)?;
    let Ok(layer_id) = Uuid::parse_str(&layer_id_raw) else {
        return Err(EditError::InvalidRequest.into());
    };

    let Some(mut scope) =
        edit_auth::open(&state, &actor, &org, &proj, Guard::ScopedAllowed).await?
    else {
        return Err(EditError::InvalidRequest.into());
    };
    require_csrf(&state, &actor, &parts.headers)?;
    scope.authorize(Permission::Edit).await?;

    let prepared = prepare_sync_plan(&mut scope, &adapter_key, layer_id, &parsed).await?;
    scope.rollback().await?;

    let create_count = prepared.diff["create_count"].as_u64().unwrap_or(0);
    let update_count = prepared.diff["update_count"].as_u64().unwrap_or(0);
    let delete_count = prepared.diff["delete_count"].as_u64().unwrap_or(0);
    let unchanged_count = prepared.diff["unchanged_count"].as_u64().unwrap_or(0);
    let omitted = prepared.diff["omitted_server_feature_ids"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let omitted_total = omitted.len();
    let omitted_preview: Vec<Value> = omitted.into_iter().take(MAX_SYNC_PREVIEW_ITEMS).collect();
    let ops_preview: Vec<Value> = prepared.diff["operations"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .take(MAX_SYNC_PREVIEW_ITEMS)
        .map(|op| {
            json!({
                "op_type": op["op_type"],
                "feature_id": op["feature_id"],
                "changed_fields": op["changed_fields"],
                "geometry_changed": op["geometry_changed"],
            })
        })
        .collect();
    let can_commit = !prepared.stale_revision && (create_count + update_count + delete_count) > 0;

    Ok((
        StatusCode::OK,
        [(header::CACHE_CONTROL, "no-store")],
        Json(json!({
            "schema_version": 1,
            "adapter_key": prepared.adapter_key,
            "layer_id": prepared.layer_id.to_string(),
            "source_id": prepared.source_id.to_string(),
            "stored_crs_code": prepared.stored_crs_code,
            "geometry_family": prepared.geometry_family,
            "base_content_revision": parsed.base_content_revision,
            "current_content_revision": prepared.current_content_revision,
            "base_schema_revision": parsed.base_schema_revision,
            "current_schema_revision": prepared.current_schema_revision,
            "stale_revision": prepared.stale_revision,
            "deletions_blocked": !parsed.allow_deletes,
            "create_count": create_count,
            "update_count": update_count,
            "delete_count": delete_count,
            "unchanged_count": unchanged_count,
            "omitted_server_feature_count": omitted_total,
            "omitted_server_feature_ids": omitted_preview,
            "diff_sha256": prepared.diff["diff_sha256"],
            "can_commit": can_commit,
            "operations": ops_preview,
        })),
    )
        .into_response())
}

async fn push_integration_layer_sync(
    State(state): State<AppState>,
    Path((org, proj, adapter_key, layer_id_raw)): Path<(String, String, String, String)>,
    request: Request,
) -> Result<Response, Failure> {
    let (mut parts, body) = request.into_parts();
    let actor = authenticate_actor(&state, &mut parts, &org).await?;
    let content_type = parts
        .headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned();
    let raw = to_bytes(body, MULTIPART_BODY_CAP).await.map_err(|_| {
        EditError::detail(413, "Uploaded GeoPackage exceeds maximum size of 16 MiB.")
    })?;
    let parsed = parse_sync_multipart(&content_type, &raw, true)?;
    let Ok(layer_id) = Uuid::parse_str(&layer_id_raw) else {
        return Err(EditError::InvalidRequest.into());
    };

    let Some(mut scope) =
        edit_auth::open(&state, &actor, &org, &proj, Guard::ScopedAllowed).await?
    else {
        return Err(EditError::InvalidRequest.into());
    };
    require_csrf(&state, &actor, &parts.headers)?;
    scope.authorize(Permission::Edit).await?;

    let operation_id = parsed
        .operation_id
        .clone()
        .ok_or(EditError::InvalidRequest)?;
    let expected_diff_sha256 = parsed
        .expected_diff_sha256
        .clone()
        .ok_or(EditError::InvalidRequest)?;
    if operation_id.trim().starts_with("field-sync-") {
        return Err(EditError::detail(
            422,
            "Reserved internal changeset operation_id prefix 'field-sync-' is not allowed.",
        )
        .into());
    }

    let prepared = prepare_sync_plan(&mut scope, &adapter_key, layer_id, &parsed).await?;
    if prepared.stale_revision {
        return Err(EditError::detail(
            409,
            "Layer revision changed since export; re-export and preview before syncing.",
        )
        .into());
    }
    let diff_sha256 = prepared.diff["diff_sha256"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    if diff_sha256 != expected_diff_sha256 {
        return Err(EditError::detail(
            409,
            "Uploaded GeoPackage diff does not match expected_diff_sha256; run sync-preview again.",
        )
        .into());
    }

    let raw_ops = prepared.diff["operations"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    if raw_ops.is_empty() {
        return Err(
            EditError::detail(409, "No material creates, updates, or deletes to sync.").into(),
        );
    }

    let mut edit_ops = Vec::with_capacity(raw_ops.len());
    for op in raw_ops {
        let op_type = match op["op_type"].as_str() {
            Some("create") => OpType::Create,
            Some("delete") => OpType::Delete,
            _ => OpType::Update,
        };
        let feature_id = op["feature_id"].as_str().unwrap_or_default().to_owned();
        let geometry = if op_type == OpType::Delete {
            None
        } else {
            op["geometry"].as_object().cloned()
        };
        let properties = if op_type == OpType::Delete {
            None
        } else {
            op["properties"].as_object().cloned()
        };
        edit_ops.push(EditOperation {
            op_type,
            feature_id,
            geometry,
            properties,
        });
    }

    let changeset = scope
        .commit_changeset(
            prepared.source_id,
            CommitRequest {
                operation_id,
                base_content_revision: parsed.base_content_revision,
                base_schema_revision: Some(parsed.base_schema_revision),
                summary: parsed.summary,
                operations: edit_ops,
            },
        )
        .await?;
    scope.commit().await?;

    let omitted_count = prepared.diff["omitted_server_feature_ids"]
        .as_array()
        .map_or(0, Vec::len);

    Ok((
        StatusCode::CREATED,
        [(header::CACHE_CONTROL, "no-store")],
        Json(json!({
            "changeset_id": changeset["id"],
            "source_id": changeset["source_id"],
            "layer_id": prepared.layer_id.to_string(),
            "operation_id": changeset["operation_id"],
            "resulting_content_revision": changeset["resulting_content_revision"],
            "resulting_catalog_revision": changeset["resulting_catalog_revision"],
            "operation_count": changeset["operation_count"],
            "create_count": prepared.diff["create_count"],
            "update_count": prepared.diff["update_count"],
            "delete_count": prepared.diff["delete_count"],
            "omitted_server_feature_count": omitted_count,
            "deletions_blocked": !parsed.allow_deletes,
            "diff_sha256": diff_sha256,
            "audit_event_id": changeset["audit_event_id"],
        })),
    )
        .into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_and_hex_helpers_match_python() {
        assert_eq!(safe_filename_slug("Điểm khảo sát #1"), "diem-khao-sat-1");
        assert_eq!(safe_filename_slug("!!!"), "layer");
        assert!(is_hex64(&"a".repeat(64)));
        assert!(!is_hex64(&"A".repeat(64)));
        assert_eq!(format_distance_g(30.0), "30");
        assert_eq!(format_distance_g(12.5), "12.5");
    }
}
