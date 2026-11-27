//! Ports of `catalog_recovery.catalog_preview` and `prefix_recovery.prefix_plan`.
//!
//! Review queues over existing source geometry; they never invent a line.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde_json::{json, Map, Value};

use crate::assignment::{
    address_label, alley_name, digest, non_address, py_id, py_round, py_str, road_name, text,
    RouteIndex,
};
use crate::prefix::address_chain;

pub const MAX_JUNCTION_M: f64 = 8.0;

type PathKey = (String, String);

/// Insertion-ordered `Counter`/`dict` stand-in.
struct Ordered<V> {
    keys: Vec<String>,
    values: HashMap<String, V>,
}

impl<V> Default for Ordered<V> {
    fn default() -> Self {
        Self {
            keys: Vec::new(),
            values: HashMap::new(),
        }
    }
}

impl<V> Ordered<V> {
    fn entry(&mut self, key: &str, make: impl FnOnce() -> V) -> &mut V {
        if !self.values.contains_key(key) {
            self.keys.push(key.to_owned());
        }
        self.values.entry(key.to_owned()).or_insert_with(make)
    }

    fn iter(&self) -> impl Iterator<Item = (&String, &V)> + '_ {
        self.keys.iter().map(|k| (k, &self.values[k]))
    }
}

/// `collections.Counter` rendered as a dict with only incremented keys.
#[derive(Debug, Default)]
pub struct Counts(BTreeMap<String, i64>);

impl Counts {
    fn add(&mut self, key: &str, n: i64) {
        *self.0.entry(key.to_owned()).or_default() += n;
    }

    fn to_json(&self) -> Value {
        json!(self.0)
    }
}

struct PrefixGroup {
    id: String,
    road: String,
    normalized_road: String,
    alley: String,
    codes: Ordered<i64>,
    old_routes: Ordered<Value>,
    building_ids: Vec<String>,
}

/// First pass of `catalog_preview` over routes, source lines and observations.
pub struct CatalogScan {
    counts: Counts,
    source_paths: HashMap<PathKey, Vec<Value>>,
    groups: Ordered<Map<String, Value>>,
    prefix_groups: Ordered<PrefixGroup>,
    missing: Vec<(PathKey, Map<String, Value>)>,
    code_observations: HashMap<String, HashMap<PathKey, i64>>,
    by_id: HashMap<String, (String, usize)>,
    /// `(building_id, route_id)` evidence to measure.
    pub pairs: Vec<(String, String)>,
}

