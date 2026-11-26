//! Ports of `building_assignment.py`, `assignment_audit.py` and `cluster_correction.py`.
//!
//! Pure rules over rows that persistence supplies; nothing here guesses a route
//! from proximity alone or writes data.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use ovg_core::AppError;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use unicode_normalization::char::canonical_combining_class;
use unicode_normalization::UnicodeNormalization;

pub type Ids = BTreeSet<String>;

const UNNAMED: &[&str] = &["chua ro ten", "chua co ten", "khong ten"];

/// `normalize`: casefold, đ→d, strip accents, keep `[a-z0-9/]` words.
pub fn normalize(value: &str) -> String {
    // shortcut: to_lowercase + ß matches str.casefold for Vietnamese text only; port a full casefold table if other scripts appear.
    let lowered = value.to_lowercase().replace('ß', "ss").replace('đ', "d");
    let mut out = String::with_capacity(lowered.len());
    for c in lowered.nfd() {
        if canonical_combining_class(c) != 0 {
            continue;
        }
        out.push(
            if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '/' {
                c
            } else {
                ' '
            },
        );
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn strip_word<'a>(text: &'a str, words: &[&str]) -> &'a str {
    for word in words {
        if let Some(rest) = text.strip_prefix(word).and_then(|r| r.strip_prefix(' ')) {
            return rest;
        }
    }
    text
}

pub fn road_name(value: &str) -> String {
    let normalized = normalize(value);
    let name = strip_word(&normalized, &["duong", "street"]);
    if matches!(name, "ql50" | "ql 50" | "quoc lo 50") {
        "quoc lo 50".to_owned()
    } else {
        name.to_owned()
    }
}

pub fn alley_name(value: &str) -> String {
    let normalized = normalize(value);
    let v = strip_word(&normalized, &["hem", "ngo", "ngach", "nhanh", "alley"]);
    if matches!(
        v,
        "" | "0" | "khong" | "khong co" | "mat tien" | "none" | "null"
    ) {
        return String::new();
    }
    v.replace(" /", "/").replace("/ ", "/")
}

/// Unnamed or wastewater survey roads are never address routes.
pub fn non_address(road: &str) -> bool {
    UNNAMED.contains(&road) || road.contains("nuoc thai")
}

/// `route_recovery.address_label`: full "road hẻm 12/3" label, never an unnamed street.
pub fn address_label(name: &str) -> Option<(String, String)> {
    let normalized = normalize(name);
    let tokens: Vec<&str> = normalized.split(' ').collect();
    if tokens.len() < 3 {
        return None;
    }
    let (head, tail) = tokens.split_at(tokens.len() - 2);
    let syntactic = tail[1].split('/').all(|part| {
        !part.is_empty()
            && part
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
    });
    if !["hem", "ngo", "ngach", "nhanh", "alley"].contains(&tail[0]) || !syntactic {
        return None;
    }
    let prefix = crate::prefix::canonical_prefix(tail[1]).ok()?;
    Some((road_name(&head.join(" ")), prefix))
}

/// Python `str(value or "")` for a JSON attribute value.
pub fn py_str(value: &Value) -> String {
    match value {
        Value::Null | Value::Bool(false) => String::new(),
        Value::Bool(true) => "True".to_owned(),
        Value::String(text) => text.clone(),
        Value::Number(number) if number.as_f64() == Some(0.0) => String::new(),
        Value::Array(items) if items.is_empty() => String::new(),
        Value::Object(map) if map.is_empty() => String::new(),
        // shortcut: nested lists/dicts render as JSON, not Python repr; upgrade if survey attributes hold them.
        other => other.to_string(),
    }
}

/// Text of a JSON column (`->>` text or a uuid), empty for null.
pub fn text(value: &Value) -> &str {
    value.as_str().unwrap_or_default()
}

/// Python `round(x, digits)`: exact decimal rounding of the binary value.
pub fn py_round(value: f64, digits: usize) -> f64 {
    format!("{value:.digits$}").parse().unwrap_or(value)
}

/// Python `str(uuid_or_none)`.
pub fn py_id(value: &Value) -> String {
    value
        .as_str()
        .map_or_else(|| "None".to_owned(), str::to_owned)
}

