//! E02 write routes for spatial sources, layers, manual layers, and geometry preview.

use axum::body::to_bytes;
use axum::extract::{Path, Request, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::routing::{patch, post};
use axum::{Json, Router};
use ovg_core::AppError;
use ovg_storage::catalog::{
    GeometryPreviewInput, ManualFieldInput, ProjectRead, ProjectReadPolicy,
};
use ovg_storage::editing::domain::py_strip;
use ovg_storage::editing::{authenticate_service, EditError, ServiceAuth};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

use crate::catalog::CatalogError;
use crate::identity::{self, HumanSession};
use crate::AppState;

const MAX_BODY_BYTES: usize = 16 * 1024 * 1024;
const MAX_REVISION: i64 = 9_007_199_254_740_991;

pub(crate) fn router() -> Router<AppState> {
    const PROJECT: &str = "/organizations/{organization_id}/projects/{project_id}";
    Router::new()
        .route(&format!("{PROJECT}/spatial/sources"), post(register_source))
        .route(
            &format!("{PROJECT}/spatial/sources/{{source_id}}"),
            patch(update_source),
        )
        .route(
            &format!("{PROJECT}/spatial/sources/{{source_id}}/retire"),
            post(retire_source),
        )
        .route(&format!("{PROJECT}/layers"), post(create_layer))
        .route(
            &format!("{PROJECT}/layers/manual"),
            post(create_manual_layer),
        )
        .route(&format!("{PROJECT}/layers/{{layer_id}}"), patch(rename_layer))
        .route(
            &format!("{PROJECT}/layers/{{layer_id}}/retire"),
            post(retire_layer),
        )
        .route(
            &format!("{PROJECT}/layers/{{layer_id}}/geometry-preview"),
            post(preview_geometry),
        )
}

enum Actor {
    Human(HumanSession),
    Service(ServiceAuth),
}

impl Actor {
    fn display_name<'a>(&'a self, read: &'a ProjectRead<'_>) -> &'a str {
        match self {
            Self::Human(h) => h.display_name(),
            Self::Service(_) => read.service_display_name().unwrap_or(""),
        }
    }
}

fn json_content_type(headers: &HeaderMap) -> bool {
    let Some(value) = headers.get(header::CONTENT_TYPE) else {
        return true;
    };
    let value = value.to_str().unwrap_or("invalid");
    if value.is_empty() {
        return true;
    }
    let mime = value
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    mime.split_once('/').is_some_and(|(main, sub)| {
        main == "application" && (sub == "json" || sub.ends_with("+json"))
    })
}

async fn authenticate_json(
    state: &AppState,
    request: Request,
    organization: &str,
) -> Result<(Actor, HeaderMap, Value), CatalogError> {
    let (mut parts, body) = request.into_parts();
    let bytes = to_bytes(body, MAX_BODY_BYTES)
        .await
        .map_err(|_| CatalogError::InvalidRequest)?;
    if bytes.is_empty() || !json_content_type(&parts.headers) {
        return Err(CatalogError::InvalidRequest);
    }
    let raw_json: Value =
        serde_json::from_slice(&bytes).map_err(|_| CatalogError::InvalidRequest)?;

    let bearer = parts
        .headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .filter(|value| {
            value
                .get(..7)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("bearer "))
        })
        .map(|value| py_strip(&value[7..]));
    let actor = if let Some(raw) = bearer {
        let org = Uuid::parse_str(organization).map_err(|_| {
            CatalogError::Application(AppError::unauthorized("invalid service credential"))
        })?;
        let pool = state.db_pool.as_ref().ok_or_else(|| {
            CatalogError::Application(AppError::service_unavailable(
                "DATABASE_UNAVAILABLE",
                "database unavailable",
            ))
        })?;
        let svc = authenticate_service(pool, org, raw)
            .await
            .map_err(edit_to_catalog_error)?;
        Actor::Service(svc)
    } else {
        use axum::extract::FromRequestParts;
        let session = HumanSession::from_request_parts(&mut parts, state)
            .await
            .map_err(CatalogError::Identity)?;
        Actor::Human(session)
    };
    if let Actor::Human(session) = &actor {
        identity::require_csrf(state, session, &parts.headers).map_err(CatalogError::Identity)?;
    }
    Ok((actor, parts.headers, raw_json))
}