pub fn catalog_scan(idx: &RouteIndex, lines: &[Value], obs: &[Value], tree: bool) -> CatalogScan {
    let mut occupied: HashSet<(String, String)> = HashSet::new();
    for r in idx.routes.values() {
        match r["segments"].as_array().filter(|s| !s.is_empty()) {
            Some(segments) => {
                for s in segments {
                    occupied.insert((py_id(&s["source_id"]), py_str(&s["feature_id"])));
                }
            }
            None => {
                occupied.insert((
                    py_id(&r["linked_source_id"]),
                    py_str(&r["linked_feature_id"]),
                ));
            }
        }
    }
    let mut scan = CatalogScan {
        counts: Counts::default(),
        source_paths: HashMap::new(),
        groups: Ordered::default(),
        prefix_groups: Ordered::default(),
        missing: Vec::new(),
        code_observations: HashMap::new(),
        by_id: HashMap::new(),
        pairs: Vec::new(),
    };
    for f in lines {
        let key = (py_id(&f["source_id"]), py_str(&f["feature_id"]));
        if occupied.contains(&key) || !idx.coded(text(&f["code"])).is_empty() {
            continue;
        }
        let name = text(&f["name"]);
        let path = if let Some((road, prefix)) = address_label(name) {
            (road, prefix.to_lowercase())
        } else if f["route_type"] == "street" && py_str(&f["prefix"]).is_empty() {
            (road_name(name), String::new())
        } else {
            continue;
        };
        scan.source_paths.entry(path).or_default().push(json!({
            "source_id": key.0,
            "feature_id": f["feature_id"],
            "name": f["name"],
            "code": f["code"],
            "reason": f["review_reason"],
        }));
    }
    for row in obs {
        let code = text(&row["code"]).trim().to_owned();
        let (raw_road, raw_alley) = (text(&row["road"]), text(&row["alley"]));
        let name = road_name(raw_road);
        let alley = alley_name(raw_alley);
        *scan
            .code_observations
            .entry(code.clone())
            .or_default()
            .entry((name.clone(), alley.clone()))
            .or_default() += 1;
        let mapped = row["mapped"] == true;
        if !py_str(&row["route_id"]).is_empty()
            || row["locked"] == true
            || row["usable"] != true
            || !mapped
            || name.is_empty()
            || non_address(&name)
        {
            continue;
        }
        let (_, why) = idx.match_route(raw_road, raw_alley, &code, mapped);
        let (exact, _) = idx.match_route(raw_road, raw_alley, "", true);
        if why != "attribute_conflict" || !exact.is_empty() {
            continue;
        }
        scan.counts.add("total", 1);
        let path = (name.clone(), alley.clone());
        if let Some(source) = scan.source_paths.get(&path).filter(|s| !s.is_empty()) {
            scan.counts.add("source_path_houses", 1);
            let sources: Vec<Value> = source.iter().take(10).cloned().collect();
            let position = match scan.missing.iter().position(|(k, _)| *k == path) {
                Some(i) => i,
                None => {
                    let fresh = json!({"count": 0, "sources": [], "road": "", "alley": ""});
                    scan.missing
                        .push((path.clone(), fresh.as_object().cloned().unwrap_or_default()));
                    scan.missing.len() - 1
                }
            };
            let x = &mut scan.missing[position].1;
            x.insert("road".into(), json!(raw_road));
            x.insert("alley".into(), json!(alley));
            x.insert("sources".into(), Value::Array(sources));
            let count = x["count"].as_i64().unwrap_or(0) + 1;
            x.insert("count".into(), json!(count));
        }
        let coded = idx.coded(&code);
        if coded.len() != 1 {
            scan.counts.add("ambiguous_code", 1);
            continue;
        }
        let rid = coded.first().cloned().unwrap_or_default();
        let route = &idx.routes[&rid];
        let bid = py_str(&row["id"]);
        if alley != alley_name(&py_str(&route["prefix"])) {
            scan.counts.add("prefix_mismatch", 1);
            if tree {
                let key = digest(&json!([name, alley]))[..32].to_owned();
                let g = scan.prefix_groups.entry(&key, || PrefixGroup {
                    id: key.clone(),
                    road: raw_road.to_owned(),
                    normalized_road: name.clone(),
                    alley: alley.clone(),
                    codes: Ordered::default(),
                    old_routes: Ordered::default(),
                    building_ids: Vec::new(),
                });
                g.building_ids.push(bid);
                *g.codes.entry(&code, || 0) += 1;
                *g.old_routes.entry(&rid, || Value::Null) = json!({
                    "route_id": rid,
                    "name": idx.root_route_name(&rid),
                    "prefix": route["prefix"],
                    "source_id": py_id(&route["linked_source_id"]),
                    "feature_id": route["linked_feature_id"],
                });
            }
            continue;
        }
        if idx.root_name.get(&rid) == Some(&name) {
            scan.counts.add("other", 1);
            continue;
        }
        scan.counts.add("same_prefix", 1);
        if tree {
            continue;
        }
        let key = digest(&json!([rid, name, alley, code]))[..32].to_owned();
        let g = scan.groups.entry(&key, || {
            json!({
                "id": key,
                "route_id": rid,
                "route_name": route["name"],
                "root_name": idx.root_route_name(&rid),
                "prefix": route["prefix"],
                "route_revision": route["revision"],
                "content_revision": route["content_revision"],
                "source_name": route["source_name"],
                "planned_name": route["planned_name"],
                "source_id": py_id(&route["linked_source_id"]),
                "feature_id": route["linked_feature_id"],
                "road": raw_road,
                "normalized_road": name,
                "alley": alley,
                "code": code,
                "count": 0,
                "near": 0,
                "observations": 0,
                "evidence": [],
            })
            .as_object()
            .cloned()
            .unwrap_or_default()
        });
        g["count"] = json!(g["count"].as_i64().unwrap_or(0) + 1);
        let eligible = row["eligibility"] != "ineligible";
        let evidence = g.get_mut("evidence").and_then(Value::as_array_mut);
        let index = evidence.as_ref().map_or(0, |e| e.len());
        if let Some(evidence) = evidence {
            evidence.push(json!({
                "id": bid,
                "revision": row["revision"],
                "content_revision": row["content_revision"],
                "distance": null,
                "eligible": eligible,
            }));
        }
        scan.by_id.insert(bid.clone(), (key, index));
        if eligible {
            scan.pairs.push((bid, rid));
        }
    }
    scan
}