pub fn sha256_hex(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

/// `json.dumps(v, ensure_ascii=False, sort_keys=True, separators=(",", ":"))` → sha256.
pub fn digest(value: &Value) -> String {
    sha256_hex(&value.to_string())
}

/// `cluster_key`: default `json.dumps` separators over the normalized address.
pub fn cluster_key(road: &str, alley: &str, code: &str) -> String {
    let parts = [road_name(road), alley_name(alley), code.trim().to_owned()]
        .map(|part| Value::String(part).to_string());
    sha256_hex(&format!("[{}]", parts.join(", ")))[..24].to_owned()
}

pub struct RouteIndex {
    pub routes: BTreeMap<String, Value>,
    pub codes: HashMap<String, Ids>,
    names: HashMap<String, Ids>,
    aliases: HashMap<(String, String), Ids>,
    pub root: BTreeMap<String, String>,
    pub root_name: BTreeMap<String, String>,
}

fn parent_of(route: &Value) -> Option<String> {
    route
        .get("parent_route_id")
        .map(py_str)
        .filter(|id| !id.is_empty())
}

impl RouteIndex {
    pub fn new(rows: Vec<Value>) -> Self {
        let routes: BTreeMap<String, Value> =
            rows.into_iter().map(|r| (py_str(&r["id"]), r)).collect();
        let mut index = Self {
            routes: BTreeMap::new(),
            codes: HashMap::new(),
            names: HashMap::new(),
            aliases: HashMap::new(),
            root: BTreeMap::new(),
            root_name: BTreeMap::new(),
        };
        for (rid, r) in &routes {
            let mut current = r;
            let mut seen = BTreeSet::from([rid.clone()]);
            let mut broken = false;
            while let Some(parent) = parent_of(current) {
                match routes.get(&parent) {
                    Some(next) if !seen.contains(&parent) => {
                        seen.insert(parent);
                        current = next;
                    }
                    _ => {
                        broken = true;
                        break;
                    }
                }
            }
            if broken || current["route_type"] != "street" {
                continue;
            }
            let root_name = road_name(text(&current["name"]));
            index.root.insert(rid.clone(), py_str(&current["id"]));
            index.root_name.insert(rid.clone(), root_name.clone());
            index.add_code(rid.clone(), rid);
            for segment in r["segments"].as_array().into_iter().flatten() {
                let code = segment.get("code").map(py_str).unwrap_or_default();
                if !code.is_empty() {
                    index.add_code(code, rid);
                }
            }
            if let Some(code) = text(&r["source_label"]).strip_prefix("route-tree-import:") {
                index.add_code(code.trim().to_owned(), rid);
            }
            match text(&r["route_type"]) {
                "street" => {
                    index
                        .names
                        .entry(root_name)
                        .or_default()
                        .insert(rid.clone());
                }
                "alley" => {
                    let aliases = BTreeSet::from([
                        alley_name(&py_str(&r["prefix"])),
                        alley_name(text(&r["name"])),
                    ]);
                    for alias in aliases.into_iter().filter(|a| !a.is_empty()) {
                        for road in [root_name.clone(), String::new()] {
                            index
                                .aliases
                                .entry((road, alias.clone()))
                                .or_default()
                                .insert(rid.clone());
                        }
                    }
                }
                _ => {}
            }
        }
        index.routes = routes;
        index
    }

    fn add_code(&mut self, code: String, rid: &str) {
        self.codes.entry(code).or_default().insert(rid.to_owned());
    }

    pub fn coded(&self, code: &str) -> Ids {
        self.codes.get(code).cloned().unwrap_or_default()
    }

    /// Root street name of a rooted route.
    pub fn root_route_name(&self, rid: &str) -> Value {
        self.routes[&self.root[rid]]["name"].clone()
    }

    /// `RouteIndex.match`: survey code and address must agree; never nearest-guess.
    pub fn match_route(
        &self,
        road: &str,
        alley: &str,
        code: &str,
        alley_mapped: bool,
    ) -> (Ids, &'static str) {
        let name = road_name(road);
        let branch = alley_name(alley);
        let key = code.trim();
        let by_code = if key.is_empty() {
            Ids::new()
        } else {
            self.coded(key)
        };
        let candidates = if !branch.is_empty() {
            self.aliases.get(&(name.clone(), branch.clone()))
        } else if !name.is_empty() {
            self.names.get(&name)
        } else {
            None
        }
        .cloned()
        .unwrap_or_default();
        if !key.is_empty() {
            // A reviewed alias is only valid for its exact original code, road and
            // full prefix. Never override a real canonical address or another code.
            if by_code.len() == 1 && candidates.is_empty() && !name.is_empty() && alley_mapped {
                let route = &self.routes[by_code.first().map_or("", String::as_str)];
                let reviewed = route["reviewed_aliases"].as_array().is_some_and(|aliases| {
                    aliases.iter().any(|alias| {
                        alias.get("code").and_then(Value::as_str) == Some(key)
                            && alias.get("normalized_road").and_then(Value::as_str)
                                == Some(name.as_str())
                            && alias.get("alley").and_then(Value::as_str) == Some(branch.as_str())
                    })
                });
                if reviewed && branch == alley_name(&py_str(&route["prefix"])) {
                    return (by_code, "exact_code");
                }
            }
            if by_code.is_empty() {
                return (candidates, "unknown_code");
            }
            if !name.is_empty() || !branch.is_empty() {
                if !candidates.is_empty() && candidates.is_disjoint(&by_code) {
                    return (&candidates | &by_code, "attribute_conflict");
                }
                if !name.is_empty() && by_code.iter().any(|r| self.root_name.get(r) != Some(&name))
                {
                    return (by_code, "attribute_conflict");
                }
                if !branch.is_empty() && candidates.is_empty() {
                    return (by_code, "attribute_conflict");
                }
                if alley_mapped
                    && branch.is_empty()
                    && by_code
                        .iter()
                        .any(|r| self.routes[r]["route_type"] == "alley")
                {
                    return (by_code, "attribute_conflict");
                }
            }
            return (by_code, "exact_code");
        }
        if !branch.is_empty() {
            let reason = if !name.is_empty() && !candidates.is_empty() {
                "exact_address"
            } else if !candidates.is_empty() {
                "alley_only"
            } else {
                "unknown_address"
            };
            return (candidates, reason);
        }
        if !name.is_empty() {
            let reason = if alley_mapped && !candidates.is_empty() {
                "exact_address"
            } else if !candidates.is_empty() {
                "road_only"
            } else {
                "unknown_address"
            };
            return (candidates, reason);
        }
        (Ids::new(), "no_attributes")
    }
}

fn first(ids: &Ids) -> Option<String> {
    ids.first().cloned()
}

/// `score_candidates`: attribute match first, distance only as a tie-breaker.
pub fn score_candidates(
    index: &RouteIndex,
    mut row: Map<String, Value>,
    textual: &Ids,
    reason: &str,
    distances: &HashMap<String, f64>,
    max_distance: f64,
) -> Map<String, Value> {
    let locked = row.get("locked") == Some(&Value::Bool(true));
    if locked || row.get("geometry_usable") != Some(&Value::Bool(true)) {
        row.insert("confidence".into(), json!("blocked"));
        let why = if locked {
            "official_number"
        } else {
            "invalid_geometry"
        };
        row.insert("reason".into(), json!(why));
        row.insert("candidates".into(), json!([]));
        row.insert("suggested_route_id".into(), Value::Null);
        return row;
    }
    let exact = textual.len() == 1 && matches!(reason, "exact_code" | "exact_address");
    let all: Ids = textual.iter().chain(distances.keys()).cloned().collect();
    let mut items: Vec<(f64, f64, String, bool, Value)> = Vec::new();
    for rid in all {
        let (Some(root), Some(&distance)) = (index.root.get(&rid), distances.get(&rid)) else {
            continue;
        };
        let r = &index.routes[&rid];
        let attr = textual.contains(&rid);
        let base = if attr && exact {
            70
        } else if attr {
            45
        } else {
            0
        };
        let bonus = 30.0 * (1.0 - distance / max_distance);
        // Python `max(0, bonus)` keeps the int 0, so the score stays an int.
        let (score, score_json) = if bonus > 0.0 {
            let s = py_round(f64::from(base) + bonus, 1);
            (s, json!(s))
        } else {
            (f64::from(base), json!(base))
        };
        let rounded = py_round(distance, 2);
        items.push((
            score,
            rounded,
            rid.clone(),
            attr,
            json!({
                "route_id": rid,
                "name": r["name"],
                "root_name": index.routes[root]["name"],
                "prefix": r["prefix"],
                "route_status": r["status"],
                "route_revision": r["revision"],
                "content_revision": r["content_revision"],
                "distance_m": rounded,
                "score": score_json,
                "attribute_match": attr,
            }),
        ));
    }
    items.sort_by(|a, b| {
        b.0.total_cmp(&a.0)
            .then(a.1.total_cmp(&b.1))
            .then_with(|| a.2.cmp(&b.2))
    });
    row.insert("reason".into(), json!(reason));
    let Some((_, best_distance, best_id, best_attr, _)) = items.first().cloned() else {
        row.insert("confidence".into(), json!("unmatched"));
        row.insert("candidates".into(), json!([]));
        row.insert("suggested_route_id".into(), Value::Null);
        return row;
    };
    let confidence = if exact && best_attr && best_distance <= max_distance.min(35.0) {
        "high"
    } else if exact && best_attr && best_distance <= max_distance {
        "medium"
    } else if matches!(
        reason,
        "attribute_conflict" | "unknown_code" | "unknown_address"
    ) || textual.len() > 1
    {
        "conflict"
    } else {
        "low"
    };
    row.insert("confidence".into(), json!(confidence));
    row.insert(
        "candidates".into(),
        Value::Array(items.into_iter().take(10).map(|item| item.4).collect()),
    );
    row.insert(
        "suggested_route_id".into(),
        if matches!(confidence, "high" | "medium") {
            json!(best_id)
        } else {
            Value::Null
        },
    );
    row
}

/// Attribute names mapped to road / alley / survey code (`mapping.get(k, "")`).
#[derive(Debug, Clone, Default)]
pub struct Mapping {
    pub road: String,
    pub alley: String,
    pub code: String,
}

/// One page of `BuildingRouteAssigner.preview` before distances are known.
pub struct PreviewPage {
    pub rows: Vec<Map<String, Value>>,
    matched: Vec<(Ids, &'static str)>,
    /// `(building_id, route_id)` textual candidates to measure.
    pub pairs: Vec<(String, String)>,
    /// Usable, unlocked buildings to measure.
    pub ids: Vec<String>,
}

pub fn preview_match(
    index: &RouteIndex,
    page: &[Value],
    mapping: &Mapping,
    strict: bool,
) -> PreviewPage {
    let mut out = PreviewPage {
        rows: Vec::new(),
        matched: Vec::new(),
        pairs: Vec::new(),
        ids: Vec::new(),
    };
    let empty = Map::new();
    for b in page {
        let attrs = b["attributes"].as_object().unwrap_or(&empty);
        let get = |key: &str| attrs.get(key).cloned().unwrap_or(Value::Null);
        let (road, alley, code) = (get(&mapping.road), get(&mapping.alley), get(&mapping.code));
        let rid = py_str(&b["id"]);
        let alley_present = !mapping.alley.is_empty() && attrs.contains_key(&mapping.alley);
        let road_text = py_str(&road);
        let (mut ids, mut reason) =
            index.match_route(&road_text, &py_str(&alley), &py_str(&code), alley_present);
        let normalized = road_name(&road_text);
        if strict && (normalized.is_empty() || !alley_present) {
            (ids, reason) = (Ids::new(), "incomplete_address");
        }
        if strict && non_address(&normalized) {
            (ids, reason) = (Ids::new(), "non_address_route");
        }
        if strict && (ids.len() != 1 || !matches!(reason, "exact_code" | "exact_address")) {
            ids = Ids::new();
        }
        let usable = b["geometry_usable"] == true;
        let locked = b["locked"] == true;
        if usable && !locked {
            out.pairs
                .extend(ids.iter().take(20).map(|v| (rid.clone(), v.clone())));
            out.ids.push(rid.clone());
        }
        let area = py_str(&b["area_id"]);
        let row = json!({
            "building_id": rid,
            "source_id": py_id(&b["linked_source_id"]),
            "feature_id": py_id(&b["linked_feature_id"]),
            "revision": b["revision"],
            "content_revision": b["content_revision"],
            "area_id": if area.is_empty() { Value::Null } else { json!(area) },
            "road": road_text,
            "alley": py_str(&alley),
            "code": py_str(&code),
            "geometry_usable": usable,
            "locked": locked,
        });
        out.rows.push(row.as_object().cloned().unwrap_or_default());
        out.matched.push((ids, reason));
    }
    out
}

/// Score a preview page and seal it with the Python-compatible preview token.
pub fn preview_finish(
    index: &RouteIndex,
    page: PreviewPage,
    distances: &HashMap<String, HashMap<String, f64>>,
    max_distance: f64,
    strict: bool,
) -> (Vec<Value>, Value, String) {
    let none = HashMap::new();
    let mut result: Vec<Value> = page
        .rows
        .into_iter()
        .zip(page.matched)
        .map(|(row, (ids, reason))| {
            let d = distances.get(&py_str(&row["building_id"])).unwrap_or(&none);
            Value::Object(score_candidates(index, row, &ids, reason, d, max_distance))
        })
        .collect();
    if strict {
        for row in &mut result {
            if !matches!(text(&row["confidence"]), "high" | "blocked") {
                row["confidence"] = json!("unmatched");
                row["suggested_route_id"] = Value::Null;
                row["candidates"] = json!([]);
            }
        }
    }
    let mut counts = Map::new();
    for key in ["high", "medium", "low", "conflict", "unmatched", "blocked"] {
        let n = result.iter().filter(|r| r["confidence"] == key).count();
        counts.insert(key.into(), json!(n));
    }
    let token = digest(&Value::Array(result.clone()));
    (result, Value::Object(counts), token)
}

const GROUPS: &[(&str, &str, &str)] = &[
    (
        "ready",
        "Đủ điều kiện xem trước gán tự động",
        "Xem trước và xác nhận gán theo mã/địa chỉ.",
    ),
    (
        "parent_missing",
        "Tuyến cha thiếu hoặc sai",
        "Phục hồi hoặc vẽ tuyến cha trong phần Import / phục hồi cây tuyến.",
    ),
    (
        "prefix_invalid",
        "Tiền tố / phân cấp hẻm sai",
        "Rà lại tiền tố và cấp đường/hẻm trong phần phục hồi cây tuyến.",
    ),
    (
        "route_duplicate",
        "Tuyến trùng hoặc mơ hồ",
        "Đối soát đoạn tuyến trùng; chỉ ghép khi hình học liên tục.",
    ),
    (
        "route_pending",
        "Tuyến nguồn chưa import",
        "Kiểm tra và import tuyến nguồn bị giữ lại.",
    ),
    (
        "code_unknown",
        "Mã tuyến không tồn tại",
        "Đối soát mã tuyến nguồn và chọn tuyến đúng.",
    ),
    (
        "address_conflict",
        "Mã tuyến và địa chỉ mâu thuẫn",
        "Kiểm tra tên đường, hẻm và mã; xác minh trên bản đồ.",
    ),
    (
        "address_ambiguous",
        "Địa chỉ khớp nhiều tuyến",
        "Chọn đúng tuyến theo địa chỉ và cửa vào.",
    ),
    (
        "address_missing",
        "Thiếu tên đường / thông tin hẻm",
        "Bổ sung thuộc tính khảo sát; không suy diễn từ tuyến gần.",
    ),
    (
        "route_missing",
        "Chưa có tuyến khớp địa chỉ",
        "Kiểm tra tên đường/hẻm hoặc bổ sung tuyến GIS.",
    ),
    (
        "distance",
        "Vị trí xa tuyến khớp địa chỉ",
        "Xác minh cửa vào và hình học trước khi gán.",
    ),
    (
        "non_address_route",
        "Tên tuyến chưa rõ / không phải đường địa chỉ",
        "Xác minh tên chính thức và loại tuyến trước khi gán.",
    ),
    (
        "invalid_geometry",
        "Hình học công trình không dùng được",
        "Sửa hình học/lớp dữ liệu nguồn.",
    ),
    (
        "locked",
        "Có số nhà được bảo vệ",
        "Kiểm tra hồ sơ số nhà và quyền chuyển tuyến.",
    ),
];

pub fn audit_group_valid(group: &str) -> bool {
    GROUPS.iter().any(|(key, _, _)| *key == group)
}

pub fn classify_pending(reasons: &BTreeSet<&str>) -> &'static str {
    let any = |set: &[&str]| set.iter().any(|r| reasons.contains(r));
    if any(&["ambiguous_sibling_prefix"]) {
        "route_duplicate"
    } else if any(&[
        "parent_required",
        "parent_not_found",
        "parent_has_errors",
        "parent_prefix_missing",
        "street_cannot_have_parent_or_prefix",
    ]) {
        "parent_missing"
    } else if any(&[
        "invalid_prefix",
        "prefix_must_be_one_level",
        "prefix_does_not_follow_parent",
    ]) {
        "prefix_invalid"
    } else {
        "route_pending"
    }
}

/// `audit_unlinked` classification before distances: items + pairs to measure.
pub fn audit_classify(
    index: &RouteIndex,
    pending_rows: &[Value],
    rows: &[Value],
    group: Option<&str>,
) -> (Vec<Value>, Vec<(String, String)>) {
    let mut pending: HashMap<String, Vec<&Value>> = HashMap::new();
    for row in pending_rows {
        pending
            .entry(py_str(&row["code"]).trim().to_owned())
            .or_default()
            .push(row);
    }
    let mut result = Vec::new();
    let mut pairs = Vec::new();
    for row in rows {
        let bid = py_str(&row["id"]);
        let (raw_road, raw_alley) = (text(&row["road"]), text(&row["alley"]));
        let code = text(&row["code"]).trim().to_owned();
        let road = road_name(raw_road);
        let mapped = row["alley_mapped"] == true;
        let (matched, reason) = index.match_route(raw_road, raw_alley, &code, mapped);
        let legacy: &[&Value] = if !code.is_empty() && index.coded(&code).is_empty() {
            pending.get(&code).map(Vec::as_slice).unwrap_or_default()
        } else {
            &[]
        };
        let mut route_id = Value::Null;
        let kind = if row["locked"] == true {
            "locked"
        } else if row["geometry_usable"] != true {
            "invalid_geometry"
        } else if !legacy.is_empty() {
            let reasons = legacy
                .iter()
                .flat_map(|x| text(&x["reason"]).split(';'))
                .collect();
            classify_pending(&reasons)
        } else if road.is_empty() || !mapped {
            "address_missing"
        } else if non_address(&road) {
            "non_address_route"
        } else if reason == "unknown_code" {
            "code_unknown"
        } else if reason == "attribute_conflict" {
            "address_conflict"
        } else if matched.len() > 1 {
            "address_ambiguous"
        } else if matched.len() == 1 && matches!(reason, "exact_code" | "exact_address") {
            let rid = first(&matched).unwrap_or_default();
            pairs.push((bid.clone(), rid.clone()));
            route_id = json!(rid);
            "distance"
        } else {
            "route_missing"
        };
        let mut correction = Value::Null;
        if kind == "address_conflict"
            && group == Some("address_conflict")
            && row["eligibility"] != "ineligible"
        {
            let (possible, why) = index.match_route(raw_road, raw_alley, "", mapped);
            if possible.len() == 1 && why == "exact_address" {
                let rid = first(&possible).unwrap_or_default();
                pairs.push((bid.clone(), rid.clone()));
                correction = json!(rid);
            }
        }
        let code_paths: Vec<Value> = index
            .coded(&code)
            .into_iter()
            .take(5)
            .map(|r| {
                json!({
                    "route_id": r,
                    "name": index.routes[&r]["name"],
                    "root_name": index.root_route_name(&r),
                    "prefix": index.routes[&r]["prefix"],
                })
            })
            .collect();
        let pending_items: Vec<Value> = legacy
            .iter()
            .take(5)
            .map(|x| {
                json!({
                    "source_id": py_id(&x["source_id"]),
                    "feature_id": x["feature_id"],
                    "reason": x["reason"],
                    "parent_code": x["parent_code"],
                    "name": x["name"],
                })
            })
            .collect();
        result.push(json!({
            "cluster": cluster_key(raw_road, raw_alley, &code),
            "code_paths": code_paths,
            "building_id": bid,
            "source_id": py_id(&row["linked_source_id"]),
            "feature_id": py_id(&row["linked_feature_id"]),
            "road": raw_road,
            "alley": raw_alley,
            "code": code,
            "group": kind,
            "reason": reason,
            "route_id": route_id,
            "distance_m": null,
            "correction_candidate": correction,
            "correction_ready": false,
            "pending": pending_items,
        }));
    }
    (result, pairs)
}

pub struct AuditPage<'a> {
    pub total: usize,
    pub group: Option<&'a str>,
    pub cluster: Option<&'a str>,
    pub after_id: Option<String>,
    pub limit: usize,
    pub max_distance: f64,
}

