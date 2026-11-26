//! E10 3D model vault and saved-scene read control plane.

use axum::body::Body;
use axum::extract::rejection::JsonRejection;
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use ovg_core::AppError;
use ovg_storage::catalog::ProjectReadPolicy;
use ovg_storage::three_d;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::catalog::{project_read, CatalogError};
use crate::identity::{require_csrf, HumanSession};
use crate::AppState;

const MODELS: &str = "/organizations/{organization_id}/projects/{project_id}/3d/models";
const SCENES: &str = "/organizations/{organization_id}/projects/{project_id}/3d/scenes";

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route(MODELS, get(list_models))
        .route(
            &format!("{MODELS}/{{model_id}}/placements"),
            get(model_history),
        )
        .route(
            &format!("{MODELS}/{{model_id}}/download"),
            get(download_model),
        )
        .route(SCENES, get(list_scenes).post(create_scene))
        .route(
            &format!("{SCENES}/{{scene_id}}/versions"),
            get(scene_history).post(revise_scene),
        )
        .route(
            &format!("{SCENES}/{{scene_id}}/versions/{{version}}"),
            get(open_scene),
        )
}
fn headers() -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    h.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    h
}
async fn list_models(
    State(state): State<AppState>,
    auth: HumanSession,
    Path((o, p)): Path<(Uuid, Uuid)>,
) -> Result<(HeaderMap, Json<Value>), ThreeDError> {
    let mut r = project_read(&state, &auth, o, p, ProjectReadPolicy::Unrestricted)
        .await
        .map_err(ThreeDError::Access)?;
    let v = three_d::list_models(&mut r)
        .await
        .map_err(ThreeDError::Domain)?;
    r.commit().await.map_err(ThreeDError::Domain)?;
    Ok((headers(), Json(v)))
}
async fn model_history(
    State(state): State<AppState>,
    auth: HumanSession,
    Path((o, p, id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<(HeaderMap, Json<Value>), ThreeDError> {
    let mut r = project_read(&state, &auth, o, p, ProjectReadPolicy::Unrestricted)
        .await
        .map_err(ThreeDError::Access)?;
    let v = three_d::model_history(&mut r, id)
        .await
        .map_err(ThreeDError::Domain)?;
    r.commit().await.map_err(ThreeDError::Domain)?;
    v.map(|v| (headers(), Json(v)))
        .ok_or(ThreeDError::NotFound("Model not found"))
}
fn python_quote_filename(filename: &str) -> String {
    let mut out = String::with_capacity(filename.len());
    for byte in filename.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(*byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(*byte));
        } else {
            use std::fmt::Write;
            write!(&mut out, "%{byte:02X}").expect("string write");
        }
    }
    out
}

async fn download_model(
    State(state): State<AppState>,
    auth: HumanSession,
    Path((o, p, id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<Response, ThreeDError> {
    let mut r = project_read(&state, &auth, o, p, ProjectReadPolicy::Unrestricted)
        .await
        .map_err(ThreeDError::Access)?;
    let binary = three_d::model_binary(&mut r, id)
        .await
        .map_err(ThreeDError::Domain)?;
    r.commit().await.map_err(ThreeDError::Domain)?;
    let Some(binary) = binary else {
        return Err(ThreeDError::NotFound("Model not found"));
    };
    let mut response = Response::new(Body::from(binary.payload));
    *response.status_mut() = StatusCode::OK;
    let response_headers = response.headers_mut();
    response_headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    response_headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    response_headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    response_headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!(
            "attachment; filename*=UTF-8''{}",
            python_quote_filename(&binary.filename)
        ))
        .map_err(|_| ThreeDError::Domain(AppError::internal("invalid model filename header")))?,
    );
    Ok(response)
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SceneModelPin {
    id: Uuid,
    version: i32,
    #[serde(default = "default_true")]
    visible: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SceneCamera {
    crs_code: i32,
    longitude: f64,
    latitude: f64,
    zoom: f64,
    bearing: f64,
    pitch: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
enum VerticalReference {
    Local,
    Orthometric,
    Ellipsoidal,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SceneDem {
    id: Uuid,
    vertical_reference: VerticalReference,
    reference_label: String,
    #[serde(default = "default_true")]
    visible: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SceneFootprint {
    layer_id: Uuid,
    height_field: String,
    #[serde(default = "default_true")]
    visible: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct SceneCreateRequest {
    name: String,
    #[serde(default)]
    models: Vec<SceneModelPin>,
    camera: SceneCamera,
    #[serde(default)]
    dem: Option<SceneDem>,
    #[serde(default)]
    footprint: Option<SceneFootprint>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct SceneReviseRequest {
    name: String,
    #[serde(default)]
    models: Vec<SceneModelPin>,
    camera: SceneCamera,
    #[serde(default)]
    dem: Option<SceneDem>,
    #[serde(default)]
    footprint: Option<SceneFootprint>,
    expected_version: i32,
}

const fn default_true() -> bool {
    true
}

fn vertical_wire(value: &VerticalReference) -> &'static str {
    match value {
        VerticalReference::Local => "local",
        VerticalReference::Orthometric => "orthometric",
        VerticalReference::Ellipsoidal => "ellipsoidal",
    }
}

fn scene_write(
    name: String,
    models: Vec<SceneModelPin>,
    camera: SceneCamera,
    mut dem: Option<SceneDem>,
    footprint: Option<SceneFootprint>,
) -> Result<three_d::SceneWrite, ThreeDError> {
    if name.is_empty() || name.chars().count() > 160 {
        return Err(ThreeDError::InvalidRequest);
    }
    let name = name.trim().to_owned();
    if name.is_empty() || (models.is_empty() && dem.is_none() && footprint.is_none()) {
        return Err(ThreeDError::InvalidRequest);
    }
    if models.len() > 8
        || models.iter().any(|pin| !(1..=50).contains(&pin.version))
        || models
            .iter()
            .map(|pin| pin.id)
            .collect::<std::collections::HashSet<_>>()
            .len()
            != models.len()
    {
        return Err(ThreeDError::InvalidRequest);
    }
    if camera.crs_code != 4326
        || !camera.longitude.is_finite()
        || !camera.latitude.is_finite()
        || !camera.zoom.is_finite()
        || !camera.bearing.is_finite()
        || !camera.pitch.is_finite()
        || !(-180.0..=180.0).contains(&camera.longitude)
        || !(-85.0..=85.0).contains(&camera.latitude)
        || !(0.0..=22.0).contains(&camera.zoom)
        || !(-180.0..=180.0).contains(&camera.bearing)
        || !(0.0..=75.0).contains(&camera.pitch)
    {
        return Err(ThreeDError::InvalidRequest);
    }
    if let Some(value) = &mut dem {
        if value.reference_label.is_empty() || value.reference_label.chars().count() > 120 {
            return Err(ThreeDError::InvalidRequest);
        }
        value.reference_label = value.reference_label.trim().to_owned();
        if value.reference_label.is_empty() {
            return Err(ThreeDError::InvalidRequest);
        }
    }
    if footprint.as_ref().is_some_and(|value| {
        value.height_field.is_empty() || value.height_field.chars().count() > 64
    }) {
        return Err(ThreeDError::InvalidRequest);
    }

    let profile = serde_json::json!({
        "models": models,
        "camera": camera,
        "dem": dem,
        "footprint": footprint,
    });
    let model_refs = profile["models"]
        .as_array()
        .expect("serialized model list")
        .iter()
        .map(|pin| {
            Ok(three_d::SceneModelPin {
                id: Uuid::parse_str(pin["id"].as_str().ok_or(ThreeDError::InvalidRequest)?)
                    .map_err(|_| ThreeDError::InvalidRequest)?,
                version: i32::try_from(pin["version"].as_i64().ok_or(ThreeDError::InvalidRequest)?)
                    .map_err(|_| ThreeDError::InvalidRequest)?,
            })
        })
        .collect::<Result<Vec<_>, ThreeDError>>()?;
    let dem_ref = dem.as_ref().map(|value| three_d::SceneDemRef {
        id: value.id,
        vertical_reference: vertical_wire(&value.vertical_reference).to_owned(),
        reference_label: value.reference_label.clone(),
    });
    let footprint_ref = footprint.as_ref().map(|value| three_d::SceneFootprintRef {
        layer_id: value.layer_id,
        height_field: value.height_field.clone(),
    });

    Ok(three_d::SceneWrite {
        name,
        profile,
        models: model_refs,
        dem: dem_ref,
        footprint: footprint_ref,
    })
}

async fn create_scene(
    State(state): State<AppState>,
    auth: HumanSession,
    request_headers: HeaderMap,
    Path((o, p)): Path<(Uuid, Uuid)>,
    body: Result<Json<SceneCreateRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<Value>), ThreeDError> {
    let Json(body) = body.map_err(|_| ThreeDError::InvalidRequest)?;
    require_csrf(&state, &auth, &request_headers)
        .map_err(|error| ThreeDError::Access(CatalogError::Identity(error)))?;
    let input = scene_write(
        body.name,
        body.models,
        body.camera,
        body.dem,
        body.footprint,
    )?;
    let mut edit = project_read(&state, &auth, o, p, ProjectReadPolicy::UnrestrictedEdit)
        .await
        .map_err(ThreeDError::Access)?;
    let value = three_d::save_scene(
        &mut edit,
        auth.actor_id(),
        auth.display_name(),
        Uuid::new_v4(),
        0,
        input,
    )
    .await
    .map_err(ThreeDError::SceneStore)?;
    edit.commit().await.map_err(ThreeDError::Domain)?;
    Ok((StatusCode::CREATED, Json(value)))
}

async fn revise_scene(
    State(state): State<AppState>,
    auth: HumanSession,
    request_headers: HeaderMap,
    Path((o, p, scene_id)): Path<(Uuid, Uuid, Uuid)>,
    body: Result<Json<SceneReviseRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<Value>), ThreeDError> {
    let Json(body) = body.map_err(|_| ThreeDError::InvalidRequest)?;
    if !(1..=50).contains(&body.expected_version) {
        return Err(ThreeDError::InvalidRequest);
    }
    require_csrf(&state, &auth, &request_headers)
        .map_err(|error| ThreeDError::Access(CatalogError::Identity(error)))?;
    let expected = body.expected_version;
    let input = scene_write(
        body.name,
        body.models,
        body.camera,
        body.dem,
        body.footprint,
    )?;
    let mut edit = project_read(&state, &auth, o, p, ProjectReadPolicy::UnrestrictedEdit)
        .await
        .map_err(ThreeDError::Access)?;
    let value = three_d::save_scene(
        &mut edit,
        auth.actor_id(),
        auth.display_name(),
        scene_id,
        expected,
        input,
    )
    .await
    .map_err(ThreeDError::SceneStore)?;
    edit.commit().await.map_err(ThreeDError::Domain)?;
    Ok((StatusCode::CREATED, Json(value)))
}

async fn open_scene(
    State(state): State<AppState>,
    auth: HumanSession,
    Path((o, p, scene_id, version)): Path<(Uuid, Uuid, Uuid, i32)>,
) -> Result<(HeaderMap, Json<Value>), ThreeDError> {
    let mut read = project_read(&state, &auth, o, p, ProjectReadPolicy::Unrestricted)
        .await
        .map_err(ThreeDError::Access)?;
    let value = three_d::open_scene(&mut read, scene_id, version)
        .await
        .map_err(ThreeDError::SceneStore)?;
    read.commit().await.map_err(ThreeDError::Domain)?;
    value
        .map(|value| (headers(), Json(value)))
        .ok_or(ThreeDError::NotFound("Scene version not found"))
}

async fn list_scenes(
    State(state): State<AppState>,
    auth: HumanSession,
    Path((o, p)): Path<(Uuid, Uuid)>,
) -> Result<(HeaderMap, Json<Value>), ThreeDError> {
    let mut r = project_read(&state, &auth, o, p, ProjectReadPolicy::Unrestricted)
        .await
        .map_err(ThreeDError::Access)?;
    let v = three_d::list_scenes(&mut r)
        .await
        .map_err(ThreeDError::Domain)?;
    r.commit().await.map_err(ThreeDError::Domain)?;
    Ok((headers(), Json(v)))
}
async fn scene_history(
    State(state): State<AppState>,
    auth: HumanSession,
    Path((o, p, id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<(HeaderMap, Json<Value>), ThreeDError> {
    let mut r = project_read(&state, &auth, o, p, ProjectReadPolicy::Unrestricted)
        .await
        .map_err(ThreeDError::Access)?;
    let v = three_d::scene_history(&mut r, id)
        .await
        .map_err(ThreeDError::Domain)?;
    r.commit().await.map_err(ThreeDError::Domain)?;
    v.map(|v| (headers(), Json(v)))
        .ok_or(ThreeDError::NotFound("Scene not found"))
}
enum ThreeDError {
    InvalidRequest,
    Access(CatalogError),
    Domain(AppError),
    SceneStore(three_d::SceneStoreError),
    NotFound(&'static str),
}
impl IntoResponse for ThreeDError {
    fn into_response(self) -> Response {
        match self {
            Self::InvalidRequest => CatalogError::InvalidRequest.into_response(),
            Self::Access(e) => e.into_response(),
            Self::SceneStore(error) => match error {
                three_d::SceneStoreError::Database => {
                    (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response()
                }
                three_d::SceneStoreError::NotFound => (
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({"detail":"Scene not found"})),
                )
                    .into_response(),
                three_d::SceneStoreError::Changed => (
                    StatusCode::CONFLICT,
                    Json(serde_json::json!({"detail":"Scene changed; reload"})),
                )
                    .into_response(),
                three_d::SceneStoreError::HistoryLimit => (
                    StatusCode::PAYLOAD_TOO_LARGE,
                    Json(serde_json::json!({"detail":"Scene history limit"})),
                )
                    .into_response(),
                three_d::SceneStoreError::ProjectQuota => (
                    StatusCode::PAYLOAD_TOO_LARGE,
                    Json(serde_json::json!({"detail":"Project scene quota exceeded"})),
                )
                    .into_response(),
                three_d::SceneStoreError::DemUnavailable => (
                    StatusCode::CONFLICT,
                    Json(serde_json::json!({"detail":"Scene DEM no longer available"})),
                )
                    .into_response(),
                three_d::SceneStoreError::HeightFieldUnavailable => (
                    StatusCode::CONFLICT,
                    Json(
                        serde_json::json!({"detail":"Scene GIS height field no longer available"}),
                    ),
                )
                    .into_response(),
                three_d::SceneStoreError::ModelUnavailable => (
                    StatusCode::CONFLICT,
                    Json(serde_json::json!({"detail":"Pinned model revision unavailable"})),
                )
                    .into_response(),
                three_d::SceneStoreError::ElevationMismatch => (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(
                        serde_json::json!({"detail":"Model and scene elevation references differ"}),
                    ),
                )
                    .into_response(),
            },
            Self::NotFound(d) => {
                (StatusCode::NOT_FOUND, Json(serde_json::json!({"detail":d}))).into_response()
            }
            Self::Domain(AppError::Internal { internal_detail }) => {
                tracing::error!(detail=%internal_detail,"3D request failed");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response()
            }
            Self::Domain(AppError::Conflict {
                code: "MODEL_INTEGRITY_FAILED",
                ..
            }) => (
                StatusCode::CONFLICT,
                Json(serde_json::json!({"detail":"Model integrity check failed"})),
            )
                .into_response(),
            Self::Domain(e) => {
                let s = StatusCode::from_u16(e.status_code())
                    .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
                (s, Json(e.to_envelope(None))).into_response()
            }
        }
    }
}
