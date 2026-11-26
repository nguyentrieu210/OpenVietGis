//! Ports of `address_verification.py`, `address_suggestions.py`, `address_batch.py`
//! and `verified_links.py`: explicit address evidence for staged route promotion.
//!
//! Pure rules only; persistence supplies the staged snapshot, building rows and
//! measured distances. Tokens use Python's `json.dumps(sort_keys=True,
//! ensure_ascii=False, separators=(",", ":"), default=str)` encoding.

use std::collections::{BTreeMap, HashMap};

use ovg_core::AppError;
use serde_json::{json, Map, Value};

use crate::assignment::{address_label, alley_name, py_round, py_str, road_name, RouteIndex};
use crate::prefix::canonical_prefix;
use crate::route_import::{py_digest, truthy};

pub const NEAR_M: f64 = 80.0;
pub const CLOSE_M: f64 = 35.0;
pub const MAX_BATCH: usize = 100;
const UNKNOWN_ROADS: [&str; 11] = [
    "",
    "0",
    "none",
    "null",
    "unknown",
    "khong ro",
    "chua ro",
    "chua ro ten",
    "chua co ten",
    "khong ten",
    "chua xac dinh",
];
const UNVERIFIED_ROADS: [&str; 9] = [
    "",
    "0",
    "none",
    "null",
    "unknown",
    "khong ro",
    "chua ro",
    "chua ro ten",
    "chua xac dinh",
];
pub const LINK_STATUSES: [&str; 5] = ["ready", "linked", "other_address", "blocked", "stale"];

fn text(value: &Value) -> String {
    py_str(value)
}

fn pick(map: &Map<String, Value>, keys: &[&str]) -> Value {
    Value::Object(
        keys.iter()
            .map(|k| ((*k).to_owned(), map.get(*k).cloned().unwrap_or(Value::Null)))
            .collect(),
    )
}

/// `identity`: digest of the staged route topology and its parent junction.
pub fn identity(row: &Map<String, Value>) -> String {
    const FIELDS: [&str; 11] = [
        "code",
        "name",
        "route_type",
        "parent_id",
        "resolved_prefix",
        "direction_state",
        "source_id",
        "feature_id",
        "content_revision",
        "segments",
        "segment_revisions",
    ];
    let empty = Map::new();
    let parent = row
        .get("parent_evidence")
        .and_then(Value::as_object)
        .filter(|p| !p.is_empty())
        .unwrap_or(&empty);
    py_digest(&json!([
        pick(row, &FIELDS),
        pick(parent, &["code", "revision", "geometry_revision"]),
    ]))
}

/// `refs`: every staged source segment with its captured revision.
pub fn refs(snapshot: &Map<String, Value>) -> Vec<Value> {
    let segments = match snapshot.get("segments") {
        Some(Value::Array(items)) if !items.is_empty() => items.clone(),
        _ => vec![json!({
            "source_id": snapshot.get("source_id"),
            "feature_id": snapshot.get("feature_id"),
            "code": snapshot.get("code"),
        })],
    };
    let revisions = match snapshot.get("segment_revisions") {
        Some(Value::Array(items)) if !items.is_empty() => items.clone(),
        _ => vec![snapshot
            .get("content_revision")
            .cloned()
            .unwrap_or(Value::Null)],
    };
    segments
        .into_iter()
        .zip(revisions)
        .map(|(mut s, rev)| {
            if let Some(map) = s.as_object_mut() {
                map.insert("revision".into(), rev);
            }
            s
        })
        .collect()
}

pub fn review_kind(road: &str, branch: &str, root: &str, full: &str) -> &'static str {
    if !road.is_empty() && road != root {
        return "different_road";
    }
    if !branch.is_empty() && branch != full {
        return if road == root && branch.starts_with(&format!("{full}/")) {
            "descendant"
        } else {
            "different_branch"
        };
    }
    if road.is_empty() {
        return "missing_road";
    }
    if branch.is_empty() {
        return "missing_prefix";
    }
    "exact"
}

/// Inputs of the `_report` queries derived from the staged snapshot.
#[derive(Debug, Clone)]
pub struct ReportScope {
    pub root: String,
    pub full: String,
    pub refs: Value,
    pub codes: Vec<String>,
    pub parent: Value,
    pub parent_revision: Value,
    pub parent_geometry: Value,
}

pub fn report_scope(snapshot: &Map<String, Value>) -> Result<ReportScope, AppError> {
    let Some((root, full)) = address_label(&text(snapshot.get("name").unwrap_or(&Value::Null)))
    else {
        return Err(AppError::validation(
            "INVALID_GEOMETRY_REVIEW",
            "Thiếu đường dẫn tuyến chờ",
        ));
    };
    let mut codes = vec![text(snapshot.get("code").unwrap_or(&Value::Null))];
    for segment in snapshot
        .get("segments")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if truthy(segment.get("code")) {
            codes.push(text(&segment["code"]));
        }
    }
    codes.sort();
    codes.dedup();
    let evidence = snapshot
        .get("parent_evidence")
        .cloned()
        .unwrap_or(Value::Null);
    Ok(ReportScope {
        root,
        full: full.to_lowercase(),
        refs: Value::Array(refs(snapshot)),
        codes,
        parent: snapshot.get("parent_id").cloned().unwrap_or(Value::Null),
        parent_revision: evidence["revision"].clone(),
        parent_geometry: evidence["geometry_revision"].clone(),
    })
}

