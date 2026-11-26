//! Location inspection, numbering previews and preview application
//! (port of the `HouseNumberingService` numbering workflow).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::post;
use axum::{Json, Router};
use ovg_core::AppError;
use ovg_module_house_numbering::domain::{
    locate_on_route, suggest_number, Address, DomainError, Rule, SuggestOptions,
};
use ovg_module_house_numbering::planning::{
    audit_route_numbering, build_route_plan, find_tree_collisions, summarize_audit, NumberRow,
    Occupant, PointRow, RouteSnapshot, TargetRow,
};
use ovg_storage::catalog::ProjectRead;
use ovg_storage::house_numbering as store;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use super::{
    audit_actor, csrf, decode, module_edit, module_read, path_id, scope_ids, HnResult,
    HouseNumberingError, BASE,
};
use crate::identity::HumanSession;
use crate::AppState;

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route(&format!("{BASE}/inspect"), post(inspect))
        .route(&format!("{BASE}/previews"), post(preview))
        .route(
            &format!("{BASE}/previews/{{preview_id}}/apply"),
            post(apply_preview),
        )
}

/// Python lets `DomainError` escape as an unhandled 500.
fn domain(error: DomainError) -> AppError {
    AppError::internal(error.0)
}

fn domain_rule(rule: &Value) -> Result<Rule, AppError> {
    Rule::new(
        rule["version"].as_str().unwrap_or_default(),
        rule["left_odd"].as_bool().unwrap_or(true),
        rule["suffix_style"].as_str().unwrap_or_default(),
        rule["max_distance_m"].as_f64().unwrap_or(0.0),
        rule["authority_ref"]
            .as_str()
            .filter(|value| !value.is_empty()),
    )
    .map_err(domain)
}

