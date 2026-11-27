//! Ports of `route_import.py`, `route_recovery.py`, `route_topology.py` and
//! `geometry_review.stage_eligible`.
//!
//! Rows are the same loosely typed dicts Python plans over (`Row`), so the preview
//! token, a sha256 over Python's `json.dumps(sort_keys=True, ensure_ascii=False,
//! separators=(",", ":"))`, is byte-identical between runtimes. Nothing here reads or
//! writes the database; persistence supplies features, routes and measured distances.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use ovg_core::AppError;
use serde_json::{json, Map, Value};

use crate::assignment::{
    address_label, alley_name, py_round, py_str, road_name, sha256_hex, RouteIndex,
};
use crate::prefix::canonical_prefix;

pub type Row = Map<String, Value>;
/// `(source_id, feature_id)` of a linked line.
pub type FeatureKey = (String, String);
/// `(child key, parent key)` → `(endpoint distance m, parent geometry revision)`.
pub type Junctions = HashMap<(String, String), (f64, Value)>;

pub const LIMIT: usize = 5000;
pub const MAX_JUNCTION_M: f64 = 8.0;
const UNNAMED: [&str; 3] = ["chua ro ten", "chua co ten", "khong ten"];
// shortcut: Python raises RecursionError near 1000 frames; the exact depth depends on the interpreter stack, so only very deep trees may differ.
const MAX_DEPTH: usize = 980;

// ---------------------------------------------------------------- Python JSON

/// Python `repr(float)`.
pub fn py_float(value: f64) -> String {
    if value == 0.0 {
        return if value.is_sign_negative() {
            "-0.0"
        } else {
            "0.0"
        }
        .to_owned();
    }
    if !value.is_finite() {
        return if value.is_nan() {
            "NaN"
        } else if value > 0.0 {
            "Infinity"
        } else {
            "-Infinity"
        }
        .to_owned();
    }
    let sign = if value < 0.0 { "-" } else { "" };
    let sci = format!("{:e}", value.abs());
    let (mantissa, exp) = sci.split_once('e').unwrap_or((&sci, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let n = digits.len() as i32;
    let body = if (-4..16).contains(&exp) {
        if exp >= 0 {
            if n <= exp + 1 {
                format!("{digits}{}.0", "0".repeat((exp + 1 - n) as usize))
            } else {
                let cut = (exp + 1) as usize;
                format!("{}.{}", &digits[..cut], &digits[cut..])
            }
        } else {
            format!("0.{}{digits}", "0".repeat((-exp - 1) as usize))
        }
    } else {
        let mant = if n == 1 {
            digits.clone()
        } else {
            format!("{}.{}", &digits[..1], &digits[1..])
        };
        let esign = if exp < 0 { '-' } else { '+' };
        format!("{mant}e{esign}{:02}", exp.abs())
    };
    format!("{sign}{body}")
}

/// `json.dumps(v, sort_keys=True, ensure_ascii=False, separators=(",", ":"))`.
pub fn py_json(value: &Value) -> String {
    fn walk(out: &mut String, value: &Value) {
        match value {
            Value::Number(number) if number.is_f64() => {
                out.push_str(&py_float(number.as_f64().unwrap_or_default()));
            }
            Value::Array(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    walk(out, item);
                }
                out.push(']');
            }
            Value::Object(map) => {
                out.push('{');
                for (i, (key, item)) in map.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    out.push_str(&Value::String(key.clone()).to_string());
                    out.push(':');
                    walk(out, item);
                }
                out.push('}');
            }
            other => out.push_str(&other.to_string()),
        }
    }
    let mut out = String::new();
    walk(&mut out, value);
    out
}

/// sha256 hex of [`py_json`].
pub fn py_digest(value: &Value) -> String {
    sha256_hex(&py_json(value))
}

/// Python `str(value).strip()` (empty for `None`).
pub fn py_text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.trim().to_owned(),
        Value::Bool(flag) => if *flag { "True" } else { "False" }.to_owned(),
        Value::Number(number) if number.is_f64() => py_float(number.as_f64().unwrap_or_default()),
        // shortcut: lists/dicts render as JSON, not Python repr; upgrade if layer attributes hold them.
        other => other.to_string().trim().to_owned(),
    }
}

/// Python truthiness of an optional JSON value.
pub fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(flag)) => *flag,
        Some(Value::Number(number)) => number.as_f64() != Some(0.0),
        Some(Value::String(text)) => !text.is_empty(),
        Some(Value::Array(items)) => !items.is_empty(),
        Some(Value::Object(map)) => !map.is_empty(),
    }
}

// ---------------------------------------------------------------- row helpers

pub fn s<'a>(row: &'a Row, key: &str) -> &'a str {
    row.get(key).and_then(Value::as_str).unwrap_or_default()
}

/// A truthy string field (`row.get(k)` used as a key).
fn opt(row: &Row, key: &str) -> Option<String> {
    row.get(key)
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
}

pub fn push_error(row: &mut Row, error: &str) {
    match row.get_mut("errors") {
        Some(Value::Array(items)) => items.push(json!(error)),
        _ => {
            row.insert("errors".into(), json!([error]));
        }
    }
}

pub fn has_error(row: &Row, error: &str) -> bool {
    errors(row).any(|e| e == error)
}

fn errors(row: &Row) -> impl Iterator<Item = &str> {
    row.get("errors")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
}

fn has_errors(row: &Row) -> bool {
    truthy(row.get("errors"))
}

fn fail(row: &mut Row) {
    row.insert("state".into(), json!("error"));
}

fn fail_if_errors(row: &mut Row) {
    if has_errors(row) {
        fail(row);
    }
}

/// `row.get("segments") or [{"source_id", "feature_id"}]` as link keys.
fn links(row: &Row) -> Vec<Value> {
    match row.get("segments") {
        Some(Value::Array(items)) if !items.is_empty() => items.clone(),
        _ => vec![json!({"source_id": row.get("source_id"), "feature_id": row.get("feature_id")})],
    }
}

fn link_key(link: &Value) -> FeatureKey {
    (py_str(&link["source_id"]), py_str(&link["feature_id"]))
}

fn segment_codes(row: &Row) -> Vec<String> {
    row.get("segments")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|s| truthy(s.get("code")))
        .map(|s| py_str(&s["code"]))
        .collect()
}

/// Python `str(uuid)` of a non-null id column.
fn id_of(route: &Value) -> String {
    py_str(&route["id"])
}

/// `str(x) if x else None`.
fn opt_id(value: &Value) -> Option<String> {
    truthy(Some(value)).then(|| py_str(value))
}

fn invalid(code: &'static str, message: &str) -> AppError {
    AppError::validation(code, message)
}

// ---------------------------------------------------------------- layer rows

/// One route layer of the import body (`RouteImportLayer.model_dump()`).
#[derive(Debug, Clone)]
pub struct Layer {
    pub source_id: String,
    pub mapping: BTreeMap<String, String>,
    pub default_type: String,
    pub direction_state: String,
}

/// Rows of one layer from `(feature_id, attributes, content_revision, geometry_state,
/// geometry_type, srid)` features.
pub fn layer_rows(layer: &Layer, entries: &[Value]) -> Vec<Row> {
    let aliases = [
        ("đường", "street"),
        ("duong", "street"),
        ("hẻm", "alley"),
        ("hem", "alley"),
        ("ngách", "alley"),
        ("ngach", "alley"),
        ("nhánh", "alley"),
        ("nhanh", "alley"),
    ];
    entries
        .iter()
        .map(|f| {
            let attrs = &f["attributes"];
            let feature = py_str(&f["feature_id"]);
            let read = |field: &str, default: &str| match layer.mapping.get(field) {
                Some(name) if !name.is_empty() => {
                    py_text(attrs.get(name.as_str()).unwrap_or(&Value::Null))
                }
                _ => default.to_owned(),
            };
            let mut kind = read("route_type", &layer.default_type).to_lowercase();
            if let Some((_, to)) = aliases.iter().find(|(from, _)| *from == kind) {
                kind = (*to).to_owned();
            }
            let geometry_ok = f["geometry_state"] == "present"
                && matches!(
                    f["geometry_type"].as_str(),
                    Some("ST_LineString" | "ST_MultiLineString")
                )
                && truthy(f.get("srid"));
            let mut review = py_text(attrs.get("review_reason").unwrap_or(&Value::Null));
            if review.is_empty() && attrs.get("import_state") == Some(&json!("review_required")) {
                review = "review_required".to_owned();
            }
            let errors: Vec<&str> = if geometry_ok {
                vec![]
            } else {
                vec!["invalid_line_geometry"]
            };
            let row = json!({
                "key": format!("{}:{feature}", layer.source_id),
                "source_id": layer.source_id,
                "feature_id": feature,
                "content_revision": f["content_revision"],
                "code": read("code", &feature),
                "logical_code": read("logical_code", ""),
                "name": read("name", ""),
                "geometry_type": f["geometry_type"],
                "parent_code": read("parent_code", ""),
                "prefix": read("prefix", ""),
                "route_type": kind,
                "direction_state": layer.direction_state,
                "errors": errors,
                "review_reason": review,
            });
            row.as_object().cloned().unwrap_or_default()
        })
        .collect()
}

// ---------------------------------------------------------------- grouping