fn report_counts(total: usize, verified: usize, pending: usize, blocked: usize) -> Value {
    json!({"total": total, "verified": verified, "pending": pending, "blocked": blocked})
}

/// Report when the staged line no longer meets its parent.
pub fn report_unavailable() -> Value {
    json!({
        "geometry_current": false,
        "items": [],
        "counts": report_counts(0, 0, 0, 0),
        "overflow": false,
    })
}

pub const REPORT_LIMIT: usize = 10_000;

/// `_report` over building rows (`distance` as a float or null).
pub fn report_items(snapshot: &Map<String, Value>, scope: &ReportScope, rows: &[Value]) -> Value {
    if rows.len() > REPORT_LIMIT {
        let n = rows.len();
        return json!({
            "geometry_current": true, "items": [], "counts": report_counts(n, 0, n, n), "overflow": true,
        });
    }
    let stage_identity = identity(snapshot);
    let mut items = Vec::new();
    let (mut verified, mut blocked) = (0, 0);
    for x in rows {
        let kind = review_kind(
            &road_name(&text(&x["road"])),
            &alley_name(&text(&x["alley"])),
            &scope.root,
            &scope.full,
        );
        if kind == "exact" {
            continue;
        }
        let distance = x["distance"].as_f64();
        let eligible = x["route_id"].is_null()
            && x["locked"] != true
            && distance.is_some()
            && x["eligibility"] != "ineligible";
        let token = py_digest(&json!([
            stage_identity,
            text(&x["id"]),
            x["revision"],
            text(&x["source_id"]),
            x["feature_id"],
            x["content_revision"],
            x["road"],
            x["alley"],
            x["code"],
            distance,
            eligible,
        ]));
        let decided = eligible
            && x["evidence_token"] == json!(token)
            && matches!(x["decision"].as_str(), Some("belongs" | "other_address"));
        verified += usize::from(decided);
        blocked += usize::from(!eligible);
        let status = if decided {
            "verified"
        } else if !eligible {
            "blocked"
        } else {
            "pending"
        };
        let revision = if truthy(x.get("decision_revision")) {
            x["decision_revision"].clone()
        } else {
            json!(0)
        };
        let mut item = json!({
            "building_id": text(&x["id"]),
            "source_id": text(&x["source_id"]),
            "feature_id": x["feature_id"],
            "road": text(&x["road"]),
            "alley": text(&x["alley"]),
            "code": text(&x["code"]),
            "kind": kind,
            "distance_m": distance.map(|d| py_round(d, 3)),
            "eligible": eligible,
            "can_belong": eligible && distance.is_some_and(|d| d <= CLOSE_M),
            "evidence_token": token,
            "revision": revision,
            "status": status,
        });
        if decided {
            for key in ["decision", "verified_road", "verified_prefix", "evidence"] {
                item[key] = x[key].clone();
            }
        }
        items.push(item);
    }
    let total = items.len();
    json!({
        "geometry_current": true,
        "items": items,
        "counts": report_counts(total, verified, total - verified, blocked),
        "overflow": false,
    })
}

// ---------------------------------------------------------------- staged reviews in preview

/// Mark candidate rows whose source segments another staged route already owns;
/// returns `route code → row index` of the non-existing candidates.
pub fn stage_overlaps(rows: &mut [Map<String, Value>], stages: &[Value]) -> HashMap<String, usize> {
    let mut candidates = HashMap::new();
    for (i, r) in rows.iter().enumerate() {
        if !truthy(r.get("existing_id")) {
            candidates.insert(text(r.get("code").unwrap_or(&Value::Null)), i);
        }
    }
    if candidates.is_empty() {
        return candidates;
    }
    let mut owners: HashMap<(String, String), Vec<String>> = HashMap::new();
    for stage in stages {
        let Some(snapshot) = stage["snapshot"].as_object() else {
            continue;
        };
        for r in refs(snapshot) {
            owners
                .entry((text(&r["source_id"]), text(&r["feature_id"])))
                .or_default()
                .push(text(&stage["route_code"]));
        }
    }
    for &i in candidates.values() {
        let row = &rows[i];
        let code = text(row.get("code").unwrap_or(&Value::Null));
        let sources = match row.get("segments") {
            Some(Value::Array(items)) if !items.is_empty() => items.clone(),
            _ => vec![
                json!({"source_id": row.get("source_id"), "feature_id": row.get("feature_id")}),
            ],
        };
        let overlap = sources.iter().any(|r| {
            owners
                .get(&(text(&r["source_id"]), text(&r["feature_id"])))
                .is_some_and(|codes| codes.iter().any(|c| *c != code))
        });
        if overlap {
            let row = &mut rows[i];
            crate::route_import::push_error(row, "review_stage_overlap");
            row.insert("state".into(), json!("error"));
        }
    }
    candidates
}

/// A staged snapshot still matches its planned row.
pub fn stage_matches(row: &Map<String, Value>, stage: &Value) -> bool {
    truthy(row.get("review_reason"))
        && stage["snapshot"]
            .as_object()
            .is_some_and(|snapshot| identity(row) == identity(snapshot))
}

pub fn mark_stage_stale(row: &mut Map<String, Value>) {
    crate::route_import::push_error(row, "review_stage_stale");
    row.insert("state".into(), json!("error"));
}