/// Port of `_route_vertices`.
fn route_vertices(geometry: &Value) -> Result<Vec<(f64, f64)>, AppError> {
    let invalid = |message: &str| AppError::validation("ROUTE_GEOMETRY_INVALID", message);
    if geometry["type"].as_str() != Some("LineString") {
        return Err(invalid("route must resolve to LineString geometry"));
    }
    let coordinates = geometry["coordinates"]
        .as_array()
        .filter(|items| items.len() >= 2)
        .ok_or_else(|| invalid("route has insufficient vertices"))?;
    coordinates
        .iter()
        .map(|coordinate| {
            let pair = coordinate.as_array().filter(|pair| pair.len() >= 2);
            match pair.map(|pair| (pair[0].as_f64(), pair[1].as_f64())) {
                Some((Some(x), Some(y))) => Ok((x, y)),
                _ => Err(invalid("route contains invalid coordinates")),
            }
        })
        .collect()
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct InspectBody {
    x: f64,
    y: f64,
    crs_code: i64,
    route_id: Uuid,
    rule_id: Uuid,
    #[serde(default)]
    existing_building_id: Option<Uuid>,
}

async fn inspect(
    State(state): State<AppState>,
    auth: HumanSession,
    Path(params): Path<HashMap<String, String>>,
    body: Bytes,
) -> HnResult<Json<Value>> {
    let (org, project) = scope_ids(&params)?;
    let body: InspectBody = decode(&body)?;
    let crs_code = i32::try_from(body.crs_code).map_err(|_| HouseNumberingError::InvalidRequest)?;
    if crs_code <= 0 {
        return Err(HouseNumberingError::InvalidRequest);
    }
    let mut read = module_read(&state, &auth, org, project).await?;
    let result = inspect_location(&mut read, &body, crs_code).await?;
    read.commit().await?;
    Ok(Json(result))
}

async fn inspect_location(
    read: &mut ProjectRead<'_>,
    body: &InspectBody,
    crs_code: i32,
) -> Result<Value, AppError> {
    let route = store::route_geometry(read, body.route_id).await?;
    let rule = domain_rule(&store::get_rule(read, body.rule_id).await?)?;
    let context = store::inspection_context(read, body.existing_building_id, body.route_id).await?;
    let target = route.srid;
    let point = if crs_code == target {
        (body.x, body.y)
    } else {
        store::transform_point(read, body.x, body.y, crs_code, target).await?
    };
    let vertices = route_vertices(&route.geometry)?;
    let crs = format!("EPSG:{target}");
    let location = locate_on_route(point, &vertices, &crs).map_err(domain)?;
    let route_id = body.route_id.to_string();
    let Some(side) = location.side else {
        return Ok(json!({
            "route_id": route_id,
            "location": {
                "measure_m": location.measure,
                "distance_m": location.distance,
                "side": null,
                "crs": location.crs,
            },
            "suggestion": {
                "number": null,
                "reason": "ambiguous_route_side",
                "rule_version": rule.version,
            },
            "neighbors": {"before": null, "after": null},
            "context": context,
        }));
    };
    let rows = store::building_points(read, body.route_id, target).await?;
    let mut addresses = Vec::new();
    let mut located = Vec::new();
    let mut existing_address_id = None;
    for row in &rows {
        let candidate = locate_on_route((row.x, row.y), &vertices, &crs).map_err(domain)?;
        let Some(candidate_side) = candidate.side else {
            continue;
        };
        located.push((row, candidate.measure, candidate_side));
        if let Some(text) = row.number_text.as_deref().filter(|text| !text.is_empty()) {
            let kind = row.kind.as_deref().filter(|kind| !kind.is_empty());
            let state = kind
                .filter(|kind| {
                    matches!(
                        *kind,
                        "old"
                            | "certificate"
                            | "cad"
                            | "survey"
                            | "proposed"
                            | "submitted"
                            | "approved"
                            | "reserved"
                    )
                })
                .unwrap_or("proposed");
            let address_id = row.number_id.unwrap_or(row.id).to_string();
            addresses.push(
                Address::new(&address_id, text, candidate.measure, candidate_side, state)
                    .map_err(domain)?,
            );
            if body.existing_building_id == Some(row.id) {
                existing_address_id = Some(address_id);
            }
        }
    }
    for number in store::route_numbers(read, body.route_id).await? {
        if number.kind != "reserved" || number.building_id.is_some() {
            continue;
        }
        let (Some(number_side), Some(measure)) = (number.side.as_deref(), number.measure_m) else {
            continue;
        };
        addresses.push(
            Address::new(
                &number.id.to_string(),
                &number.number_text,
                measure,
                number_side,
                "reserved",
            )
            .map_err(domain)?,
        );
    }
    let suggestion = suggest_number(
        &addresses,
        location.measure,
        side,
        &rule,
        SuggestOptions {
            prefix: route.prefix.as_deref().unwrap_or(""),
            distance: location.distance,
            existing_id: existing_address_id.as_deref(),
            batch_before_first: false,
        },
    )
    .map_err(domain)?;
    let mut same_side: Vec<(&store::BuildingPoint, f64)> = located
        .iter()
        .filter(|(row, _, row_side)| *row_side == side && row.number_text.is_some())
        .map(|(row, measure, _)| (*row, *measure))
        .collect();
    same_side.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    let neighbor = |item: Option<&(&store::BuildingPoint, f64)>| match item {
        None => Value::Null,
        Some((row, measure)) => json!({
            "building_id": row.id.to_string(),
            "number": row.number_text,
            "measure_m": measure,
        }),
    };
    let before = same_side
        .iter()
        .rev()
        .find(|(_, measure)| *measure < location.measure);
    let after = same_side
        .iter()
        .find(|(_, measure)| *measure > location.measure);
    Ok(json!({
        "route_id": route_id,
        "location": {
            "measure_m": location.measure,
            "distance_m": location.distance,
            "side": side,
            "crs": location.crs,
        },
        "suggestion": {
            "number": suggestion.number,
            "reason": suggestion.reason,
            "rule_version": suggestion.rule_version,
        },
        "neighbors": {"before": neighbor(before), "after": neighbor(after)},
        "context": context,
    }))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PreviewBody {
    building_ids: Vec<Uuid>,
    rule_id: Uuid,
    operation_id: Uuid,
}

async fn preview(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path(params): Path<HashMap<String, String>>,
    body: Bytes,
) -> HnResult<(StatusCode, Json<Value>)> {
    csrf(&state, &auth, &headers)?;
    let (org, project) = scope_ids(&params)?;
    let body: PreviewBody = decode(&body)?;
    if !(1..=5000).contains(&body.building_ids.len()) {
        return Err(HouseNumberingError::InvalidRequest);
    }
    let unique: BTreeSet<&Uuid> = body.building_ids.iter().collect();
    if unique.len() != body.building_ids.len() {
        return Err(AppError::validation(
            "VALIDATION_ERROR",
            "numbering requires unique building IDs",
        )
        .into());
    }
    let mut edit = module_edit(&state, &auth, org, project).await?;
    let result = preview_assignment(&mut edit, &auth, &body).await?;
    edit.commit().await?;
    Ok((StatusCode::CREATED, Json(result)))
}

struct RoutePlan {
    route: store::RouteGeometry,
    snapshot: String,
    changes: Vec<ovg_module_house_numbering::planning::Change>,
}

async fn preview_assignment(
    edit: &mut ProjectRead<'_>,
    auth: &HumanSession,
    body: &PreviewBody,
) -> Result<Value, AppError> {
    let rule_row = store::get_rule(edit, body.rule_id).await?;
    let rule = domain_rule(&rule_row)?;
    let targets = store::numbering_targets_for(edit, &body.building_ids).await?;
    if targets.len() != body.building_ids.len() {
        return Err(AppError::not_found("building not found"));
    }
    let mut warnings: Vec<String> = Vec::new();
    let mut groups: BTreeMap<String, Vec<&store::NumberingTarget>> = BTreeMap::new();
    for target in &targets {
        match target.route_id {
            None => warnings.push(format!("building {}: no_route", target.id)),
            Some(route_id) => groups.entry(route_id.to_string()).or_default().push(target),
        }
    }
    let mut audits: Vec<Value> = Vec::new();
    let mut plans: Vec<RoutePlan> = Vec::new();
    let mut inspected: BTreeSet<String> = BTreeSet::new();
    for (route_key, group) in &groups {
        let route_id = Uuid::parse_str(route_key).map_err(|e| AppError::internal(e.to_string()))?;
        let snapshot = store::route_snapshot(edit, route_id).await?;
        let geometry = match store::route_geometry(edit, route_id).await {
            Ok(route) => route_vertices(&route.geometry).map(|vertices| (route, vertices)),
            Err(error) => Err(error),
        };
        let (route, vertices) = match geometry {
            Ok(value) => value,
            Err(AppError::NotFound { .. } | AppError::Validation { .. }) => {
                warnings.extend(
                    group
                        .iter()
                        .map(|b| format!("building {}: invalid_route_geometry", b.id)),
                );
                continue;
            }
            Err(other) => return Err(other),
        };
        let points: Vec<PointRow> = store::building_points(edit, route_id, route.srid)
            .await?
            .into_iter()
            .map(|row| PointRow {
                id: row.id.to_string(),
                number_id: row.number_id.map(|id| id.to_string()),
                number_text: row.number_text,
                kind: row.kind,
                x: row.x,
                y: row.y,
            })
            .collect();
        let numbers: Vec<NumberRow> = store::route_numbers(edit, route_id)
            .await?
            .into_iter()
            .map(|row| NumberRow {
                id: row.id.to_string(),
                building_id: row.building_id.map(|id| id.to_string()),
                number_text: row.number_text,
                kind: row.kind,
                side: row.side,
                measure_m: row.measure_m,
            })
            .collect();
        let target_rows: Vec<TargetRow> = group
            .iter()
            .map(|b| TargetRow {
                id: b.id.to_string(),
                state: b.state.clone(),
                eligibility: b.eligibility.clone(),
                protected_number: b.protected_number,
            })
            .collect();
        let snapshot_view = RouteSnapshot {
            route_id: route_key,
            srid: route.srid,
            prefix: route.prefix.as_deref().unwrap_or(""),
            vertices: &vertices,
        };
        let Ok((mut proposed, issues)) =
            build_route_plan(&snapshot_view, &rule, &points, &numbers, &target_rows)
        else {
            warnings.extend(
                group
                    .iter()
                    .map(|b| format!("building {}: invalid_route_or_number_data", b.id)),
            );
            continue;
        };
        if snapshot != store::route_snapshot(edit, route_id).await? {
            return Err(AppError::conflict(
                "STALE_PREVIEW",
                "route numbering changed during preview",
            ));
        }
        let current = store::route_geometry(edit, route_id).await?;
        if current.content_revision != route.content_revision || current.revision != route.revision
        {
            return Err(AppError::conflict(
                "STALE_PREVIEW",
                "route changed during preview",
            ));
        }
        let mut audit = audit_route_numbering(&snapshot_view, &rule, &points, &numbers, &proposed)
            .map_err(domain)?;
        audit["route_name"] = store::route_name(edit, route_id).await?;
        audit["cross_route_duplicates"] = json!([]);
        inspected.extend(numbers.iter().map(|n| n.number_text.clone()));
        inspected.extend(proposed.iter().map(|c| c.number.clone()));
        warnings.extend(issues);
        let blocking = audit["blocking_issues"].as_i64().unwrap_or(0);
        audits.push(audit);
        if blocking > 0 {
            warnings.push(format!(
                "route {route_key}: numbering_audit_blocked ({blocking} issues)"
            ));
            proposed.clear();
        }
        if !proposed.is_empty() {
            plans.push(RoutePlan {
                route,
                snapshot,
                changes: proposed,
            });
        }
    }
    let revisions: HashMap<String, i64> = targets
        .iter()
        .map(|t| (t.id.to_string(), t.revision))
        .collect();
    let mut tree_collisions: Vec<Value> = Vec::new();
    let mut affected: BTreeSet<String> = BTreeSet::new();
    if !inspected.is_empty() {
        let numbers: Vec<String> = inspected.iter().cloned().collect();
        let (parents, existing) = store::tree_occupancy(edit, &numbers).await?;
        let mut occupants: Vec<Occupant> = existing
            .into_iter()
            .map(|(route_id, number_text, building_id)| Occupant {
                route_id,
                number_text,
                building_id,
                origin: "existing",
            })
            .collect();
        occupants.extend(plans.iter().flat_map(|plan| {
            plan.changes.iter().map(|c| Occupant {
                route_id: c.route_id.clone(),
                number_text: c.number.clone(),
                building_id: Some(c.building_id.clone()),
                origin: "proposal",
            })
        }));
        tree_collisions = find_tree_collisions(&parents, &occupants);
        let audited: BTreeSet<String> = audits
            .iter()
            .filter_map(|a| a["route_id"].as_str().map(str::to_owned))
            .collect();
        for issue in &tree_collisions {
            for entry in issue["occupants"].as_array().into_iter().flatten() {
                if let Some(route) = entry["route_id"].as_str().filter(|r| audited.contains(*r)) {
                    affected.insert(route.to_owned());
                }
            }
        }
        for detail in &mut audits {
            let route = detail["route_id"].clone();
            let relevant: Vec<Value> = tree_collisions
                .iter()
                .filter(|issue| {
                    issue["occupants"]
                        .as_array()
                        .is_some_and(|items| items.iter().any(|e| e["route_id"] == route))
                })
                .cloned()
                .collect();
            if !relevant.is_empty() {
                let count = i64::try_from(relevant.len()).unwrap_or(i64::MAX);
                detail["cross_route_duplicates"] =
                    json!(relevant.into_iter().take(40).collect::<Vec<_>>());
                let blocking = detail["blocking_issues"].as_i64().unwrap_or(0);
                detail["blocking_issues"] = json!(blocking + count);
            }
        }
        warnings.extend(
            affected
                .iter()
                .map(|rid| format!("route {rid}: cross_route_number_duplicate")),
        );
    }
    let mut base = serde_json::Map::new();
    let mut changes: Vec<Value> = Vec::new();
    for plan in plans
        .iter()
        .filter(|plan| !affected.contains(&plan.route.id.to_string()))
    {
        let rid = plan.route.id;
        base.insert(format!("route:{rid}"), json!(plan.route.revision));
        base.insert(
            format!("route_geometry:{rid}"),
            plan.route.content_revision.clone(),
        );
        base.insert(format!("route_snapshot:{rid}"), json!(plan.snapshot));
        for change in &plan.changes {
            base.insert(
                format!("building:{}", change.building_id),
                json!(revisions.get(&change.building_id).copied().unwrap_or(0)),
            );
            changes.push(change.to_json());
        }
    }
    let mut result = store::create_preview(
        edit,
        super::audit_actor(auth),
        store::PreviewInput {
            operation_id: body.operation_id,
            rule_id: body.rule_id,
            base_revisions: &Value::Object(base),
            changes: &Value::Array(changes),
            warnings: &json!(warnings),
        },
    )
    .await?;
    let mut summary = summarize_audit(&audits);
    summary["cross_route_duplicate_count"] = json!(tree_collisions.len());
    summary["cross_route_duplicates"] =
        json!(tree_collisions.into_iter().take(40).collect::<Vec<_>>());
    result["audit"] = summary;
    Ok(result)
}

async fn apply_preview(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path(params): Path<HashMap<String, String>>,
) -> HnResult<Json<Value>> {
    csrf(&state, &auth, &headers)?;
    let (org, project) = scope_ids(&params)?;
    let preview_id = path_id(&params, "preview_id")?;
    let mut edit = module_edit(&state, &auth, org, project).await?;
    let tree_conflict = |parents: &store::RouteParents, rows: &[store::TreeOccupant]| {
        let occupants: Vec<Occupant> = rows
            .iter()
            .map(|(route_id, number_text, building_id, proposal)| Occupant {
                route_id: route_id.clone(),
                number_text: number_text.clone(),
                building_id: building_id.clone(),
                origin: if *proposal { "proposal" } else { "existing" },
            })
            .collect();
        find_tree_collisions(parents, &occupants)
            .iter()
            .any(|issue| {
                issue["occupants"]
                    .as_array()
                    .is_some_and(|items| items.iter().any(|o| o["origin"] == "proposal"))
            })
    };
    let result =
        store::apply_preview(&mut edit, audit_actor(&auth), preview_id, &tree_conflict).await?;
    edit.commit().await?;
    Ok(Json(result))
}