/// `group_routes`: opt-in grouping by logical code; never merge by name alone.
pub fn group_routes(rows: &mut [Row]) -> Vec<Row> {
    let mut raw_counts: HashMap<String, usize> = HashMap::new();
    for row in rows.iter() {
        *raw_counts.entry(s(row, "code").to_owned()).or_default() += 1;
    }
    let mut aliases: HashMap<String, String> = HashMap::new();
    let mut order: Vec<(bool, String)> = Vec::new();
    let mut groups: HashMap<(bool, String), Vec<usize>> = HashMap::new();
    for (i, row) in rows.iter_mut().enumerate() {
        let logical = s(row, "logical_code").to_owned();
        let code = s(row, "code").to_owned();
        if raw_counts[&code] != 1 || code.is_empty() {
            push_error(row, "duplicate_or_empty_code");
        }
        let logical_id = if logical.is_empty() {
            code.clone()
        } else {
            logical.clone()
        };
        aliases.insert(code, logical_id.clone());
        let key = if logical.is_empty() {
            (false, s(row, "key").to_owned())
        } else {
            (true, logical_id)
        };
        groups
            .entry(key.clone())
            .or_insert_with(|| {
                order.push(key);
                Vec::new()
            })
            .push(i);
    }
    let alias = |code: &str| {
        aliases
            .get(code)
            .cloned()
            .unwrap_or_else(|| code.to_owned())
    };
    let mut result = Vec::new();
    for key in order {
        let mut members = groups.remove(&key).unwrap_or_default();
        if truthy(rows[members[0]].get("recovery_group")) {
            members.sort_by(|a, b| s(&rows[*a], "key").cmp(s(&rows[*b], "key")));
        }
        let mut first = rows[members[0]].clone();
        let all_errors: BTreeSet<&str> = members.iter().flat_map(|&m| errors(&rows[m])).collect();
        first.insert("errors".into(), json!(all_errors));
        let reasons: BTreeSet<&str> = members
            .iter()
            .filter(|&&m| truthy(rows[m].get("review_reason")))
            .map(|&m| s(&rows[m], "review_reason"))
            .collect();
        first.insert(
            "review_reason".into(),
            json!(reasons.into_iter().collect::<Vec<_>>().join("; ")),
        );
        if let Some(group) = opt(&first, "logical_code") {
            first.insert("code".into(), json!(group));
            let segments: Vec<Value> = members
                .iter()
                .map(|&m| {
                    let r = &rows[m];
                    json!({"source_id": r["source_id"], "feature_id": r["feature_id"], "code": r["code"]})
                })
                .collect();
            first.insert("segments".into(), Value::Array(segments));
            let revisions: Vec<Value> = members
                .iter()
                .map(|&m| {
                    rows[m]
                        .get("content_revision")
                        .cloned()
                        .unwrap_or(Value::Null)
                })
                .collect();
            first.insert("segment_revisions".into(), Value::Array(revisions));
            let consistency: BTreeSet<(String, String, String, String, String)> = members
                .iter()
                .map(|&m| {
                    let r = &rows[m];
                    (
                        s(r, "name").to_owned(),
                        s(r, "route_type").to_owned(),
                        alias(s(r, "parent_code")),
                        s(r, "prefix").to_owned(),
                        s(r, "direction_state").to_owned(),
                    )
                })
                .collect();
            if consistency.len() != 1 {
                push_error(&mut first, "inconsistent_logical_route");
            }
            if members.len() > 200 {
                push_error(&mut first, "too_many_segments");
            }
            if aliases.get(&group).is_some_and(|a| *a != group) {
                push_error(&mut first, "duplicate_or_empty_code");
            }
        }
        let parent = alias(s(&first, "parent_code"));
        first.insert("parent_code".into(), json!(parent));
        result.push(first);
    }
    result
}

// ---------------------------------------------------------------- planner

fn prefix_join(parent: &str, value: &str) -> Result<String, &'static str> {
    let value = canonical_prefix(value).map_err(|_| "invalid_prefix")?;
    if parent.is_empty() {
        return if value.contains('/') {
            Err("prefix_must_be_one_level")
        } else {
            Ok(value)
        };
    }
    match value.rsplit_once('/') {
        None => Ok(format!("{parent}/{value}")),
        Some((head, _)) if head != parent => Err("prefix_does_not_follow_parent"),
        Some(_) => Ok(value),
    }
}

struct Planner<'a> {
    rows: Vec<Row>,
    counts: HashMap<String, usize>,
    by_code: HashMap<String, usize>,
    by_id: HashMap<String, &'a Value>,
    by_saved_code: HashMap<String, Vec<&'a Value>>,
    by_feature: HashMap<FeatureKey, Vec<&'a Value>>,
    states: HashMap<String, u8>,
    stack: Vec<usize>,
    result: Vec<usize>,
}

impl<'a> Planner<'a> {
    fn new(rows: Vec<Row>, existing: &'a [Value]) -> Self {
        let mut counts: HashMap<String, usize> = HashMap::new();
        for r in &rows {
            *counts.entry(s(r, "code").to_owned()).or_default() += 1;
        }
        let by_code = rows
            .iter()
            .enumerate()
            .filter(|(_, r)| counts[s(r, "code")] == 1)
            .map(|(i, r)| (s(r, "code").to_owned(), i))
            .collect();
        let by_id = existing.iter().map(|r| (id_of(r), r)).collect();
        let mut by_saved_code: HashMap<String, Vec<&Value>> = HashMap::new();
        for r in existing {
            if let Some(code) = r["source_label"]
                .as_str()
                .and_then(|l| l.strip_prefix("route-tree-import:"))
            {
                by_saved_code.entry(code.to_owned()).or_default().push(r);
            }
        }
        let mut by_feature: HashMap<FeatureKey, Vec<&Value>> = HashMap::new();
        for r in existing {
            let primary = (
                py_str(&r["linked_source_id"]),
                py_str(&r["linked_feature_id"]),
            );
            by_feature.entry(primary.clone()).or_default().push(r);
            for segment in r["segments"].as_array().into_iter().flatten() {
                let key = link_key(segment);
                if key != primary {
                    by_feature.entry(key).or_default().push(r);
                }
                if truthy(segment.get("code")) {
                    let code = py_str(&segment["code"]);
                    let list = by_saved_code.entry(code).or_default();
                    if list.iter().all(|old| old["id"] != r["id"]) {
                        list.push(r);
                    }
                }
            }
        }
        Self {
            rows,
            counts,
            by_code,
            by_id,
            by_saved_code,
            by_feature,
            states: HashMap::new(),
            stack: Vec::new(),
            result: Vec::new(),
        }
    }

    fn err(&mut self, i: usize, error: &str) {
        push_error(&mut self.rows[i], error);
    }

    fn resolve(&mut self, i: usize, depth: usize) -> Result<(), AppError> {
        if depth > MAX_DEPTH {
            return Err(invalid("ROUTE_DEPTH_LIMIT", "Cây tuyến quá sâu"));
        }
        let key = s(&self.rows[i], "key").to_owned();
        match self.states.get(&key) {
            Some(2) => return Ok(()),
            Some(1) => {
                for &m in &self.stack {
                    if !has_error(&self.rows[m], "cycle") {
                        push_error(&mut self.rows[m], "cycle");
                    }
                }
                return Ok(());
            }
            _ => {}
        }
        self.states.insert(key, 1);
        self.stack.push(i);
        let code = s(&self.rows[i], "code").to_owned();
        let name = s(&self.rows[i], "name").to_owned();
        let route_type = s(&self.rows[i], "route_type").to_owned();
        if code.is_empty() || code.chars().count() > 160 || self.counts[&code] > 1 {
            self.err(i, "duplicate_or_empty_code");
        }
        if name.is_empty() || name.chars().count() > 200 {
            self.err(i, "invalid_name");
        }
        let mut found: Vec<&'a Value> = Vec::new();
        for link in links(&self.rows[i]) {
            for old in self.by_feature.get(&link_key(&link)).into_iter().flatten() {
                if found.iter().all(|f| f["id"] != old["id"]) {
                    found.push(old);
                }
            }
        }
        if found.len() > 1 {
            self.err(i, "multiple_existing_routes");
        }
        if found.len() == 1 {
            self.rows[i].insert("existing_id".into(), json!(id_of(found[0])));
        }
        let existing_id = self.rows[i]
            .get("existing_id")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let mut aliases = vec![code.clone()];
        aliases.extend(segment_codes(&self.rows[i]));
        let taken = aliases.iter().any(|alias| {
            self.by_saved_code
                .get(alias)
                .into_iter()
                .flatten()
                .any(|old| Some(id_of(old)) != existing_id)
        });
        if taken {
            self.err(i, "duplicate_or_empty_code");
        }
        if !matches!(route_type.as_str(), "street" | "alley") {
            self.err(i, "invalid_route_type");
        }
        let reference = s(&self.rows[i], "parent_code").to_owned();
        let mut parent_id: Option<String> = None;
        let mut parent_prefix = String::new();
        let mut parent_type: Option<String> = None;
        if !reference.is_empty() {
            if let Some(&p) = self.by_code.get(&reference) {
                self.resolve(p, depth + 1)?;
                if has_errors(&self.rows[p]) {
                    self.err(i, "parent_has_errors");
                }
                parent_id = opt(&self.rows[p], "existing_id");
                parent_prefix = opt(&self.rows[p], "resolved_prefix").unwrap_or_default();
                parent_type = Some(s(&self.rows[p], "route_type").to_owned());
                let parent_key = self.rows[p]["key"].clone();
                self.rows[i].insert("parent_key".into(), parent_key);
            } else if let Some(p) = self.by_id.get(&reference).copied().or_else(|| {
                self.by_saved_code
                    .get(&reference)
                    .filter(|list| list.len() == 1)
                    .map(|list| list[0])
            }) {
                parent_id = Some(id_of(p));
                parent_prefix = py_str(&p["prefix"]);
                parent_type = Some(py_str(&p["route_type"]));
                self.rows[i].insert("parent_key".into(), Value::Null);
                self.rows[i].insert("parent_revision".into(), p["revision"].clone());
            } else {
                self.err(i, "parent_not_found");
            }
        }
        if route_type == "street" {
            if !reference.is_empty() || !s(&self.rows[i], "prefix").is_empty() {
                self.err(i, "street_cannot_have_parent_or_prefix");
            }
            self.rows[i].insert("resolved_prefix".into(), Value::Null);
        } else if route_type == "alley" {
            if reference.is_empty() {
                self.err(i, "parent_required");
            }
            if parent_type.as_deref() == Some("alley") && parent_prefix.is_empty() {
                self.err(i, "parent_prefix_missing");
            }
            match prefix_join(&parent_prefix, s(&self.rows[i], "prefix")) {
                Ok(prefix) => {
                    let long = prefix.chars().count() > 100;
                    self.rows[i].insert("resolved_prefix".into(), json!(prefix));
                    if long {
                        self.err(i, "prefix_too_long");
                    }
                }
                Err(error) => self.err(i, error),
            }
        }
        self.rows[i].insert("parent_id".into(), json!(parent_id));
        if let [old] = found[..] {
            let r = &self.rows[i];
            let old_prefix = if truthy(old.get("prefix")) {
                old["prefix"].clone()
            } else {
                Value::Null
            };
            let segments = |v: Option<&Value>| match v {
                Some(Value::Array(items)) => items.clone(),
                _ => Vec::new(),
            };
            let conflict = old["name"] != r["name"]
                || old["route_type"] != r["route_type"]
                || old_prefix != *r.get("resolved_prefix").unwrap_or(&Value::Null)
                || opt_id(&old["parent_route_id"]) != parent_id
                || old["direction_state"] != r["direction_state"]
                || segments(old.get("segments")) != segments(r.get("segments"));
            if conflict {
                self.err(i, "existing_route_conflict");
            }
            self.rows[i].insert("existing_revision".into(), old["revision"].clone());
        }
        self.stack.pop();
        let key = s(&self.rows[i], "key").to_owned();
        self.states.insert(key, 2);
        self.result.push(i);
        Ok(())
    }
}