pub struct Window<'a> {
    pub search: &'a str,
    pub offset: usize,
    pub limit: usize,
}

fn page(items: Vec<Value>, window: &Window<'_>) -> (Vec<Value>, Value) {
    let end = window.offset.saturating_add(window.limit);
    let next = if items.len() > end {
        json!(end)
    } else {
        Value::Null
    };
    let slice = items
        .into_iter()
        .skip(window.offset)
        .take(window.limit)
        .collect();
    (slice, next)
}

/// Finish the alias catalog with evidence distances (`building_id → metres`).
pub fn catalog_finish(
    mut scan: CatalogScan,
    distances: &[(String, Option<f64>)],
    window: &Window<'_>,
) -> Value {
    for (bid, d) in distances {
        let (Some((key, index)), Some(d)) = (scan.by_id.get(bid), d) else {
            continue;
        };
        let g = scan.groups.entry(key, Map::new);
        g["evidence"][*index]["distance"] = json!(py_round(*d, 3));
        if *d <= 35.0 {
            g["near"] = json!(g["near"].as_i64().unwrap_or(0) + 1);
        }
    }
    let total_groups = scan.groups.keys.len();
    let mut result: Vec<Value> = Vec::new();
    for (_, g) in scan.groups.iter() {
        let mut g = g.clone();
        let observed = scan.code_observations.get(text(&g["code"]));
        let observations: i64 = observed.map_or(0, |o| o.values().sum());
        let key = (py_str(&g["normalized_road"]), py_str(&g["alley"]));
        let matching = observed.and_then(|o| o.get(&key)).copied().unwrap_or(0);
        g.insert("observations".into(), json!(observations));
        g.insert("matching_observations".into(), json!(matching));
        g.insert(
            "reviewable".into(),
            json!(g["near"].as_i64().unwrap_or(0) > 0),
        );
        let token = digest(&Value::Object(g.clone()));
        g.insert("token".into(), json!(token));
        g.remove("evidence");
        result.push(Value::Object(g));
    }
    let num = |v: &Value, k: &str| v[k].as_i64().unwrap_or(0);
    scan.counts
        .0
        .insert("near".into(), result.iter().map(|g| num(g, "near")).sum());
    let reviewable = result.iter().filter(|g| g["reviewable"] == true).count();
    scan.counts.0.insert(
        "reviewable_groups".into(),
        i64::try_from(reviewable).unwrap_or(i64::MAX),
    );
    result.sort_by(|a, b| {
        num(b, "near")
            .cmp(&num(a, "near"))
            .then(num(b, "count").cmp(&num(a, "count")))
            .then_with(|| text(&a["id"]).cmp(text(&b["id"])))
    });
    let needle = road_name(window.search);
    if !needle.is_empty() {
        result.retain(|g| {
            let hay = [
                text(&g["road"]).to_owned(),
                py_str(&g["root_name"]),
                text(&g["code"]).to_owned(),
                py_str(&g["prefix"]),
            ]
            .join(" ");
            road_name(&hay).contains(&needle)
        });
    }
    let mut missing: Vec<Map<String, Value>> = scan.missing.into_iter().map(|(_, m)| m).collect();
    missing.sort_by_key(|m| std::cmp::Reverse(m["count"].as_i64().unwrap_or(0)));
    missing.truncate(30);
    let filtered = result.len();
    let (items, next) = page(result, window);
    json!({
        "counts": scan.counts.to_json(),
        "total_groups": total_groups,
        "filtered_groups": filtered,
        "items": items,
        "next_offset": next,
        "source_matches": missing,
    })
}