fn edit_to_catalog_error(err: EditError) -> CatalogError {
    match err {
        EditError::InvalidRequest => CatalogError::InvalidRequest,
        EditError::Detail(status, msg) => CatalogError::Detail(
            StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            msg,
        ),
        EditError::Internal(msg) => CatalogError::Application(AppError::internal(msg)),
        EditError::Spatial(status, code, msg) => CatalogError::Spatial(
            StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            code,
            msg.to_owned(),
        ),
        EditError::Platform {
            status,
            code,
            message,
            ..
        } => CatalogError::Spatial(
            StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            code,
            message,
        ),
    }
}

async fn open_scope<'a>(
    state: &'a AppState,
    actor: &Actor,
    organization_id: Uuid,
    project_id: Uuid,
    policy: ProjectReadPolicy,
) -> Result<ProjectRead<'a>, CatalogError> {
    let pool = state.db_pool.as_ref().ok_or_else(|| {
        CatalogError::Application(AppError::service_unavailable(
            "DATABASE_UNAVAILABLE",
            "database unavailable",
        ))
    })?;
    match actor {
        Actor::Human(h) => ProjectRead::begin(
            pool,
            organization_id,
            project_id,
            h.actor_id(),
            h.session_id(),
            policy,
        )
        .await
        .map_err(CatalogError::spatial),
        Actor::Service(s) => {
            ProjectRead::begin_service(pool, organization_id, project_id, s, policy)
                .await
                .map_err(CatalogError::spatial)
        }
    }
}

fn parse_uuid(raw: &str) -> Result<Uuid, CatalogError> {
    Uuid::parse_str(raw).map_err(|_| CatalogError::InvalidRequest)
}

fn decode<T: DeserializeOwned>(value: Value) -> Result<T, CatalogError> {
    serde_json::from_value(value).map_err(|_| CatalogError::InvalidRequest)
}

fn validate_safe_name(raw: &str) -> Result<String, CatalogError> {
    let cleaned = raw.trim();
    if cleaned.is_empty() || cleaned.chars().count() > 200 || cleaned.contains('\0') {
        return Err(CatalogError::InvalidRequest);
    }
    Ok(cleaned.to_owned())
}

fn validate_reference_key(raw: &str) -> Result<(), CatalogError> {
    if !(1..=100).contains(&raw.len())
        || !raw
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
    {
        return Err(CatalogError::InvalidRequest);
    }
    Ok(())
}

fn validate_revision(v: i64) -> Result<(), CatalogError> {
    if !(1..=MAX_REVISION).contains(&v) {
        return Err(CatalogError::InvalidRequest);
    }
    Ok(())
}