/// `plan_routes`: resolve dependencies; no row is ever written by the planner.
pub fn plan_routes(rows: Vec<Row>, existing: &[Value]) -> Result<Vec<Row>, AppError> {
    let mut planner = Planner::new(rows, existing);
    for i in 0..planner.rows.len() {
        planner.resolve(i, 0)?;
    }
    let Planner {
        mut rows, result, ..
    } = planner;
    let mut occupied: HashSet<(String, String)> = HashSet::new();
    for old in existing {
        if truthy(old.get("prefix")) && truthy(old.get("parent_route_id")) {
            occupied.insert((py_str(&old["parent_route_id"]), py_str(&old["prefix"])));
        }
    }
    let mut new_occupied: HashMap<(Option<String>, String), Vec<usize>> = HashMap::new();
    for &i in &result {
        let r = &rows[i];
        if has_errors(r) || truthy(r.get("existing_id")) || !truthy(r.get("resolved_prefix")) {
            continue;
        }
        let parent_ref = opt(r, "parent_id").or_else(|| opt(r, "parent_key"));
        let prefix = s(r, "resolved_prefix").to_owned();
        if parent_ref
            .as_ref()
            .is_some_and(|p| occupied.contains(&(p.clone(), prefix.clone())))
        {
            push_error(&mut rows[i], "duplicate_sibling_prefix");
        } else {
            new_occupied
                .entry((parent_ref, prefix))
                .or_default()
                .push(i);
        }
    }
    for duplicates in new_occupied.values().filter(|d| d.len() > 1) {
        for &i in duplicates {
            push_error(&mut rows[i], "duplicate_sibling_prefix");
        }
    }
    let by_key: HashMap<String, usize> = result
        .iter()
        .map(|&i| (s(&rows[i], "key").to_owned(), i))
        .collect();
    for &i in &result {
        if let Some(&p) = opt(&rows[i], "parent_key").and_then(|k| by_key.get(&k)) {
            if has_errors(&rows[p]) && !has_error(&rows[i], "parent_has_errors") {
                push_error(&mut rows[i], "parent_has_errors");
            }
        }
        let r = &rows[i];
        let state = if has_errors(r) {
            "error"
        } else if truthy(r.get("existing_id")) {
            "existing"
        } else {
            "ready"
        };
        rows[i].insert("state".into(), json!(state));
    }
    Ok(result
        .into_iter()
        .map(|i| std::mem::take(&mut rows[i]))
        .collect())
}

// ---------------------------------------------------------------- overrides

/// `RouteImportOverride.model_dump()`.
#[derive(Debug, Clone)]
pub struct Override {
    pub parent_code: String,
    pub prefix: String,
    pub name: Option<String>,
    pub route_type: Option<String>,
    pub reviewed: bool,
}

/// Apply operator overrides to grouped rows (aliases come from the ungrouped rows).
pub fn apply_overrides(
    grouped: &mut [Row],
    rows: &[Row],
    overrides: &HashMap<String, Override>,
) -> Result<(), AppError> {
    let aliases: HashMap<&str, String> = rows
        .iter()
        .map(|r| {
            let logical = opt(r, "logical_code").unwrap_or_else(|| s(r, "code").to_owned());
            (s(r, "code"), logical)
        })
        .collect();
    for row in grouped.iter_mut() {
        let o = overrides.get(s(row, "key"));
        let parent = o.map_or_else(
            || s(row, "parent_code").to_owned(),
            |o| o.parent_code.clone(),
        );
        let parent = aliases.get(parent.as_str()).cloned().unwrap_or(parent);
        row.insert("parent_code".into(), json!(parent));
        let locked = truthy(row.get("manual_parent")) || truthy(row.get("recovery_group"));
        if let Some(o) = o {
            let differs = |field: &str, value: Option<&String>| {
                value.is_some_and(|v| row.get(field) != Some(&json!(v)))
            };
            if locked
                && (differs("name", o.name.as_ref())
                    || differs("prefix", Some(&o.prefix))
                    || differs("route_type", o.route_type.as_ref()))
            {
                return Err(invalid(
                    "RECOVERY_IDENTITY_LOCKED",
                    "Không đổi đường dẫn của nhóm ghép hoặc tuyến cha bổ sung; hãy bỏ nhóm và xem lại",
                ));
            }
            row.insert("prefix".into(), json!(o.prefix));
            if let Some(name) = &o.name {
                row.insert("name".into(), json!(name));
            }
            if let Some(kind) = &o.route_type {
                row.insert("route_type".into(), json!(kind));
            }
        }
        row.insert("reviewed".into(), json!(o.is_some_and(|o| o.reviewed)));
    }
    Ok(())
}

/// A recovered review feature already linked to exactly one saved route takes that
/// route's identity (only exact link sets qualify).
pub fn restore_reviewed(grouped: &mut [Row], existing: &[Value]) {
    let mut by_feature: HashMap<FeatureKey, Vec<&Value>> = HashMap::new();
    for saved in existing {
        let key = (
            py_str(&saved["linked_source_id"]),
            py_str(&saved["linked_feature_id"]),
        );
        by_feature.entry(key).or_default().push(saved);
    }
    for row in grouped.iter_mut() {
        if !truthy(row.get("review_reason")) {
            continue;
        }
        let key = (
            s(row, "source_id").to_owned(),
            s(row, "feature_id").to_owned(),
        );
        let Some([old]) = by_feature.get(&key).map(Vec::as_slice) else {
            continue;
        };
        let list = |v: Option<&Value>| match v {
            Some(Value::Array(items)) => items.clone(),
            _ => Vec::new(),
        };
        if list(old.get("segments")) != list(row.get("segments")) {
            continue;
        }
        let parent = opt_id(&old["parent_route_id"]).unwrap_or_default();
        row.insert("name".into(), old["name"].clone());
        row.insert("route_type".into(), old["route_type"].clone());
        row.insert("parent_code".into(), json!(parent));
        row.insert("prefix".into(), json!(py_str(&old["prefix"])));
        row.insert("direction_state".into(), old["direction_state"].clone());
        row.insert("reviewed".into(), json!(true));
    }
}

/// Legacy review rows need an explicit review and an alley type for full labels.
pub fn review_flags(planned: &mut [Row]) {
    for row in planned.iter_mut() {
        if !truthy(row.get("review_reason")) || truthy(row.get("existing_id")) {
            continue;
        }
        if !truthy(row.get("reviewed")) {
            push_error(row, "review_required");
        }
        if s(row, "route_type") == "street" && address_label(s(row, "name")).is_some() {
            push_error(row, "review_alley_type_required");
        }
        fail_if_errors(row);
    }
}

/// Rows whose grouped/multi-part geometry is checked like `create_route` would.
pub fn needs_segment_check(row: &Row) -> bool {
    !truthy(row.get("existing_id"))
        && !truthy(row.get("recovery_group"))
        && (truthy(row.get("segments"))
            || truthy(row.get("manual_parent"))
            || s(row, "geometry_type") == "ST_MultiLineString")
}

pub fn mark_invalid_segments(row: &mut Row) {
    push_error(row, "invalid_route_segments");
    fail(row);
}

/// Propagate all parent errors (including post-plan geometry and review failures).
pub fn propagate_parent_errors(planned: &mut [Row]) {
    let by_key: HashMap<String, usize> = planned
        .iter()
        .enumerate()
        .map(|(i, r)| (s(r, "key").to_owned(), i))
        .collect();
    for i in 0..planned.len() {
        let parent = opt(&planned[i], "parent_key").and_then(|k| by_key.get(&k).copied());
        if parent.is_some_and(|p| planned[p].get("state") == Some(&json!("error"))) {
            if !has_error(&planned[i], "parent_has_errors") {
                push_error(&mut planned[i], "parent_has_errors");
            }
            fail(&mut planned[i]);
        }
    }
}

