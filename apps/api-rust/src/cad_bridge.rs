//! E10 Rust-native CAD/3D control plane entry points.
//!
//! Native CAD engines and the Windows V8i adapter stay replaceable. OpenVietGIS
//! owns the authenticated capability contract and project boundary in Rust.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use ovg_storage::cad_documents;
use ovg_storage::catalog::ProjectReadPolicy;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::catalog::{project_read, CatalogError};
use crate::identity::HumanSession;
use crate::AppState;

const V8I_BASE: &str = "/organizations/{organization_id}/projects/{project_id}/integrations/v8i";
const CAD_BASE: &str = "/organizations/{organization_id}/projects/{project_id}/cad/documents";

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route(
            &format!("{V8I_BASE}/capabilities"),
            get(read_v8i_capabilities),
        )
        .route(CAD_BASE, get(list_documents))
        .route(
            &format!("{CAD_BASE}/{{document_id}}/versions"),
            get(list_document_versions),
        )
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

enum CadReadError {
    Access(CatalogError),
    Domain(ovg_core::AppError),
    DocumentNotFound,
}

impl IntoResponse for CadReadError {
    fn into_response(self) -> Response {
        match self {
            Self::Access(error) => error.into_response(),
            Self::Domain(ovg_core::AppError::Internal { internal_detail }) => {
                tracing::error!(detail = %internal_detail, "CAD read request failed");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response()
            }
            Self::Domain(error) => {
                let status = StatusCode::from_u16(error.status_code())
                    .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
                (status, Json(error.to_envelope(None))).into_response()
            }
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
    // HumanSession rejects service credentials. Unrestricted project.read also
    // rejects GIS-scoped actors, matching the retained FastAPI boundary.
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
}