/// `prefix_plan` before geometry checks: rows plus SQL specs and parent pairs.
pub struct PrefixPlan {
    results: Vec<Map<String, Value>>,
    by_id: HashMap<String, usize>,
    /// `[{id, links, building_ids}]` candidate source paths to merge.
    pub specs: Vec<Value>,
    /// `[{id, parent_id}]` junctions to measure.
    pub pairs: Vec<Value>,
}

/// Check the actual ancestor IDs, not uniqueness of street labels.
fn valid_parent(idx: &RouteIndex, parent_id: &str, root: &str, chain: &[String]) -> bool {
    let mut current = parent_id.to_owned();
    for expected in chain.iter().rev() {
        let Some(r) = idx.routes.get(&current) else {
            return false;
        };
        if idx.root_name.get(&current).map(String::as_str) != Some(root)
            || alley_name(&py_str(&r["prefix"])) != *expected
        {
            return false;
        }
        current = py_str(&r["parent_route_id"]);
    }
    current.is_empty()
}

pub fn prefix_plan(idx: &RouteIndex, scan: &CatalogScan) -> PrefixPlan {
    let mut paths: HashMap<PathKey, Vec<Value>> = HashMap::new();
    for (rid, root) in &idx.root_name {
        let r = &idx.routes[rid];
        paths
            .entry((root.clone(), alley_name(&py_str(&r["prefix"]))))
            .or_default()
            .push(json!({
                "route_id": rid,
                "source_id": py_id(&r["linked_source_id"]),
                "feature_id": r["linked_feature_id"],
                "name": r["name"],
            }));
    }
    let none = Vec::new();
    let mut plan = PrefixPlan {
        results: Vec::new(),
        by_id: HashMap::new(),
        specs: Vec::new(),
        pairs: Vec::new(),
    };
    for (_, g) in scan.prefix_groups.iter() {
        let mut codes: Vec<(&String, &i64)> = g.codes.iter().collect();
        codes.sort_by(|a, b| b.1.cmp(a.1));
        let codes: Vec<Value> = codes
            .into_iter()
            .take(10)
            .map(|(code, count)| json!({"code": code, "count": count}))
            .collect();
        let old_routes: Vec<Value> = g
            .old_routes
            .iter()
            .take(10)
            .map(|(_, v)| v.clone())
            .collect();
        let mut row = json!({
            "id": g.id,
            "road": g.road,
            "normalized_road": g.normalized_road,
            "alley": g.alley,
            "codes": codes,
            "old_routes": old_routes,
            "count": g.building_ids.len(),
            "chain": [],
            "sources": [],
            "near": 0,
            "junction_m": null,
            "parent_candidates": [],
            "geometry_connected": false,
            "status": "missing_geometry",
        })
        .as_object()
        .cloned()
        .unwrap_or_default();
        let Ok(chain) = address_chain(&g.alley) else {
            row.insert("status".into(), json!("invalid_prefix"));
            plan.results.push(row);
            continue;
        };
        let mut chain_rows = Vec::new();
        for p in &chain {
            let key = (g.normalized_road.clone(), p.clone());
            let saved = paths.get(&key).unwrap_or(&none);
            let pending = scan.source_paths.get(&key).unwrap_or(&none);
            chain_rows.push(json!({
                "prefix": p,
                "existing_count": saved.len(),
                "source_count": pending.len(),
                "existing": saved.iter().take(10).collect::<Vec<_>>(),
                "sources": pending.iter().take(10).collect::<Vec<_>>(),
            }));
        }
        let leaf_existing = chain_rows
            .last()
            .map_or(0, |c| c["existing_count"].as_u64().unwrap_or(0));
        row.insert("chain".into(), Value::Array(chain_rows));
        let leaf = chain.last().cloned().unwrap_or_default();
        let pending = scan
            .source_paths
            .get(&(g.normalized_road.clone(), leaf))
            .unwrap_or(&none);
        let status = if leaf_existing > 0 {
            "existing_path_conflict"
        } else if pending.is_empty() {
            "missing_geometry"
        } else if pending.len() > 200 {
            "ambiguous_geometry"
        } else {
            row.insert(
                "sources".into(),
                json!(pending.iter().take(10).collect::<Vec<_>>()),
            );
            plan.specs
                .push(json!({"id": g.id, "links": pending, "building_ids": g.building_ids}));
            if chain.len() == 1 {
                "root_review"
            } else {
                let parent_chain = &chain[..chain.len() - 1];
                let key = (
                    g.normalized_road.clone(),
                    parent_chain[parent_chain.len() - 1].clone(),
                );
                let parents: Vec<&Value> = paths
                    .get(&key)
                    .unwrap_or(&none)
                    .iter()
                    .filter(|p| {
                        valid_parent(idx, text(&p["route_id"]), &g.normalized_road, parent_chain)
                    })
                    .collect();
                if !parents.is_empty() && parents.len() <= 100 {
                    for p in parents {
                        plan.pairs
                            .push(json!({"id": g.id, "parent_id": p["route_id"]}));
                    }
                    "junction_pending"
                } else if parents.is_empty() {
                    "parent_missing"
                } else {
                    "ambiguous_parent"
                }
            }
        };
        row.insert("status".into(), json!(status));
        plan.by_id.insert(g.id.clone(), plan.results.len());
        plan.results.push(row);
    }
    plan
}