/// `selection_rows`: only valid new rows; dependencies must be explicitly selected.
pub fn selection_rows<'r>(
    items: &'r [Row],
    error_count: usize,
    selected: Option<&[String]>,
) -> Result<Vec<&'r Row>, AppError> {
    let Some(selected) = selected else {
        if error_count > 0 {
            return Err(invalid(
                "ROUTE_IMPORT_ERRORS",
                "Cần xử lý toàn bộ lỗi trước khi áp dụng",
            ));
        }
        return Ok(items.iter().collect());
    };
    let keys: HashSet<&str> = selected.iter().map(String::as_str).collect();
    let by_key: HashMap<&str, &Row> = items.iter().map(|r| (s(r, "key"), r)).collect();
    let state = |k: &str| by_key.get(k).map(|r| s(r, "state"));
    if keys.is_empty()
        || keys.len() != selected.len()
        || keys.iter().any(|k| state(k) != Some("ready"))
    {
        return Err(invalid(
            "INVALID_ROUTE_SELECTION",
            "Chỉ chọn các tuyến mới đã hợp lệ",
        ));
    }
    for k in &keys {
        if let Some(parent) = opt(by_key[k], "parent_key") {
            let ok = match state(parent.as_str()) {
                Some("existing") => true,
                Some("ready") => keys.contains(parent.as_str()),
                _ => false,
            };
            if !ok {
                return Err(invalid(
                    "ROUTE_PARENT_NOT_SELECTED",
                    "Cần chọn cả tuyến cha mới trước khi áp dụng",
                ));
            }
        }
    }
    Ok(items
        .iter()
        .filter(|r| s(r, "state") == "existing" || keys.contains(s(r, "key")))
        .collect())
}

// ---------------------------------------------------------------- recovery evidence

pub fn expected_parent(row: &Row) -> Option<(String, String)> {
    let parent = |full: &str| full.rsplit_once('/').map_or("", |(h, _)| h).to_owned();
    if let Some((root, full)) = address_label(s(row, "name")) {
        return Some((root, parent(&full)));
    }
    let full = canonical_prefix(s(row, "prefix")).ok()?;
    Some((String::new(), parent(&full)))
}

const PARENT_ERRORS: [&str; 7] = [
    "parent_not_found",
    "parent_required",
    "prefix_does_not_follow_parent",
    "prefix_must_be_one_level",
    "parent_has_errors",
    "parent_prefix_missing",
    "street_cannot_have_parent_or_prefix",
];

/// First half of `recover_parent_evidence`: candidate parents and the junction pairs
/// whose endpoint distances persistence must measure.
pub struct ParentEvidence {
    pub pairs: Vec<(String, String)>,
    /// `{key, links}` of planned rows that take part in a pair.
    pub specs: Vec<Value>,
    pub existing_ids: Vec<String>,
    candidates: HashMap<String, Row>,
    inspected: Vec<(usize, String, String, Option<String>)>,
    resolved_roots: HashMap<String, String>,
}

pub fn parent_evidence(rows: &mut [Row], existing: &[Value], index: &RouteIndex) -> ParentEvidence {
    let by_key: HashMap<String, usize> = rows
        .iter()
        .enumerate()
        .map(|(i, r)| (s(r, "key").to_owned(), i))
        .collect();
    let mut candidates: HashMap<String, Row> = HashMap::new();
    let mut paths: HashMap<(String, String), Vec<String>> = HashMap::new();
    let mut by_prefix: HashMap<String, Vec<String>> = HashMap::new();
    let mut resolved_roots: HashMap<String, String> = HashMap::new();
    for route in existing {
        let rid = id_of(route);
        let Some(root_name) = index.root_name.get(&rid) else {
            continue;
        };
        let root = road_name(root_name);
        let prefix = py_str(&route["prefix"]);
        let candidate = json!({
            "code": rid, "name": route["name"], "root": root, "prefix": prefix,
            "state": "existing", "revision": route["revision"],
        });
        candidates.insert(
            rid.clone(),
            candidate.as_object().cloned().unwrap_or_default(),
        );
        paths
            .entry((root, prefix.clone()))
            .or_default()
            .push(rid.clone());
        by_prefix.entry(prefix).or_default().push(rid);
    }
    for row in rows.iter() {
        let key = s(row, "key").to_owned();
        let parent = opt(row, "parent_key").or_else(|| opt(row, "parent_id"));
        let root = if s(row, "route_type") == "street" {
            road_name(s(row, "name"))
        } else {
            parent
                .and_then(|p| {
                    resolved_roots
                        .get(&p)
                        .cloned()
                        .or_else(|| candidates.get(&p).map(|c| s(c, "root").to_owned()))
                })
                .unwrap_or_default()
        };
        resolved_roots.insert(key.clone(), root.clone());
        if truthy(row.get("existing_id")) {
            continue;
        }
        let (root, prefix) = if let Some(label) = address_label(s(row, "name")) {
            label
        } else if s(row, "route_type") == "street" {
            (root, String::new())
        } else if let Some(prefix) = opt(row, "resolved_prefix") {
            (root, prefix)
        } else {
            match canonical_prefix(s(row, "prefix")) {
                Ok(prefix) => (root, prefix),
                Err(_) => continue,
            }
        };
        let candidate = json!({
            "code": row["code"], "name": row["name"], "root": root, "prefix": prefix,
            "state": row.get("state"),
        });
        candidates.insert(
            key.clone(),
            candidate.as_object().cloned().unwrap_or_default(),
        );
        paths
            .entry((root, prefix.clone()))
            .or_default()
            .push(key.clone());
        by_prefix.entry(prefix).or_default().push(key);
    }
    // Same full address path cannot be imported twice by selecting only one of its conflicting segments.
    for row in rows.iter_mut() {
        if !truthy(row.get("review_reason")) || truthy(row.get("existing_id")) {
            continue;
        }
        let Some(candidate) = candidates.get_mut(s(row, "key")) else {
            continue;
        };
        let path = (
            s(candidate, "root").to_owned(),
            s(candidate, "prefix").to_owned(),
        );
        if !path.0.is_empty() && paths.get(&path).map_or(0, Vec::len) > 1 {
            push_error(row, "review_duplicate_address_path");
            fail(row);
            candidate.insert("state".into(), json!("error"));
        }
    }
    let mut pairs = Vec::new();
    let mut inspected = Vec::new();
    for i in 0..rows.len() {
        let row = &rows[i];
        let label = address_label(s(row, "name"));
        if truthy(row.get("existing_id")) || (s(row, "route_type") != "alley" && label.is_none()) {
            continue;
        }
        if !truthy(row.get("review_reason")) && !errors(row).any(|e| PARENT_ERRORS.contains(&e)) {
            continue;
        }
        let Some((root, prefix)) = expected_parent(row) else {
            continue;
        };
        let key = s(row, "key").to_owned();
        let mut actual = opt(row, "parent_key").or_else(|| opt(row, "parent_id"));
        if let Some(existing_id) = actual
            .as_ref()
            .and_then(|a| by_key.get(a))
            .and_then(|&j| opt(&rows[j], "existing_id"))
        {
            actual = Some(existing_id);
        }
        let mut options = if root.is_empty() {
            by_prefix.get(&prefix).cloned()
        } else {
            paths.get(&(root.clone(), prefix.clone())).cloned()
        }
        .unwrap_or_default();
        if let Some(a) = &actual {
            if candidates.contains_key(a) && !options.contains(a) {
                options.push(a.clone());
            }
        }
        // Bound comparison size for broad, unnamed paths.
        let truncated = options.len() > 250;
        if truncated {
            options.truncate(250);
            if let Some(a) = &actual {
                if !options.contains(a) && candidates.contains_key(a) {
                    options.push(a.clone());
                }
            }
        }
        let row = &mut rows[i];
        row.insert("expected_parent_prefix".into(), json!(prefix));
        row.insert("expected_root".into(), json!(root));
        row.insert("suggested_type".into(), json!("alley"));
        if let Some((_, full)) = label {
            row.insert("suggested_prefix".into(), json!(full));
        }
        if truncated {
            row.insert("parent_search_truncated".into(), json!(true));
        }
        for other in options {
            if other != key {
                pairs.push((key.clone(), other));
            }
        }
        inspected.push((i, root, prefix, actual));
    }
    let needed: BTreeSet<&String> = pairs.iter().flat_map(|(c, p)| [c, p]).collect();
    let specs = needed
        .iter()
        .filter_map(|key| by_key.get(*key))
        .map(|&i| json!({"key": rows[i]["key"], "links": links(&rows[i])}))
        .collect();
    let existing_ids = needed
        .iter()
        .filter(|key| index.routes.contains_key(key.as_str()))
        .map(|key| (*key).clone())
        .collect();
    ParentEvidence {
        pairs,
        specs,
        existing_ids,
        candidates,
        inspected,
        resolved_roots,
    }
}

fn option_order(a: &Value, b: &Value) -> std::cmp::Ordering {
    let key = |v: &Value| {
        (
            v["address_match"] != true,
            v["connected"] != true,
            v["distance_m"].as_f64().unwrap_or(f64::INFINITY),
        )
    };
    let (ka, kb) = (key(a), key(b));
    ka.0.cmp(&kb.0)
        .then(ka.1.cmp(&kb.1))
        .then(ka.2.partial_cmp(&kb.2).unwrap_or(std::cmp::Ordering::Equal))
        .then(py_str(&a["code"]).cmp(&py_str(&b["code"])))
}

