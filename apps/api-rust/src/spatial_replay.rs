//! Staging-only polling of project-wide spatial changes.

use axum::extract::rejection::{PathRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderValue};
use axum::middleware;
use axum::response::Response;
use axum::routing::get;
use axum::{Json, Router};
use ovg_core::AppError;
use ovg_storage::catalog::ProjectReadPolicy;
use ovg_storage::e11_outbox::{SpatialCursor, SpatialReplay};
use serde::Deserialize;
use uuid::Uuid;

use crate::catalog::{project_read, CatalogError};
use crate::identity::HumanSession;
use crate::AppState;

pub(crate) fn router() -> Router<AppState> {
    if !cfg!(feature = "e11-candidate") {
        return Router::new();
    }

    Router::new()
        .route(
            "/organizations/{organization_id}/projects/{project_id}/collaboration/spatial-invalidations",
            get(replay),
        )
        .layer(middleware::map_response(no_store))
}

async fn no_store(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReplayQuery {
    epoch: Option<Uuid>,
    after: Option<i64>,
    limit: Option<usize>,
}

async fn replay(
    auth: HumanSession,
    State(state): State<AppState>,
    path: Result<Path<(Uuid, Uuid)>, PathRejection>,
    query: Result<Query<ReplayQuery>, QueryRejection>,
) -> Result<Json<SpatialReplay>, CatalogError> {
    let Path((organization, project)) = path.map_err(|_| CatalogError::InvalidRequest)?;
    let mut read = project_read(
        &state,
        &auth,
        organization,
        project,
        ProjectReadPolicy::Unrestricted,
    )
    .await?;
    let Query(query) = query.map_err(|_| CatalogError::InvalidRequest)?;
    let cursor = match (query.epoch, query.after) {
        (None, None) => None,
        (Some(epoch), Some(sequence)) => Some(SpatialCursor { epoch, sequence }),
        _ => return Err(CatalogError::spatial(AppError::invalid_cursor())),
    };
    let result = read
        .replay_spatial_invalidations(cursor, query.limit.unwrap_or(100))
        .await
        .map_err(CatalogError::spatial)?;
    read.commit().await?;
    Ok(Json(result))
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    use super::*;

    #[tokio::test]
    async fn stage_replay_route_is_opt_in_and_requires_a_human_session() {
        let state = AppState::new(crate::AppConfig::default(), None).unwrap();
        let uri = format!(
            "/api/v1/organizations/{}/projects/{}/collaboration/spatial-invalidations",
            Uuid::nil(),
            Uuid::nil()
        );
        let response = crate::build_router(state)
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        if cfg!(feature = "e11-candidate") {
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
            assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        } else {
            assert_eq!(response.status(), StatusCode::NOT_FOUND);
        }
    }
}