/// `audit_unlinked` report: distance upgrades, group counts, clusters and a page.
pub fn audit_report(
    mut result: Vec<Value>,
    distances: &HashMap<String, f64>,
    page: &AuditPage<'_>,
) -> Value {
    for item in &mut result {
        let Some(&distance) = distances.get(text(&item["building_id"])) else {
            continue;
        };
        item["distance_m"] = json!(py_round(distance, 2));
        if !item["correction_candidate"].is_null() {
            item["correction_ready"] = json!(distance <= 35.0);
        } else if distance <= page.max_distance.min(35.0) {
            item["group"] = json!("ready");
        }
    }
    let groups: Vec<Value> = GROUPS
        .iter()
        .map(|(key, label, action)| {
            let count = result.iter().filter(|r| r["group"] == *key).count();
            json!({"key": key, "label": label, "count": count, "action": action})
        })
        .collect();
    let scoped: Vec<&Value> = result
        .iter()
        .filter(|r| page.group.is_some_and(|g| r["group"] == g))
        .collect();
    let mut order: Vec<String> = Vec::new();
    let mut clusters: HashMap<String, Value> = HashMap::new();
    for row in &scoped {
        let key = text(&row["cluster"]).to_owned();
        let entry = clusters.entry(key.clone()).or_insert_with(|| {
            order.push(key.clone());
            json!({
                "key": key,
                "road": row["road"],
                "alley": row["alley"],
                "code": row["code"],
                "count": 0,
                "correction_ready": 0,
            })
        });
        entry["count"] = json!(entry["count"].as_i64().unwrap_or(0) + 1);
        let ready = i64::from(row["correction_ready"] == true);
        entry["correction_ready"] = json!(entry["correction_ready"].as_i64().unwrap_or(0) + ready);
    }
    let num = |v: &Value, k: &str| v[k].as_i64().unwrap_or(0);
    let mut top: Vec<&Value> = order.iter().map(|k| &clusters[k]).collect();
    top.sort_by(|a, b| {
        num(b, "correction_ready")
            .cmp(&num(a, "correction_ready"))
            .then(num(b, "count").cmp(&num(a, "count")))
            .then_with(|| text(&a["key"]).cmp(text(&b["key"])))
    });
    top.truncate(30);
    let filtered: Vec<&Value> = scoped
        .iter()
        .copied()
        .filter(|row| {
            page.cluster.is_none_or(|c| row["cluster"] == c)
                && page
                    .after_id
                    .as_deref()
                    .is_none_or(|after| text(&row["building_id"]) > after)
        })
        .collect();
    let items: Vec<&Value> = filtered.iter().copied().take(page.limit).collect();
    let next = if filtered.len() > page.limit {
        items.last().map(|row| row["building_id"].clone())
    } else {
        None
    };
    json!({
        "total": page.total,
        "groups": groups,
        "clusters": top,
        "active_cluster": page.cluster.and_then(|c| clusters.get(c)),
        "cluster_count": clusters.len(),
        "correction_ready": clusters.values().map(|c| num(c, "correction_ready")).sum::<i64>(),
        "items": items,
        "next_after_id": next,
    })
}