/// Second half of `recover_parent_evidence` once distances are measured.
pub fn finish_parent_evidence(rows: &mut [Row], plan: ParentEvidence, distances: &Junctions) {
    let ParentEvidence {
        pairs,
        candidates,
        inspected,
        resolved_roots,
        ..
    } = plan;
    let by_child: HashMap<String, (&String, &String, &Option<String>)> = inspected
        .iter()
        .map(|(i, root, prefix, actual)| (s(&rows[*i], "key").to_owned(), (root, prefix, actual)))
        .collect();
    let manual_keys: HashSet<String> = rows
        .iter()
        .filter(|r| truthy(r.get("manual_parent")) && !truthy(r.get("existing_id")))
        .map(|r| s(r, "key").to_owned())
        .collect();
    let mut results: HashMap<String, Vec<Value>> = HashMap::new();
    let mut child_matches: HashMap<String, HashSet<String>> = HashMap::new();
    for (child, parent) in &pairs {
        let (root, prefix, actual) = by_child[child];
        let candidate = &candidates[parent];
        let measured = distances.get(&(child.clone(), parent.clone()));
        let address_match = !root.is_empty()
            && s(candidate, "root") == root.as_str()
            && s(candidate, "prefix") == prefix.as_str();
        let connected = measured.is_some_and(|(d, _)| *d <= MAX_JUNCTION_M);
        let mut option = candidate.clone();
        option.insert(
            "distance_m".into(),
            measured.map_or(Value::Null, |(d, _)| json!(py_round(*d, 2))),
        );
        option.insert("address_match".into(), json!(address_match));
        option.insert("connected".into(), json!(connected));
        option.insert("current".into(), json!(actual.as_ref() == Some(parent)));
        option.insert(
            "geometry_revision".into(),
            measured.map_or(Value::Null, |(_, rev)| rev.clone()),
        );
        results
            .entry(child.clone())
            .or_default()
            .push(Value::Object(option));
        // A newly supplied parent feature must physically meet an immediate child.
        if manual_keys.contains(parent) && address_match && connected {
            child_matches
                .entry(parent.clone())
                .or_default()
                .insert(child.clone());
        }
    }
    for row in rows.iter_mut() {
        let key = s(row, "key").to_owned();
        if manual_keys.contains(&key) {
            let count = child_matches.get(&key).map_or(0, HashSet::len);
            row.insert("manual_parent_child_count".into(), json!(count));
            if count == 0 {
                push_error(row, "review_parent_has_no_connected_child");
                fail(row);
            }
        }
    }
    for (i, root, _, _) in &inspected {
        let row = &mut rows[*i];
        let key = s(row, "key").to_owned();
        let mut options = results.remove(&key).unwrap_or_default();
        options.sort_by(option_order);
        let current = options.iter().find(|v| v["current"] == true).cloned();
        let mut shown: Vec<Value> = options.into_iter().take(6).collect();
        if let Some(current) = &current {
            if !shown.contains(current) {
                shown.push(current.clone());
            }
        }
        row.insert("parent_suggestions".into(), Value::Array(shown));
        row.insert(
            "parent_evidence".into(),
            current.clone().unwrap_or(Value::Null),
        );
        if truthy(row.get("review_reason")) && s(row, "route_type") == "alley" {
            // A review cannot certify a different root simply by relabeling the parent.
            let label = address_label(s(row, "name"));
            let actual_root = resolved_roots.get(&key).cloned().unwrap_or_default();
            let matched = current.as_ref().is_some_and(|c| c["address_match"] == true);
            let label_ok = label
                .as_ref()
                .is_some_and(|(_, full)| row.get("resolved_prefix") == Some(&json!(full)));
            if !label_ok || !matched || actual_root != *root {
                push_error(row, "review_address_mismatch");
            }
            if !current.as_ref().is_some_and(|c| c["connected"] == true) {
                push_error(row, "review_parent_not_connected");
            }
            fail_if_errors(row);
        }
    }
}

// ---------------------------------------------------------------- survey evidence

/// Codes of rows awaiting review (`None` when no row needs survey evidence).
pub fn survey_codes(rows: &[Row]) -> Option<Vec<String>> {
    let pending: Vec<&Row> = rows
        .iter()
        .filter(|r| truthy(r.get("review_reason")) && !truthy(r.get("existing_id")))
        .collect();
    if pending.is_empty() {
        return None;
    }
    let codes: BTreeSet<String> = pending
        .iter()
        .flat_map(|r| std::iter::once(s(r, "code").to_owned()).chain(segment_codes(r)))
        .collect();
    Some(codes.into_iter().collect())
}

