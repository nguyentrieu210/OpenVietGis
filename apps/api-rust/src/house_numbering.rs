//! E15 Rust-native read-side for the official House Numbering module.

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use ovg_core::AppError;
use ovg_module_sdk::ModuleManifest;
use ovg_storage::catalog::{ProjectRead, ProjectReadPolicy};
use ovg_storage::house_numbering;
use ovg_storage::integration_hub::{self, EffectiveProjectLevel};
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

use crate::catalog::{project_read, CatalogError};
use crate::identity::{require_csrf, HumanSession};
use crate::AppState;

const BASE: &str = "/organizations/{organization_id}/projects/{project_id}/modules/house-numbering";

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route(&format!("{BASE}/rules"), get(list_rules).post(create_rule))
        .route(&format!("{BASE}/areas"), get(list_areas).post(create_area))
        .route(&format!("{BASE}/routes"), get(list_routes))
        .route(&format!("{BASE}/buildings"), get(list_buildings))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleCreateBody {
    version: String,
    #[serde(default = "draft_status")]
    status: String,
    #[serde(default)]
    authority_ref: Option<String>,
    #[serde(default = "default_true")]
    left_odd: bool,
    #[serde(default = "default_suffix")]
    suffix_style: String,
    #[serde(default = "default_distance")]
    max_distance_m: f64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AreaCreateBody {
    name: String,
    #[serde(default = "default_area_kind")]
    kind: String,
    #[serde(default)]
    parent_id: Option<Uuid>,
    #[serde(default)]
    source_id: Option<Uuid>,
    #[serde(default)]
    feature_id: Option<String>,
}

fn draft_status() -> String {
    "draft".to_owned()
}

fn default_true() -> bool {
    true
}

fn default_suffix() -> String {
    "letters".to_owned()
}

fn default_distance() -> f64 {
    150.0
}

fn default_area_kind() -> String {
    "current".to_owned()
}

#[derive(Debug, Deserialize)]
struct BuildingQuery {
    route_id: Option<Uuid>,
}

async fn module_scope<'a>(
    state: &'a AppState,
    auth: &HumanSession,
    organization_id: Uuid,
    project_id: Uuid,
    policy: ProjectReadPolicy,
) -> Result<ProjectRead<'a>, HouseNumberingError> {
    // The global Python project_access_guard does not whitelist module paths.
    // Unrestricted policies therefore preserve the scoped-GIS fail-closed behavior.
    let mut read = project_read(state, auth, organization_id, project_id, policy)
        .await
        .map_err(HouseNumberingError::Access)?;

    let active = house_numbering::active_manifest_contract(&mut read)
        .await
        .map_err(HouseNumberingError::Domain)?;
    let shipped = state
        .modules
        .list_manifests()
        .into_iter()
        .find(|manifest| manifest.extension_key == "house_numbering");

    let compatible = active
        .and_then(|(contract, enabled)| {
            serde_json::from_value::<ModuleManifest>(contract)
                .ok()
                .map(|stored| (stored, enabled))
        })
        .zip(shipped)
        .is_some_and(|((stored, enabled), shipped)| {
            shipped.runtime_contract_matches(&stored, enabled)
        });

    if !compatible {
        return Err(HouseNumberingError::Domain(AppError::not_found(
            "official module activation does not match shipped runtime",
        )));
    }
    Ok(read)
}

async fn module_read<'a>(
    state: &'a AppState,
    auth: &HumanSession,
    organization_id: Uuid,
    project_id: Uuid,
) -> Result<ProjectRead<'a>, HouseNumberingError> {
    module_scope(
        state,
        auth,
        organization_id,
        project_id,
        ProjectReadPolicy::Unrestricted,
    )
    .await
}

async fn module_edit<'a>(
    state: &'a AppState,
    auth: &HumanSession,
    organization_id: Uuid,
    project_id: Uuid,
) -> Result<ProjectRead<'a>, HouseNumberingError> {
    module_scope(
        state,
        auth,
        organization_id,
        project_id,
        ProjectReadPolicy::UnrestrictedEdit,
    )
    .await
}

