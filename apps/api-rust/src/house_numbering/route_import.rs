//! Route-tree import (preview/apply), staged geometry, address verification and
//! verified links (port of the `route-import/*` and `geometry-reviews/{id}/*` routes).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use ovg_core::AppError;
use ovg_module_house_numbering::address_review as review;
use ovg_module_house_numbering::assignment::{address_label, py_str, RouteIndex};
use ovg_module_house_numbering::canonical_prefix;
use ovg_module_house_numbering::route_import::{
    self as import, s, truthy, Junctions, Layer, Override, ParentPick, Row,
};
use ovg_storage::catalog::ProjectRead;
use ovg_storage::house_numbering as store;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use uuid::Uuid;

use super::hierarchy::query_int;
use super::{
    audit_actor, csrf, decode, module_edit, module_read, path_id, scope_ids, text_len, HnResult,
    HouseNumberingError, BASE,
};
use crate::identity::HumanSession;
use crate::AppState;

pub(super) fn router() -> Router<AppState> {
    let review_path = |tail: &str| format!("{BASE}/geometry-reviews/{{review_id}}{tail}");
    Router::new()
        .route(&format!("{BASE}/route-import/preview"), post(preview))
        .route(&format!("{BASE}/route-import/apply"), post(apply))
        .route(
            &format!("{BASE}/route-import/stage-geometry"),
            post(stage_geometry),
        )
        .route(&review_path("/addresses"), get(addresses))
        .route(&review_path("/addresses/batch"), post(address_batch))
        .route(
            &review_path("/addresses/{building_id}"),
            put(verify_address),
        )
        .route(
            &review_path("/verified-links"),
            get(links_preview).post(links_apply),
        )
}

// ---------------------------------------------------------------- request bodies

fn default_street() -> String {
    "street".to_owned()
}