/// `survey_evidence` over aggregated `(code, road, alley, unassigned, amount, examples)`
/// rows; never exposes occupant data or changes houses.
pub fn survey_finish(rows: &mut [Row], index: &RouteIndex, observations: &[Value]) {
    let mut by_code: HashMap<String, Vec<&Value>> = HashMap::new();
    for item in observations {
        by_code.entry(py_str(&item["code"])).or_default().push(item);
    }
    for row in rows.iter_mut() {
        if !truthy(row.get("review_reason")) || truthy(row.get("existing_id")) {
            continue;
        }
        let row_codes: BTreeSet<String> = std::iter::once(s(row, "code").to_owned())
            .chain(segment_codes(row))
            .collect();
        let (root, full) = address_label(s(row, "name"))
            .unwrap_or_else(|| (road_name(s(row, "name")), String::new()));
        let full = full.to_lowercase();
        let (mut total, mut unassigned, mut conflict) = (0_i64, 0_i64, 0_i64);
        let mut groups: Vec<Value> = Vec::new();
        let mut counts: BTreeMap<&str, i64> = BTreeMap::new();
        for code in &row_codes {
            for v in by_code.get(code).into_iter().flatten() {
                let amount = v["amount"].as_i64().unwrap_or(0);
                total += amount;
                let free = v["unassigned"] == true;
                if free {
                    unassigned += amount;
                }
                let road = road_name(&py_str(&v["road"]));
                let alley = alley_name(&py_str(&v["alley"]));
                let kind = if !road.is_empty() && road != root {
                    "different_road"
                } else if !alley.is_empty() && alley != full {
                    if road == root && !full.is_empty() && alley.starts_with(&format!("{full}/")) {
                        "descendant"
                    } else {
                        "different_branch"
                    }
                } else if road.is_empty() {
                    "missing_road"
                } else if !full.is_empty() && alley.is_empty() {
                    "missing_prefix"
                } else {
                    "exact"
                };
                if matches!(kind, "different_road" | "descendant" | "different_branch") {
                    conflict += amount;
                }
                *counts.entry(kind).or_default() += amount;
                if kind != "exact" {
                    let (candidates, _) =
                        index.match_route(&py_str(&v["road"]), &py_str(&v["alley"]), "", true);
                    let examples: Vec<Value> = v["example_sources"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .zip(v["example_features"].as_array().into_iter().flatten())
                        .map(|(s, f)| json!({"source_id": s, "feature_id": f}))
                        .collect();
                    groups.push(json!({
                        "kind": kind,
                        "road": py_str(&v["road"]),
                        "alley": py_str(&v["alley"]),
                        "code": code,
                        "count": amount,
                        "unassigned": free,
                        "candidate_count": candidates.len(),
                        "examples": examples,
                    }));
                }
            }
        }
        let total_groups = groups.len();
        groups.sort_by(|a, b| {
            let key = |g: &Value| {
                (
                    matches!(g["kind"].as_str(), Some("missing_road" | "missing_prefix")),
                    -g["count"].as_i64().unwrap_or(0),
                    py_str(&g["code"]),
                    py_str(&g["road"]),
                    py_str(&g["alley"]),
                )
            };
            key(a).cmp(&key(b))
        });
        groups.truncate(100);
        row.insert(
            "survey_evidence".into(),
            json!({"referenced": total, "unassigned": unassigned, "address_conflicts": conflict}),
        );
        // Explicit diagnostic only: no waiver of conflict or linked-house guard.
        row.insert(
            "survey_review".into(),
            json!({"counts": counts, "groups": groups, "total_groups": total_groups}),
        );
        if conflict != 0 {
            push_error(row, "review_survey_address_conflict");
        }
        if total != unassigned {
            push_error(row, "review_building_already_linked");
        }
        fail_if_errors(row);
    }
}

// ---------------------------------------------------------------- topology

/// `stable_id`: compact JSON of the parts → sha256 hex prefix.
pub fn stable_id(parts: &[&str]) -> String {
    sha256_hex(&py_json(&json!(parts)))[..32].to_owned()
}

pub fn linked_features(existing: &[Value]) -> HashSet<FeatureKey> {
    let mut out = HashSet::new();
    for r in existing {
        match r["segments"].as_array() {
            Some(items) if !items.is_empty() => out.extend(items.iter().map(link_key)),
            _ => {
                out.insert((
                    py_str(&r["linked_source_id"]),
                    py_str(&r["linked_feature_id"]),
                ));
            }
        }
    }
    out
}

/// Existing routes by `(normalized root road, prefix)`.
pub fn existing_paths(existing: &[Value]) -> HashMap<(String, String), Vec<Value>> {
    let index = RouteIndex::new(existing.to_vec());
    let mut paths: HashMap<(String, String), Vec<Value>> = HashMap::new();
    for route in existing {
        let rid = id_of(route);
        if let Some(root) = index.root_name.get(&rid) {
            paths
                .entry((road_name(root), py_str(&route["prefix"])))
                .or_default()
                .push(route.clone());
        }
    }
    paths
}

pub fn route_path(row: &Row) -> Option<(String, String)> {
    let label = address_label(s(row, "name"));
    if let Some(label) = &label {
        if !UNNAMED.contains(&label.0.as_str()) {
            return Some(label.clone());
        }
    }
    if s(row, "route_type") == "street" && s(row, "prefix").is_empty() && label.is_none() {
        let road = road_name(s(row, "name"));
        if !road.is_empty() && !UNNAMED.contains(&road.as_str()) {
            return Some((road, String::new()));
        }
    }
    None
}

fn row_feature(row: &Row) -> FeatureKey {
    (
        s(row, "source_id").to_owned(),
        s(row, "feature_id").to_owned(),
    )
}

/// `merge_options` before the geometry check: every unlinked review segment sharing
/// one unambiguous address path (`status` stays `geometry_pending` until measured).
pub fn merge_suggestions(rows: &[Row], existing: &[Value]) -> Vec<Row> {
    let occupied = linked_features(existing);
    let paths = existing_paths(existing);
    let mut by_path: BTreeMap<(String, String), Vec<&Row>> = BTreeMap::new();
    for row in rows {
        if !truthy(row.get("review_reason"))
            || truthy(row.get("logical_code"))
            || truthy(row.get("manual_parent"))
            || occupied.contains(&row_feature(row))
        {
            continue;
        }
        if let Some(path) = route_path(row).filter(|p| !p.1.is_empty()) {
            by_path.entry(path).or_default().push(row);
        }
    }
    let mut suggestions = Vec::new();
    for ((root, prefix), mut members) in by_path {
        if members.len() < 2 {
            continue;
        }
        members.sort_by(|a, b| s(a, "key").cmp(s(b, "key")));
        let keys: Vec<&str> = members.iter().map(|r| s(r, "key")).collect();
        let fields = [
            "key",
            "code",
            "source_id",
            "feature_id",
            "name",
            "parent_code",
            "prefix",
            "content_revision",
        ];
        let projected: Vec<Value> = members
            .iter()
            .map(|r| {
                Value::Object(
                    fields
                        .iter()
                        .map(|k| ((*k).to_owned(), r.get(*k).cloned().unwrap_or(Value::Null)))
                        .collect(),
                )
            })
            .collect();
        let status = if paths.contains_key(&(root.clone(), prefix.clone())) {
            "existing_conflict"
        } else if members.len() > 200 {
            "too_many_segments"
        } else {
            let prefix_conflict = members
                .iter()
                .any(|r| !canonical_prefix(s(r, "prefix")).is_ok_and(|p| p == prefix));
            let directions: HashSet<&str> =
                members.iter().map(|r| s(r, "direction_state")).collect();
            let codes: HashSet<&str> = members.iter().map(|r| s(r, "code")).collect();
            if prefix_conflict
                || directions.len() != 1
                || members.iter().any(|r| has_errors(r))
                || codes.len() != members.len()
            {
                "attribute_conflict"
            } else {
                "geometry_pending"
            }
        };
        let item = json!({
            "id": format!("recovery-{}", stable_id(&keys)),
            "root": root,
            "prefix": prefix,
            "name": members[0]["name"],
            "members": projected,
            "status": status,
        });
        suggestions.push(item.as_object().cloned().unwrap_or_default());
    }
    suggestions
}

/// `{id, links}` specs of suggestions whose merged geometry must be resolved.
pub fn merge_specs(suggestions: &[Row]) -> Vec<Value> {
    suggestions
        .iter()
        .filter(|s_| s(s_, "status") == "geometry_pending")
        .map(|item| json!({"id": item["id"], "links": item["members"]}))
        .collect()
}

pub fn merge_finish(suggestions: &mut [Row], connected: &HashSet<String>) {
    for item in suggestions.iter_mut() {
        if s(item, "status") == "geometry_pending" {
            let status = if connected.contains(s(item, "id")) {
                "connected"
            } else {
                "disconnected"
            };
            item.insert("status".into(), json!(status));
        }
    }
}

fn merge_code_conflict() -> AppError {
    invalid(
        "MERGE_CODE_CONFLICT",
        "Mã nhóm ghép đã được dùng cho đoạn khác",
    )
}

/// `apply_merges`: restore saved recovery groups and apply explicitly selected ones.
pub fn apply_merges(
    rows: &mut [Row],
    existing: &[Value],
    options: &[Row],
    selected: &[String],
) -> Result<(), AppError> {
    let by_key: HashMap<String, usize> = rows
        .iter()
        .enumerate()
        .map(|(i, r)| (s(r, "key").to_owned(), i))
        .collect();
    if selected.iter().collect::<HashSet<_>>().len() != selected.len() {
        return Err(invalid("DUPLICATE_MERGE_GROUP", "Nhóm ghép bị trùng"));
    }
    let used_elsewhere = |rows: &[Row], members: &HashSet<String>, code: &str| {
        rows.iter().any(|r| {
            !members.contains(s(r, "key"))
                && (r.get("logical_code") == Some(&json!(code)) || s(r, "code") == code)
        })
    };
    let mut saved_ids = HashSet::new();
    // Restore previously saved recovery groups automatically, preserving every segment alias.
    for old in existing {
        let label = old["source_label"].as_str().unwrap_or_default();
        let code = label.strip_prefix("route-tree-import:").unwrap_or(label);
        let segments = old["segments"].as_array().filter(|s| !s.is_empty());
        let (true, Some(segments)) = (code.starts_with("recovery-"), segments) else {
            continue;
        };
        let keys: Vec<String> = segments
            .iter()
            .map(|seg| {
                format!(
                    "{}:{}",
                    py_str(&seg["source_id"]),
                    py_str(&seg["feature_id"])
                )
            })
            .collect();
        if !keys.iter().all(|k| by_key.contains_key(k)) {
            continue;
        }
        if keys.iter().zip(segments).any(|(k, seg)| {
            rows[by_key[k]].get("code") != Some(seg.get("code").unwrap_or(&Value::Null))
        }) {
            continue;
        }
        let members: HashSet<String> = keys.iter().cloned().collect();
        if used_elsewhere(rows, &members, code) {
            return Err(merge_code_conflict());
        }
        saved_ids.insert(code.to_owned());
        for k in &keys {
            let row = &mut rows[by_key[k]];
            row.insert("logical_code".into(), json!(code));
            row.insert("recovery_group".into(), json!(true));
            row.insert("name".into(), old["name"].clone());
            row.insert("route_type".into(), old["route_type"].clone());
            row.insert("prefix".into(), json!(py_str(&old["prefix"])));
            row.insert("parent_code".into(), json!(py_str(&old["parent_route_id"])));
            row.insert("direction_state".into(), old["direction_state"].clone());
        }
    }
    for key in selected {
        if saved_ids.contains(key) {
            continue;
        }
        let item = options.iter().find(|o| s(o, "id") == key.as_str());
        let Some(item) = item.filter(|o| s(o, "status") == "connected") else {
            return Err(invalid(
                "INVALID_MERGE_GROUP",
                "Chỉ ghép nhóm cùng địa chỉ và có hình học liên tục",
            ));
        };
        let member_keys: Vec<String> = item["members"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|m| py_str(&m["key"]))
            .collect();
        let members: HashSet<String> = member_keys.iter().cloned().collect();
        if used_elsewhere(rows, &members, key) {
            return Err(merge_code_conflict());
        }
        let old_parents: BTreeSet<String> = member_keys
            .iter()
            .filter_map(|k| by_key.get(k))
            .map(|&i| s(&rows[i], "parent_code").to_owned())
            .filter(|p| !p.is_empty())
            .collect();
        // Parent disagreement is not silently guessed: require a new reviewed selection.
        let parent = if old_parents.len() == 1 {
            old_parents.first().cloned().unwrap_or_default()
        } else {
            String::new()
        };
        for k in &member_keys {
            let Some(&i) = by_key.get(k) else { continue };
            let row = &mut rows[i];
            row.insert("logical_code".into(), json!(key));
            row.insert("recovery_group".into(), json!(true));
            row.insert("name".into(), item["name"].clone());
            row.insert("route_type".into(), json!("alley"));
            row.insert("prefix".into(), item["prefix"].clone());
            row.insert("parent_code".into(), json!(parent));
            row.insert("legacy_parent_codes".into(), json!(old_parents));
        }
    }
    Ok(())
}

/// Leading text before the first ` hẻm|ngõ|ngách|nhánh|alley ` word (Python `re.split`).
fn raw_root(name: &str) -> String {
    const WORDS: [&str; 9] = [
        "hẻm", "hem", "ngõ", "ngo", "ngách", "ngach", "nhánh", "nhanh", "alley",
    ];
    let chars: Vec<(usize, char)> = name.char_indices().collect();
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].1.is_whitespace() {
            i += 1;
            continue;
        }
        let start = chars[i].0;
        let mut j = i;
        while j < chars.len() && chars[j].1.is_whitespace() {
            j += 1;
        }
        let mut k = j;
        while k < chars.len() && !chars[k].1.is_whitespace() {
            k += 1;
        }
        if j < chars.len() && k < chars.len() {
            let end = chars[k].0;
            let word = name[chars[j].0..end].to_lowercase();
            if WORDS.contains(&word.as_str()) {
                return name[..start].trim().to_owned();
            }
        }
        i = j.max(i + 1);
    }
    name.trim().to_owned()
}

/// `parent_tasks`: missing ancestor address paths across the loaded layers.
pub fn parent_tasks(rows: &[Row], existing: &[Value]) -> Vec<Value> {
    let mut known: HashSet<(String, String)> = existing_paths(existing).into_keys().collect();
    let occupied = linked_features(existing);
    let labeled: Vec<(&Row, Option<(String, String)>)> = rows
        .iter()
        .filter(|r| !occupied.contains(&row_feature(r)))
        .map(|r| (r, route_path(r)))
        .collect();
    known.extend(labeled.iter().filter_map(|(_, p)| p.clone()));
    let mut tasks: HashMap<(String, String), Row> = HashMap::new();
    for (row, path) in &labeled {
        let Some((root, full)) = path else { continue };
        if full.is_empty() || truthy(row.get("existing_id")) || !truthy(row.get("review_reason")) {
            continue;
        }
        let pieces: Vec<&str> = full.split('/').collect();
        let raw = raw_root(s(row, "name"));
        for i in 0..pieces.len() {
            let prefix = pieces[..i].join("/");
            if known.contains(&(root.clone(), prefix.clone())) {
                continue;
            }
            let item = tasks
                .entry((root.clone(), prefix.clone()))
                .or_insert_with(|| {
                    let name = if prefix.is_empty() {
                        raw.clone()
                    } else {
                        format!("{raw} hẻm {prefix}")
                    };
                    let parent_prefix = prefix.rsplit_once('/').map_or("", |(h, _)| h);
                    json!({
                    "id": stable_id(&[root.as_str(), prefix.as_str()]),
                    "root": root, "prefix": prefix, "name": name,
                    "parent_prefix": parent_prefix, "children": 0, "buildings": 0, "examples": [],
                })
                .as_object()
                .cloned()
                .unwrap_or_default()
                });
            let children = item["children"].as_i64().unwrap_or(0) + 1;
            item.insert("children".into(), json!(children));
            let referenced = row
                .get("survey_evidence")
                .and_then(|e| e.get("referenced"))
                .and_then(Value::as_i64)
                .unwrap_or(0);
            let buildings = item["buildings"].as_i64().unwrap_or(0) + referenced;
            item.insert("buildings".into(), json!(buildings));
            if let Some(Value::Array(examples)) = item.get_mut("examples") {
                if examples.len() < 5 {
                    examples.push(json!({
                        "source_id": row["source_id"], "feature_id": row["feature_id"], "name": row["name"],
                    }));
                }
            }
        }
    }
    let mut out: Vec<Row> = tasks.into_values().collect();
    out.sort_by(|a, b| {
        let key = |t: &Row| {
            let prefix = s(t, "prefix");
            let depth = if prefix.is_empty() {
                -1
            } else {
                prefix.matches('/').count() as i64
            };
            (s(t, "root").to_owned(), depth, prefix.to_owned())
        };
        key(a).cmp(&key(b))
    });
    out.into_iter().map(Value::Object).collect()
}

