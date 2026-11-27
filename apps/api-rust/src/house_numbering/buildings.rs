//! Building registry, eligibility, plaque and lineage routes.

use std::collections::HashMap;
use std::fmt;

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, patch, post, put};
use axum::{Json, Router};
use ovg_storage::house_numbering as store;
use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::Value;
use uuid::Uuid;

use super::{
    audit_actor, csrf, decode, module_edit, module_manage, module_read, opt_text_len, path_id,
    query_positive, scope_ids, text_len, HnResult, HouseNumberingError, BASE,
};
use crate::identity::HumanSession;
use crate::AppState;

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route(
            &format!("{BASE}/buildings/lineage"),
            post(record_building_lineage),
        )
        .route(
            &format!("{BASE}/buildings/{{building_id}}"),
            put(update_building).delete(retire_building),
        )
        .route(
            &format!("{BASE}/buildings/{{building_id}}/eligibility"),
            patch(set_eligibility),
        )
        .route(
            &format!("{BASE}/buildings/{{building_id}}/plaque"),
            patch(set_plaque),
        )
        .route(
            &format!("{BASE}/buildings/{{building_id}}/lineage"),
            get(list_building_lineage),
        )
}

fn default_unknown() -> String {
    "unknown".to_owned()
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BuildingCreateBody {
    #[serde(default)]
    route_id: Option<Uuid>,
    #[serde(default)]
    area_id: Option<Uuid>,
    #[serde(default)]
    parcel_ref: Option<String>,
    source_id: Uuid,
    feature_id: String,
    #[serde(default = "default_unknown")]
    eligibility: String,
    #[serde(default)]
    ineligibility_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BuildingUpdateBody {
    expected_revision: i64,
    #[serde(default)]
    route_id: Option<Uuid>,
    #[serde(default)]
    area_id: Option<Uuid>,
    #[serde(default)]
    parcel_ref: Option<String>,
    source_id: Uuid,
    feature_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EligibilityBody {
    expected_revision: i64,
    eligibility: String,
    #[serde(default)]
    reason: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PlaqueBody {
    expected_revision: i64,
    status: String,
}

/// Ordered `dict[str, PositiveInt]`; keys are validated as UUIDs by the service.
#[derive(Debug)]
struct OrderedRevisions(Vec<(String, i64)>);

impl<'de> Deserialize<'de> for OrderedRevisions {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OrderedVisitor;
        impl<'de> Visitor<'de> for OrderedVisitor {
            type Value = OrderedRevisions;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("an object of positive integers")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut items = Vec::new();
                while let Some((key, value)) = map.next_entry::<String, i64>()? {
                    items.push((key, value));
                }
                Ok(OrderedRevisions(items))
            }
        }
        deserializer.deserialize_map(OrderedVisitor)
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LineageBody {
    relation_kind: String,
    operation_id: Uuid,
    predecessor_building_ids: Vec<Uuid>,
    successor_building_ids: Vec<Uuid>,
    expected_revisions: OrderedRevisions,
}

fn valid_eligibility(value: &str) -> bool {
    matches!(value, "unknown" | "eligible" | "ineligible")
}

pub(super) async fn create_building(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path(params): Path<HashMap<String, String>>,
    body: Bytes,
) -> HnResult<(StatusCode, Json<Value>)> {
    csrf(&state, &auth, &headers)?;
    let (org, project) = scope_ids(&params)?;
    let body: BuildingCreateBody = decode(&body)?;
    if !opt_text_len(body.parcel_ref.as_deref(), 200)
        || !text_len(&body.feature_id, 1, 128)
        || !valid_eligibility(&body.eligibility)
        || !opt_text_len(body.ineligibility_reason.as_deref(), 500)
    {
        return Err(HouseNumberingError::InvalidRequest);
    }
    let mut edit = module_edit(&state, &auth, org, project).await?;
    let result = store::create_building(
        &mut edit,
        audit_actor(&auth),
        store::BuildingCreateInput {
            route_id: body.route_id,
            area_id: body.area_id,
            parcel_ref: body.parcel_ref.as_deref(),
            source_id: body.source_id,
            feature_id: &body.feature_id,
            eligibility: &body.eligibility,
            ineligibility_reason: body.ineligibility_reason.as_deref(),
        },
    )
    .await?;
    edit.commit().await?;
    Ok((StatusCode::CREATED, Json(result)))
}

async fn update_building(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path(params): Path<HashMap<String, String>>,
    body: Bytes,
) -> HnResult<Json<Value>> {
    csrf(&state, &auth, &headers)?;
    let (org, project) = scope_ids(&params)?;
    let building_id = path_id(&params, "building_id")?;
    let body: BuildingUpdateBody = decode(&body)?;
    if body.expected_revision <= 0
        || !opt_text_len(body.parcel_ref.as_deref(), 200)
        || !text_len(&body.feature_id, 1, 128)
    {
        return Err(HouseNumberingError::InvalidRequest);
    }
    let mut edit = module_edit(&state, &auth, org, project).await?;
    let result = store::update_building(
        &mut edit,
        audit_actor(&auth),
        building_id,
        store::BuildingUpdateInput {
            expected_revision: body.expected_revision,
            route_id: body.route_id,
            area_id: body.area_id,
            parcel_ref: body.parcel_ref.as_deref(),
            source_id: body.source_id,
            feature_id: &body.feature_id,
        },
    )
    .await?;
    edit.commit().await?;
    Ok(Json(result))
}

async fn retire_building(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path(params): Path<HashMap<String, String>>,
    Query(query): Query<HashMap<String, String>>,
) -> HnResult<StatusCode> {
    csrf(&state, &auth, &headers)?;
    let (org, project) = scope_ids(&params)?;
    let building_id = path_id(&params, "building_id")?;
    let expected_revision = query_positive(&query, "expected_revision")?;
    let mut edit = module_edit(&state, &auth, org, project).await?;
    store::retire_building(
        &mut edit,
        audit_actor(&auth),
        building_id,
        expected_revision,
    )
    .await?;
    edit.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn set_eligibility(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path(params): Path<HashMap<String, String>>,
    body: Bytes,
) -> HnResult<Json<Value>> {
    csrf(&state, &auth, &headers)?;
    let (org, project) = scope_ids(&params)?;
    let building_id = path_id(&params, "building_id")?;
    let body: EligibilityBody = decode(&body)?;
    if body.expected_revision <= 0
        || !valid_eligibility(&body.eligibility)
        || !opt_text_len(body.reason.as_deref(), 500)
    {
        return Err(HouseNumberingError::InvalidRequest);
    }
    let mut edit = module_edit(&state, &auth, org, project).await?;
    let result = store::set_building_eligibility(
        &mut edit,
        audit_actor(&auth),
        building_id,
        body.expected_revision,
        &body.eligibility,
        body.reason.as_deref(),
    )
    .await?;
    edit.commit().await?;
    Ok(Json(result))
}

async fn set_plaque(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path(params): Path<HashMap<String, String>>,
    body: Bytes,
) -> HnResult<Json<Value>> {
    csrf(&state, &auth, &headers)?;
    let (org, project) = scope_ids(&params)?;
    let building_id = path_id(&params, "building_id")?;
    let body: PlaqueBody = decode(&body)?;
    if body.expected_revision <= 0
        || !matches!(
            body.status.as_str(),
            "not_planned" | "planned" | "installed" | "replaced" | "removed"
        )
    {
        return Err(HouseNumberingError::InvalidRequest);
    }
    let mut edit = module_manage(&state, &auth, org, project).await?;
    let result = store::set_plaque_status(
        &mut edit,
        audit_actor(&auth),
        building_id,
        body.expected_revision,
        &body.status,
    )
    .await?;
    edit.commit().await?;
    Ok(Json(result))
}

async fn record_building_lineage(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path(params): Path<HashMap<String, String>>,
    body: Bytes,
) -> HnResult<(StatusCode, Json<Value>)> {
    csrf(&state, &auth, &headers)?;
    let (org, project) = scope_ids(&params)?;
    let body: LineageBody = decode(&body)?;
    if !matches!(body.relation_kind.as_str(), "split" | "merge")
        || !(1..=100).contains(&body.predecessor_building_ids.len())
        || !(1..=100).contains(&body.successor_building_ids.len())
        || body
            .expected_revisions
            .0
            .iter()
            .any(|(_, value)| *value <= 0)
    {
        return Err(HouseNumberingError::InvalidRequest);
    }
    // The application service validates keys before opening the module scope.
    let expected_revisions = body
        .expected_revisions
        .0
        .iter()
        .map(|(key, value)| Uuid::parse_str(key).map(|id| (id, *value)))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| {
            HouseNumberingError::Domain(ovg_core::AppError::validation(
                "LINEAGE_REVISION_SET",
                "invalid lineage expected_revisions",
            ))
        })?;
    let mut edit = module_manage(&state, &auth, org, project).await?;
    let result = store::record_building_lineage(
        &mut edit,
        audit_actor(&auth),
        store::LineageInput {
            relation_kind: &body.relation_kind,
            operation_id: body.operation_id,
            predecessor_ids: body.predecessor_building_ids,
            successor_ids: body.successor_building_ids,
            expected_revisions,
        },
    )
    .await?;
    edit.commit().await?;
    Ok((StatusCode::CREATED, Json(result)))
}

async fn list_building_lineage(
    State(state): State<AppState>,
    auth: HumanSession,
    Path(params): Path<HashMap<String, String>>,
) -> HnResult<Json<Value>> {
    let (org, project) = scope_ids(&params)?;
    let building_id = path_id(&params, "building_id")?;
    let mut read = module_read(&state, &auth, org, project).await?;
    let result = store::list_building_lineage(&mut read, building_id).await?;
    read.commit().await?;
    Ok(Json(result))
}