/// Apply a current address report to its planned row (`apply_staged_reviews`).
pub fn apply_stage_report(row: &mut Map<String, Value>, stage: &Value, report: &Value) {
    let items = report["items"].as_array().cloned().unwrap_or_default();
    let digest = py_digest(&Value::Array(
        items
            .iter()
            .map(|x| {
                json!([
                    x["building_id"],
                    x["evidence_token"],
                    x["revision"],
                    x["status"]
                ])
            })
            .collect(),
    ));
    let mut review = report["counts"].as_object().cloned().unwrap_or_default();
    review.insert("review_id".into(), json!(text(&stage["id"])));
    review.insert(
        "geometry_current".into(),
        report["geometry_current"].clone(),
    );
    review.insert("digest".into(), json!(digest));
    row.insert("address_review".into(), Value::Object(review));
    if report["geometry_current"] != true {
        crate::route_import::push_error(row, "review_stage_stale");
    } else if report["overflow"] == true || report["counts"]["pending"].as_i64() != Some(0) {
        crate::route_import::push_error(row, "review_address_pending");
    } else if let Some(Value::Array(errors)) = row.get_mut("errors") {
        errors.retain(|e| e != "review_survey_address_conflict");
    }
    let state = if truthy(row.get("errors")) {
        "error"
    } else {
        "ready"
    };
    row.insert("state".into(), json!(state));
}

/// The snapshot persisted by `stage_geometry` (topology and aggregate survey review only).
pub fn stage_snapshot(row: &Map<String, Value>) -> Value {
    const FIELDS: [&str; 15] = [
        "key",
        "code",
        "name",
        "route_type",
        "parent_id",
        "resolved_prefix",
        "direction_state",
        "source_id",
        "feature_id",
        "content_revision",
        "segments",
        "segment_revisions",
        "parent_evidence",
        "survey_evidence",
        "survey_review",
    ];
    Value::Object(
        FIELDS
            .iter()
            .filter_map(|k| row.get(*k).map(|v| ((*k).to_owned(), v.clone())))
            .collect(),
    )
}

// ---------------------------------------------------------------- verification

/// `AddressVerificationApply` fields after Pydantic validation.
#[derive(Debug, Clone)]
pub struct ReviewInput<'a> {
    pub evidence_token: &'a str,
    pub expected_revision: i64,
    pub decision: &'a str,
    pub verified_road: &'a str,
    pub verified_prefix: &'a str,
    pub evidence: &'a str,
}

/// `(road, branch, evidence)` once a decision is valid for the current row.
pub type ValidReview = (String, String, String);

fn invalid_review(message: &str) -> AppError {
    AppError::validation("INVALID_ADDRESS_REVIEW", message)
}

/// `_validate_review`.
pub fn validate_review(
    report: &Value,
    row: Option<&Value>,
    snapshot: &Map<String, Value>,
    input: &ReviewInput<'_>,
) -> Result<ValidReview, AppError> {
    let Some(row) = row.filter(|row| {
        report["geometry_current"] == true
            && row["evidence_token"] == input.evidence_token
            && row["revision"].as_i64() == Some(input.expected_revision)
    }) else {
        return Err(AppError::conflict(
            "STALE_ADDRESS_REVIEW",
            "Hồ sơ hoặc tuyến đã thay đổi, hãy tải lại",
        ));
    };
    if row["eligible"] != true {
        return Err(invalid_review(
            "Nhà đã liên kết, có số bảo vệ hoặc hình học không hợp lệ",
        ));
    }
    let road = input.verified_road.trim().to_owned();
    let mut branch = input.verified_prefix.trim().to_owned();
    let evidence = input.evidence.trim().to_owned();
    let evidence_len = evidence.chars().count();
    if UNVERIFIED_ROADS.contains(&road_name(&road).as_str())
        || road.chars().count() > 200
        || !(10..=500).contains(&evidence_len)
    {
        return Err(invalid_review("Cần địa chỉ xác minh và căn cứ cụ thể"));
    }
    if !branch.is_empty() {
        branch =
            canonical_prefix(&branch).map_err(|_| invalid_review("Tiền tố hẻm không hợp lệ"))?;
    }
    let label =
        address_label(&text(snapshot.get("name").unwrap_or(&Value::Null))).unwrap_or_default();
    let same = road_name(&road) == label.0 && alley_name(&branch) == label.1.to_lowercase();
    let valid = match input.decision {
        "belongs" => same && row["can_belong"] == true,
        "other_address" => !same,
        _ => false,
    };
    if !valid {
        return Err(invalid_review(
            "Quyết định không khớp địa chỉ hoặc khoảng cách",
        ));
    }
    Ok((road, branch, evidence))
}

// ---------------------------------------------------------------- suggestions