/// One `RouteParentFeature` of the import body.
#[derive(Debug, Clone)]
pub struct ParentPick {
    pub task_id: String,
    pub source_id: String,
    pub feature_id: String,
}

/// `append_parent_features` state; checks run per pick around the feature lookup.
pub struct ParentFeatures {
    tasks: HashMap<String, Value>,
    by_saved: HashMap<String, Value>,
    used: HashSet<FeatureKey>,
    linked: HashSet<FeatureKey>,
    paths: HashMap<(String, String), Vec<Value>>,
}

fn stale_task(message: &str) -> AppError {
    invalid("PARENT_TASK_STALE", message)
}

impl ParentFeatures {
    pub fn new(rows: &[Row], existing: &[Value]) -> Self {
        Self {
            tasks: parent_tasks(rows, existing)
                .into_iter()
                .map(|t| (py_str(&t["id"]), t))
                .collect(),
            by_saved: existing
                .iter()
                .map(|r| (py_str(&r["source_label"]), r.clone()))
                .collect(),
            used: rows.iter().map(row_feature).collect(),
            linked: linked_features(existing),
            paths: existing_paths(existing),
        }
    }

    fn saved(&self, pick: &ParentPick) -> Option<&Value> {
        self.by_saved.get(&format!(
            "route-tree-import:recovery-parent-{}",
            pick.task_id
        ))
    }

    /// Checks that precede the feature lookup.
    pub fn check(&self, pick: &ParentPick) -> Result<(), AppError> {
        let key = (pick.source_id.clone(), pick.feature_id.clone());
        let saved = self.saved(pick);
        if self.used.contains(&key) {
            return Err(invalid(
                "PARENT_FEATURE_ALREADY_SELECTED",
                "Đối tượng đã nằm trong lớp import; hãy dùng dòng tuyến có sẵn",
            ));
        }
        let same = |saved: &Value| {
            py_str(&saved["linked_source_id"]) == pick.source_id
                && saved["linked_feature_id"] == json!(pick.feature_id)
        };
        if self.linked.contains(&key)
            && !saved.is_some_and(|sv| same(sv) && !truthy(sv.get("segments")))
        {
            return Err(invalid(
                "PARENT_FEATURE_ALREADY_LINKED",
                "Đối tượng đã liên kết tuyến khác",
            ));
        }
        if !self.tasks.contains_key(&pick.task_id) && saved.is_none() {
            return Err(stale_task(
                "Tuyến cha không còn nằm trong danh sách thiếu; xem trước lại",
            ));
        }
        if saved.is_some_and(|sv| !same(sv)) {
            return Err(stale_task("Tuyến cha đã được liên kết bằng đối tượng khác"));
        }
        Ok(())
    }

    /// Build the manual parent row from the looked-up feature.
    pub fn row(&mut self, pick: &ParentPick, feature: Option<&Value>) -> Result<Row, AppError> {
        let Some(feature) = feature.filter(|f| {
            f["geometry_state"] == "present"
                && matches!(
                    f["geometry_type"].as_str(),
                    Some("ST_LineString" | "ST_MultiLineString")
                )
                && truthy(f.get("srid"))
        }) else {
            return Err(invalid(
                "INVALID_PARENT_FEATURE",
                "Tuyến cha phải dùng đối tượng đường GIS hợp lệ trong dự án",
            ));
        };
        let (name, prefix, kind, parent, direction) = if let Some(saved) = self.saved(pick) {
            (
                saved["name"].clone(),
                py_str(&saved["prefix"]),
                saved["route_type"].clone(),
                py_str(&saved["parent_route_id"]),
                saved["direction_state"].clone(),
            )
        } else {
            let task = &self.tasks[&pick.task_id];
            let prefix = py_str(&task["prefix"]);
            let mut parent = String::new();
            if !prefix.is_empty() {
                let key = (py_str(&task["root"]), py_str(&task["parent_prefix"]));
                if let Some([only]) = self.paths.get(&key).map(Vec::as_slice) {
                    parent = id_of(only);
                }
            }
            let kind = if prefix.is_empty() { "street" } else { "alley" };
            (
                task["name"].clone(),
                prefix,
                json!(kind),
                parent,
                json!("forward"),
            )
        };
        let row = json!({
            "key": format!("{}:{}", pick.source_id, pick.feature_id),
            "source_id": pick.source_id,
            "feature_id": pick.feature_id,
            "content_revision": feature["content_revision"],
            "geometry_type": feature["geometry_type"],
            "code": format!("recovery-parent-{}", pick.task_id),
            "logical_code": "",
            "name": name,
            "parent_code": parent,
            "prefix": prefix,
            "route_type": kind,
            "direction_state": direction,
            "errors": [],
            "review_reason": "manual_parent_feature",
            "manual_parent": true,
            "parent_task_id": pick.task_id,
        });
        self.used
            .insert((pick.source_id.clone(), pick.feature_id.clone()));
        Ok(row.as_object().cloned().unwrap_or_default())
    }

