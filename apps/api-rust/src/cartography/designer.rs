//! E06 Layout Designer CRUD and schema/geometry validation (`layout_designer.py`).

use std::collections::HashMap;

use axum::extract::rejection::{JsonRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, put};
use axum::{Json, Router};
use ovg_storage::cartography::{self as store, DesignerStoreError, DesignerWrite};
use ovg_storage::catalog::ProjectReadPolicy;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use super::{validate_hex_color, PaperSize};
use crate::catalog::{project_read, CatalogError};
use crate::identity::{require_csrf, HumanSession};
use crate::AppState;

const BASE: &str = "/organizations/{organization_id}/projects/{project_id}/cartography/designer";

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route(BASE, get(list_designer).post(create_designer))
        .route(
            &format!("{BASE}/{{layout_id}}"),
            put(update_designer).delete(delete_designer),
        )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
enum ElementKind {
    Map,
    Text,
    Legend,
    North,
    Scale,
    Rect,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Camera {
    lng: f64,
    lat: f64,
    zoom: f64,
    bearing: f64,
    reference_width: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Element {
    id: Uuid,
    kind: ElementKind,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    #[serde(default)]
    text: String,
    #[serde(default = "default_font_size")]
    font_size: f64,
    #[serde(default = "default_color")]
    color: String,
    #[serde(default = "default_true")]
    border: bool,
    #[serde(default)]
    grid: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    locked: bool,
    #[serde(default)]
    camera: Option<Camera>,
    #[serde(default)]
    map_id: Option<Uuid>,
}

const fn default_font_size() -> f64 {
    4.0
}

fn default_color() -> String {
    "#15383b".to_owned()
}

const fn is_false(value: &bool) -> bool {
    !*value
}

const fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Document {
    #[serde(default = "default_version")]
    version: i32,
    #[serde(default = "default_paper")]
    paper: PaperSize,
    #[serde(default = "default_true")]
    landscape: bool,
    #[serde(default = "default_dpi")]
    dpi: i32,
    items: Vec<Element>,
}

const fn default_version() -> i32 {
    1
}

const fn default_paper() -> PaperSize {
    PaperSize::A4
}

const fn default_dpi() -> i32 {
    150
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateDesignerRequest {
    name: String,
    document: Document,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateDesignerRequest {
    name: String,
    document: Document,
    revision: i32,
}

#[derive(Debug, Deserialize)]
struct DeleteDesignerQuery {
    revision: i32,
}

async fn list_designer(
    State(state): State<AppState>,
    auth: HumanSession,
    Path((organization_id, project_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>, DesignerError> {
    let mut read = project_read(
        &state,
        &auth,
        organization_id,
        project_id,
        ProjectReadPolicy::Unrestricted,
    )
    .await
    .map_err(DesignerError::Access)?;
    let result = store::list_designer_layouts(&mut read)
        .await
        .map_err(DesignerError::Store)?;
    read.commit()
        .await
        .map_err(|_| DesignerError::Store(DesignerStoreError::Database))?;
    Ok(Json(result))
}

async fn create_designer(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path((organization_id, project_id)): Path<(Uuid, Uuid)>,
    body: Result<Json<CreateDesignerRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<Value>), DesignerError> {
    let Json(body) = body.map_err(|_| DesignerError::InvalidRequest)?;
    require_csrf(&state, &auth, &headers)
        .map_err(|error| DesignerError::Access(CatalogError::Identity(error)))?;
    let input = validate_designer_write(body.name, &body.document)?;

    let mut edit = project_read(
        &state,
        &auth,
        organization_id,
        project_id,
        ProjectReadPolicy::UnrestrictedEdit,
    )
    .await
    .map_err(DesignerError::Access)?;
    let result =
        store::create_designer_layout(&mut edit, auth.actor_id(), auth.display_name(), input)
            .await
            .map_err(DesignerError::Store)?;
    edit.commit()
        .await
        .map_err(|_| DesignerError::Store(DesignerStoreError::Database))?;
    Ok((StatusCode::CREATED, Json(result)))
}

async fn update_designer(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path((organization_id, project_id, layout_id)): Path<(Uuid, Uuid, Uuid)>,
    body: Result<Json<UpdateDesignerRequest>, JsonRejection>,
) -> Result<Json<Value>, DesignerError> {
    let Json(body) = body.map_err(|_| DesignerError::InvalidRequest)?;
    if body.revision < 1 {
        return Err(DesignerError::InvalidRequest);
    }
    require_csrf(&state, &auth, &headers)
        .map_err(|error| DesignerError::Access(CatalogError::Identity(error)))?;
    let input = validate_designer_write(body.name, &body.document)?;

    let mut edit = project_read(
        &state,
        &auth,
        organization_id,
        project_id,
        ProjectReadPolicy::UnrestrictedEdit,
    )
    .await
    .map_err(DesignerError::Access)?;
    let result = store::update_designer_layout(
        &mut edit,
        auth.actor_id(),
        auth.display_name(),
        layout_id,
        body.revision,
        input,
    )
    .await
    .map_err(DesignerError::Store)?;
    edit.commit()
        .await
        .map_err(|_| DesignerError::Store(DesignerStoreError::Database))?;
    Ok(Json(result))
}

async fn delete_designer(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path((organization_id, project_id, layout_id)): Path<(Uuid, Uuid, Uuid)>,
    query: Result<Query<DeleteDesignerQuery>, QueryRejection>,
) -> Result<StatusCode, DesignerError> {
    let Query(query) = query.map_err(|_| DesignerError::InvalidRequest)?;
    require_csrf(&state, &auth, &headers)
        .map_err(|error| DesignerError::Access(CatalogError::Identity(error)))?;

    let mut edit = project_read(
        &state,
        &auth,
        organization_id,
        project_id,
        ProjectReadPolicy::UnrestrictedEdit,
    )
    .await
    .map_err(DesignerError::Access)?;
    store::delete_designer_layout(
        &mut edit,
        auth.actor_id(),
        auth.display_name(),
        layout_id,
        query.revision,
    )
    .await
    .map_err(DesignerError::Store)?;
    edit.commit()
        .await
        .map_err(|_| DesignerError::Store(DesignerStoreError::Database))?;
    Ok(StatusCode::NO_CONTENT)
}

fn validate_designer_write(name: String, doc: &Document) -> Result<DesignerWrite, DesignerError> {
    if name.is_empty() || name.chars().count() > 120 {
        return Err(DesignerError::InvalidRequest);
    }
    let trimmed = name.trim().to_owned();
    if trimmed.is_empty() {
        return Err(DesignerError::Http(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Layout name is required",
        ));
    }
    validate_document(doc).map_err(|_| DesignerError::InvalidRequest)?;
    let doc_json = serde_json::to_string(doc).map_err(|_| DesignerError::InvalidRequest)?;
    if doc_json.len() > 120_000 {
        return Err(DesignerError::InvalidRequest);
    }
    let document: Value =
        serde_json::from_str(&doc_json).map_err(|_| DesignerError::InvalidRequest)?;
    Ok(DesignerWrite {
        name: trimmed,
        document,
    })
}

fn paper_mm(paper: PaperSize, landscape: bool) -> (f64, f64) {
    let (w, h) = match paper {
        PaperSize::A0 => (841.0, 1189.0),
        PaperSize::A1 => (594.0, 841.0),
        PaperSize::A2 => (420.0, 594.0),
        PaperSize::A3 => (297.0, 420.0),
        PaperSize::A4 => (210.0, 297.0),
    };
    if landscape {
        (h, w)
    } else {
        (w, h)
    }
}

fn finite_in(value: f64, lo: f64, hi: f64) -> bool {
    value.is_finite() && (lo..=hi).contains(&value)
}

fn validate_document(doc: &Document) -> Result<(), ()> {
    if doc.version != 1
        || !matches!(doc.dpi, 150 | 300)
        || doc.items.is_empty()
        || doc.items.len() > 40
    {
        return Err(());
    }
    if !matches!(doc.paper, PaperSize::A3 | PaperSize::A4) && doc.dpi != 150 {
        return Err(());
    }

    let mut by_id = HashMap::with_capacity(doc.items.len());
    let mut map_count = 0usize;
    for item in &doc.items {
        if !finite_in(item.x, 0.0, 1200.0)
            || !finite_in(item.y, 0.0, 1200.0)
            || !finite_in(item.w, 5.0, 1200.0)
            || !finite_in(item.h, 5.0, 1200.0)
            || item.text.chars().count() > 500
            || !finite_in(item.font_size, 2.0, 40.0)
            || validate_hex_color(&item.color).is_err()
        {
            return Err(());
        }
        match (&item.camera, item.kind) {
            (Some(cam), ElementKind::Map) => {
                if !finite_in(cam.lng, -180.0, 180.0)
                    || !finite_in(cam.lat, -85.0, 85.0)
                    || !finite_in(cam.zoom, 0.0, 24.0)
                    || !finite_in(cam.bearing, -180.0, 180.0)
                    || !finite_in(cam.reference_width, 1.0, 100_000.0)
                {
                    return Err(());
                }
                map_count += 1;
            }
            (None, kind) if kind != ElementKind::Map => {}
            _ => return Err(()),
        }
        if item.map_id.is_some() && !matches!(item.kind, ElementKind::North | ElementKind::Scale) {
            return Err(());
        }
        if by_id.insert(item.id, item.kind).is_some() {
            return Err(());
        }
    }
    if !(1..=4).contains(&map_count) {
        return Err(());
    }
    let (paper_w, paper_h) = paper_mm(doc.paper, doc.landscape);
    for item in &doc.items {
        if item.x + item.w > paper_w + 0.0001 || item.y + item.h > paper_h + 0.0001 {
            return Err(());
        }
        if matches!(item.kind, ElementKind::Scale | ElementKind::North) {
            let Some(target) = item.map_id.and_then(|id| by_id.get(&id)) else {
                return Err(());
            };
            if *target != ElementKind::Map {
                return Err(());
            }
        }
    }
    Ok(())
}

enum DesignerError {
    InvalidRequest,
    Access(CatalogError),
    Store(DesignerStoreError),
    Http(StatusCode, &'static str),
}

impl IntoResponse for DesignerError {
    fn into_response(self) -> Response {
        match self {
            Self::InvalidRequest => CatalogError::InvalidRequest.into_response(),
            Self::Access(error) => error.into_response(),
            Self::Store(DesignerStoreError::Quota) => (
                StatusCode::PAYLOAD_TOO_LARGE,
                Json(serde_json::json!({"detail": "Maximum 50 layouts per project"})),
            )
                .into_response(),
            Self::Store(DesignerStoreError::Conflict) => (
                StatusCode::CONFLICT,
                Json(serde_json::json!({"detail": "Layout changed or removed; reload"})),
            )
                .into_response(),
            Self::Store(DesignerStoreError::Database) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response()
            }
            Self::Http(status, detail) => {
                (status, Json(serde_json::json!({"detail": detail}))).into_response()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn validates_multi_frame_designer_document_and_rejects_bad_geometry() {
        let map_a = Uuid::new_v4();
        let map_b = Uuid::new_v4();
        let valid = json!({
            "version": 1,
            "paper": "A4",
            "landscape": true,
            "dpi": 300,
            "items": [
                {
                    "id": map_a,
                    "kind": "map",
                    "x": 5, "y": 20, "w": 150, "h": 140,
                    "camera": {"lng": 106.66, "lat": 10.70, "zoom": 14, "bearing": 10, "reference_width": 900},
                    "grid": true
                },
                {
                    "id": map_b,
                    "kind": "map",
                    "x": 160, "y": 20, "w": 130, "h": 140,
                    "camera": {"lng": 106.66, "lat": 10.70, "zoom": 17, "bearing": 10, "reference_width": 900}
                },
                {
                    "id": Uuid::new_v4(),
                    "kind": "scale",
                    "x": 5, "y": 175, "w": 110, "h": 15,
                    "map_id": map_a
                },
                {
                    "id": Uuid::new_v4(),
                    "kind": "text",
                    "x": 5, "y": 3, "w": 280, "h": 15,
                    "text": "BẢN ĐỒ BÌNH HƯNG",
                    "font_size": 6
                }
            ]
        });
        let doc: Document = serde_json::from_value(valid.clone()).unwrap();
        assert!(validate_document(&doc).is_ok());

        let mut overflow = valid.clone();
        overflow["items"][0]["w"] = json!(300);
        assert!(validate_document(&serde_json::from_value(overflow).unwrap()).is_err());

        let mut bad_map = valid.clone();
        bad_map["items"][2]["map_id"] = json!(Uuid::new_v4());
        assert!(validate_document(&serde_json::from_value(bad_map).unwrap()).is_err());

        let mut bad_dpi = valid;
        bad_dpi["paper"] = json!("A2");
        assert!(validate_document(&serde_json::from_value(bad_dpi).unwrap()).is_err());
    }
}