/// `correction_preview` before distances: per-building rows and route sets.
pub struct CorrectionPlan {
    pub items: Vec<Value>,
    pub candidates: Ids,
    pub original: Ids,
}

impl CorrectionPlan {
    pub fn eligible(&self) -> Vec<String> {
        self.items
            .iter()
            .filter(|x| x["eligible"] == true)
            .map(|x| py_str(&x["building_id"]))
            .collect()
    }

    pub fn routes(&self) -> Ids {
        &self.candidates | &self.original
    }
}

pub fn correction_items(
    index: &RouteIndex,
    data: &[Value],
    cluster: &str,
) -> Result<CorrectionPlan, AppError> {
    let mut plan = CorrectionPlan {
        items: Vec::new(),
        candidates: Ids::new(),
        original: Ids::new(),
    };
    for row in data {
        let (raw_road, raw_alley) = (text(&row["road"]), text(&row["alley"]));
        if !row["route_id"].is_null()
            || row["state"] != "active"
            || cluster_key(raw_road, raw_alley, text(&row["code"])) != cluster
        {
            return Err(AppError::conflict(
                "STALE_CORRECTION",
                "Nhà không còn thuộc cụm chưa liên kết",
            ));
        }
        let road = road_name(raw_road);
        let code = text(&row["code"]).trim().to_owned();
        let mapped = row["alley_mapped"] == true;
        let (_, source_reason) = index.match_route(raw_road, raw_alley, &code, mapped);
        let (mut possible, address_reason) = index.match_route(raw_road, raw_alley, "", mapped);
        if road.is_empty() || !mapped || non_address(&road) || address_reason != "exact_address" {
            possible = Ids::new();
        }
        if source_reason != "attribute_conflict" {
            return Err(AppError::validation(
                "NOT_CODE_CONFLICT",
                "Cụm không có mâu thuẫn mã tuyến cần sửa",
            ));
        }
        plan.candidates.extend(possible);
        plan.original.extend(index.coded(&code));
        let usable = row["geometry_usable"] == true;
        let locked = row["locked"] == true;
        let ineligible = row["eligibility"] == "ineligible";
        let blocked = if locked {
            json!("protected_number")
        } else if ineligible {
            json!("ineligible")
        } else if !usable {
            json!("invalid_geometry")
        } else {
            Value::Null
        };
        plan.items.push(json!({
            "building_id": py_str(&row["id"]),
            "revision": row["revision"],
            "source_id": py_id(&row["linked_source_id"]),
            "feature_id": row["linked_feature_id"],
            "content_revision": row["content_revision"],
            "road": raw_road,
            "alley": raw_alley,
            "code": code,
            "eligible": usable && !locked && !ineligible,
            "blocked_reason": blocked,
            "distances": {},
            "old_distances": {},
        }));
    }
    if plan.candidates.len() > 30 {
        // Do not truncate an ambiguous address and accidentally imply a unique route.
        return Err(AppError::validation(
            "AMBIGUOUS_ROUTE",
            "Địa chỉ có quá nhiều tuyến trùng; cần sửa danh mục tuyến trước",
        ));
    }
    Ok(plan)
}