    /// After every pick: reject duplicate tasks, then propose the unique immediate
    /// parent from the same draft or existing tree.
    pub fn finish(&self, rows: &mut [Row], picks: &[ParentPick]) -> Result<(), AppError> {
        let tasks: HashSet<&str> = picks.iter().map(|p| p.task_id.as_str()).collect();
        if tasks.len() != picks.len() {
            return Err(invalid(
                "DUPLICATE_PARENT_TASK",
                "Một tuyến cha chỉ được chọn một đối tượng",
            ));
        }
        let mut candidates: HashMap<(String, String), BTreeSet<String>> = HashMap::new();
        for r in rows.iter() {
            if let Some(path) = route_path(r) {
                if !self.linked.contains(&row_feature(r)) {
                    let code = opt(r, "logical_code").unwrap_or_else(|| s(r, "code").to_owned());
                    candidates.entry(path).or_default().insert(code);
                }
            }
        }
        for row in rows.iter_mut() {
            if !truthy(row.get("manual_parent"))
                || s(row, "prefix").is_empty()
                || !s(row, "parent_code").is_empty()
            {
                continue;
            }
            if let Some((root, full)) = address_label(s(row, "name")) {
                let parent = full.rsplit_once('/').map_or("", |(h, _)| h).to_owned();
                if let Some(refs) = candidates.get(&(root, parent)).filter(|r| r.len() == 1) {
                    let code = refs.first().cloned().unwrap_or_default();
                    row.insert("parent_code".into(), json!(code));
                }
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------- staging

/// `geometry_review.stage_eligible`.
pub fn stage_eligible(row: &Row) -> bool {
    const ALLOWED: [&str; 4] = [
        "review_survey_address_conflict",
        "review_required",
        "review_address_pending",
        "review_stage_stale",
    ];
    let errs: HashSet<&str> = errors(row).collect();
    let empty = Value::Object(Map::new());
    let evidence = match row.get("survey_evidence") {
        Some(v) if truthy(Some(v)) => v,
        _ => &empty,
    };
    let parent = match row.get("parent_evidence") {
        Some(v) if truthy(Some(v)) => v,
        _ => &empty,
    };
    let connected = row
        .get("parent_suggestions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|x| truthy(x.get("address_match")) && truthy(x.get("connected")))
        .count();
    let stale = errs.contains("review_stage_stale");
    s(row, "state") == "error"
        && !truthy(row.get("existing_id"))
        && s(row, "route_type") == "alley"
        && truthy(row.get("resolved_prefix"))
        && truthy(row.get("parent_id"))
        && (errs.contains("review_survey_address_conflict") || stale)
        && errs.iter().all(|e| ALLOWED.contains(e))
        && (evidence
            .get("address_conflicts")
            .and_then(Value::as_f64)
            .unwrap_or(0.0)
            > 0.0
            || stale)
        && evidence.get("referenced") == evidence.get("unassigned")
        && truthy(parent.get("address_match"))
        && truthy(parent.get("connected"))
        && parent.get("state") == Some(&json!("existing"))
        && py_str(parent.get("code").unwrap_or(&Value::Null)) == py_str(&row["parent_id"])
        && !truthy(row.get("parent_search_truncated"))
        && connected == 1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(code: &str, kind: &str, parent: &str, prefix: &str) -> Row {
        json!({
            "key": format!("s:{code}"), "source_id": "s", "feature_id": code, "code": code,
            "name": code, "route_type": kind, "parent_code": parent, "prefix": prefix,
            "direction_state": "forward", "errors": [],
        })
        .as_object()
        .cloned()
        .unwrap()
    }

    fn street(code: &str) -> Row {
        row(code, "street", "", "")
    }

    fn errs(r: &Row) -> Vec<&str> {
        errors(r).collect()
    }

    fn by_code(plan: &[Row]) -> HashMap<String, Row> {
        plan.iter()
            .map(|r| (s(r, "code").to_owned(), r.clone()))
            .collect()
    }

    #[test]
    fn multilevel_out_of_order_and_local_prefix() {
        let plan = plan_routes(
            vec![
                row("C", "alley", "B", "6"),
                row("B", "alley", "A", "155"),
                street("A"),
            ],
            &[],
        )
        .unwrap();
        let codes: Vec<&str> = plan.iter().map(|r| s(r, "code")).collect();
        assert_eq!(codes, ["A", "B", "C"]);
        let prefixes: Vec<Value> = plan.iter().map(|r| r["resolved_prefix"].clone()).collect();
        assert_eq!(prefixes, [Value::Null, json!("155"), json!("155/6")]);
        assert!(plan.iter().all(|r| s(r, "state") == "ready"));
    }

    #[test]
    fn full_prefix_parent_and_duplicate_checks() {
        let plan = plan_routes(
            vec![
                street("A"),
                row("B", "alley", "A", "155"),
                row("C", "alley", "B", "155/3"),
                row("D", "alley", "B", "3"),
                row("E", "alley", "B", "157/4"),
            ],
            &[],
        )
        .unwrap();
        let by = by_code(&plan);
        assert_eq!(by["C"]["resolved_prefix"], "155/3");
        assert!(errs(&by["D"]).contains(&"duplicate_sibling_prefix"));
        assert!(errs(&by["E"]).contains(&"prefix_does_not_follow_parent"));
    }

    #[test]
    fn cycle_or_missing_parent_block() {
        let plan = plan_routes(
            vec![
                row("A", "alley", "B", "1"),
                row("B", "alley", "A", "2"),
                row("C", "alley", "missing", "4"),
            ],
            &[],
        )
        .unwrap();
        assert!(plan.iter().all(|r| s(r, "state") == "error"));
        assert!(errs(&by_code(&plan)["A"]).contains(&"cycle"));
    }

    fn old_a() -> Value {
        json!({
            "id": "old-id", "name": "A", "route_type": "street", "prefix": null,
            "parent_route_id": null, "linked_source_id": "s", "linked_feature_id": "A",
            "direction_state": "forward", "source_label": "route-tree-import:A", "revision": 1,
        })
    }

    #[test]
    fn existing_parent_by_saved_code_and_idempotency() {
        let old = [old_a()];
        let plan = plan_routes(vec![street("A"), row("B", "alley", "A", "155")], &old).unwrap();
        assert_eq!(s(&plan[0], "state"), "existing");
        assert_eq!(s(&plan[1], "state"), "ready");
        assert_eq!(s(&plan[1], "parent_id"), "old-id");
        let next = plan_routes(vec![row("C", "alley", "A", "9")], &old).unwrap();
        assert_eq!(s(&next[0], "state"), "ready");
        assert_eq!(s(&next[0], "parent_id"), "old-id");
    }

    #[test]
    fn labels_and_expected_parent() {
        assert_eq!(
            address_label("Đường Phạm Hùng hẻm C19/30a"),
            Some(("pham hung".into(), "C19/30A".into()))
        );
        assert_eq!(address_label("Hẻm 30A"), None);
        assert_eq!(
            address_label("QL50 hẻm 2"),
            Some(("quoc lo 50".into(), "2".into()))
        );
        let r = json!({"name": "Đường A hẻm 155/6/3", "prefix": "3"});
        assert_eq!(
            expected_parent(r.as_object().unwrap()),
            Some(("a".into(), "155/6".into()))
        );
    }

    #[test]
    fn duplicate_paths_block_both_candidates_and_children() {
        let plan = plan_routes(
            vec![
                street("R"),
                row("A", "alley", "R", "C19"),
                row("B", "alley", "R", "c19"),
                row("C", "alley", "A", "30A"),
            ],
            &[],
        )
        .unwrap();
        let by = by_code(&plan);
        assert!(errs(&by["A"]).contains(&"duplicate_sibling_prefix"));
        assert!(errs(&by["B"]).contains(&"duplicate_sibling_prefix"));
        assert!(errs(&by["C"]).contains(&"parent_has_errors"));
    }

    #[test]
    fn partial_selection_requires_ancestors_and_blocks_error_rows() {
        let items = plan_routes(
            vec![
                street("R"),
                row("A", "alley", "R", "C19"),
                row("B", "alley", "A", "30A"),
                row("bad", "alley", "missing", "1"),
            ],
            &[],
        )
        .unwrap();
        let keys = |k: &[&str]| k.iter().map(|x| (*x).to_owned()).collect::<Vec<_>>();
        let picked =
            selection_rows(&items, 1, Some(keys(&["s:R", "s:A", "s:B"]).as_slice())).unwrap();
        let codes: Vec<&str> = picked.iter().map(|r| s(r, "code")).collect();
        assert_eq!(codes, ["R", "A", "B"]);
        assert!(selection_rows(&items, 1, None).is_err());
        for bad in [
            vec![],
            vec!["s:A"],
            vec!["s:B", "s:R"],
            vec!["s:bad"],
            vec!["s:R", "s:R"],
            vec!["unknown"],
        ] {
            assert!(
                selection_rows(&items, 1, Some(keys(&bad).as_slice())).is_err(),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn survey_conflicts_and_existing_links_block_review() {
        let mut rows = vec![json!({
            "code": "A", "name": "Đường A hẻm C19", "review_reason": "legacy",
            "state": "ready", "errors": [],
        })
        .as_object()
        .cloned()
        .unwrap()];
        assert_eq!(survey_codes(&rows), Some(vec!["A".to_owned()]));
        let observations = [
            json!({"code": "A", "road": "Đường A", "alley": "C19", "unassigned": true, "amount": 2}),
            json!({"code": "A", "road": "Đường B", "alley": "C19", "unassigned": true, "amount": 3,
                   "example_sources": ["source-b"], "example_features": ["house-b"]}),
            json!({"code": "A", "road": "Đường A", "alley": "C19", "unassigned": false, "amount": 1}),
        ];
        survey_finish(&mut rows, &RouteIndex::new(vec![]), &observations);
        let c = &rows[0];
        assert_eq!(
            c["survey_evidence"],
            json!({"referenced": 6, "unassigned": 5, "address_conflicts": 3})
        );
        assert_eq!(
            c["survey_review"]["counts"],
            json!({"exact": 3, "different_road": 3})
        );
        assert_eq!(
            c["survey_review"]["groups"][0]["examples"],
            json!([{"source_id": "source-b", "feature_id": "house-b"}])
        );
        assert_eq!(s(c, "state"), "error");
        let mut e = errs(c);
        e.sort_unstable();
        assert_eq!(
            e,
            [
                "review_building_already_linked",
                "review_survey_address_conflict"
            ]
        );
    }

    fn route(code: &str, name: &str, prefix: &str, kind: &str, parent: &str) -> Row {
        let mut r = row(code, kind, parent, prefix);
        r.insert("name".into(), json!(name));
        r.insert("review_reason".into(), json!("legacy"));
        r
    }

    #[test]
    fn missing_ancestor_levels_are_distinct_and_aggregated() {
        let rows = vec![
            route("R", "Đường A", "", "street", ""),
            route("A", "Đường A hẻm C19/30A/6", "C19/30A/6", "alley", ""),
            route("B", "Đường A hẻm C19/30A/2", "C19/30A/2", "alley", ""),
        ];
        let tasks = parent_tasks(&rows, &[]);
        let prefixes: Vec<&str> = tasks
            .iter()
            .map(|t| t["prefix"].as_str().unwrap())
            .collect();
        assert_eq!(prefixes, ["C19", "C19/30A"]);
        let children: Vec<i64> = tasks
            .iter()
            .map(|t| t["children"].as_i64().unwrap())
            .collect();
        assert_eq!(children, [2, 2]);
        assert_eq!(tasks[0]["id"], json!(stable_id(&["a", "C19"])));
        assert_eq!(tasks[0]["name"], "Đường A hẻm C19");
    }

    #[test]
    fn explicit_merge_preserves_source_codes_and_disputed_parent() {
        let mut rows = vec![
            route("A", "Đường A hẻm C19", "C19", "alley", "X"),
            route("B", "Đường A hẻm C19", "C19", "alley", "Y"),
        ];
        let id = format!("recovery-{}", stable_id(&["s:A", "s:B"]));
        let item = json!({
            "id": id, "name": "Đường A hẻm C19", "prefix": "C19",
            "members": [{"key": "s:A"}, {"key": "s:B"}], "status": "connected",
        })
        .as_object()
        .cloned()
        .unwrap();
        apply_merges(&mut rows, &[], &[item.clone()], &[id.clone()]).unwrap();
        assert_eq!(s(&rows[0], "code"), "A");
        assert_eq!(s(&rows[1], "code"), "B");
        assert!(rows.iter().all(|r| s(r, "logical_code") == id
            && s(r, "parent_code").is_empty()
            && r["legacy_parent_codes"] == json!(["X", "Y"])));
        let mut disconnected = item;
        disconnected.insert("status".into(), json!("disconnected"));
        assert!(apply_merges(&mut rows, &[], &[disconnected], &[id]).is_err());
    }

    #[test]
    fn python_float_and_json_encoding() {
        assert_eq!(py_float(1.0), "1.0");
        assert_eq!(py_float(12.34), "12.34");
        assert_eq!(py_float(1e-5), "1e-05");
        assert_eq!(py_float(1.5e16), "1.5e+16");
        assert_eq!(py_float(0.0001), "0.0001");
        assert_eq!(py_float(123456789.0), "123456789.0");
        assert_eq!(
            py_json(&json!({"b": [1, 2.5, null], "a": "Đường \"x\""})),
            r#"{"a":"Đường \"x\"","b":[1,2.5,null]}"#
        );
        assert_eq!(raw_root("Đường A  HẺM 12/3"), "Đường A");
        assert_eq!(raw_root("Hẻm 5"), "Hẻm 5");
    }
}