fn default_forward() -> String {
    "forward".to_owned()
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LayerBody {
    source_id: Uuid,
    mapping: HashMap<String, String>,
    #[serde(default = "default_street")]
    default_type: String,
    #[serde(default = "default_forward")]
    direction_state: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OverrideBody {
    parent_code: String,
    prefix: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    route_type: Option<String>,
    #[serde(default)]
    reviewed: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ParentBody {
    task_id: String,
    source_id: Uuid,
    feature_id: String,
}

/// `RouteImportBody` plus the apply/stage fields; allowed keys are checked per route.
#[derive(Debug, Deserialize)]
struct ImportBody {
    layers: Vec<LayerBody>,
    #[serde(default)]
    overrides: HashMap<String, OverrideBody>,
    #[serde(default)]
    merge_groups: Vec<String>,
    #[serde(default)]
    parent_features: Vec<ParentBody>,
    #[serde(default)]
    preview_token: Option<String>,
    #[serde(default)]
    selected_keys: Option<Vec<String>>,
    #[serde(default)]
    reason: Option<String>,
}

const IMPORT_KEYS: [&str; 4] = ["layers", "overrides", "merge_groups", "parent_features"];

fn hex(value: &str, len: usize) -> bool {
    value.len() == len
        && value
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn unique<T: std::hash::Hash + Eq>(items: &[T]) -> bool {
    items.iter().collect::<HashSet<_>>().len() == items.len()
}

/// Decode a body whose model forbids extra fields beyond `extra`.
fn import_body(body: &[u8], extra: &[&str]) -> HnResult<ImportBody> {
    let raw: Value = decode(body)?;
    let known = raw.as_object().is_some_and(|map| {
        map.keys()
            .all(|k| IMPORT_KEYS.contains(&k.as_str()) || extra.contains(&k.as_str()))
    });
    if !known {
        return Err(HouseNumberingError::InvalidRequest);
    }
    let body: ImportBody =
        serde_json::from_value(raw).map_err(|_| HouseNumberingError::InvalidRequest)?;
    let mapping_keys = [
        "name",
        "code",
        "parent_code",
        "prefix",
        "route_type",
        "logical_code",
    ];
    let layers_ok = (1..=20).contains(&body.layers.len())
        && body.layers.iter().all(|l| {
            l.mapping.keys().all(|k| mapping_keys.contains(&k.as_str()))
                && matches!(l.default_type.as_str(), "street" | "alley")
                && matches!(l.direction_state.as_str(), "forward" | "reverse")
        });
    let overrides_ok = body.overrides.len() <= 5000
        && body.overrides.values().all(|o| {
            text_len(&o.parent_code, 0, 200)
                && text_len(&o.prefix, 0, 100)
                && o.name.as_deref().is_none_or(|n| text_len(n, 1, 200))
                && o.route_type
                    .as_deref()
                    .is_none_or(|t| matches!(t, "street" | "alley"))
        });
    let parents_ok = body.parent_features.len() <= 500
        && body
            .parent_features
            .iter()
            .all(|p| hex(&p.task_id, 32) && text_len(&p.feature_id, 1, 300));
    if !layers_ok || !overrides_ok || body.merge_groups.len() > 5000 || !parents_ok {
        return Err(HouseNumberingError::InvalidRequest);
    }
    Ok(body)
}

struct ImportInput {
    layers: Vec<Layer>,
    overrides: HashMap<String, Override>,
    merge_groups: Vec<String>,
    picks: Vec<ParentPick>,
}

fn import_input(body: &ImportBody) -> ImportInput {
    ImportInput {
        layers: body
            .layers
            .iter()
            .map(|l| Layer {
                source_id: l.source_id.to_string(),
                mapping: l
                    .mapping
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
                default_type: l.default_type.clone(),
                direction_state: l.direction_state.clone(),
            })
            .collect(),
        overrides: body
            .overrides
            .iter()
            .map(|(k, o)| {
                let value = Override {
                    parent_code: o.parent_code.clone(),
                    prefix: o.prefix.clone(),
                    name: o.name.clone(),
                    route_type: o.route_type.clone(),
                    reviewed: o.reviewed,
                };
                (k.clone(), value)
            })
            .collect(),
        merge_groups: body.merge_groups.clone(),
        picks: body
            .parent_features
            .iter()
            .map(|p| ParentPick {
                task_id: p.task_id.clone(),
                source_id: p.source_id.to_string(),
                feature_id: p.feature_id.clone(),
            })
            .collect(),
    }
}

// ---------------------------------------------------------------- preview

fn parse_id(value: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(value).map_err(|_| AppError::internal("house-numbering id is not a uuid"))
}

/// Owned `(source_id, feature_id, code)` of a row's segments.
fn segment_links(row: &Row) -> Vec<(Option<Uuid>, String, String)> {
    row.get("segments")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|seg| {
            (
                Uuid::parse_str(&py_str(&seg["source_id"])).ok(),
                py_str(&seg["feature_id"]),
                py_str(&seg["code"]),
            )
        })
        .collect()
}

/// `repo.validate_route_segments` in preview: `false` on a validation/conflict error.
async fn segments_valid(read: &mut ProjectRead<'_>, row: &Row) -> Result<bool, AppError> {
    let Ok(source) = Uuid::parse_str(s(row, "source_id")) else {
        return Ok(false);
    };
    let links = segment_links(row);
    if links.iter().any(|(sid, _, _)| sid.is_none()) {
        return Ok(false);
    }
    let segments: Vec<store::RouteSegmentInput<'_>> = links
        .iter()
        .map(|(sid, fid, code)| store::RouteSegmentInput {
            source_id: sid.unwrap_or_default(),
            feature_id: fid,
            code,
        })
        .collect();
    match store::check_route_segments(read, source, s(row, "feature_id"), &segments).await {
        Ok(()) => Ok(true),
        Err(AppError::Validation { .. } | AppError::Conflict { .. }) => Ok(false),
        Err(other) => Err(other),
    }
}

fn snapshot_of(stage: &Value) -> Map<String, Value> {
    stage["snapshot"].as_object().cloned().unwrap_or_default()
}

/// `_report` for one staged review.
async fn run_report(read: &mut ProjectRead<'_>, stage: &Value) -> Result<Value, AppError> {
    let snapshot = snapshot_of(stage);
    let scope = review::report_scope(&snapshot)?;
    let args = store::ReportArgs {
        review_id: parse_id(&py_str(&stage["id"]))?,
        refs: &scope.refs,
        codes: &scope.codes,
        parent: scope.parent.as_str(),
        parent_revision: scope.parent_revision.as_i64(),
        parent_geometry: scope.parent_geometry.as_str(),
    };
    if !store::report_current(read, &args).await? {
        return Ok(review::report_unavailable());
    }
    let rows = store::report_rows(read, &args).await?;
    Ok(review::report_items(&snapshot, &scope, &rows))
}

/// `apply_staged_reviews`.
async fn apply_staged(read: &mut ProjectRead<'_>, rows: &mut [Row]) -> Result<(), AppError> {
    if rows.iter().all(|r| truthy(r.get("existing_id"))) {
        return Ok(());
    }
    let stages = store::geometry_stages(read).await?;
    let candidates = review::stage_overlaps(rows, &stages);
    for stage in &stages {
        let Some(&i) = candidates.get(&py_str(&stage["route_code"])) else {
            continue;
        };
        if !review::stage_matches(&rows[i], stage) {
            review::mark_stage_stale(&mut rows[i]);
            continue;
        }
        let report = run_report(read, stage).await?;
        review::apply_stage_report(&mut rows[i], stage, &report);
    }
    Ok(())
}

fn limit_error(message: &str) -> AppError {
    AppError::validation("ROUTE_IMPORT_LIMIT", message)
}

/// `RouteTreeImporter.preview`.
async fn run_preview(read: &mut ProjectRead<'_>, input: &ImportInput) -> Result<Value, AppError> {
    let mut rows: Vec<Row> = Vec::new();
    let mut seen = HashSet::new();
    for layer in &input.layers {
        if !seen.insert(layer.source_id.clone()) {
            return Err(AppError::validation(
                "DUPLICATE_SOURCE",
                "Một lớp chỉ được chọn một lần",
            ));
        }
        let limit = i64::try_from(import::LIMIT + 1).unwrap_or(i64::MAX);
        let entries =
            store::route_import_features(read, parse_id(&layer.source_id)?, limit).await?;
        if entries.is_empty() {
            return Err(AppError::validation(
                "SOURCE_NOT_READY",
                "Lớp không có đối tượng hoặc chưa sẵn sàng",
            ));
        }
        if rows.len() + entries.len() > import::LIMIT {
            return Err(limit_error(&format!(
                "Tối đa {} tuyến mỗi đợt",
                import::LIMIT
            )));
        }
        rows.extend(import::layer_rows(layer, &entries));
    }
    let existing = store::list_routes(read).await?["items"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let mut merges = import::merge_suggestions(&rows, &existing);
    let specs = import::merge_specs(&merges);
    if !specs.is_empty() {
        let connected: HashSet<String> = store::merge_connected(read, specs)
            .await?
            .into_iter()
            .collect();
        import::merge_finish(&mut merges, &connected);
    }
    import::apply_merges(&mut rows, &existing, &merges, &input.merge_groups)?;
    if !input.picks.is_empty() {
        if rows.len() + input.picks.len() > import::LIMIT {
            return Err(limit_error("Quá giới hạn tuyến mỗi đợt"));
        }
        let mut parents = import::ParentFeatures::new(&rows, &existing);
        let mut picks = input.picks.clone();
        picks.sort_by(|a, b| a.task_id.cmp(&b.task_id));
        for pick in &picks {
            parents.check(pick)?;
            let feature =
                store::parent_feature(read, parse_id(&pick.source_id)?, &pick.feature_id).await?;
            let row = parents.row(pick, feature.as_ref())?;
            rows.push(row);
        }
        parents.finish(&mut rows, &input.picks)?;
    }
    let mut grouped = import::group_routes(&mut rows);
    import::apply_overrides(&mut grouped, &rows, &input.overrides)?;
    import::restore_reviewed(&mut grouped, &existing);
    let mut planned = import::plan_routes(grouped, &existing)?;
    import::review_flags(&mut planned);
    for row in planned.iter_mut() {
        // Explicit recovery groups already passed the batched merge geometry check.
        if import::needs_segment_check(row) && !segments_valid(read, row).await? {
            import::mark_invalid_segments(row);
        }
    }
    // Evidence is advisory until the operator explicitly confirms the row.
    let index = RouteIndex::new(existing.clone());
    let evidence = import::parent_evidence(&mut planned, &existing, &index);
    let mut junctions = Junctions::new();
    if !evidence.pairs.is_empty() {
        let measured = store::parent_distances(
            read,
            evidence.specs.clone(),
            &evidence.pairs,
            &evidence.existing_ids,
        )
        .await?;
        for (child, parent, revision, distance) in measured {
            if let Some(distance) = distance {
                junctions.insert((child, parent), (distance, revision));
            }
        }
    }
    import::finish_parent_evidence(&mut planned, evidence, &junctions);
    if let Some(codes) = import::survey_codes(&planned) {
        let observations = store::survey_observations(read, &codes).await?;
        import::survey_finish(&mut planned, &index, &observations);
    }
    apply_staged(read, &mut planned).await?;
    import::propagate_parent_errors(&mut planned);
    for row in planned.iter_mut() {
        let eligible = import::stage_eligible(row);
        row.insert("geometry_stage_eligible".into(), json!(eligible));
    }
    let tasks = import::parent_tasks(&planned, &existing);
    let count = |state: &str| planned.iter().filter(|r| s(r, "state") == state).count();
    let (ready, existing_count, errors) = (count("ready"), count("existing"), count("error"));
    let items: Vec<Value> = planned.into_iter().map(Value::Object).collect();
    let merges: Vec<Value> = merges.into_iter().map(Value::Object).collect();
    let token = import::py_digest(&json!({
        "items": items, "merge_options": merges, "missing_parents": tasks,
    }));
    Ok(json!({
        "preview_token": token,
        "items": items,
        "merge_options": merges,
        "missing_parents": tasks,
        "ready": ready,
        "existing": existing_count,
        "errors": errors,
    }))
}

async fn preview(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path(params): Path<HashMap<String, String>>,
    body: Bytes,
) -> HnResult<Json<Value>> {
    csrf(&state, &auth, &headers)?;
    let (org, project) = scope_ids(&params)?;
    let body = import_body(&body, &[])?;
    let input = import_input(&body);
    let mut edit = module_edit(&state, &auth, org, project).await?;
    let result = run_preview(&mut edit, &input).await?;
    edit.commit().await?;
    Ok(Json(result))
}

fn rows_of(plan: &Value) -> Vec<Row> {
    plan["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
        .cloned()
        .collect()
}

fn count_of(plan: &Value, key: &str) -> usize {
    usize::try_from(plan[key].as_u64().unwrap_or(0)).unwrap_or(0)
}

async fn apply(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path(params): Path<HashMap<String, String>>,
    body: Bytes,
) -> HnResult<(StatusCode, Json<Value>)> {
    csrf(&state, &auth, &headers)?;
    let (org, project) = scope_ids(&params)?;
    let body = import_body(&body, &["preview_token", "selected_keys"])?;
    let token = body.preview_token.clone().unwrap_or_default();
    if !hex(&token, 64)
        || body
            .selected_keys
            .as_ref()
            .is_some_and(|k| k.is_empty() || k.len() > 5000)
    {
        return Err(HouseNumberingError::InvalidRequest);
    }
    let input = import_input(&body);
    let mut edit = module_edit(&state, &auth, org, project).await?;
    store::project_lock(&mut edit, "route-import").await?;
    let plan = run_preview(&mut edit, &input).await?;
    if plan["preview_token"] != token.as_str() {
        return Err(AppError::conflict(
            "STALE_ROUTE_IMPORT",
            "Dữ liệu đã thay đổi, hãy xem trước lại",
        )
        .into());
    }
    let items = rows_of(&plan);
    let selected = import::selection_rows(
        &items,
        count_of(&plan, "errors"),
        body.selected_keys.as_deref(),
    )?;
    let actor = audit_actor(&auth);
    let mut ids: HashMap<String, Uuid> = HashMap::new();
    let mut created = 0_usize;
    for r in selected {
        let key = s(r, "key").to_owned();
        if truthy(r.get("existing_id")) {
            ids.insert(key, parse_id(s(r, "existing_id"))?);
            continue;
        }
        let parent = if truthy(r.get("parent_key")) {
            ids.get(s(r, "parent_key")).copied()
        } else if truthy(r.get("parent_id")) {
            Some(parse_id(s(r, "parent_id"))?)
        } else {
            None
        };
        let prefix = r
            .get("resolved_prefix")
            .and_then(Value::as_str)
            .map(canonical_prefix)
            .transpose()
            .map_err(|_| AppError::validation("INVALID_PREFIX", "invalid alley prefix"))?;
        let links = segment_links(r);
        let segments = links
            .iter()
            .map(|(sid, fid, code)| store::RouteSegmentInput {
                source_id: sid.unwrap_or_default(),
                feature_id: fid,
                code,
            })
            .collect();
        let label = format!("route-tree-import:{}", s(r, "code"));
        let route = store::create_route(
            &mut edit,
            actor,
            store::RouteCreateInput {
                name: s(r, "name"),
                route_type: s(r, "route_type"),
                parent_route_id: parent,
                prefix: prefix.as_deref(),
                source_id: parse_id(s(r, "source_id"))?,
                feature_id: s(r, "feature_id"),
                direction_state: s(r, "direction_state"),
                source_label: &label,
                status: "draft",
                segments,
            },
        )
        .await?;
        ids.insert(key, parse_id(&py_str(&route["id"]))?);
        created += 1;
    }
    edit.commit().await?;
    let remaining = count_of(&plan, "ready") + count_of(&plan, "errors") - created;
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "created": created,
            "existing": plan["existing"],
            "remaining": remaining,
        })),
    ))
}