fn route_score(
    raw_road: &str,
    raw_prefix: &str,
    route: &Value,
    distance: f64,
    stage: (&str, &str),
) -> Value {
    let name = road_name(&text(&route["road"]));
    let prefix = alley_name(&text(&route["prefix"]));
    let road_match = !raw_road.is_empty() && !UNKNOWN_ROADS.contains(&raw_road) && raw_road == name;
    let prefix_match = !raw_prefix.is_empty() && raw_prefix == prefix;
    let road_conflict = !UNKNOWN_ROADS.contains(&raw_road) && !road_match;
    let prefix_conflict = !raw_prefix.is_empty() && !prefix_match;
    let staged = route["origin"] == "staged";
    let same = name == road_name(stage.0) && prefix == alley_name(stage.1);
    let base = 30 * i64::from(road_match) + 35 * i64::from(prefix_match) + 15 * i64::from(staged);
    let penalty = 20 * i64::from(road_conflict) + 20 * i64::from(prefix_conflict);
    let near = 1.0 - distance / NEAR_M;
    // Python `max(0, x)` keeps the int 0, so the score stays an int past 80 m.
    let score = if near > 0.0 {
        json!(py_round(base as f64 + 20.0 * near - penalty as f64, 1))
    } else {
        json!(base - penalty)
    };
    let mut signals = Vec::new();
    if road_match {
        signals.push("road_match");
    }
    if prefix_match {
        signals.push("prefix_match");
    }
    if staged {
        signals.push("original_code");
    }
    signals.push(if distance <= CLOSE_M {
        "within_35m"
    } else if distance <= NEAR_M {
        "within_80m"
    } else {
        "far"
    });
    if road_conflict {
        signals.push("road_conflict");
    }
    if prefix_conflict {
        signals.push("prefix_conflict");
    }
    if UNKNOWN_ROADS.contains(&raw_road) {
        signals.push("missing_road");
    }
    if raw_prefix.is_empty() {
        signals.push("missing_prefix");
    }
    if route.get("status") == Some(&json!("draft")) {
        signals.push("draft_route");
    }
    let mut out = route.as_object().cloned().unwrap_or_default();
    out.insert("distance_m".into(), json!(py_round(distance, 2)));
    out.insert("score".into(), score);
    out.insert("signals".into(), json!(signals));
    out.insert("full_match".into(), json!(road_match && prefix_match));
    out.insert("conflict".into(), json!(road_conflict || prefix_conflict));
    out.insert(
        "decision".into(),
        json!(if same { "belongs" } else { "other_address" }),
    );
    out.insert("same".into(), json!(same));
    Value::Object(out)
}

/// `candidate`: advisory score of one route for one queued building.
pub fn candidate(
    row: &Value,
    route: &Value,
    distance: f64,
    stage_road: &str,
    stage_prefix: &str,
) -> Value {
    let raw_road = road_name(&text(&row["road"]));
    let raw_prefix = alley_name(&text(&row["alley"]));
    let mut out = route_score(
        &raw_road,
        &raw_prefix,
        route,
        distance,
        (stage_road, stage_prefix),
    );
    let same = out["same"] == true;
    let usable = row["eligible"] == true
        && distance <= if same { CLOSE_M } else { NEAR_M }
        && (!same || row["can_belong"] == true);
    if let Some(map) = out.as_object_mut() {
        map.remove("same");
        map.insert("can_prefill".into(), json!(usable));
    }
    out
}

fn num(value: &Value) -> f64 {
    value.as_f64().unwrap_or(0.0)
}

/// `rank`: priority of one building and its top candidates.
pub fn rank(
    row: &Value,
    available: &[(Value, f64)],
    stage_road: &str,
    stage_prefix: &str,
    ambiguous: bool,
) -> Value {
    if row["eligible"] != true {
        return json!({"priority": "blocked", "candidates": []});
    }
    let mut candidates: Vec<Value> = available
        .iter()
        .map(|(r, d)| candidate(row, r, *d, stage_road, stage_prefix))
        .collect();
    candidates.sort_by(|a, b| {
        num(&b["score"])
            .partial_cmp(&num(&a["score"]))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(
                num(&a["distance_m"])
                    .partial_cmp(&num(&b["distance_m"]))
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
            .then(text(&a["key"]).cmp(&text(&b["key"])))
    });
    let exact: Vec<&Value> = candidates
        .iter()
        .filter(|c| c["full_match"] == true)
        .collect();
    let top = candidates.first();
    let priority = if !ambiguous
        && exact.len() == 1
        && num(&exact[0]["distance_m"]) <= CLOSE_M
        && exact[0]["can_prefill"] == true
        && exact[0]["status"] == "confirmed"
    {
        "strong"
    } else if !ambiguous
        && exact.len() <= 1
        && top.is_some_and(|c| {
            c["can_prefill"] == true && c["conflict"] != true && num(&c["distance_m"]) <= CLOSE_M
        })
    {
        "partial"
    } else if candidates.iter().any(|c| c["can_prefill"] == true) {
        "conflict"
    } else {
        "unmatched"
    };
    let stage = candidates.iter().find(|c| c["origin"] == "staged").cloned();
    let mut shown: Vec<Value> = candidates.into_iter().take(5).collect();
    if let Some(stage) = stage {
        if !shown.contains(&stage) {
            shown.push(stage);
        }
    }
    json!({"priority": priority, "candidates": shown})
}

fn zero_counts() -> BTreeMap<&'static str, i64> {
    ["strong", "partial", "conflict", "unmatched", "blocked"]
        .into_iter()
        .map(|k| (k, 0))
        .collect()
}

/// Suggestion counts when the page is empty (no routes are read).
pub fn empty_suggestion_counts() -> Value {
    json!(zero_counts())
}