async fn create_rule(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path((organization_id, project_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<RuleCreateBody>,
) -> Result<(StatusCode, Json<Value>), HouseNumberingError> {
    require_csrf(&state, &auth, &headers)
        .map_err(|error| HouseNumberingError::Access(CatalogError::Identity(error)))?;
    if body.version.is_empty()
        || body.version.chars().count() > 64
        || body
            .authority_ref
            .as_ref()
            .is_some_and(|value| value.chars().count() > 500)
        || !matches!(body.status.as_str(), "draft" | "authoritative")
        || !matches!(body.suffix_style.as_str(), "letters" | "dash")
        || !body.max_distance_m.is_finite()
        || !(body.max_distance_m > 0.0 && body.max_distance_m <= 5_000.0)
    {
        return Err(HouseNumberingError::InvalidRequest);
    }

    let mut edit = module_edit(&state, &auth, organization_id, project_id).await?;
    let level = integration_hub::catalog_context(&mut edit, auth.actor_id())
        .await
        .map_err(HouseNumberingError::Domain)?
        .level;
    if level != EffectiveProjectLevel::Manage {
        return Err(HouseNumberingError::Permission(
            "missing permission: project.manage",
        ));
    }

    let actor = house_numbering::AuditActor {
        id: auth.actor_id(),
        display_name: auth.display_name(),
    };
    let input = house_numbering::RuleCreateInput {
        version: &body.version,
        status: &body.status,
        authority_ref: body.authority_ref.as_deref(),
        left_odd: body.left_odd,
        suffix_style: &body.suffix_style,
        max_distance_m: body.max_distance_m,
    };
    let result = house_numbering::create_rule(&mut edit, actor, input)
        .await
        .map_err(HouseNumberingError::Domain)?;
    edit.commit().await.map_err(HouseNumberingError::Domain)?;
    Ok((StatusCode::CREATED, Json(result)))
}

async fn create_area(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path((organization_id, project_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<AreaCreateBody>,
) -> Result<(StatusCode, Json<Value>), HouseNumberingError> {
    require_csrf(&state, &auth, &headers)
        .map_err(|error| HouseNumberingError::Access(CatalogError::Identity(error)))?;
    if body.name.is_empty()
        || body.name.chars().count() > 200
        || !matches!(body.kind.as_str(), "current" | "historic" | "custom")
        || body
            .feature_id
            .as_ref()
            .is_some_and(|value| value.chars().count() > 128)
    {
        return Err(HouseNumberingError::InvalidRequest);
    }

    let mut edit = module_edit(&state, &auth, organization_id, project_id).await?;
    let actor = house_numbering::AuditActor {
        id: auth.actor_id(),
        display_name: auth.display_name(),
    };
    let input = house_numbering::AreaCreateInput {
        name: &body.name,
        kind: &body.kind,
        parent_id: body.parent_id,
        source_id: body.source_id,
        feature_id: body.feature_id.as_deref(),
    };
    let result = house_numbering::create_area(&mut edit, actor, input)
        .await
        .map_err(HouseNumberingError::Domain)?;
    edit.commit().await.map_err(HouseNumberingError::Domain)?;
    Ok((StatusCode::CREATED, Json(result)))
}

async fn list_rules(
    State(state): State<AppState>,
    auth: HumanSession,
    Path((organization_id, project_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>, HouseNumberingError> {
    let mut read = module_read(&state, &auth, organization_id, project_id).await?;
    let result = house_numbering::list_rules(&mut read)
        .await
        .map_err(HouseNumberingError::Domain)?;
    read.commit().await.map_err(HouseNumberingError::Domain)?;
    Ok(Json(result))
}

async fn list_areas(
    State(state): State<AppState>,
    auth: HumanSession,
    Path((organization_id, project_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>, HouseNumberingError> {
    let mut read = module_read(&state, &auth, organization_id, project_id).await?;
    let result = house_numbering::list_areas(&mut read)
        .await
        .map_err(HouseNumberingError::Domain)?;
    read.commit().await.map_err(HouseNumberingError::Domain)?;
    Ok(Json(result))
}

async fn list_routes(
    State(state): State<AppState>,
    auth: HumanSession,
    Path((organization_id, project_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>, HouseNumberingError> {
    let mut read = module_read(&state, &auth, organization_id, project_id).await?;
    let result = house_numbering::list_routes(&mut read)
        .await
        .map_err(HouseNumberingError::Domain)?;
    read.commit().await.map_err(HouseNumberingError::Domain)?;
    Ok(Json(result))
}

async fn list_buildings(
    State(state): State<AppState>,
    auth: HumanSession,
    Path((organization_id, project_id)): Path<(Uuid, Uuid)>,
    Query(query): Query<BuildingQuery>,
) -> Result<Json<Value>, HouseNumberingError> {
    let mut read = module_read(&state, &auth, organization_id, project_id).await?;
    let result = house_numbering::list_buildings(&mut read, query.route_id)
        .await
        .map_err(HouseNumberingError::Domain)?;
    read.commit().await.map_err(HouseNumberingError::Domain)?;
    Ok(Json(result))
}

enum HouseNumberingError {
    InvalidRequest,
    Permission(&'static str),
    Access(CatalogError),
    Domain(AppError),
}

impl IntoResponse for HouseNumberingError {
    fn into_response(self) -> Response {
        match self {
            Self::InvalidRequest => (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(serde_json::json!({
                    "detail": [{"loc": [], "msg": "invalid request payload", "type": "value_error"}]
                })),
            )
                .into_response(),
            Self::Permission(detail) => (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({"detail": detail})),
            )
                .into_response(),
            Self::Access(error) => error.into_response(),
            Self::Domain(AppError::Internal { internal_detail }) => {
                tracing::error!(detail = %internal_detail, "house-numbering request failed");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response()
            }
            Self::Domain(error) => {
                let status = StatusCode::from_u16(error.status_code())
                    .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
                (status, Json(error.to_envelope(None))).into_response()
            }
        }
    }
}