fn invalid_stage(message: &str) -> AppError {
    AppError::validation("INVALID_GEOMETRY_REVIEW", message)
}

/// `stage_geometry`: persist verified geometry candidates apart from routes.
async fn stage_geometry(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path(params): Path<HashMap<String, String>>,
    body: Bytes,
) -> HnResult<(StatusCode, Json<Value>)> {
    csrf(&state, &auth, &headers)?;
    let (org, project) = scope_ids(&params)?;
    let body = import_body(&body, &["preview_token", "selected_keys", "reason"])?;
    let token = body.preview_token.clone().unwrap_or_default();
    let (Some(selected), Some(reason)) = (&body.selected_keys, &body.reason) else {
        return Err(HouseNumberingError::InvalidRequest);
    };
    if !hex(&token, 64) || !(1..=100).contains(&selected.len()) || !text_len(reason, 10, 500) {
        return Err(HouseNumberingError::InvalidRequest);
    }
    let reason = reason.trim();
    if !text_len(reason, 10, 500) || !unique(selected) {
        return Err(invalid_stage("Chọn tối đa 100 tuyến và ghi lý do lưu hình học").into());
    }
    let input = import_input(&body);
    let mut edit = module_edit(&state, &auth, org, project).await?;
    store::project_lock(&mut edit, "route-import").await?;
    let plan = run_preview(&mut edit, &input).await?;
    if plan["preview_token"] != token.as_str() {
        return Err(AppError::conflict(
            "STALE_GEOMETRY_REVIEW",
            "Dữ liệu hình học hoặc khảo sát đã đổi; xem trước lại",
        )
        .into());
    }
    let items = rows_of(&plan);
    let by_key: HashMap<&str, &Row> = items.iter().map(|r| (s(r, "key"), r)).collect();
    if selected.iter().any(|k| {
        !by_key
            .get(k.as_str())
            .is_some_and(|r| import::stage_eligible(r))
    }) {
        return Err(invalid_stage(
            "Chỉ lưu hình học đã nối đúng cha, chưa có tuyến và chỉ còn lỗi khảo sát",
        )
        .into());
    }
    let mut keys: Vec<&String> = selected.iter().collect();
    keys.sort();
    let actor = audit_actor(&auth);
    let mut staged = Vec::new();
    for key in keys {
        let row = by_key[key.as_str()];
        // Capture only route topology and aggregate survey review, not occupants or numbers.
        let snapshot = review::stage_snapshot(row);
        let raw = import::py_json(&snapshot);
        let digest = import::py_digest(&snapshot);
        let code = s(row, "code");
        let saved =
            match store::stage_upsert(&mut edit, actor.id, code, &raw, &digest, reason).await? {
                Some(saved) => {
                    let survey = row
                        .get("survey_evidence")
                        .cloned()
                        .ok_or_else(|| AppError::internal("staged row has no survey evidence"))?;
                    let metadata = json!({
                        "route_code": code,
                        "prefix": row.get("resolved_prefix"),
                        "parent_id": row.get("parent_id"),
                        "snapshot_token": digest,
                        "survey_evidence": survey,
                        "reason": reason,
                    });
                    let id = parse_id(&py_str(&saved["id"]))?;
                    store::audit_event(
                        &mut edit,
                        actor,
                        "house_numbering.route.stage_geometry",
                        ("house_numbering_geometry_review", id),
                        metadata,
                    )
                    .await?;
                    saved
                }
                None => store::stage_lookup(&mut edit, code).await?,
            };
        staged.push(
            json!({"id": saved["id"], "revision": saved["revision"], "name": row.get("name")}),
        );
    }
    edit.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "staged": staged.len(),
            "items": staged,
            "routes_created": 0,
            "buildings_assigned": 0,
        })),
    ))
}

