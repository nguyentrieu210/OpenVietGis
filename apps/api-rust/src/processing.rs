//! E08 Rust-native processing control plane.
//!
//! The geometry engines remain replaceable OSS workers. This module owns the
//! authenticated OpenVietGIS API contract and algorithm registry.

use axum::extract::rejection::QueryRejection;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use ovg_core::AppError;
use ovg_storage::catalog::ProjectReadPolicy;
use ovg_storage::processing_jobs;
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

use crate::catalog::{project_read, CatalogError};
use crate::identity::{require_csrf, HumanSession};
use crate::AppState;

const BASE: &str = "/organizations/{organization_id}/projects/{project_id}/processing";

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route(&format!("{BASE}/algorithms"), get(list_algorithms))
        .route(&format!("{BASE}/jobs"), get(list_jobs))
        .route(&format!("{BASE}/jobs/{{job_id}}"), get(get_job))
        .route(&format!("{BASE}/jobs/{{job_id}}/cancel"), post(cancel_job))
}

fn algorithm_registry_wire() -> Value {
    serde_json::json!({ "items": ovg_processing::registry::algorithm_registry() })
}

async fn list_algorithms(
    State(state): State<AppState>,
    auth: HumanSession,
    Path((organization_id, project_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>, CatalogError> {
    // Match FastAPI context_for(...): this is an unrestricted project.read
    // contract, so GIS-scoped actors fail closed.
    let _scope = project_read(
        &state,
        &auth,
        organization_id,
        project_id,
        ProjectReadPolicy::Unrestricted,
    )
    .await?;

    Ok(Json(algorithm_registry_wire()))
}

#[derive(Debug, Deserialize)]
struct ListJobsQuery {
    #[serde(default = "default_job_limit")]
    limit: i64,
}

const fn default_job_limit() -> i64 {
    50
}

async fn list_jobs(
    State(state): State<AppState>,
    auth: HumanSession,
    Path((organization_id, project_id)): Path<(Uuid, Uuid)>,
    query: Result<Query<ListJobsQuery>, QueryRejection>,
) -> Result<Json<Value>, ProcessingError> {
    let Query(query) = query.map_err(|_| ProcessingError::Access(CatalogError::InvalidRequest))?;
    if !(1..=200).contains(&query.limit) {
        return Err(ProcessingError::Access(CatalogError::InvalidRequest));
    }
    let mut read = project_read(
        &state,
        &auth,
        organization_id,
        project_id,
        ProjectReadPolicy::Unrestricted,
    )
    .await
    .map_err(ProcessingError::Access)?;
    let result = processing_jobs::list_jobs(&mut read, query.limit)
        .await
        .map_err(ProcessingError::Domain)?;
    read.commit().await.map_err(ProcessingError::Domain)?;
    Ok(Json(result))
}

async fn get_job(
    State(state): State<AppState>,
    auth: HumanSession,
    Path((organization_id, project_id, job_id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<Json<Value>, ProcessingError> {
    let mut read = project_read(
        &state,
        &auth,
        organization_id,
        project_id,
        ProjectReadPolicy::Unrestricted,
    )
    .await
    .map_err(ProcessingError::Access)?;
    let result = processing_jobs::get_job(&mut read, job_id)
        .await
        .map_err(ProcessingError::Domain)?;
    read.commit().await.map_err(ProcessingError::Domain)?;
    Ok(Json(result))
}

async fn cancel_job(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path((organization_id, project_id, job_id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<Json<Value>, ProcessingError> {
    require_csrf(&state, &auth, &headers)
        .map_err(|error| ProcessingError::Access(CatalogError::Identity(error)))?;
    let mut write = project_read(
        &state,
        &auth,
        organization_id,
        project_id,
        ProjectReadPolicy::UnrestrictedEdit,
    )
    .await
    .map_err(ProcessingError::Access)?;
    let result = processing_jobs::cancel_job(&mut write, job_id)
        .await
        .map_err(ProcessingError::Domain)?;
    write.commit().await.map_err(ProcessingError::Domain)?;
    Ok(Json(result))
}

enum ProcessingError {
    Access(CatalogError),
    Domain(AppError),
}

impl IntoResponse for ProcessingError {
    fn into_response(self) -> Response {
        match self {
            Self::Access(error) => error.into_response(),
            Self::Domain(AppError::Internal { internal_detail }) => {
                tracing::error!(detail = %internal_detail, "processing request failed");
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_matches_the_frozen_python_contract() {
        let body = algorithm_registry_wire();
        let items = body["items"].as_array().unwrap();
        assert_eq!(items.len(), 3);
        assert_eq!(items[0]["id"], "buffer");
        assert_eq!(items[0]["parameters_schema"]["quad_segs"]["default"], 8);
        assert_eq!(items[1]["required_inputs"][1], "overlay_layer_id");
        assert_eq!(
            items[2]["parameters_schema"]["group_by_field"]["nullable"],
            true
        );
    }
}