/// Finish `prefix_plan` with merged-line stats `(id, connected, near)` and
/// junction distances `(id, parent_id, metres)`.
pub fn prefix_finish(
    idx: &RouteIndex,
    mut plan: PrefixPlan,
    stats: &[(String, bool, i64)],
    junctions: &[(String, String, Option<f64>)],
    window: &Window<'_>,
) -> Value {
    for (id, connected, near) in stats {
        let Some(&i) = plan.by_id.get(id) else {
            continue;
        };
        let row = &mut plan.results[i];
        row.insert("near".into(), json!(near));
        row.insert("geometry_connected".into(), json!(connected));
        if !connected {
            row.insert("status".into(), json!("ambiguous_geometry"));
        }
    }
    for (id, parent_id, d) in junctions {
        let Some(&i) = plan.by_id.get(id) else {
            continue;
        };
        let r = &idx.routes[parent_id];
        if let Some(list) = plan.results[i]
            .get_mut("parent_candidates")
            .and_then(Value::as_array_mut)
        {
            list.push(json!({
                "route_id": parent_id,
                "name": r["name"],
                "source_id": py_id(&r["linked_source_id"]),
                "feature_id": r["linked_feature_id"],
                "distance_m": d.map(|d| py_round(d, 2)),
                "connected": d.is_some_and(|d| d <= MAX_JUNCTION_M),
            }));
        }
    }
    let mut counts = Counts::default();
    let mut total = 0;
    for row in &mut plan.results {
        let mut candidates = row["parent_candidates"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let dist = |c: &Value| c["distance_m"].as_f64().unwrap_or(f64::INFINITY);
        candidates.sort_by(|a, b| {
            (a["connected"] != true)
                .cmp(&(b["connected"] != true))
                .then(dist(a).total_cmp(&dist(b)))
                .then_with(|| text(&a["route_id"]).cmp(text(&b["route_id"])))
        });
        let connected = candidates.iter().filter(|c| c["connected"] == true).count();
        let near = row["near"].as_i64().unwrap_or(0);
        if row["status"] == "junction_pending" {
            let first = candidates
                .first()
                .map_or(Value::Null, |c| c["distance_m"].clone());
            row.insert("junction_m".into(), first);
            let status = if connected == 1 && near != 0 {
                "connected_review"
            } else if connected > 1 {
                "ambiguous_parent"
            } else if connected == 0 {
                "disconnected"
            } else {
                "distance"
            };
            row.insert("status".into(), json!(status));
        }
        candidates.truncate(10);
        row.insert("parent_candidates".into(), Value::Array(candidates));
        if row["status"] == "root_review" && near == 0 {
            row.insert("status".into(), json!("distance"));
        }
        let count = row["count"].as_i64().unwrap_or(0);
        counts.add(text(&row["status"]), count);
        total += count;
    }
    counts.0.insert("total".into(), total);
    let needle = road_name(window.search);
    let priority = |status: &str| match status {
        "connected_review" => 0,
        "root_review" => 1,
        "parent_missing" => 2,
        "ambiguous_geometry" => 3,
        "ambiguous_parent" => 4,
        _ => 5,
    };
    let num = |v: &Map<String, Value>, k: &str| v[k].as_i64().unwrap_or(0);
    let total_groups = plan.results.len();
    let mut filtered: Vec<Map<String, Value>> = plan
        .results
        .into_iter()
        .filter(|r| {
            if needle.is_empty() {
                return true;
            }
            let codes: Vec<&str> = r["codes"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|c| text(&c["code"]))
                .collect();
            let hay = format!(
                "{} {} {}",
                text(&r["road"]),
                text(&r["alley"]),
                codes.join(" ")
            );
            road_name(&hay).contains(&needle)
        })
        .collect();
    filtered.sort_by(|a, b| {
        priority(text(&a["status"]))
            .cmp(&priority(text(&b["status"])))
            .then(num(b, "near").cmp(&num(a, "near")))
            .then(num(b, "count").cmp(&num(a, "count")))
            .then_with(|| text(&a["id"]).cmp(text(&b["id"])))
    });
    let filtered_groups = filtered.len();
    let (items, next) = page(filtered.into_iter().map(Value::Object).collect(), window);
    json!({
        "counts": counts.to_json(),
        "total_groups": total_groups,
        "filtered_groups": filtered_groups,
        "items": items,
        "next_offset": next,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn route(id: &str, name: &str, parent: Option<&str>, prefix: Option<&str>) -> Value {
        json!({
            "id": id, "name": name,
            "route_type": if parent.is_some() { "alley" } else { "street" },
            "parent_route_id": parent, "prefix": prefix, "source_label": "manual",
            "status": "confirmed", "revision": 1, "content_revision": "a",
            "linked_source_id": "s", "linked_feature_id": id, "segments": [],
            "source_name": null, "planned_name": null,
        })
    }

    #[test]
    fn valid_parent_walks_real_ancestors() {
        let idx = RouteIndex::new(vec![
            route("root", "Đường A", None, None),
            route("p", "Đường A hẻm 12", Some("root"), Some("12")),
        ]);
        let chain = vec![String::new(), "12".to_owned()];
        assert!(valid_parent(&idx, "p", "a", &chain));
        assert!(!valid_parent(&idx, "root", "a", &chain));
    }

    #[test]
    fn catalog_groups_same_prefix_code_conflicts() {
        let idx = RouteIndex::new(vec![route("old", "Đường số 9", None, None)]);
        let obs = |id: &str, alley: &str| {
            json!({"id": id, "revision": 1, "eligibility": "eligible", "route_id": null,
                   "content_revision": "a", "road": "Đường số 9 Ấp 21", "alley": alley,
                   "code": "old", "mapped": true, "usable": true, "locked": false})
        };
        let lines = vec![json!({"source_id": "s2", "feature_id": "pending",
            "name": "Đường số 9 Ấp 21 hẻm 77", "prefix": "77", "route_type": "alley",
            "code": "legacy_77", "review_reason": "parent_required"})];
        let rows = vec![obs("a", ""), obs("b", ""), obs("c", "77")];
        let scan = catalog_scan(&idx, &lines, &rows, false);
        assert_eq!(scan.pairs.len(), 2);
        let window = Window {
            search: "",
            offset: 0,
            limit: 100,
        };
        let report = catalog_finish(
            scan,
            &[("a".into(), Some(3.0)), ("b".into(), Some(80.0))],
            &window,
        );
        assert_eq!(report["counts"]["total"], 3);
        assert_eq!(report["counts"]["prefix_mismatch"], 1);
        assert_eq!(report["counts"]["source_path_houses"], 1);
        assert_eq!(report["counts"]["near"], 1);
        assert_eq!(report["items"][0]["count"], 2);
        assert_eq!(report["items"][0]["reviewable"], true);
        assert!(report["items"][0].get("evidence").is_none());
    }
}