/// Catalog routes a queued building may belong to, by address.
pub struct SuggestionCatalog {
    stage_road: String,
    stage_prefix: String,
    stage_route: Value,
    catalog: BTreeMap<String, Value>,
    by_address: HashMap<(String, String), Vec<String>>,
}

/// Without a rooted parent or a matching label every building is only
/// `unmatched`/`blocked`.
pub fn plain_suggestions(items: &mut [Value]) -> Value {
    let mut counts = zero_counts();
    for row in items.iter_mut() {
        let priority = if row["eligible"] == true {
            "unmatched"
        } else {
            "blocked"
        };
        row["suggestions"] = json!({"priority": priority, "candidates": []});
        *counts.entry(priority).or_default() += 1;
    }
    json!(counts)
}

/// Build the catalog for the staged route (`None` → [`plain_suggestions`]).
pub fn suggestion_catalog(
    snapshot: &Map<String, Value>,
    stage_id: &str,
    routes: Vec<Value>,
) -> Option<SuggestionCatalog> {
    let index = RouteIndex::new(routes);
    let parent = text(snapshot.get("parent_id").unwrap_or(&Value::Null));
    let root = index.root.get(&parent)?;
    let stage_road = text(&index.routes[root]["name"]);
    let stage_prefix = text(snapshot.get("resolved_prefix").unwrap_or(&Value::Null));
    let (label_road, label_prefix) =
        address_label(&text(snapshot.get("name").unwrap_or(&Value::Null)))?;
    if road_name(&stage_road) != label_road
        || alley_name(&stage_prefix) != label_prefix.to_lowercase()
    {
        return None;
    }
    let stage_route = json!({
        "key": format!("stage:{stage_id}"),
        "origin": "staged",
        "name": snapshot.get("name"),
        "road": stage_road,
        "prefix": stage_prefix,
        "status": "staged",
        "source_id": snapshot.get("source_id"),
        "feature_id": snapshot.get("feature_id"),
    });
    let mut catalog = BTreeMap::new();
    let mut by_address: HashMap<(String, String), Vec<String>> = HashMap::new();
    for (rid, r) in &index.routes {
        let Some(root) = index.root.get(rid) else {
            continue;
        };
        let root_name = text(&index.routes[root]["name"]);
        let normalized = road_name(&root_name);
        if UNKNOWN_ROADS.contains(&normalized.as_str()) || normalized.contains("nuoc thai") {
            continue;
        }
        let prefix = if r["route_type"] == "alley" {
            match r["prefix"].as_str().map(canonical_prefix) {
                Some(Ok(prefix)) => prefix,
                _ => continue,
            }
        } else {
            String::new()
        };
        by_address
            .entry((normalized, alley_name(&prefix)))
            .or_default()
            .push(rid.clone());
        catalog.insert(
            rid.clone(),
            json!({
                "key": format!("route:{rid}"),
                "route_id": rid,
                "origin": "catalog",
                "name": r["name"],
                "road": root_name,
                "prefix": prefix,
                "status": r["status"],
                "source_id": text(&r["linked_source_id"]),
                "feature_id": r["linked_feature_id"],
            }),
        );
    }
    Some(SuggestionCatalog {
        stage_road,
        stage_prefix,
        stage_route,
        catalog,
        by_address,
    })
}

fn address_of(row: &Value) -> (String, String) {
    (
        road_name(&text(&row["road"])),
        alley_name(&text(&row["alley"])),
    )
}

impl SuggestionCatalog {
    pub fn is_empty(&self) -> bool {
        self.catalog.is_empty()
    }

    pub fn route_ids(&self) -> Vec<String> {
        self.catalog.keys().cloned().collect()
    }

    /// `(building, route)` pairs measured even beyond the 80 m radius.
    pub fn pairs(&self, items: &[Value]) -> Vec<(String, String)> {
        let mut pairs = Vec::new();
        for row in items.iter().filter(|r| r["eligible"] == true) {
            let (name, branch) = address_of(row);
            if UNKNOWN_ROADS.contains(&name.as_str()) || branch.is_empty() {
                continue;
            }
            for rid in self
                .by_address
                .get(&(name, branch))
                .into_iter()
                .flatten()
                .take(20)
            {
                pairs.push((text(&row["building_id"]), rid.clone()));
            }
        }
        pairs
    }

    /// Rank every queued building; `distances[building][route]` in metres.
    pub fn finish(
        &self,
        items: &mut [Value],
        distances: &HashMap<String, BTreeMap<String, f64>>,
    ) -> Value {
        let mut counts = zero_counts();
        for row in items.iter_mut() {
            let mut available: Vec<(Value, f64)> = distances
                .get(&text(&row["building_id"]))
                .into_iter()
                .flatten()
                .filter_map(|(rid, d)| self.catalog.get(rid).map(|r| (r.clone(), *d)))
                .collect();
            if let Some(d) = row["distance_m"].as_f64() {
                available.push((self.stage_route.clone(), d));
            }
            let (name, branch) = address_of(row);
            let ambiguous =
                !branch.is_empty() && self.by_address.get(&(name, branch)).map_or(0, Vec::len) > 1;
            let result = rank(
                row,
                &available,
                &self.stage_road,
                &self.stage_prefix,
                ambiguous,
            );
            let priority = result["priority"]
                .as_str()
                .unwrap_or("unmatched")
                .to_owned();
            *counts
                .entry(match priority.as_str() {
                    "strong" => "strong",
                    "partial" => "partial",
                    "conflict" => "conflict",
                    "blocked" => "blocked",
                    _ => "unmatched",
                })
                .or_default() += 1;
            row["suggestions"] = result;
        }
        json!(counts)
    }
}

