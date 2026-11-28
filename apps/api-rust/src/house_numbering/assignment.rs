//! Building→route assignment review, unlinked-building audit, alias/prefix
//! recovery and cluster corrections (port of the `building-assignment/*` routes).

use super::module_assign;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt;

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::routing::post;
use axum::{Json, Router};
use ovg_core::AppError;
use ovg_module_house_numbering::assignment::{
    audit_classify, audit_group_valid, audit_report, correction_finish, correction_items,
    preview_finish, preview_match, AuditPage, Mapping, RouteIndex,
};
use ovg_module_house_numbering::recovery::{
    catalog_finish, catalog_scan, prefix_finish, prefix_plan, Window,
};
use ovg_storage::catalog::ProjectRead;
use ovg_storage::house_numbering as store;
use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::{json, Value};
use uuid::Uuid;

use super::{
    audit_actor, csrf, decode, module_read, scope_ids, text_len, HnResult,
    HouseNumberingError, BASE,
};
use crate::identity::HumanSession;
use crate::AppState;

pub(super) fn router() -> Router<AppState> {
    let at = |path: &str| format!("{BASE}/building-assignment{path}");
    Router::new()
        .route(&at("/audit"), post(audit))
        .route(&at("/preview"), post(preview))
        .route(&at("/apply"), post(apply))
        .route(&at("/catalog-recovery/preview"), post(catalog_preview))
        .route(&at("/catalog-recovery/apply"), post(catalog_apply))
        .route(&at("/prefix-recovery/preview"), post(prefix_preview))
        .route(&at("/correction/preview"), post(correction_preview))
        .route(&at("/correction/apply"), post(correction_apply))
}

fn default_mapping() -> HashMap<String, String> {
    [
        ("road", "ten_duong"),
        ("alley", "hem"),
        ("code", "tuyen_id"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v.to_owned()))
    .collect()
}

fn limit_200() -> i64 {
    200
}

fn limit_100() -> i64 {
    100
}

fn distance_80() -> f64 {
    80.0
}

fn distance_35() -> f64 {
    35.0
}

/// `dict[Literal["road","alley","code"], str]`.
fn mapping_of(map: &HashMap<String, String>) -> HnResult<Mapping> {
    if map
        .keys()
        .any(|k| !matches!(k.as_str(), "road" | "alley" | "code"))
    {
        return Err(HouseNumberingError::InvalidRequest);
    }
    let get = |k: &str| map.get(k).cloned().unwrap_or_default();
    Ok(Mapping {
        road: get("road"),
        alley: get("alley"),
        code: get("code"),
    })
}

fn names(mapping: &Mapping) -> store::AttributeNames<'_> {
    store::AttributeNames {
        road: &mapping.road,
        alley: &mapping.alley,
        code: &mapping.code,
    }
}