// ---------------------------------------------------------------- address verification

/// `offset`/`limit` query window, checked before the module scope opens.
fn window(
    query: &HashMap<String, String>,
    code: &'static str,
    message: &str,
) -> HnResult<(usize, usize)> {
    let offset = query_int(query, "offset", 0, i64::MIN, i64::MAX)?;
    let limit = query_int(query, "limit", 100, i64::MIN, i64::MAX)?;
    if offset < 0 || !(1..=100).contains(&limit) {
        return Err(AppError::validation(code, message).into());
    }
    Ok((
        usize::try_from(offset).unwrap_or(usize::MAX),
        usize::try_from(limit).unwrap_or(0),
    ))
}

fn page(items: Vec<Value>, offset: usize, limit: usize) -> (Vec<Value>, Value) {
    let end = offset.saturating_add(limit);
    let next = if items.len() > end {
        json!(end)
    } else {
        Value::Null
    };
    let page = items.into_iter().skip(offset).take(limit).collect();
    (page, next)
}

/// `verified_path`: the staged label must match the parent's actual root street.
async fn verified_path(
    read: &mut ProjectRead<'_>,
    snapshot: &Map<String, Value>,
) -> Result<Option<(String, String)>, AppError> {
    if address_label(&py_str(snapshot.get("name").unwrap_or(&Value::Null))).is_none() {
        return Ok(None);
    }
    let parent = snapshot.get("parent_id").and_then(Value::as_str);
    let root = store::verified_root(read, parent).await?;
    Ok(review::verified_path(snapshot, root.as_deref()))
}