/// Finish `correction_preview` with measured distances; returns the sealed payload.
pub fn correction_finish(
    index: &RouteIndex,
    mut plan: CorrectionPlan,
    distances: &HashMap<(String, String), f64>,
    cluster: &str,
    max_distance: f64,
) -> Value {
    let routes = plan.routes();
    for x in &mut plan.items {
        let bid = py_str(&x["building_id"]);
        for rid in &routes {
            let Some(&d) = distances.get(&(bid.clone(), rid.clone())) else {
                continue;
            };
            if !d.is_finite() {
                continue;
            }
            let value = json!({"distance_m": py_round(d, 2), "within_limit": d <= max_distance});
            if plan.candidates.contains(rid) {
                x["distances"][rid] = value.clone();
            }
            if plan.original.contains(rid) {
                x["old_distances"][rid] = value;
            }
        }
    }
    let items = &plan.items;
    let descriptor = |rid: &String| -> Value {
        let r = &index.routes[rid];
        let usable = items
            .iter()
            .filter(|x| x["eligible"] == true && x["distances"][rid]["within_limit"] == true)
            .count();
        let ds: Vec<f64> = items
            .iter()
            .filter_map(|x| {
                distances
                    .get(&(py_str(&x["building_id"]), rid.clone()))
                    .copied()
            })
            .collect();
        let mean = if ds.is_empty() {
            Value::Null
        } else {
            json!(py_round(ds.iter().sum::<f64>() / ds.len() as f64, 2))
        };
        json!({
            "route_id": rid,
            "name": r["name"],
            "root_name": index.root_route_name(rid),
            "prefix": r["prefix"],
            "revision": r["revision"],
            "content_revision": r["content_revision"],
            "status": r["status"],
            "within_limit": usable,
            "mean_distance_m": mean,
        })
    };
    let mut targets: Vec<Value> = plan.candidates.iter().map(descriptor).collect();
    let mean = |v: &Value| v["mean_distance_m"].as_f64().unwrap_or(f64::INFINITY);
    targets.sort_by(|a, b| {
        let within = |v: &Value| v["within_limit"].as_i64().unwrap_or(0);
        within(b)
            .cmp(&within(a))
            .then(mean(a).total_cmp(&mean(b)))
            .then_with(|| text(&a["route_id"]).cmp(text(&b["route_id"])))
    });
    let previous: Vec<Value> = plan.original.iter().map(descriptor).collect();
    let recommended = if targets.len() == 1 && targets[0]["within_limit"].as_i64() != Some(0) {
        targets[0]["route_id"].clone()
    } else {
        Value::Null
    };
    let mut payload = json!({
        "cluster": cluster,
        "items": plan.items,
        "candidates": targets,
        "original_routes": previous,
        "max_distance_m": max_distance,
        "recommended_route_id": recommended,
    });
    payload["preview_token"] = json!(digest(&payload));
    payload
}