// ---------------------------------------------------------------- batches

/// `group_key`: only non-conflicting missing fields of one source code batch together.
pub fn group_key(snapshot: &Map<String, Value>, row: &Value) -> Option<String> {
    if row["status"] != "pending" || row["eligible"] != true || row["can_belong"] != true {
        return None;
    }
    let (root, prefix) = address_label(&text(snapshot.get("name").unwrap_or(&Value::Null)))?;
    if root.is_empty() || prefix.is_empty() {
        return None;
    }
    let (road, alley) = address_of(row);
    let kind = text(&row["kind"]);
    let shared = (kind == "missing_prefix" && road == root && alley.is_empty())
        || (kind == "missing_road" && road.is_empty() && alley == prefix.to_lowercase());
    if !shared || text(&row["code"]).is_empty() {
        return None;
    }
    Some(py_digest(&json!([
        identity(snapshot),
        kind,
        road,
        alley,
        row["code"]
    ])))
}

/// Groups of at least two pending rows, largest first.
pub fn batch_candidates(
    snapshot: &Map<String, Value>,
    items: &[Value],
) -> Vec<(String, Vec<Value>)> {
    let mut order: Vec<String> = Vec::new();
    let mut groups: HashMap<String, Vec<Value>> = HashMap::new();
    for row in items {
        if let Some(key) = group_key(snapshot, row) {
            groups
                .entry(key.clone())
                .or_insert_with(|| {
                    order.push(key);
                    Vec::new()
                })
                .push(row.clone());
        }
    }
    let mut out: Vec<(String, Vec<Value>)> = order
        .into_iter()
        .filter_map(|k| groups.remove(&k).map(|rows| (k, rows)))
        .filter(|(_, rows)| rows.len() >= 2)
        .collect();
    out.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then(a.0.cmp(&b.0)));
    out
}

/// `verified_path` once the parent's root street name is known.
pub fn verified_path(
    snapshot: &Map<String, Value>,
    root: Option<&str>,
) -> Option<(String, String)> {
    let (label_road, label_prefix) =
        address_label(&text(snapshot.get("name").unwrap_or(&Value::Null)))?;
    let root = root.filter(|r| !r.is_empty())?;
    let prefix = text(snapshot.get("resolved_prefix").unwrap_or(&Value::Null));
    if road_name(root) != label_road || alley_name(&prefix) != label_prefix.to_lowercase() {
        return None;
    }
    Some((root.to_owned(), prefix))
}

/// `group_list` rows for a verified path.
pub fn batch_groups(groups: Vec<(String, Vec<Value>)>, path: &(String, String)) -> Vec<Value> {
    const MEMBER: [&str; 8] = [
        "building_id",
        "source_id",
        "feature_id",
        "distance_m",
        "evidence_token",
        "revision",
        "road",
        "alley",
    ];
    groups
        .into_iter()
        .map(|(key, rows)| {
            let distances: Vec<f64> = rows.iter().map(|r| num(&r["distance_m"])).collect();
            let pick_dist = |best: fn(f64, f64) -> f64| {
                rows.iter()
                    .zip(&distances)
                    .fold(None::<(f64, &Value)>, |acc, (r, d)| match acc {
                        Some((a, _)) if best(a, *d) == a => acc,
                        _ => Some((*d, &r["distance_m"])),
                    })
                    .map(|(_, v)| v.clone())
            };
            let members: Vec<Value> = rows
                .iter()
                .map(|r| pick(r.as_object().unwrap_or(&Map::new()), &MEMBER))
                .collect();
            json!({
                "key": key,
                "kind": rows[0]["kind"],
                "road": rows[0]["road"],
                "alley": rows[0]["alley"],
                "code": rows[0]["code"],
                "verified_road": path.0,
                "verified_prefix": path.1,
                "count": rows.len(),
                "distance_min": pick_dist(f64::min),
                "distance_max": pick_dist(f64::max),
                "members": members,
            })
        })
        .collect()
}

// ---------------------------------------------------------------- verified links

/// `link_status` of one recorded decision against the current report row.
pub fn link_status(
    record: &Value,
    current: Option<&Value>,
    target_id: &str,
    path: (&str, &str),
) -> &'static str {
    if record["decision"] != "belongs" {
        return "other_address";
    }
    if record["state"] != "active" || record["eligibility"] == "ineligible" {
        return "blocked";
    }
    if truthy(record.get("route_id")) {
        return if text(&record["route_id"]) == target_id {
            "linked"
        } else {
            "blocked"
        };
    }
    if record["locked"] == true {
        return "blocked";
    }
    let Some(current) = current.filter(|c| c["status"] == "verified" && c["decision"] == "belongs")
    else {
        return "stale";
    };
    if current["can_belong"] != true
        || record["evidence_token"] != current["evidence_token"]
        || record["review_revision"] != current["revision"]
    {
        return "stale";
    }
    if road_name(&text(&record["verified_road"])) != road_name(path.0)
        || alley_name(&text(&record["verified_prefix"])) != alley_name(path.1)
    {
        return "stale";
    }
    "ready"
}