fn validate_hex64(raw: &str) -> bool {
    raw.len() == 64 && raw.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn validate_field_id(raw: &str) -> bool {
    let bytes = raw.as_bytes();
    (1..=64).contains(&bytes.len())
        && bytes[0].is_ascii_alphabetic()
        && bytes[1..]
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b'_')
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RegisterSourceBody {
    reference_key: String,
    name: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DeclaredCrsBody {
    authority: String,
    code: i64,
    coordinate_order: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateSourceBody {
    expected_revision: i64,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    declared_crs: Option<DeclaredCrsBody>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RetireBody {
    expected_revision: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateLayerBody {
    source_id: Uuid,
    name: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ManualFieldBody {
    field_id: String,
    name: String,
    #[serde(rename = "type")]
    field_type: String,
    nullable: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ManualLayerBody {
    operation_id: Uuid,
    name: String,
    geometry_family: String,
    #[serde(default)]
    fields: Vec<ManualFieldBody>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RenameLayerBody {
    expected_revision: i64,
    name: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GeometryPreviewBody {
    action: String,
    content_revision: String,
    schema_revision: String,
    feature_ids: Vec<String>,
    #[serde(default)]
    blade: Vec<Vec<f64>>,
}

async fn register_source(
    State(state): State<AppState>,
    Path((org, project)): Path<(String, String)>,
    request: Request,
) -> Result<(StatusCode, Json<Value>), CatalogError> {
    let (actor, _, raw) = authenticate_json(&state, request, &org).await?;
    let org_id = parse_uuid(&org)?;
    let proj_id = parse_uuid(&project)?;
    let body: RegisterSourceBody = decode(raw)?;
    validate_reference_key(&body.reference_key)?;
    let name = validate_safe_name(&body.name)?;

    let mut edit = open_scope(
        &state,
        &actor,
        org_id,
        proj_id,
        ProjectReadPolicy::UnrestrictedEdit,
    )
    .await?;
    let display = actor.display_name(&edit).to_owned();
    let result = edit
        .register_source(&body.reference_key, &name, &display)
        .await
        .map_err(CatalogError::spatial)?;
    edit.commit().await?;
    Ok((StatusCode::CREATED, Json(result)))
}

async fn update_source(
    State(state): State<AppState>,
    Path((org, project, source)): Path<(String, String, String)>,
    request: Request,
) -> Result<Json<Value>, CatalogError> {
    let (actor, _, raw) = authenticate_json(&state, request, &org).await?;
    let org_id = parse_uuid(&org)?;
    let proj_id = parse_uuid(&project)?;
    let source_id = parse_uuid(&source)?;

    let obj = raw.as_object().ok_or(CatalogError::InvalidRequest)?;
    let has_name = obj.contains_key("name");
    let has_crs = obj.contains_key("declared_crs");
    if (!has_name && !has_crs)
        || (has_name && obj["name"].is_null())
        || (has_crs && obj["declared_crs"].is_null())
    {
        return Err(CatalogError::InvalidRequest);
    }
    let body: UpdateSourceBody = decode(raw)?;
    validate_revision(body.expected_revision)?;
    let name = body.name.as_deref().map(validate_safe_name).transpose()?;
    let declared_crs = match body.declared_crs {
        Some(crs) => {
            if crs.authority != "EPSG"
                || crs.coordinate_order != "xy"
                || !(1..=i64::from(i32::MAX)).contains(&crs.code)
            {
                return Err(CatalogError::InvalidRequest);
            }
            Some(crs.code as i32)
        }
        None => None,
    };

    let mut edit = open_scope(
        &state,
        &actor,
        org_id,
        proj_id,
        ProjectReadPolicy::UnrestrictedEdit,
    )
    .await?;
    let display = actor.display_name(&edit).to_owned();
    let result = edit
        .update_source(
            source_id,
            body.expected_revision,
            name.as_deref(),
            declared_crs,
            &display,
        )
        .await
        .map_err(CatalogError::spatial)?;
    edit.commit().await?;
    Ok(Json(result))
}

async fn retire_source(
    State(state): State<AppState>,
    Path((org, project, source)): Path<(String, String, String)>,
    request: Request,
) -> Result<Json<Value>, CatalogError> {
    let (actor, _, raw) = authenticate_json(&state, request, &org).await?;
    let org_id = parse_uuid(&org)?;
    let proj_id = parse_uuid(&project)?;
    let source_id = parse_uuid(&source)?;
    let body: RetireBody = decode(raw)?;
    validate_revision(body.expected_revision)?;

    let mut edit = open_scope(
        &state,
        &actor,
        org_id,
        proj_id,
        ProjectReadPolicy::UnrestrictedManage,
    )
    .await?;
    let display = actor.display_name(&edit).to_owned();
    let result = edit
        .retire_source(source_id, body.expected_revision, &display)
        .await
        .map_err(CatalogError::spatial)?;
    edit.commit().await?;
    Ok(Json(result))
}

async fn create_layer(
    State(state): State<AppState>,
    Path((org, project)): Path<(String, String)>,
    request: Request,
) -> Result<(StatusCode, Json<Value>), CatalogError> {
    let (actor, _, raw) = authenticate_json(&state, request, &org).await?;
    let org_id = parse_uuid(&org)?;
    let proj_id = parse_uuid(&project)?;
    let body: CreateLayerBody = decode(raw)?;
    let name = validate_safe_name(&body.name)?;

    let mut edit = open_scope(
        &state,
        &actor,
        org_id,
        proj_id,
        ProjectReadPolicy::UnrestrictedEdit,
    )
    .await?;
    let display = actor.display_name(&edit).to_owned();
    let result = edit
        .create_layer(body.source_id, &name, &display)
        .await
        .map_err(CatalogError::spatial)?;
    edit.commit().await?;
    Ok((StatusCode::CREATED, Json(result)))
}

async fn create_manual_layer(
    State(state): State<AppState>,
    Path((org, project)): Path<(String, String)>,
    request: Request,
) -> Result<(StatusCode, Json<Value>), CatalogError> {
    let (actor, _, raw) = authenticate_json(&state, request, &org).await?;
    let org_id = parse_uuid(&org)?;
    let proj_id = parse_uuid(&project)?;
    let body: ManualLayerBody = decode(raw)?;
    let name = validate_safe_name(&body.name)?;
    if !matches!(
        body.geometry_family.as_str(),
        "Point" | "LineString" | "Polygon"
    ) || body.fields.len() > 100
    {
        return Err(CatalogError::InvalidRequest);
    }
    let mut fields = Vec::with_capacity(body.fields.len());
    for f in body.fields {
        if !validate_field_id(&f.field_id)
            || !matches!(
                f.field_type.as_str(),
                "text" | "integer" | "decimal" | "boolean" | "timestamp"
            )
        {
            return Err(CatalogError::InvalidRequest);
        }
        let fname = validate_safe_name(&f.name)?;
        fields.push(ManualFieldInput {
            field_id: f.field_id,
            name: fname,
            field_type: f.field_type,
            nullable: f.nullable,
        });
    }

    let mut edit = open_scope(
        &state,
        &actor,
        org_id,
        proj_id,
        ProjectReadPolicy::UnrestrictedEdit,
    )
    .await?;
    let display = actor.display_name(&edit).to_owned();
    let result = edit
        .create_manual_layer(
            body.operation_id,
            &name,
            &body.geometry_family,
            &fields,
            &display,
        )
        .await
        .map_err(CatalogError::spatial)?;
    edit.commit().await?;
    Ok((StatusCode::CREATED, Json(result)))
}

async fn rename_layer(
    State(state): State<AppState>,
    Path((org, project, layer)): Path<(String, String, String)>,
    request: Request,
) -> Result<Json<Value>, CatalogError> {
    let (actor, _, raw) = authenticate_json(&state, request, &org).await?;
    let org_id = parse_uuid(&org)?;
    let proj_id = parse_uuid(&project)?;
    let layer_id = parse_uuid(&layer)?;
    let body: RenameLayerBody = decode(raw)?;
    validate_revision(body.expected_revision)?;
    let name = validate_safe_name(&body.name)?;

    let mut edit = open_scope(
        &state,
        &actor,
        org_id,
        proj_id,
        ProjectReadPolicy::UnrestrictedEdit,
    )
    .await?;
    let display = actor.display_name(&edit).to_owned();
    let result = edit
        .rename_layer(layer_id, body.expected_revision, &name, &display)
        .await
        .map_err(CatalogError::spatial)?;
    edit.commit().await?;
    Ok(Json(result))
}

async fn retire_layer(
    State(state): State<AppState>,
    Path((org, project, layer)): Path<(String, String, String)>,
    request: Request,
) -> Result<Json<Value>, CatalogError> {
    let (actor, _, raw) = authenticate_json(&state, request, &org).await?;
    let org_id = parse_uuid(&org)?;
    let proj_id = parse_uuid(&project)?;
    let layer_id = parse_uuid(&layer)?;
    let body: RetireBody = decode(raw)?;
    validate_revision(body.expected_revision)?;

    let mut edit = open_scope(
        &state,
        &actor,
        org_id,
        proj_id,
        ProjectReadPolicy::UnrestrictedEdit,
    )
    .await?;
    let display = actor.display_name(&edit).to_owned();
    let result = edit
        .retire_layer(layer_id, body.expected_revision, &display)
        .await
        .map_err(CatalogError::spatial)?;
    edit.commit().await?;
    Ok(Json(result))
}

async fn preview_geometry(
    State(state): State<AppState>,
    Path((org, project, layer)): Path<(String, String, String)>,
    request: Request,
) -> Result<Json<Value>, CatalogError> {
    let (actor, _, raw) = authenticate_json(&state, request, &org).await?;
    let org_id = parse_uuid(&org)?;
    let proj_id = parse_uuid(&project)?;
    let layer_id = parse_uuid(&layer)?;
    let body: GeometryPreviewBody = decode(raw)?;
    if !matches!(body.action.as_str(), "split" | "merge")
        || !validate_hex64(&body.content_revision)
        || !validate_hex64(&body.schema_revision)
        || !(1..=16).contains(&body.feature_ids.len())
        || body.blade.len() > 64
    {
        return Err(CatalogError::InvalidRequest);
    }

    let mut edit = open_scope(
        &state,
        &actor,
        org_id,
        proj_id,
        ProjectReadPolicy::UnrestrictedEdit,
    )
    .await?;
    let result = edit
        .preview_geometry(
            layer_id,
            &GeometryPreviewInput {
                action: body.action,
                content_revision: body.content_revision,
                schema_revision: body.schema_revision,
                feature_ids: body.feature_ids,
                blade: body.blade,
            },
        )
        .await
        .map_err(CatalogError::spatial)?;
    edit.commit().await?;
    Ok(Json(result))
}