#[cfg(test)]
mod tests {
    use super::*;

    fn route(
        id: &str,
        name: &str,
        parent: Option<&str>,
        prefix: Option<&str>,
        code: Option<&str>,
    ) -> Value {
        json!({
            "id": id,
            "name": name,
            "route_type": if parent.is_some() { "alley" } else { "street" },
            "parent_route_id": parent,
            "prefix": prefix,
            "source_label": code.map_or("manual".to_owned(), |c| format!("route-tree-import:{c}")),
            "status": "confirmed",
            "revision": 1,
            "content_revision": "a".repeat(64),
        })
    }

    fn usable() -> Map<String, Value> {
        json!({"locked": false, "geometry_usable": true})
            .as_object()
            .cloned()
            .unwrap()
    }

    fn dist(items: &[(&str, f64)]) -> HashMap<String, f64> {
        items.iter().map(|(k, v)| ((*k).to_owned(), *v)).collect()
    }

    fn set(items: &[&str]) -> Ids {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn normalizes_vietnamese_names() {
        assert_eq!(normalize("Đường Nguyễn Văn Linh"), "duong nguyen van linh");
        assert_eq!(road_name("Đường Nguyễn Văn Linh"), "nguyen van linh");
        assert_eq!(road_name("QL 50"), "quoc lo 50");
        assert_eq!(alley_name("Hẻm 155 / 3"), "155/3");
        assert_eq!(alley_name("Mặt tiền"), "");
        assert_eq!(
            address_label("Đường số 9 Ấp 21 hẻm 77"),
            Some(("so 9 ap 21".to_owned(), "77".to_owned()))
        );
        assert_eq!(
            address_label("Đường A hẻm 12/3a"),
            Some(("a".to_owned(), "12/3A".to_owned()))
        );
        assert_eq!(address_label("Hẻm 12"), None);
        assert_eq!(cluster_key("Đường Đúng", "", "x").len(), 24);
    }

    #[test]
    fn cluster_key_matches_python_json_dumps() {
        // hashlib.sha256(json.dumps(["dung", "", "x"], ensure_ascii=False).encode()).hexdigest()[:24]
        assert_eq!(
            sha256_hex("[\"dung\", \"\", \"x\"]")[..24],
            cluster_key("Đường Đúng", "", " x ")[..]
        );
    }

    #[test]
    fn address_beats_nearest_and_normalizes_vietnamese() {
        let ix = RouteIndex::new(vec![
            route("main", "Đường Nguyễn Văn Linh", None, None, None),
            route("alley", "Hẻm 155", Some("main"), Some("155"), None),
        ]);
        let (ids, reason) = ix.match_route("Nguyen Van Linh", "Hẻm 155", "", true);
        assert_eq!((ids.clone(), reason), (set(&["alley"]), "exact_address"));
        let result = score_candidates(
            &ix,
            usable(),
            &ids,
            reason,
            &dist(&[("main", 1.0), ("alley", 9.0)]),
            80.0,
        );
        assert_eq!(result["confidence"], "high");
        assert_eq!(result["suggested_route_id"], "alley");
    }

    #[test]
    fn missing_alley_is_not_equivalent_to_explicit_main_road() {
        let ix = RouteIndex::new(vec![
            route("main", "Nguyễn Văn Linh", None, None, None),
            route("alley", "Hẻm 155", Some("main"), Some("155"), None),
        ]);
        let (ids, reason) = ix.match_route("Nguyễn Văn Linh", "", "", false);
        let result = score_candidates(
            &ix,
            usable(),
            &ids,
            reason,
            &dist(&[("main", 1.0), ("alley", 2.0)]),
            80.0,
        );
        assert_eq!(result["confidence"], "low");
        assert!(result["suggested_route_id"].is_null());
    }

    #[test]
    fn duplicate_road_names_and_code_conflict_require_review() {
        let ix = RouteIndex::new(vec![
            route("a", "Đường A", None, None, Some("T01")),
            route("b", "Đường A", None, None, None),
            route("c", "Đường C", None, None, Some("T02")),
        ]);
        let (ids, reason) = ix.match_route("Đường A", "", "", true);
        assert_eq!(ids, set(&["a", "b"]));
        let result = score_candidates(
            &ix,
            usable(),
            &ids,
            reason,
            &dist(&[("a", 1.0), ("b", 5.0)]),
            80.0,
        );
        assert_eq!(result["confidence"], "conflict");
        let (ids, reason) = ix.match_route("Đường A", "", "T02", true);
        assert_eq!(reason, "attribute_conflict");
        let result = score_candidates(
            &ix,
            usable(),
            &ids,
            reason,
            &dist(&[("a", 1.0), ("c", 2.0)]),
            80.0,
        );
        assert!(result["suggested_route_id"].is_null());
    }

    #[test]
    fn geometry_only_not_high_confidence_and_locked_building_blocked() {
        let ix = RouteIndex::new(vec![route("a", "Đường A", None, None, None)]);
        let (ids, reason) = ix.match_route("", "", "", false);
        let result = score_candidates(&ix, usable(), &ids, reason, &dist(&[("a", 0.5)]), 80.0);
        assert_eq!(result["confidence"], "low");
        let mut locked = usable();
        locked.insert("locked".into(), json!(true));
        let result = score_candidates(&ix, locked, &ids, reason, &dist(&[("a", 0.5)]), 80.0);
        assert_eq!(result["confidence"], "blocked");
    }

    #[test]
    fn deep_alley_prefix_keeps_hierarchy() {
        let ix = RouteIndex::new(vec![
            route("a", "Đường A", None, None, None),
            route("b", "Hẻm 155", Some("a"), Some("155"), None),
            route("c", "Hẻm 155/3", Some("b"), Some("155/3"), None),
        ]);
        let (ids, reason) = ix.match_route("Đường A", "155 / 3", "", true);
        assert_eq!((ids, reason), (set(&["c"]), "exact_address"));
    }

    #[test]
    fn far_textual_candidate_keeps_an_integer_score() {
        let ix = RouteIndex::new(vec![route("a", "Đường A", None, None, None)]);
        let (ids, reason) = ix.match_route("Đường A", "", "", true);
        let result = score_candidates(&ix, usable(), &ids, reason, &dist(&[("a", 120.0)]), 80.0);
        assert_eq!(result["candidates"][0]["score"], json!(70));
        assert_eq!(result["confidence"], "low");
    }

    #[test]
    fn pending_reasons_classify_like_python() {
        assert_eq!(
            classify_pending(&BTreeSet::from(["parent_required", "invalid_prefix"])),
            "parent_missing"
        );
        assert_eq!(
            classify_pending(&BTreeSet::from(["invalid_prefix"])),
            "prefix_invalid"
        );
        assert_eq!(classify_pending(&BTreeSet::from(["x"])), "route_pending");
    }
}
