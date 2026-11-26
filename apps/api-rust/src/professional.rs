//! E08 professional GIS routes (`/pro/geometry`, `/pro/topology`,
//! `/pro/schema`, `/pro/tiles/{layer_id}/{z}/{x}/{y}.mvt`).

use axum::extract::{Path, Query, Request, State};
use axum::http::{header, HeaderName, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use ovg_storage::editing::domain::py_strip;
use ovg_storage::editing::{EditError, Guard, Permission};
use ovg_storage::professional::{
    self, is_cad_op, GeometryParams, SchemaParams, Selection, TileResult, TopologyParams,
};
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

use crate::edit_auth::{authenticate_actor, authenticate_json, decode, open, require_csrf, Failure};
use crate::AppState;

const BASE: &str = "/organizations/{organization_id}/projects/{project_id}/pro";

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route(&format!("{BASE}/geometry"), post(geometry_preview))
        .route(&format!("{BASE}/topology"), post(topology))
        .route(&format!("{BASE}/schema"), post(schema_change))
        .route(
            &format!("{BASE}/tiles/{{layer_id}}/{{z}}/{{x}}/{{tile}}"),
            get(tile),
        )
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SelectionBody {
    layer_id: Uuid,
    content_revision: String,
    schema_revision: String,
    #[serde(default)]
    feature_ids: Vec<String>,
}

impl SelectionBody {
    fn validate(self) -> Result<Selection, EditError> {
        if !is_pin(&self.content_revision)
            || !is_pin(&self.schema_revision)
            || self.feature_ids.len() > 100
            || self.feature_ids.iter().any(|fid| !is_fid(fid))
        {
            return Err(EditError::InvalidRequest);
        }
        Ok(Selection {
            layer_id: self.layer_id,
            content_revision: self.content_revision,
            schema_revision: self.schema_revision,
            feature_ids: self.feature_ids,
        })
    }
}

fn is_pin(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn is_fid(s: &str) -> bool {
    (1..=128).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
}

fn is_field_id(s: &str) -> bool {
    let bytes = s.as_bytes();
    (1..=64).contains(&bytes.len())
        && bytes[0].is_ascii_alphabetic()
        && bytes[1..]
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b'_')
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GeometryBody {
    operation: String,
    primary: SelectionBody,
    #[serde(default)]
    secondary: Option<SelectionBody>,
    #[serde(default)]
    distance: f64,
    #[serde(default)]
    dx: f64,
    #[serde(default)]
    dy: f64,
    #[serde(default)]
    angle: f64,
    #[serde(default = "default_factor")]
    factor: f64,
    #[serde(default = "default_vertex")]
    vertex: i32,
}

const fn default_factor() -> f64 {
    1.0
}

const fn default_vertex() -> i32 {
    1
}

async fn geometry_preview(
    State(state): State<AppState>,
    Path((organization, project)): Path<(String, String)>,
    request: Request,
) -> Result<Json<Value>, Failure> {
    let (actor, headers, body) = authenticate_json(&state, request, &organization).await?;
    let scope = open(&state, &actor, &organization, &project, Guard::ScopedAllowed).await?;
    require_csrf(&state, &actor, &headers)?;
    let body: GeometryBody = decode(body)?;
    if !matches!(
        body.operation.as_str(),
        "move_batch"
            | "copy"
            | "offset"
            | "rotate"
            | "scale"
            | "stretch"
            | "extend"
            | "trim"
            | "fillet"
            | "intersection"
            | "union"
            | "difference"
            | "centroid"
            | "hull"
            | "simplify"
            | "nearest"
            | "spatial_join"
    ) || !body.distance.is_finite()
        || !(-100_000.0..=100_000.0).contains(&body.distance)
        || !body.dx.is_finite()
        || !(-100_000.0..=100_000.0).contains(&body.dx)
        || !body.dy.is_finite()
        || !(-100_000.0..=100_000.0).contains(&body.dy)
        || !body.angle.is_finite()
        || !(-360.0..=360.0).contains(&body.angle)
        || !body.factor.is_finite()
        || !(body.factor > 0.0 && body.factor <= 100.0)
        || !(0..=4096).contains(&body.vertex)
    {
        return Err(EditError::InvalidRequest.into());
    }
    let primary = body.primary.validate()?;
    let secondary = body.secondary.map(SelectionBody::validate).transpose()?;
    let Some(mut scope) = scope else {
        return Err(EditError::InvalidRequest.into());
    };
    let perm = if is_cad_op(&body.operation) {
        Permission::Edit
    } else {
        Permission::Read
    };
    scope.authorize(perm).await?;
    let result = professional::geometry_preview(
        &mut scope,
        GeometryParams {
            operation: body.operation,
            primary,
            secondary,
            distance: body.distance,
            dx: body.dx,
            dy: body.dy,
            angle: body.angle,
            factor: body.factor,
            vertex: body.vertex,
        },
    )
    .await?;
    scope.commit().await?;
    Ok(Json(result))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TopologyBody {
    check: String,
    primary: SelectionBody,
    #[serde(default)]
    secondary: Option<SelectionBody>,
}

async fn topology(
    State(state): State<AppState>,
    Path((organization, project)): Path<(String, String)>,
    request: Request,
) -> Result<Json<Value>, Failure> {
    let (actor, headers, body) = authenticate_json(&state, request, &organization).await?;
    let scope = open(&state, &actor, &organization, &project, Guard::ScopedDenied).await?;
    require_csrf(&state, &actor, &headers)?;
    let body: TopologyBody = decode(body)?;
    if !matches!(
        body.check.as_str(),
        "duplicate" | "invalid" | "overlap" | "dangling" | "outside" | "gap"
    ) {
        return Err(EditError::InvalidRequest.into());
    }
    let primary = body.primary.validate()?;
    let secondary = body.secondary.map(SelectionBody::validate).transpose()?;
    let Some(mut scope) = scope else {
        return Err(EditError::InvalidRequest.into());
    };
    scope.authorize(Permission::Read).await?;
    let result = professional::topology(
        &mut scope,
        TopologyParams {
            check: body.check,
            primary,
            secondary,
        },
    )
    .await?;
    scope.commit().await?;
    Ok(Json(result))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SchemaBody {
    primary: SelectionBody,
    action: String,
    field_id: String,
    name: String,
    #[serde(default = "default_field_type", rename = "type")]
    field_type: String,
}

fn default_field_type() -> String {
    "text".to_owned()
}

async fn schema_change(
    State(state): State<AppState>,
    Path((organization, project)): Path<(String, String)>,
    request: Request,
) -> Result<Json<Value>, Failure> {
    let (actor, headers, body) = authenticate_json(&state, request, &organization).await?;
    let scope = open(&state, &actor, &organization, &project, Guard::ScopedDenied).await?;
    require_csrf(&state, &actor, &headers)?;
    let body: SchemaBody = decode(body)?;
    let trimmed_name = py_strip(&body.name).to_owned();
    let name_len = trimmed_name.chars().count();
    if !matches!(body.action.as_str(), "add" | "rename")
        || !is_field_id(&body.field_id)
        || !(1..=200).contains(&name_len)
        || !matches!(
            body.field_type.as_str(),
            "text" | "decimal" | "integer" | "boolean" | "timestamp"
        )
    {
        return Err(EditError::InvalidRequest.into());
    }
    let primary = body.primary.validate()?;
    let Some(mut scope) = scope else {
        return Err(EditError::InvalidRequest.into());
    };
    scope.authorize(Permission::Edit).await?;
    let result = professional::schema_change(
        &mut scope,
        SchemaParams {
            primary,
            action: body.action,
            field_id: body.field_id,
            name: trimmed_name,
            field_type: body.field_type,
        },
    )
    .await?;
    scope.commit().await?;
    Ok(Json(result))
}

#[derive(Debug, Deserialize)]
struct TileQuery {
    content_revision: String,
    schema_revision: String,
}

async fn tile(
    State(state): State<AppState>,
    Path((organization, project, layer, z_raw, x_raw, tile_raw)): Path<(
        String,
        String,
        String,
        String,
        String,
        String,
    )>,
    request: Request,
) -> Result<Response, Failure> {
    let (mut parts, _) = request.into_parts();
    let actor = authenticate_actor(&state, &mut parts, &organization).await?;
    let scope = open(
        &state,
        &actor,
        &organization,
        &project,
        Guard::ReadScopedAllowed,
    )
    .await?;
    let (Some(mut scope), Ok(layer_id), Ok(z), Ok(x), Some(y)) = (
        scope,
        Uuid::parse_str(&layer),
        z_raw.parse::<i64>(),
        x_raw.parse::<i64>(),
        tile_raw
            .strip_suffix(".mvt")
            .and_then(|s| s.parse::<i64>().ok()),
    ) else {
        return Err(EditError::InvalidRequest.into());
    };
    let Query(query) = Query::<TileQuery>::try_from_uri(&parts.uri)
        .map_err(|_| Failure::Edit(EditError::InvalidRequest))?;
    if !is_pin(&query.content_revision) || !is_pin(&query.schema_revision) {
        return Err(EditError::InvalidRequest.into());
    }
    scope.authorize(Permission::Read).await?;
    let if_none_match = parts
        .headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);

    let outcome = professional::tile(
        &mut scope,
        layer_id,
        z,
        x,
        y,
        &query.content_revision,
        &query.schema_revision,
        if_none_match.as_deref(),
    )
    .await?;
    scope.commit().await?;

    let response = match outcome {
        TileResult::NotModified { etag } => {
            let mut resp = StatusCode::NOT_MODIFIED.into_response();
            let headers = resp.headers_mut();
            headers.insert(
                header::CACHE_CONTROL,
                "private, no-cache".parse().expect("valid header"),
            );
            if let Ok(val) = etag.parse() {
                headers.insert(header::ETAG, val);
            }
            headers.insert(
                header::VARY,
                "Cookie, Authorization".parse().expect("valid header"),
            );
            resp
        }
        TileResult::Data { bytes, etag } => {
            let mut resp = (StatusCode::OK, bytes).into_response();
            let headers = resp.headers_mut();
            headers.insert(
                header::CONTENT_TYPE,
                "application/vnd.mapbox-vector-tile"
                    .parse()
                    .expect("valid header"),
            );
            if let Some(etag) = etag {
                headers.insert(
                    header::CACHE_CONTROL,
                    "private, no-cache".parse().expect("valid header"),
                );
                if let Ok(val) = etag.parse() {
                    headers.insert(header::ETAG, val);
                }
                headers.insert(
                    HeaderName::from_static("vary"),
                    "Cookie, Authorization".parse().expect("valid header"),
                );
            } else {
                headers.insert(
                    header::CACHE_CONTROL,
                    "private, no-store".parse().expect("valid header"),
                );
            }
            resp
        }
    };
    Ok(response)
}
