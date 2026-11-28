//! E05 revision-pinned map presentation routes: the FastAPI viewport and
//! display extent wire, plus native Mapbox vector tiles.

use axum::extract::rejection::{PathRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderName, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use ovg_core::AppError;
use ovg_storage::catalog::{ProjectRead, ProjectReadPolicy};
use ovg_storage::viewport::{ReadPins, ViewportBounds, MAX_TILE_ZOOM, MAX_VIEWPORT_FEATURES};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::identity::HumanSession;
use crate::tile_cache::{TileCacheKey, TileCacheLookup};
use crate::AppState;

pub(crate) fn router() -> Router<AppState> {
    const LAYER: &str = "/organizations/{organization_id}/projects/{project_id}/layers/{layer_id}";
    Router::new()
        .route(&format!("{LAYER}/viewport"), get(viewport))
        .route(&format!("{LAYER}/display-extent"), get(display_extent))
        .route(
            &format!("{LAYER}/tiles/{{z}}/{{x}}/{{tile}}"),
            get(vector_tile),
        )
}

type LayerPath = Result<Path<(Uuid, Uuid, Uuid)>, PathRejection>;
type TilePath = Result<Path<(Uuid, Uuid, Uuid, u32, u32, String)>, PathRejection>;

#[derive(Deserialize)]
struct PinQuery {
    content_revision: String,
    schema_revision: String,
}

#[derive(Deserialize)]
struct ViewportQuery {
    content_revision: String,
    schema_revision: String,
    west: f64,
    south: f64,
    east: f64,
    north: f64,
    #[serde(default = "default_viewport_limit")]
    limit: i64,
}

const fn default_viewport_limit() -> i64 {
    500
}

fn read_pins<'a>(
    content_revision: &'a str,
    schema_revision: &'a str,
) -> Result<ReadPins<'a>, RenderError> {
    let pins = ReadPins {
        content_revision,
        schema_revision,
    };
    pins.well_formed()
        .then_some(pins)
        .ok_or(RenderError::InvalidRequest)
}

async fn viewport(
    auth: HumanSession,
    State(state): State<AppState>,
    path: LayerPath,
    query: Result<Query<ViewportQuery>, QueryRejection>,
) -> Result<Response, RenderError> {
    let Path((organization_id, project_id, layer_id)) =
        path.map_err(|_| RenderError::InvalidRequest)?;
    let Query(query) = query.map_err(|_| RenderError::InvalidRequest)?;
    let pins = read_pins(&query.content_revision, &query.schema_revision)?;
    if !(1..=MAX_VIEWPORT_FEATURES).contains(&query.limit) {
        return Err(RenderError::InvalidRequest);
    }
    let bounds = ViewportBounds::new(query.west, query.south, query.east, query.north)
        .ok_or(RenderError::InvalidRequest)?;
    let mut read = project_read(&state, &auth, organization_id, project_id).await?;
    let body = read
        .viewport_json(layer_id, &pins, bounds, query.limit)
        .await?;
    read.commit().await?;
    Ok((
        [
            (header::CACHE_CONTROL, "no-store"),
            (header::CONTENT_TYPE, "application/json"),
        ],
        body,
    )
        .into_response())
}

async fn display_extent(
    auth: HumanSession,
    State(state): State<AppState>,
    path: LayerPath,
    query: Result<Query<PinQuery>, QueryRejection>,
) -> Result<Response, RenderError> {
    let Path((organization_id, project_id, layer_id)) =
        path.map_err(|_| RenderError::InvalidRequest)?;
    let Query(query) = query.map_err(|_| RenderError::InvalidRequest)?;
    let pins = read_pins(&query.content_revision, &query.schema_revision)?;
    let mut read = project_read(&state, &auth, organization_id, project_id).await?;
    let body = read.display_extent(layer_id, &pins).await?;
    read.commit().await?;
    Ok(no_store(body))
}