fn hex(value: &str, len: usize) -> bool {
    value.len() == len
        && value
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn in_range(value: f64, min: f64, max: f64) -> bool {
    value.is_finite() && value >= min && value <= max
}

fn unique<T: std::hash::Hash + Eq>(items: &[T]) -> bool {
    items.iter().collect::<HashSet<_>>().len() == items.len()
}

/// Ordered `dict[str, str]` (Python keeps first position, last value).
#[derive(Debug)]
struct OrderedStrings(Vec<(String, String)>);

impl<'de> Deserialize<'de> for OrderedStrings {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OrderedVisitor;
        impl<'de> Visitor<'de> for OrderedVisitor {
            type Value = OrderedStrings;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("an object of strings")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut items: Vec<(String, String)> = Vec::new();
                while let Some((key, value)) = map.next_entry::<String, String>()? {
                    match items.iter_mut().find(|(k, _)| *k == key) {
                        Some(item) => item.1 = value,
                        None => items.push((key, value)),
                    }
                }
                Ok(OrderedStrings(items))
            }
        }
        deserializer.deserialize_map(OrderedVisitor)
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AuditBody {
    #[serde(default)]
    source_id: Option<Uuid>,
    #[serde(default = "default_mapping")]
    mapping: HashMap<String, String>,
    #[serde(default)]
    group: Option<String>,
    #[serde(default)]
    after_id: Option<Uuid>,
    #[serde(default = "limit_200")]
    limit: i64,
    #[serde(default = "distance_80")]
    max_distance_m: f64,
    #[serde(default)]
    cluster: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PreviewBody {
    #[serde(default)]
    source_id: Option<Uuid>,
    #[serde(default)]
    after_id: Option<Uuid>,
    #[serde(default = "limit_200")]
    limit: i64,
    #[serde(default = "default_mapping")]
    mapping: HashMap<String, String>,
    #[serde(default = "distance_80")]
    max_distance_m: f64,
    #[serde(default)]
    strict: bool,
    #[serde(default)]
    building_ids: Option<Vec<Uuid>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ApplyBody {
    #[serde(default)]
    source_id: Option<Uuid>,
    #[serde(default)]
    after_id: Option<Uuid>,
    #[serde(default = "limit_200")]
    limit: i64,
    #[serde(default = "default_mapping")]
    mapping: HashMap<String, String>,
    #[serde(default = "distance_80")]
    max_distance_m: f64,
    #[serde(default)]
    strict: bool,
    #[serde(default)]
    building_ids: Option<Vec<Uuid>>,
    preview_token: String,
    assignments: BTreeMap<Uuid, Uuid>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogBody {
    #[serde(default)]
    source_id: Option<Uuid>,
    #[serde(default = "default_mapping")]
    mapping: HashMap<String, String>,
    #[serde(default)]
    search: String,
    #[serde(default)]
    offset: i64,
    #[serde(default = "limit_100")]
    limit: i64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogApplyBody {
    #[serde(default)]
    source_id: Option<Uuid>,
    #[serde(default = "default_mapping")]
    mapping: HashMap<String, String>,
    selections: OrderedStrings,
    reason: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CorrectionBody {
    cluster: String,
    building_ids: Vec<Uuid>,
    #[serde(default = "default_mapping")]
    mapping: HashMap<String, String>,
    #[serde(default = "distance_35")]
    max_distance_m: f64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CorrectionApplyBody {
    cluster: String,
    building_ids: Vec<Uuid>,
    #[serde(default = "default_mapping")]
    mapping: HashMap<String, String>,
    #[serde(default = "distance_35")]
    max_distance_m: f64,
    preview_token: String,
    route_id: Uuid,
    selected_ids: Vec<Uuid>,
    reason: String,
}

async fn audit(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path(params): Path<HashMap<String, String>>,
    body: Bytes,
) -> HnResult<Json<Value>> {
    csrf(&state, &auth, &headers)?;
    let (org, project) = scope_ids(&params)?;
    let body: AuditBody = decode(&body)?;
    let mapping = mapping_of(&body.mapping)?;
    if !(1..=500).contains(&body.limit)
        || !in_range(body.max_distance_m, 5.0, 500.0)
        || body.group.as_deref().is_some_and(|g| !audit_group_valid(g))
        || body.cluster.as_deref().is_some_and(|c| !hex(c, 24))
    {
        return Err(HouseNumberingError::InvalidRequest);
    }
    let mut read = module_read(&state, &auth, org, project).await?;
    let index = RouteIndex::new(store::assignment_routes(&mut read).await?);
    let pending = store::pending_route_features(&mut read).await?;
    let rows = store::audit_buildings(&mut read, body.source_id, names(&mapping)).await?;
    let (items, pairs) = audit_classify(&index, &pending, &rows, body.group.as_deref());
    let mut distances = HashMap::new();
    if !pairs.is_empty() {
        for (bid, d) in store::pair_distances(&mut read, &pairs).await? {
            if let Some(d) = d {
                distances.insert(bid, d);
            }
        }
    }
    let page = AuditPage {
        total: rows.len(),
        group: body.group.as_deref(),
        cluster: body.cluster.as_deref(),
        after_id: body.after_id.map(|id| id.to_string()),
        limit: usize::try_from(body.limit).unwrap_or(0),
        max_distance: body.max_distance_m,
    };
    let result = audit_report(items, &distances, &page);
    read.commit().await?;
    Ok(Json(result))
}

struct PreviewArgs {
    source_id: Option<Uuid>,
    after_id: Option<Uuid>,
    limit: i64,
    mapping: Mapping,
    max_distance: f64,
    strict: bool,
    building_ids: Option<Vec<Uuid>>,
}

fn preview_args(
    body_limit: i64,
    max_distance: f64,
    mapping: &HashMap<String, String>,
    building_ids: Option<&[Uuid]>,
) -> HnResult<Mapping> {
    if !(1..=500).contains(&body_limit)
        || !in_range(max_distance, 5.0, 500.0)
        || building_ids.is_some_and(|ids| ids.is_empty() || ids.len() > 500)
    {
        return Err(HouseNumberingError::InvalidRequest);
    }
    mapping_of(mapping)
}

/// `BuildingRouteAssigner.preview`.
async fn run_preview(read: &mut ProjectRead<'_>, a: &PreviewArgs) -> Result<Value, AppError> {
    if let Some(ids) = &a.building_ids {
        if !unique(ids) || ids.len() as i64 > a.limit || a.after_id.is_some() {
            return Err(AppError::validation(
                "INVALID_ASSIGNMENT_BATCH",
                "Lô công trình không hợp lệ",
            ));
        }
    }
    let index = RouteIndex::new(store::assignment_routes(read).await?);
    if index.root.is_empty() {
        return Err(AppError::validation(
            "ROUTES_REQUIRED",
            "Chưa có cây tuyến hợp lệ; hãy import và kiểm tra tuyến trước",
        ));
    }
    let scope = store::PreviewScope {
        source_id: a.source_id,
        after_id: a.after_id,
        limit: a.limit + 1,
        building_ids: a.building_ids.as_deref(),
    };
    let data = store::preview_buildings(read, scope).await?;
    if a.building_ids
        .as_ref()
        .is_some_and(|ids| ids.len() != data.len())
    {
        return Err(AppError::conflict(
            "STALE_ASSIGNMENT",
            "Danh sách công trình đã thay đổi, hãy phân nhóm lại",
        ));
    }
    let limit = usize::try_from(a.limit).unwrap_or(0);
    let rows = &data[..data.len().min(limit)];
    let page = preview_match(&index, rows, &a.mapping, a.strict);
    let mut distances: HashMap<String, HashMap<String, f64>> = HashMap::new();
    if !page.ids.is_empty() && (!a.strict || !page.pairs.is_empty()) {
        let route_ids: Vec<String> = if a.strict {
            let set: BTreeSet<&String> = page.pairs.iter().map(|p| &p.1).collect();
            set.into_iter().cloned().collect()
        } else {
            index.root.keys().cloned().collect()
        };
        let input = store::PreviewDistances {
            ids: &page.ids,
            route_ids: &route_ids,
            pairs: &page.pairs,
            max_distance: a.max_distance,
            strict: a.strict,
        };
        for (bid, rid, d) in store::preview_distances(read, input).await? {
            distances.entry(bid).or_default().insert(rid, d);
        }
    }
    let next = if data.len() > limit {
        rows.last().map(|b| b["id"].clone())
    } else {
        None
    };
    let (items, counts, token) = preview_finish(&index, page, &distances, a.max_distance, a.strict);
    Ok(json!({
        "items": items,
        "counts": counts,
        "preview_token": token,
        "next_after_id": next,
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
    let body: PreviewBody = decode(&body)?;
    let mapping = preview_args(
        body.limit,
        body.max_distance_m,
        &body.mapping,
        body.building_ids.as_deref(),
    )?;
    let args = PreviewArgs {
        source_id: body.source_id,
        after_id: body.after_id,
        limit: body.limit,
        mapping,
        max_distance: body.max_distance_m,
        strict: body.strict,
        building_ids: body.building_ids,
    };
    let mut edit = module_assign(&state, &auth, org, project).await?;
    let result = run_preview(&mut edit, &args).await?;
    edit.commit().await?;
    Ok(Json(result))
}

async fn apply(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path(params): Path<HashMap<String, String>>,
    body: Bytes,
) -> HnResult<Json<Value>> {
    csrf(&state, &auth, &headers)?;
    let (org, project) = scope_ids(&params)?;
    let body: ApplyBody = decode(&body)?;
    let mapping = preview_args(
        body.limit,
        body.max_distance_m,
        &body.mapping,
        body.building_ids.as_deref(),
    )?;
    if !hex(&body.preview_token, 64) || body.assignments.is_empty() || body.assignments.len() > 500
    {
        return Err(HouseNumberingError::InvalidRequest);
    }
    let args = PreviewArgs {
        source_id: body.source_id,
        after_id: body.after_id,
        limit: body.limit,
        mapping,
        max_distance: body.max_distance_m,
        strict: body.strict,
        building_ids: body.building_ids,
    };
    let mut edit = module_assign(&state, &auth, org, project).await?;
    store::assignment_lock(&mut edit).await?;
    let plan = run_preview(&mut edit, &args).await?;
    if plan["preview_token"] != body.preview_token.as_str() {
        return Err(AppError::conflict(
            "STALE_ASSIGNMENT",
            "Dữ liệu đã thay đổi, hãy xem trước lại",
        )
        .into());
    }
    let items = plan["items"].as_array().cloned().unwrap_or_default();
    for (bid, route_id) in &body.assignments {
        let rid = route_id.to_string();
        let row = items
            .iter()
            .find(|r| r["building_id"] == bid.to_string().as_str());
        let Some(row) = row.filter(|r| {
            !matches!(r["confidence"].as_str(), Some("blocked" | "unmatched"))
                && r["candidates"]
                    .as_array()
                    .is_some_and(|c| c.iter().any(|c| c["route_id"] == rid.as_str()))
        }) else {
            return Err(AppError::validation(
                "INVALID_ASSIGNMENT",
                "Tuyến chưa được đề xuất cho công trình hoặc công trình bị khóa",
            )
            .into());
        };
        if args.strict && (row["confidence"] != "high" || row["suggested_route_id"] != rid.as_str())
        {
            return Err(AppError::validation(
                "INVALID_ASSIGNMENT",
                "Chế độ tự động chỉ cho phép tuyến khớp chắc chắn",
            )
            .into());
        }
        let guard = store::StaleGuard {
            code: "STALE_ASSIGNMENT",
            message: "Công trình đã thay đổi tuyến",
            protected: None,
        };
        let revision = row["revision"].as_i64().unwrap_or(0);
        store::assign_building_route(
            &mut edit,
            audit_actor(&auth),
            *bid,
            revision,
            *route_id,
            guard,
        )
        .await?;
    }
    edit.commit().await?;
    Ok(Json(json!({
        "assigned": body.assignments.len(),
        "next_after_id": plan["next_after_id"],
    })))
}

fn catalog_window(body: &CatalogBody) -> HnResult<Window<'_>> {
    if !text_len(&body.search, 0, 200) || body.offset < 0 || !(1..=100).contains(&body.limit) {
        return Err(HouseNumberingError::InvalidRequest);
    }
    Ok(Window {
        search: &body.search,
        offset: usize::try_from(body.offset).unwrap_or(usize::MAX),
        limit: usize::try_from(body.limit).unwrap_or(0),
    })
}

/// `catalog_preview` (alias catalog, or the prefix tree queue when `tree`).
async fn catalog_report(
    read: &mut ProjectRead<'_>,
    source_id: Option<Uuid>,
    mapping: &Mapping,
    window: &Window<'_>,
    tree: bool,
) -> Result<Value, AppError> {
    let index = RouteIndex::new(store::assignment_routes(read).await?);
    let lines = store::catalog_lines(read).await?;
    let obs = store::catalog_observations(read, source_id, names(mapping)).await?;
    let scan = catalog_scan(&index, &lines, &obs, tree);
    if tree {
        let plan = prefix_plan(&index, &scan);
        let stats = if plan.specs.is_empty() {
            Vec::new()
        } else {
            store::prefix_spec_stats(read, &plan.specs).await?
        };
        let junctions = if plan.pairs.is_empty() {
            Vec::new()
        } else {
            store::prefix_junctions(read, &plan.specs, &plan.pairs).await?
        };
        return Ok(prefix_finish(&index, plan, &stats, &junctions, window));
    }
    let distances = if scan.pairs.is_empty() {
        Vec::new()
    } else {
        store::pair_distances(read, &scan.pairs).await?
    };
    Ok(catalog_finish(scan, &distances, window))
}

async fn recovery_preview(
    state: AppState,
    auth: HumanSession,
    headers: HeaderMap,
    params: HashMap<String, String>,
    body: Bytes,
    tree: bool,
) -> HnResult<Json<Value>> {
    csrf(&state, &auth, &headers)?;
    let (org, project) = scope_ids(&params)?;
    let body: CatalogBody = decode(&body)?;
    let mapping = mapping_of(&body.mapping)?;
    let window = catalog_window(&body)?;
    let mut read = module_read(&state, &auth, org, project).await?;
    let result = catalog_report(&mut read, body.source_id, &mapping, &window, tree).await?;
    read.commit().await?;
    Ok(Json(result))
}

async fn catalog_preview(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path(params): Path<HashMap<String, String>>,
    body: Bytes,
) -> HnResult<Json<Value>> {
    recovery_preview(state, auth, headers, params, body, false).await
}

async fn prefix_preview(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path(params): Path<HashMap<String, String>>,
    body: Bytes,
) -> HnResult<Json<Value>> {
    recovery_preview(state, auth, headers, params, body, true).await
}

fn stale_alias(message: &str) -> AppError {
    AppError::conflict("STALE_ALIAS_PLAN", message)
}

async fn catalog_apply(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path(params): Path<HashMap<String, String>>,
    body: Bytes,
) -> HnResult<Json<Value>> {
    csrf(&state, &auth, &headers)?;
    let (org, project) = scope_ids(&params)?;
    let body: CatalogApplyBody = decode(&body)?;
    let mapping = mapping_of(&body.mapping)?;
    let selections = &body.selections.0;
    if selections.is_empty() || selections.len() > 100 || !text_len(&body.reason, 10, 500) {
        return Err(HouseNumberingError::InvalidRequest);
    }
    let reason = body.reason.trim();
    if reason.chars().count() < 10 {
        return Err(AppError::validation(
            "INVALID_ALIAS_SELECTION",
            "Chọn tối đa 100 tên đối chiếu và ghi lý do",
        )
        .into());
    }
    let mut edit = module_assign(&state, &auth, org, project).await?;
    store::assignment_lock(&mut edit).await?;
    let window = Window {
        search: "",
        offset: 0,
        limit: 100_000,
    };
    let plan = catalog_report(&mut edit, body.source_id, &mapping, &window, false).await?;
    let items = plan["items"].as_array().cloned().unwrap_or_default();
    let mut changes: BTreeMap<String, Vec<&Value>> = BTreeMap::new();
    for (key, token) in selections {
        let group = items.iter().find(|g| g["id"] == key.as_str());
        let Some(group) = group.filter(|g| g["reviewable"] == true && g["token"] == token.as_str())
        else {
            return Err(stale_alias("Danh mục hoặc bằng chứng khảo sát đã thay đổi").into());
        };
        let rid = group["route_id"].as_str().unwrap_or_default().to_owned();
        changes.entry(rid).or_default().push(group);
    }
    let actor = audit_actor(&auth);
    for (rid, groups) in &changes {
        let route_id = Uuid::parse_str(rid).map_err(|_| AppError::internal("invalid route id"))?;
        let route = store::route_for_update(&mut edit, route_id).await?;
        if route["revision"] != groups[0]["route_revision"] {
            return Err(stale_alias("Tuyến đã thay đổi").into());
        }
        let mut aliases = route["reviewed_aliases"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let key_of = |a: &Value| {
            (
                a["code"].to_string(),
                a["normalized_road"].to_string(),
                a["alley"].to_string(),
            )
        };
        let mut keys: HashSet<(String, String, String)> = aliases.iter().map(key_of).collect();
        for g in groups {
            if !keys.insert(key_of(g)) {
                return Err(stale_alias("Tên đối chiếu đã tồn tại").into());
            }
            aliases.push(json!({
                "code": g["code"],
                "road": g["road"],
                "normalized_road": g["normalized_road"],
                "alley": g["alley"],
            }));
        }
        if aliases.len() > 200 {
            return Err(AppError::validation(
                "ALIAS_LIMIT",
                "Quá nhiều tên đối chiếu cho một tuyến",
            )
            .into());
        }
        store::save_route_aliases(&mut edit, actor, route_id, Value::Array(aliases)).await?;
        for g in groups {
            let metadata = json!({
                "code": g["code"],
                "road": g["road"],
                "alley": g["alley"],
                "near": g["near"],
                "observations": g["observations"],
                "reason": reason,
                "token": g["token"],
            });
            store::audit_event(
                &mut edit,
                actor,
                "house_numbering.route.review_address_alias",
                ("house_numbering_route", route_id),
                metadata,
            )
            .await?;
        }
    }
    edit.commit().await?;
    let potential: i64 = changes
        .values()
        .flatten()
        .map(|g| g["near"].as_i64().unwrap_or(0))
        .sum();
    Ok(Json(json!({
        "aliases": selections.len(),
        "routes": changes.len(),
        "potential_houses": potential,
    })))
}

fn correction_fields(
    cluster: &str,
    building_ids: &[Uuid],
    mapping: &HashMap<String, String>,
    max_distance: f64,
) -> HnResult<Mapping> {
    if !hex(cluster, 24)
        || building_ids.is_empty()
        || building_ids.len() > 500
        || !in_range(max_distance, 5.0, 35.0)
    {
        return Err(HouseNumberingError::InvalidRequest);
    }
    mapping_of(mapping)
}

fn correction_batch(building_ids: &[Uuid]) -> Result<(), AppError> {
    if building_ids.is_empty() || building_ids.len() > 500 || !unique(building_ids) {
        return Err(AppError::validation(
            "INVALID_CORRECTION_BATCH",
            "Lô đối soát phải có 1–500 nhà khác nhau",
        ));
    }
    Ok(())
}

/// `correction_preview`.
async fn run_correction(
    read: &mut ProjectRead<'_>,
    cluster: &str,
    building_ids: &[Uuid],
    mapping: &Mapping,
    max_distance: f64,
) -> Result<Value, AppError> {
    correction_batch(building_ids)?;
    let index = RouteIndex::new(store::assignment_routes(read).await?);
    let data = store::correction_buildings(read, building_ids, names(mapping)).await?;
    if data.len() != building_ids.len() {
        return Err(AppError::conflict(
            "STALE_CORRECTION",
            "Danh sách nhà đã thay đổi",
        ));
    }
    let plan = correction_items(&index, &data, cluster)?;
    let routes: Vec<String> = plan.routes().into_iter().collect();
    let mut distances = HashMap::new();
    if !routes.is_empty() {
        for (bid, rid, d) in store::correction_distances(read, &plan.eligible(), &routes).await? {
            if let Some(d) = d {
                distances.insert((bid, rid), d);
            }
        }
    }
    Ok(correction_finish(
        &index,
        plan,
        &distances,
        cluster,
        max_distance,
    ))
}

async fn correction_preview(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path(params): Path<HashMap<String, String>>,
    body: Bytes,
) -> HnResult<Json<Value>> {
    csrf(&state, &auth, &headers)?;
    let (org, project) = scope_ids(&params)?;
    let body: CorrectionBody = decode(&body)?;
    let mapping = correction_fields(
        &body.cluster,
        &body.building_ids,
        &body.mapping,
        body.max_distance_m,
    )?;
    correction_batch(&body.building_ids)?;
    let mut edit = module_assign(&state, &auth, org, project).await?;
    let result = run_correction(
        &mut edit,
        &body.cluster,
        &body.building_ids,
        &mapping,
        body.max_distance_m,
    )
    .await?;
    edit.commit().await?;
    Ok(Json(result))
}

fn invalid_correction(message: &str) -> HouseNumberingError {
    AppError::validation("INVALID_CORRECTION", message).into()
}

async fn correction_apply(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path(params): Path<HashMap<String, String>>,
    body: Bytes,
) -> HnResult<Json<Value>> {
    csrf(&state, &auth, &headers)?;
    let (org, project) = scope_ids(&params)?;
    let body: CorrectionApplyBody = decode(&body)?;
    let mapping = correction_fields(
        &body.cluster,
        &body.building_ids,
        &body.mapping,
        body.max_distance_m,
    )?;
    if !hex(&body.preview_token, 64)
        || body.selected_ids.is_empty()
        || body.selected_ids.len() > 500
        || !text_len(&body.reason, 10, 500)
    {
        return Err(HouseNumberingError::InvalidRequest);
    }
    let reason = body.reason.trim();
    if reason.chars().count() < 10 || !unique(&body.selected_ids) {
        return Err(invalid_correction(
            "Cần chọn nhà và ghi lý do đối soát rõ ràng",
        ));
    }
    let mut edit = module_assign(&state, &auth, org, project).await?;
    store::assignment_lock(&mut edit).await?;
    store::route_for_update(&mut edit, body.route_id).await?;
    let plan = run_correction(
        &mut edit,
        &body.cluster,
        &body.building_ids,
        &mapping,
        body.max_distance_m,
    )
    .await?;
    if plan["preview_token"] != body.preview_token.as_str() {
        return Err(AppError::conflict(
            "STALE_CORRECTION",
            "Dữ liệu đối soát đã thay đổi; xem trước lại",
        )
        .into());
    }
    let rid = body.route_id.to_string();
    let listed = plan["candidates"]
        .as_array()
        .is_some_and(|c| c.iter().any(|c| c["route_id"] == rid.as_str()));
    if !listed {
        return Err(invalid_correction(
            "Tuyến đích không khớp đường/hẻm khảo sát",
        ));
    }
    let items = plan["items"].as_array().cloned().unwrap_or_default();
    let row_of = |bid: &Uuid| {
        items
            .iter()
            .find(|x| x["building_id"] == bid.to_string().as_str())
    };
    for bid in &body.selected_ids {
        let ok = row_of(bid).is_some_and(|row| {
            row["eligible"] == true && row["distances"][rid.as_str()]["within_limit"] == true
        });
        if !ok {
            return Err(invalid_correction(
                "Nhà không đủ điều kiện khoảng cách hoặc bị khóa",
            ));
        }
    }
    let mut selected = body.selected_ids.clone();
    selected.sort();
    let actor = audit_actor(&auth);
    for bid in &selected {
        let Some(row) = row_of(bid) else { continue };
        let guard = store::StaleGuard {
            code: "STALE_CORRECTION",
            message: "Nhà đã thay đổi",
            protected: Some("Nhà đã có số bảo vệ"),
        };
        let revision = row["revision"].as_i64().unwrap_or(0);
        store::assign_building_route(&mut edit, actor, *bid, revision, body.route_id, guard)
            .await?;
        let metadata = json!({
            "cluster": plan["cluster"],
            "survey_code": row["code"],
            "survey_road": row["road"],
            "survey_alley": row["alley"],
            "route_id": rid,
            "distance_m": row["distances"][rid.as_str()]["distance_m"],
            "reason": reason,
            "preview_token": body.preview_token,
        });
        store::audit_event(
            &mut edit,
            actor,
            "house_numbering.building.correct_route_code",
            ("house_numbering_building", *bid),
            metadata,
        )
        .await?;
    }
    edit.commit().await?;
    Ok(Json(json!({
        "assigned": body.selected_ids.len(),
        "route_id": rid,
        "remaining": items.len() as i64 - body.selected_ids.len() as i64,
    })))
}