/// `add_suggestions` for the current page only.
async fn suggestions(
    read: &mut ProjectRead<'_>,
    stage: &Value,
    snapshot: &Map<String, Value>,
    page: &mut [Value],
) -> Result<Value, AppError> {
    if page.is_empty() {
        return Ok(review::empty_suggestion_counts());
    }
    let routes = store::suggestion_routes(read).await?;
    let Some(catalog) = review::suggestion_catalog(snapshot, &py_str(&stage["id"]), routes) else {
        return Ok(review::plain_suggestions(page));
    };
    let pairs = catalog.pairs(page);
    let ids: Vec<Uuid> = page
        .iter()
        .filter(|r| r["eligible"] == true)
        .filter_map(|r| Uuid::parse_str(&py_str(&r["building_id"])).ok())
        .collect();
    let mut distances: HashMap<String, BTreeMap<String, f64>> = HashMap::new();
    if !ids.is_empty() && !catalog.is_empty() {
        let route_ids: Vec<Uuid> = catalog
            .route_ids()
            .iter()
            .filter_map(|id| Uuid::parse_str(id).ok())
            .collect();
        for (building, route, distance) in
            store::suggestion_distances(read, &ids, &route_ids, review::NEAR_M, &pairs).await?
        {
            distances
                .entry(building)
                .or_default()
                .insert(route, distance);
        }
    }
    Ok(catalog.finish(page, &distances))
}

async fn addresses(
    State(state): State<AppState>,
    auth: HumanSession,
    Path(params): Path<HashMap<String, String>>,
    Query(query): Query<HashMap<String, String>>,
) -> HnResult<Json<Value>> {
    let (org, project) = scope_ids(&params)?;
    let review_id = path_id(&params, "review_id")?;
    let (offset, limit) = window(&query, "INVALID_ADDRESS_REVIEW", "Lô địa chỉ không hợp lệ")?;
    let mut read = module_read(&state, &auth, org, project).await?;
    let stage = store::geometry_stage(&mut read, review_id, false).await?;
    let snapshot = snapshot_of(&stage);
    let mut report = run_report(&mut read, &stage).await?;
    let items = match report["items"].take() {
        Value::Array(items) => items,
        _ => Vec::new(),
    };
    let (mut page, next) = page(items, offset, limit);
    let counts = suggestions(&mut read, &stage, &snapshot, &mut page).await?;
    let groups = review::batch_candidates(&snapshot, &page);
    let batch_groups = if groups.is_empty() {
        Vec::new()
    } else {
        match verified_path(&mut read, &snapshot).await? {
            Some(path) => review::batch_groups(groups, &path),
            None => Vec::new(),
        }
    };
    read.commit().await?;
    Ok(Json(json!({
        "geometry_current": report["geometry_current"],
        "counts": report["counts"],
        "overflow": report["overflow"],
        "name": snapshot.get("name"),
        "expected_prefix": snapshot.get("resolved_prefix"),
        "items": page,
        "suggestion_counts": counts,
        "batch_groups": batch_groups,
        "next_offset": next,
    })))
}