/// `GET .../layers/{layer_id}/tiles/{z}/{x}/{y}.mvt` (new in Rust, not parity).
async fn vector_tile(
    auth: HumanSession,
    State(state): State<AppState>,
    path: TilePath,
    query: Result<Query<PinQuery>, QueryRejection>,
) -> Result<Response, RenderError> {
    let Path((organization_id, project_id, layer_id, z, x, tile)) =
        path.map_err(|_| RenderError::InvalidRequest)?;
    let Query(query) = query.map_err(|_| RenderError::InvalidRequest)?;
    let pins = read_pins(&query.content_revision, &query.schema_revision)?;
    let y: u32 = tile
        .strip_suffix(".mvt")
        .and_then(|y| y.parse().ok())
        .ok_or(RenderError::InvalidRequest)?;
    if z > MAX_TILE_ZOOM || x >= 1 << z || y >= 1 << z {
        return Err(RenderError::InvalidRequest);
    }
    // Bound total time spent joining a flight, even if its leader stalls or
    // successive leaders fail. Do not bypass the same flight and stampede DB.
    let wait_deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(15);
    loop {
        // Authorization is intentionally opened before any cache lookup.  A
        // waiter releases its transaction, waits, then loops through this same
        // authorization path again before consuming the completed entry.
        let mut read = project_read(&state, &auth, organization_id, project_id).await?;
        let Some(cache_scope) = read.tile_cache_scope(layer_id, &pins).await? else {
            let (body, complete) = read.vector_tile(layer_id, &pins, z, x, y).await?;
            read.commit().await?;
            return Ok(tile_response(body.into(), complete));
        };
        let key = TileCacheKey::authorized(
            cache_scope,
            pins.content_revision,
            pins.schema_revision,
            z,
            x,
            y,
        );
        match state.tile_cache.lookup_or_reserve(key) {
            TileCacheLookup::Hit(value) => {
                read.commit().await?;
                return Ok(tile_response(value.body, value.complete));
            }
            TileCacheLookup::Wait(mut completion) => {
                read.commit().await?;
                if tokio::time::Instant::now() >= wait_deadline
                    || tokio::time::timeout_at(wait_deadline, completion.changed())
                        .await
                        .is_err()
                {
                    return Err(AppError::service_unavailable(
                        "TILE_CACHE_WAIT_TIMEOUT",
                        "tile generation is temporarily busy",
                    )
                    .into());
                }
            }
            TileCacheLookup::Bypass => {
                let (body, complete) = read.vector_tile(layer_id, &pins, z, x, y).await?;
                read.commit().await?;
                return Ok(tile_response(body.into(), complete));
            }
            TileCacheLookup::Leader(lease) => {
                let (body, complete) = read.vector_tile(layer_id, &pins, z, x, y).await?;
                read.commit().await?;
                let value = lease.finish(body, complete);
                return Ok(tile_response(value.body, value.complete));
            }
        }
    }
}

fn tile_response(body: axum::body::Bytes, complete: bool) -> Response {
    let headers = [
        (header::CONTENT_TYPE, "application/vnd.mapbox-vector-tile"),
        (header::CACHE_CONTROL, "private, no-store"),
        (
            HeaderName::from_static("x-openvietgis-tile-complete"),
            if complete { "true" } else { "false" },
        ),
    ];
    (headers, body).into_response()
}

fn no_store(body: Value) -> Response {
    ([(header::CACHE_CONTROL, "no-store")], Json(body)).into_response()
}

async fn project_read<'a>(
    state: &'a AppState,
    auth: &HumanSession,
    organization_id: Uuid,
    project_id: Uuid,
) -> Result<ProjectRead<'a>, AppError> {
    let pool = state.db_pool.as_ref().ok_or_else(|| {
        AppError::service_unavailable("DATABASE_UNAVAILABLE", "database unavailable")
    })?;
    // Feature reads stay open to GIS-scoped actors; row RLS narrows the features.
    ProjectRead::begin(
        pool,
        organization_id,
        project_id,
        auth.actor_id(),
        auth.session_id(),
        ProjectReadPolicy::LayerAcl,
    )
    .await
}

enum RenderError {
    InvalidRequest,
    Application(AppError),
}

impl From<AppError> for RenderError {
    fn from(error: AppError) -> Self {
        Self::Application(error)
    }
}

impl IntoResponse for RenderError {
    fn into_response(self) -> Response {
        let Self::Application(error) = self else {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({
                    "detail": [{"loc": [], "msg": "invalid request payload", "type": "value_error"}]
                })),
            )
                .into_response();
        };
        if let AppError::Internal { internal_detail } = &error {
            tracing::error!(detail = %internal_detail, "map render request failed");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response();
        }
        let status =
            StatusCode::from_u16(error.status_code()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        // FastAPI renders spatial domain errors as {code, detail}; tenancy ones as {detail}.
        let coded = matches!(
            error,
            AppError::Conflict { .. }
                | AppError::Validation { .. }
                | AppError::ServiceUnavailable { .. }
        );
        let body = if coded {
            json!({"code": error.code(), "detail": error.to_string()})
        } else {
            json!({"detail": error.to_string()})
        };
        (status, Json(body)).into_response()
    }
}