/// Plan items, counts and token of `_plan` once the route and report are current.
pub fn link_plan(
    snapshot: &Map<String, Value>,
    stage_revision: &Value,
    route: &Value,
    records: &[Value],
    report: &Value,
    path: (&str, &str),
) -> Value {
    let current: HashMap<String, &Value> = report["items"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|r| (text(&r["building_id"]), r))
        .collect();
    let target = text(&route["id"]);
    let mut counts: BTreeMap<&str, i64> = LINK_STATUSES.iter().map(|k| (*k, 0)).collect();
    let mut items = Vec::new();
    for r in records {
        let bid = text(&r["building_id"]);
        let c = current.get(&bid).copied();
        let status = link_status(r, c, &target, path);
        *counts.entry(status).or_default() += 1;
        let from_current = |key: &str| c.map_or(Value::Null, |c| c[key].clone());
        items.push(json!({
            "building_id": bid,
            "review_entry_id": text(&r["review_entry_id"]),
            "review_revision": r["review_revision"],
            "building_revision": r["building_revision"],
            "source_id": text(&r["source_id"]),
            "feature_id": r["feature_id"],
            "decision": r["decision"],
            "verified_road": r["verified_road"],
            "verified_prefix": r["verified_prefix"],
            "evidence": r["evidence"],
            "evidence_token": r["evidence_token"],
            "current_token": from_current("evidence_token"),
            "distance_m": from_current("distance_m"),
            "raw_road": from_current("road"),
            "raw_alley": from_current("alley"),
            "linked_route_id": if truthy(r.get("route_id")) { json!(text(&r["route_id"])) } else { Value::Null },
            "status": status,
        }));
    }
    let token = py_digest(&json!([identity(snapshot), stage_revision, route, items]));
    json!({
        "state": "current",
        "route": route,
        "counts": counts,
        "items": items,
        "preview_token": token,
    })
}

