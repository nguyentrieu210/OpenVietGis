//! E15 Rust-native read-side for the official House Numbering module.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use ovg_core::AppError;
use ovg_module_sdk::ModuleManifest;
use ovg_storage::catalog::{ProjectRead, ProjectReadPolicy};
use ovg_storage::house_numbering;
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

use crate::catalog::{project_read, CatalogError};
use crate::identity::HumanSession;
use crate::AppState;

const BASE: &str = "/organizations/{organization_id}/projects/{project_id}/modules/house-numbering";

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route(&format!("{BASE}/rules"), get(list_rules))
        .route(&format!("{BASE}/areas"), get(list_areas))
        .route(&format!("{BASE}/routes"), get(list_routes))
        .route(&format!("{BASE}/buildings"), get(list_buildings))
}

#[derive(Debug, Deserialize)]
struct BuildingQuery {
    route_id: Option<Uuid>,
}

async fn module_read<'a>(
    state: &'a AppState,
    auth: &HumanSession,
    organization_id: Uuid,
    project_id: Uuid,
) -> Result<ProjectRead<'a>, HouseNumberingError> {
    // The global Python project_access_guard does not whitelist module paths.
    // Unrestricted therefore preserves the scoped-GIS fail-closed behavior.
    let mut read = project_read(
        state,
        auth,
        organization_id,
        project_id,
        ProjectReadPolicy::Unrestricted,
    )
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
    Access(CatalogError),
    Domain(AppError),
}

impl IntoResponse for HouseNumberingError {
    fn into_response(self) -> Response {
        match self {
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