/// `_lock_review`: serialize with route import and refuse promoted reviews.
async fn lock_review(edit: &mut ProjectRead<'_>, review_id: Uuid) -> Result<Value, AppError> {
    store::project_lock(edit, "route-import").await?;
    let stage = store::geometry_stage(edit, review_id, true).await?;
    if store::stage_promoted(edit, &py_str(&stage["route_code"])).await? {
        return Err(AppError::conflict(
            "PROMOTED_ADDRESS_REVIEW",
            "Tuyến đã vào danh mục; hồ sơ xác minh chỉ đọc",
        ));
    }
    Ok(stage)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AddressBody {
    evidence_token: String,
    expected_revision: i64,
    decision: String,
    verified_road: String,
    #[serde(default)]
    verified_prefix: String,
    evidence: String,
}

async fn verify_address(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path(params): Path<HashMap<String, String>>,
    body: Bytes,
) -> HnResult<Json<Value>> {
    csrf(&state, &auth, &headers)?;
    let (org, project) = scope_ids(&params)?;
    let review_id = path_id(&params, "review_id")?;
    let building_id = path_id(&params, "building_id")?;
    let body: AddressBody = decode(&body)?;
    if !hex(&body.evidence_token, 64)
        || body.expected_revision < 0
        || !matches!(body.decision.as_str(), "belongs" | "other_address")
        || !text_len(&body.verified_road, 1, 200)
        || !text_len(&body.verified_prefix, 0, 100)
        || !text_len(&body.evidence, 10, 500)
    {
        return Err(HouseNumberingError::InvalidRequest);
    }
    let mut edit = module_edit(&state, &auth, org, project).await?;
    let stage = lock_review(&mut edit, review_id).await?;
    let snapshot = snapshot_of(&stage);
    let report = run_report(&mut edit, &stage).await?;
    let bid = building_id.to_string();
    let row = report["items"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|r| r["building_id"] == bid.as_str());
    let input = review::ReviewInput {
        evidence_token: &body.evidence_token,
        expected_revision: body.expected_revision,
        decision: &body.decision,
        verified_road: &body.verified_road,
        verified_prefix: &body.verified_prefix,
        evidence: &body.evidence,
    };
    let (road, branch, evidence) = review::validate_review(&report, row, &snapshot, &input)?;
    let decision = store::AddressDecision {
        review_id,
        building_id,
        evidence_token: &body.evidence_token,
        decision: &body.decision,
        road: &road,
        prefix: &branch,
        evidence: &evidence,
        batch_id: None,
    };
    let saved = store::write_address_review(&mut edit, audit_actor(&auth), &decision).await?;
    edit.commit().await?;
    Ok(Json(saved))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BatchMember {
    building_id: Uuid,
    evidence_token: String,
    expected_revision: i64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BatchBody {
    key: String,
    members: Vec<BatchMember>,
    evidence: String,
    confirmed: bool,
}

fn stale_batch(message: &str) -> AppError {
    AppError::conflict("STALE_ADDRESS_BATCH", message)
}

/// `save_batch`: one shared evidence for a non-conflicting missing-field group.
async fn address_batch(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path(params): Path<HashMap<String, String>>,
    body: Bytes,
) -> HnResult<Json<Value>> {
    csrf(&state, &auth, &headers)?;
    let (org, project) = scope_ids(&params)?;
    let review_id = path_id(&params, "review_id")?;
    let body: BatchBody = decode(&body)?;
    if !hex(&body.key, 64)
        || !(2..=100).contains(&body.members.len())
        || body
            .members
            .iter()
            .any(|m| !hex(&m.evidence_token, 64) || m.expected_revision < 0)
        || !text_len(&body.evidence, 10, 500)
        || !body.confirmed
    {
        return Err(HouseNumberingError::InvalidRequest);
    }
    let ids: Vec<Uuid> = body.members.iter().map(|m| m.building_id).collect();
    if !(2..=review::MAX_BATCH).contains(&ids.len()) || !unique(&ids) {
        return Err(AppError::validation(
            "INVALID_ADDRESS_BATCH",
            "Chọn từ 2 đến 100 nhà khác nhau và xác nhận căn cứ chung",
        )
        .into());
    }
    let mut edit = module_edit(&state, &auth, org, project).await?;
    let stage = lock_review(&mut edit, review_id).await?;
    let snapshot = snapshot_of(&stage);
    // Serialize with building/source changes before reading the current report.
    if store::lock_review_buildings(&mut edit, &ids, false).await? != ids.len() {
        return Err(stale_batch("Danh sách nhà đã thay đổi, hãy tải lại").into());
    }
    let report = run_report(&mut edit, &stage).await?;
    let path = verified_path(&mut edit, &snapshot).await?;
    let Some(path) =
        path.filter(|_| report["geometry_current"] == true && report["overflow"] != true)
    else {
        return Err(stale_batch("Tuyến hoặc dữ liệu đã thay đổi, hãy tải lại").into());
    };
    let rows: HashMap<String, &Value> = report["items"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|r| (py_str(&r["building_id"]), r))
        .collect();
    let mut members: Vec<&BatchMember> = body.members.iter().collect();
    members.sort_by_key(|m| m.building_id.to_string());
    let mut validated = Vec::new();
    for m in members {
        let row = rows.get(&m.building_id.to_string()).copied().filter(|row| {
            row["status"] == "pending"
                && row["evidence_token"] == m.evidence_token.as_str()
                && row["revision"].as_i64() == Some(m.expected_revision)
        });
        let Some(row) = row else {
            return Err(stale_batch("Một nhà đã thay đổi hoặc đã xác minh; chưa lưu cả lô").into());
        };
        if review::group_key(&snapshot, row).as_deref() != Some(body.key.as_str()) {
            return Err(AppError::validation(
                "INVALID_ADDRESS_BATCH",
                "Không cùng nhóm hoặc địa chỉ có mâu thuẫn; hãy xác minh riêng",
            )
            .into());
        }
        let input = review::ReviewInput {
            evidence_token: &m.evidence_token,
            expected_revision: m.expected_revision,
            decision: "belongs",
            verified_road: &path.0,
            verified_prefix: &path.1,
            evidence: &body.evidence,
        };
        let valid = review::validate_review(&report, Some(row), &snapshot, &input)?;
        validated.push((m, valid));
    }
    let batch_id = Uuid::new_v4();
    let actor = audit_actor(&auth);
    for (m, (road, branch, evidence)) in &validated {
        let decision = store::AddressDecision {
            review_id,
            building_id: m.building_id,
            evidence_token: &m.evidence_token,
            decision: "belongs",
            road,
            prefix: branch,
            evidence,
            batch_id: Some(batch_id),
        };
        store::write_address_review(&mut edit, actor, &decision).await?;
    }
    let metadata = json!({
        "batch_id": batch_id.to_string(),
        "group_key": body.key,
        "building_ids": ids.iter().map(Uuid::to_string).collect::<Vec<_>>(),
        "count": ids.len(),
        "evidence": body.evidence.trim(),
    });
    store::audit_event(
        &mut edit,
        actor,
        "house_numbering.building.verify_address_batch",
        ("house_numbering_geometry_review", review_id),
        metadata,
    )
    .await?;
    edit.commit().await?;
    Ok(Json(json!({
        "batch_id": batch_id.to_string(),
        "verified": ids.len(),
        "buildings_updated": 0,
        "routes_created": 0,
    })))
}

// ---------------------------------------------------------------- verified links

/// `_plan`: link only verified buildings to the exact promoted route.
async fn link_plan(read: &mut ProjectRead<'_>, stage: &Value) -> Result<Value, AppError> {
    let snapshot = snapshot_of(stage);
    let code = py_str(&stage["route_code"]);
    let Some(route) = store::promoted_route(read, &code, &stage["snapshot"]).await? else {
        return Ok(review::link_unavailable("not_promoted", Value::Null));
    };
    // Same source links alone are insufficient: all original revisions, junction,
    // and the actual root-to-alley address path must still be current.
    let report = run_report(read, stage).await?;
    let path = verified_path(read, &snapshot).await?;
    let Some(path) =
        path.filter(|_| report["geometry_current"] == true && report["overflow"] != true)
    else {
        return Ok(review::link_unavailable("stale_route", route));
    };
    let records = store::link_records(read, parse_id(&py_str(&stage["id"]))?).await?;
    if records.len() > review::REPORT_LIMIT {
        return Ok(review::link_unavailable("overflow", route));
    }
    Ok(review::link_plan(
        &snapshot,
        &stage["revision"],
        &route,
        &records,
        &report,
        (&path.0, &path.1),
    ))
}

async fn links_preview(
    State(state): State<AppState>,
    auth: HumanSession,
    Path(params): Path<HashMap<String, String>>,
    Query(query): Query<HashMap<String, String>>,
) -> HnResult<Json<Value>> {
    let (org, project) = scope_ids(&params)?;
    let review_id = path_id(&params, "review_id")?;
    let (offset, limit) = window(&query, "INVALID_VERIFIED_LINKS", "Lô liên kết không hợp lệ")?;
    let mut read = module_read(&state, &auth, org, project).await?;
    let stage = store::geometry_stage(&mut read, review_id, false).await?;
    let mut plan = link_plan(&mut read, &stage).await?;
    read.commit().await?;
    let items = match plan["items"].take() {
        Value::Array(items) => items,
        _ => Vec::new(),
    };
    let (page, next) = page(items, offset, limit);
    plan["items"] = Value::Array(page);
    plan["next_offset"] = next;
    Ok(Json(plan))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LinkBody {
    preview_token: String,
    building_ids: Vec<Uuid>,
    confirmed: bool,
}

fn stale_links(message: &str) -> AppError {
    AppError::conflict("STALE_VERIFIED_LINKS", message)
}

fn invalid_links(message: &str) -> AppError {
    AppError::validation("INVALID_VERIFIED_LINKS", message)
}

async fn links_apply(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path(params): Path<HashMap<String, String>>,
    body: Bytes,
) -> HnResult<Json<Value>> {
    csrf(&state, &auth, &headers)?;
    let (org, project) = scope_ids(&params)?;
    let review_id = path_id(&params, "review_id")?;
    let body: LinkBody = decode(&body)?;
    if !hex(&body.preview_token, 64)
        || !(1..=100).contains(&body.building_ids.len())
        || !body.confirmed
    {
        return Err(HouseNumberingError::InvalidRequest);
    }
    if !unique(&body.building_ids) {
        return Err(invalid_links("Chọn từ 1 đến 100 nhà khác nhau và xác nhận liên kết").into());
    }
    let mut edit = module_edit(&state, &auth, org, project).await?;
    // Serialize with route import, review decisions and the normal assignment workbench.
    store::project_lock(&mut edit, "route-import").await?;
    store::project_lock(&mut edit, "assignment").await?;
    let stage = store::geometry_stage(&mut edit, review_id, true).await?;
    let snapshot = snapshot_of(&stage);
    let routes = store::lineage_routes(
        &mut edit,
        &py_str(&stage["route_code"]),
        snapshot.get("parent_id").and_then(Value::as_str),
    )
    .await?;
    let mut links: BTreeSet<(String, String)> = review::refs(&snapshot)
        .iter()
        .map(|x| (py_str(&x["source_id"]), py_str(&x["feature_id"])))
        .collect();
    for route in &routes {
        match route["segments"].as_array() {
            Some(items) if !items.is_empty() => links.extend(
                items
                    .iter()
                    .map(|x| (py_str(&x["source_id"]), py_str(&x["feature_id"]))),
            ),
            _ => {
                links.insert((
                    py_str(&route["linked_source_id"]),
                    py_str(&route["linked_feature_id"]),
                ));
            }
        }
    }
    let links = links
        .into_iter()
        .map(|(sid, fid)| json!({"source_id": sid, "feature_id": fid}))
        .collect();
    store::lock_link_features(&mut edit, links).await?;
    if store::lock_review_buildings(&mut edit, &body.building_ids, true).await?
        != body.building_ids.len()
    {
        return Err(stale_links("Danh sách nhà đã thay đổi").into());
    }
    let plan = link_plan(&mut edit, &stage).await?;
    if !plan["preview_token"].is_string() || plan["preview_token"] != body.preview_token.as_str() {
        return Err(stale_links("Tuyến hoặc hồ sơ đã thay đổi, hãy xem trước lại").into());
    }
    let rows: HashMap<String, &Value> = plan["items"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|r| (py_str(&r["building_id"]), r))
        .collect();
    if body.building_ids.iter().any(|bid| {
        !rows
            .get(&bid.to_string())
            .is_some_and(|r| r["status"] == "ready")
    }) {
        return Err(invalid_links(
            "Chỉ liên kết nhà đã xác minh thuộc tuyến, chưa liên kết và không có số bảo vệ",
        )
        .into());
    }
    let route_id = parse_id(&py_str(&plan["route"]["id"]))?;
    let batch_id = Uuid::new_v4();
    let actor = audit_actor(&auth);
    let mut ordered = body.building_ids.clone();
    ordered.sort_by_key(Uuid::to_string);
    for bid in ordered {
        let row = rows[&bid.to_string()];
        let before = row["building_revision"].as_i64().unwrap_or(0);
        let after = store::link_verified_building(&mut edit, actor, bid, before, route_id).await?;
        let metadata = json!({
            "batch_id": batch_id.to_string(),
            "geometry_review_id": review_id.to_string(),
            "address_review_id": row["review_entry_id"],
            "review_revision": row["review_revision"],
            "evidence_token": row["evidence_token"],
            "route_id": route_id.to_string(),
            "before_revision": row["building_revision"],
            "after_revision": after,
        });
        store::audit_event(
            &mut edit,
            actor,
            "house_numbering.building.link_verified_address",
            ("house_numbering_building", bid),
            metadata,
        )
        .await?;
    }
    let metadata = json!({
        "batch_id": batch_id.to_string(),
        "route_id": route_id.to_string(),
        "building_ids": body.building_ids.iter().map(Uuid::to_string).collect::<Vec<_>>(),
        "count": body.building_ids.len(),
        "preview_token": body.preview_token,
    });
    store::audit_event(
        &mut edit,
        actor,
        "house_numbering.building.link_verified_address_batch",
        ("house_numbering_geometry_review", review_id),
        metadata,
    )
    .await?;
    edit.commit().await?;
    Ok(Json(json!({
        "linked": body.building_ids.len(),
        "route_id": route_id.to_string(),
        "batch_id": batch_id.to_string(),
        "numbers_created": 0,
    })))
}