/// `_plan` when no current promoted route or report backs the links.
pub fn link_unavailable(state: &str, route: Value) -> Value {
    let counts: BTreeMap<&str, i64> = LINK_STATUSES.iter().map(|k| (*k, 0)).collect();
    json!({"state": state, "route": route, "counts": counts, "items": [], "preview_token": null})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obj(v: Value) -> Map<String, Value> {
        v.as_object().cloned().unwrap()
    }

    fn stage() -> Value {
        json!({"key": "s", "origin": "staged", "name": "Đường A hẻm 91", "road": "Đường A",
               "prefix": "91", "status": "staged"})
    }

    fn child() -> Value {
        json!({"key": "c", "origin": "catalog", "name": "Đường A hẻm 91/14", "road": "Đường A",
               "prefix": "91/14", "status": "confirmed"})
    }

    fn street() -> Value {
        json!({"key": "r", "origin": "catalog", "name": "Đường A", "road": "Đường A",
               "prefix": "", "status": "confirmed"})
    }

    fn row(road: &str, alley: &str, eligible: bool, can_belong: bool) -> Value {
        json!({"road": road, "alley": alley, "eligible": eligible, "can_belong": can_belong})
    }

    fn signals(c: &Value) -> Vec<&str> {
        c["signals"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap())
            .collect()
    }

    #[test]
    fn missing_prefix_is_not_street_frontage() {
        let p = rank(
            &row("Đường A", "", true, true),
            &[(street(), 1.0), (stage(), 5.0)],
            "Đường A",
            "91",
            false,
        );
        assert_eq!(p["priority"], "partial");
        assert_eq!(p["candidates"][0]["key"], "s");
        assert!(p["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["full_match"] == false));
        assert!(signals(&p["candidates"][1]).contains(&"missing_prefix"));
    }

    #[test]
    fn exact_child_is_ranked_above_staged_parent() {
        let p = rank(
            &row("Đường A", "91/14", true, true),
            &[(stage(), 1.0), (child(), 15.0)],
            "Đường A",
            "91",
            false,
        );
        assert_eq!(p["priority"], "strong");
        assert_eq!(p["candidates"][0]["decision"], "other_address");
        assert_eq!(p["candidates"][0]["prefix"], "91/14");
    }

    #[test]
    fn wrong_road_is_conflict_even_when_close() {
        let p = rank(
            &row("Đường B", "91", true, true),
            &[(stage(), 0.0)],
            "Đường A",
            "91",
            false,
        );
        assert_eq!(p["priority"], "conflict");
        assert!(signals(&p["candidates"][0]).contains(&"road_conflict"));
    }

    #[test]
    fn duplicate_or_draft_not_strong() {
        let mut c2 = child();
        c2["key"] = json!("c2");
        let p = rank(
            &row("Đường A", "91/14", true, true),
            &[(child(), 2.0), (c2, 3.0)],
            "Đường A",
            "91",
            true,
        );
        assert_eq!(p["priority"], "conflict");
        let mut draft = child();
        draft["status"] = json!("draft");
        let p = rank(
            &row("Đường A", "91/14", true, true),
            &[(draft, 2.0)],
            "Đường A",
            "91",
            false,
        );
        assert_eq!(p["priority"], "partial");
    }

    #[test]
    fn distance_and_lock_prevent_prefill() {
        let p = rank(
            &row("Đường A", "", true, false),
            &[(stage(), 35.001)],
            "Đường A",
            "91",
            false,
        );
        assert_eq!(p["priority"], "unmatched");
        assert_eq!(p["candidates"][0]["can_prefill"], false);
        let p = rank(
            &row("Đường A", "", false, true),
            &[(stage(), 1.0)],
            "Đường A",
            "91",
            false,
        );
        assert_eq!(p, json!({"priority": "blocked", "candidates": []}));
        let p = rank(
            &row("Đường A", "91/14", true, true),
            &[(child(), 81.0)],
            "Đường A",
            "91",
            false,
        );
        assert_eq!(p["priority"], "unmatched");
        assert_eq!(p["candidates"][0]["can_prefill"], false);
        // Past 80 m Python keeps an int score.
        assert!(p["candidates"][0]["score"].is_i64());
    }

    fn batch_stage() -> Map<String, Value> {
        obj(json!({"name": "Đường A hẻm 91", "resolved_prefix": "91", "code": "legacy"}))
    }

    fn pending(over: Value) -> Value {
        let mut r = json!({"status": "pending", "eligible": true, "can_belong": true,
            "kind": "missing_prefix", "road": "Đường A", "alley": "", "code": "legacy"});
        for (k, v) in over.as_object().unwrap() {
            r[k] = v.clone();
        }
        r
    }

    #[test]
    fn batch_group_keys() {
        let s = batch_stage();
        let base = group_key(&s, &pending(json!({})));
        assert!(base.is_some());
        assert_eq!(base, group_key(&s, &pending(json!({"road": "Duong A"}))));
        assert_ne!(base, group_key(&s, &pending(json!({"code": "other"}))));
        for over in [
            json!({"kind": "different_road", "road": "B"}),
            json!({"kind": "descendant", "alley": "91/14"}),
            json!({"status": "verified"}),
            json!({"eligible": false}),
            json!({"can_belong": false}),
            json!({"code": ""}),
        ] {
            assert_eq!(group_key(&s, &pending(over.clone())), None, "{over}");
        }
        assert!(group_key(
            &s,
            &pending(json!({"kind": "missing_road", "road": "", "alley": "91"}))
        )
        .is_some());
        assert!(group_key(
            &s,
            &pending(json!({"kind": "missing_road", "road": "", "alley": "91/14"}))
        )
        .is_none());
        assert!(group_key(
            &s,
            &pending(json!({"kind": "missing_road", "road": "", "alley": ""}))
        )
        .is_none());
    }

    fn record(over: Value) -> Value {
        let mut r = json!({"decision": "belongs", "state": "active", "eligibility": "eligible",
            "route_id": null, "locked": false, "evidence_token": "abc", "review_revision": 1,
            "verified_road": "Đường A", "verified_prefix": "91"});
        for (k, v) in over.as_object().unwrap() {
            r[k] = v.clone();
        }
        r
    }

    fn current(over: Value) -> Value {
        let mut c = json!({"status": "verified", "decision": "belongs", "can_belong": true,
            "evidence_token": "abc", "revision": 1});
        for (k, v) in over.as_object().unwrap() {
            c[k] = v.clone();
        }
        c
    }

    #[test]
    fn link_statuses() {
        let path = ("Đường A", "91");
        let c = current(json!({}));
        assert_eq!(
            link_status(&record(json!({})), Some(&c), "target", path),
            "ready"
        );
        assert_eq!(
            link_status(
                &record(json!({"decision": "other_address"})),
                Some(&c),
                "target",
                path
            ),
            "other_address"
        );
        assert_eq!(
            link_status(&record(json!({"route_id": "target"})), None, "target", path),
            "linked"
        );
        assert_eq!(
            link_status(
                &record(json!({"route_id": "another"})),
                Some(&c),
                "target",
                path
            ),
            "blocked"
        );
        assert_eq!(
            link_status(&record(json!({"locked": true})), Some(&c), "target", path),
            "blocked"
        );
        assert_eq!(
            link_status(
                &record(json!({"eligibility": "ineligible"})),
                Some(&c),
                "target",
                path
            ),
            "blocked"
        );
        for (r, cur) in [
            (record(json!({})), None),
            (
                record(json!({})),
                Some(current(json!({"can_belong": false}))),
            ),
            (record(json!({})), Some(current(json!({"revision": 2})))),
            (
                record(json!({})),
                Some(current(json!({"evidence_token": "different"}))),
            ),
            (record(json!({"verified_prefix": "91/14"})), Some(c.clone())),
        ] {
            assert_eq!(link_status(&r, cur.as_ref(), "target", path), "stale");
        }
    }

    #[test]
    fn review_kinds_and_refs() {
        assert_eq!(review_kind("a", "", "a", "91"), "missing_prefix");
        assert_eq!(review_kind("", "91", "a", "91"), "missing_road");
        assert_eq!(review_kind("a", "91/2", "a", "91"), "descendant");
        assert_eq!(review_kind("b", "91", "a", "91"), "different_road");
        assert_eq!(review_kind("a", "91", "a", "91"), "exact");
        let snap =
            obj(json!({"source_id": "s", "feature_id": "f", "code": "c", "content_revision": "r"}));
        assert_eq!(
            refs(&snap),
            vec![json!({"source_id": "s", "feature_id": "f", "code": "c", "revision": "r"})]
        );
    }
}
