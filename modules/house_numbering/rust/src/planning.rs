//! Exact ports of `batch_numbering.build_route_plan` and `numbering_audit`.
//!
//! Pure functions over one route snapshot; persistence supplies rows and
//! revalidates at apply. Nothing here renumbers existing anchors.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde_json::{json, Value};

use crate::domain::{
    locate_on_route, parse_number, route_prefix, suggest_number, Address, DomainError, Rule,
    SuggestOptions,
};

const STATES: &[&str] = &[
    "old",
    "certificate",
    "cad",
    "survey",
    "proposed",
    "submitted",
    "approved",
    "reserved",
];
const PROTECTED: &[&str] = &["submitted", "approved", "reserved"];
const MAX_DETAILS: usize = 40;

/// Building centroid on the route with its current number (`building_points_for_route`).
#[derive(Debug, Clone)]
pub struct PointRow {
    pub id: String,
    pub number_id: Option<String>,
    pub number_text: Option<String>,
    pub kind: Option<String>,
    pub x: f64,
    pub y: f64,
}

/// Active route number (`list_route_numbers`).
#[derive(Debug, Clone)]
pub struct NumberRow {
    pub id: String,
    pub building_id: Option<String>,
    pub number_text: String,
    pub kind: String,
    pub side: Option<String>,
    pub measure_m: Option<f64>,
}

/// Numbering target state (`numbering_buildings`).
#[derive(Debug, Clone)]
pub struct TargetRow {
    pub id: String,
    pub state: String,
    pub eligibility: String,
    pub protected_number: bool,
}

#[derive(Debug, Clone)]
pub struct RouteSnapshot<'a> {
    pub route_id: &'a str,
    pub srid: i32,
    pub prefix: &'a str,
    pub vertices: &'a [(f64, f64)],
}

#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    pub building_id: String,
    pub route_id: String,
    pub number: String,
    pub side: String,
    pub measure: f64,
    pub reason: &'static str,
}

impl Change {
    pub fn to_json(&self) -> Value {
        json!({
            "building_id": self.building_id,
            "route_id": self.route_id,
            "old_number": null,
            "number": self.number,
            "side": self.side,
            "measure": self.measure,
            "reason": self.reason,
        })
    }
}

/// Python `round(value, 6)` as a hashable key.
fn round6(value: f64) -> String {
    format!("{value:.6}")
}

/// Port of `build_route_plan`: one coherent proposal for a route, plus warnings.
pub fn build_route_plan(
    route: &RouteSnapshot<'_>,
    rule: &Rule,
    points: &[PointRow],
    numbers: &[NumberRow],
    targets: &[TargetRow],
) -> Result<(Vec<Change>, Vec<String>), DomainError> {
    let crs = format!("EPSG:{}", route.srid);
    let mut warnings = Vec::new();
    let mut changes = Vec::new();
    let mut locations = HashMap::new();
    let mut primary_ids = HashSet::new();
    let mut addresses = Vec::new();
    let mut occupancy: HashMap<String, HashSet<Option<String>>> = HashMap::new();
    let mut numbers_by_building: HashMap<&str, Vec<&NumberRow>> = HashMap::new();
    for n in numbers {
        occupancy
            .entry(n.number_text.clone())
            .or_default()
            .insert(n.building_id.clone());
        if let Some(bid) = n.building_id.as_deref() {
            numbers_by_building.entry(bid).or_default().push(n);
        }
    }
    for row in points {
        let location = locate_on_route((row.x, row.y), route.vertices, &crs)?;
        if let (Some(side), Some(text)) = (location.side, row.number_text.as_deref()) {
            if location.distance <= rule.max_distance && !text.is_empty() {
                let number_id = row.number_id.clone().unwrap_or_else(|| "None".to_owned());
                primary_ids.insert(number_id.clone());
                let state = row.kind.as_deref().filter(|kind| !kind.is_empty());
                let state = state
                    .filter(|kind| STATES.contains(kind))
                    .unwrap_or("proposed");
                addresses.push(Address::new(
                    &number_id,
                    text,
                    location.measure,
                    side,
                    state,
                )?);
            }
        }
        locations.insert(row.id.as_str(), (location, row));
    }
    for n in numbers {
        if !primary_ids.contains(&n.id) {
            let side = n
                .side
                .as_deref()
                .filter(|side| matches!(*side, "left" | "right"))
                .unwrap_or("left");
            addresses.push(Address::new(
                &n.id,
                &n.number_text,
                n.measure_m.unwrap_or(0.0),
                side,
                "reserved",
            )?);
        }
    }
    let mut sorted_targets = Vec::new();
    for building in targets {
        let bid = building.id.as_str();
        if building.state != "active" || building.eligibility == "ineligible" {
            warnings.push(format!("building {bid}: ineligible_building"));
        } else if !locations.contains_key(bid) {
            warnings.push(format!("building {bid}: invalid_geometry"));
        } else if building.protected_number
            || numbers_by_building
                .get(bid)
                .is_some_and(|rows| rows.iter().any(|n| PROTECTED.contains(&n.kind.as_str())))
        {
            warnings.push(format!("building {bid}: protected_number"));
        } else if locations[bid].0.side.is_none() {
            warnings.push(format!("building {bid}: ambiguous_route_side"));
        } else if locations[bid].0.distance > rule.max_distance {
            warnings.push(format!("building {bid}: outside_route_distance"));
        } else {
            sorted_targets.push(bid);
        }
    }
    let mut counts: HashMap<(&str, String), usize> = HashMap::new();
    for bid in &sorted_targets {
        let loc = &locations[bid].0;
        *counts
            .entry((loc.side.unwrap_or_default(), round6(loc.measure)))
            .or_default() += 1;
    }
    sorted_targets.sort_by(|a, b| {
        let (la, lb) = (&locations[a].0, &locations[b].0);
        la.side
            .cmp(&lb.side)
            .then(
                la.measure
                    .partial_cmp(&lb.measure)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
            .then(a.cmp(b))
    });
    for bid in sorted_targets {
        let (loc, row) = &locations[bid];
        let side = loc.side.unwrap_or_default();
        if counts[&(side, round6(loc.measure))] > 1 {
            warnings.push(format!("building {bid}: ambiguous_location"));
            continue;
        }
        if row
            .number_text
            .as_deref()
            .is_some_and(|text| !text.is_empty())
        {
            warnings.push(if row.kind.as_deref() == Some("proposed") {
                format!("building {bid}: already_proposed")
            } else {
                format!("building {bid}: existing_number_requires_review")
            });
            continue;
        }
        let proposal = suggest_number(
            &addresses,
            loc.measure,
            side,
            rule,
            SuggestOptions {
                prefix: route.prefix,
                distance: loc.distance,
                existing_id: None,
                batch_before_first: true,
            },
        )?;
        let Some(number) = proposal.number else {
            warnings.push(format!("building {bid}: {}", proposal.reason));
            continue;
        };
        if occupancy.get(&number).is_some_and(|set| !set.is_empty()) {
            warnings.push(format!("building {bid}: number_occupied"));
            continue;
        }
        addresses.push(Address::new(
            &format!("batch:{bid}"),
            &number,
            loc.measure,
            side,
            "proposed",
        )?);
        occupancy
            .entry(number.clone())
            .or_default()
            .insert(Some(bid.to_owned()));
        changes.push(Change {
            building_id: bid.to_owned(),
            route_id: route.route_id.to_owned(),
            number,
            side: side.to_owned(),
            measure: loc.measure,
            reason: proposal.reason,
        });
    }
    Ok((changes, warnings))
}

#[derive(Debug, Clone)]
struct Anchor {
    number: String,
    building_id: String,
    measure: f64,
    parsed: (i64, i64),
}

/// Port of `audit_route_numbering`.
pub fn audit_route_numbering(
    route: &RouteSnapshot<'_>,
    rule: &Rule,
    points: &[PointRow],
    numbers: &[NumberRow],
    changes: &[Change],
) -> Result<Value, DomainError> {
    let prefix = route_prefix(route.prefix)?;
    let style = rule.suffix_style.as_str();
    let crs = format!("EPSG:{}", route.srid);
    let mut occupied: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    let mut anchors: [(&str, Vec<Anchor>); 2] = [("left", Vec::new()), ("right", Vec::new())];
    let mut parity_errors: Vec<Value> = Vec::new();
    let mut invalid_formats: Vec<Value> = Vec::new();
    let mut ordering_errors: Vec<Value> = Vec::new();
    let mut gaps: Vec<Value> = Vec::new();
    let by_id: HashMap<&str, &NumberRow> = numbers.iter().map(|n| (n.id.as_str(), n)).collect();

    let check = |number: &str,
                 side: Option<&str>,
                 origin: &str,
                 bid: Option<&str>,
                 parity_errors: &mut Vec<Value>,
                 invalid_formats: &mut Vec<Value>|
     -> Result<Option<(i64, i64)>, DomainError> {
        let parsed = parse_number(number, &prefix, style);
        match (parsed, side) {
            (None, _) => invalid_formats.push(json!({
                "number": number, "building_id": bid, "origin": origin,
            })),
            (Some(value), Some(side @ ("left" | "right")))
                if value.0 % 2 != rule.parity(side)? =>
            {
                parity_errors.push(json!({
                    "number": number, "building_id": bid, "side": side, "origin": origin,
                }));
            }
            _ => {}
        }
        Ok(parsed)
    };

    for n in numbers {
        occupied
            .entry(n.number_text.clone())
            .or_default()
            .push(json!({
                "origin": "existing", "building_id": n.building_id, "number_id": n.id,
            }));
        if n.kind != "reserved" {
            check(
                &n.number_text,
                n.side.as_deref(),
                "existing",
                n.building_id.as_deref(),
                &mut parity_errors,
                &mut invalid_formats,
            )?;
        }
    }
    for point in points {
        let number_id = point.number_id.as_deref().unwrap_or("");
        let Some(n) = by_id.get(number_id) else {
            continue;
        };
        if point.number_text.is_none() || n.kind == "reserved" {
            continue;
        }
        let loc = locate_on_route((point.x, point.y), route.vertices, &crs)?;
        let Some(side) = loc.side else {
            continue;
        };
        if loc.distance > rule.max_distance {
            continue;
        }
        if let Some(parsed) = parse_number(&n.number_text, &prefix, style) {
            if parsed.0 % 2 != rule.parity(side)?
                && !parity_errors.iter().any(|x| {
                    x["number"].as_str() == Some(n.number_text.as_str())
                        && x["building_id"].as_str() == Some(point.id.as_str())
                })
            {
                parity_errors.push(json!({
                    "number": n.number_text, "building_id": point.id,
                    "side": side, "origin": "geometry",
                }));
            }
            let slot = if side == "left" { 0 } else { 1 };
            anchors[slot].1.push(Anchor {
                number: n.number_text.clone(),
                building_id: point.id.clone(),
                measure: loc.measure,
                parsed,
            });
        }
    }
    for change in changes {
        if change.reason == "existing_number" {
            continue;
        }
        occupied
            .entry(change.number.clone())
            .or_default()
            .push(json!({
                "origin": "proposal", "building_id": change.building_id, "number_id": null,
            }));
        let parsed = check(
            &change.number,
            Some(&change.side),
            "proposal",
            Some(&change.building_id),
            &mut parity_errors,
            &mut invalid_formats,
        )?;
        if let (Some(parsed), Some(slot)) = (
            parsed,
            ["left", "right"].iter().position(|s| *s == change.side),
        ) {
            anchors[slot].1.push(Anchor {
                number: change.number.clone(),
                building_id: change.building_id.clone(),
                measure: change.measure,
                parsed,
            });
        }
    }
    let duplicates: Vec<Value> = occupied
        .iter()
        .filter(|(_, entries)| entries.len() > 1)
        .map(|(number, entries)| json!({"number": number, "occupants": entries}))
        .collect();
    let occupied_bases: HashSet<i64> = occupied
        .keys()
        .filter_map(|number| parse_number(number, &prefix, style).map(|p| p.0))
        .collect();
    let mut gap_total = 0_i64;
    for (side, values) in &mut anchors {
        values.sort_by(|a, b| {
            a.measure
                .partial_cmp(&b.measure)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.number.cmp(&b.number))
                .then_with(|| a.building_id.cmp(&b.building_id))
        });
        let parity = rule.parity(side)?;
        for pair in values.windows(2) {
            let (prev, anchor) = (&pair[0], &pair[1]);
            if prev.measure >= anchor.measure - 1e-6 || prev.parsed >= anchor.parsed {
                ordering_errors.push(json!({
                    "side": side,
                    "before": prev.number,
                    "after": anchor.number,
                    "before_building_id": prev.building_id,
                    "after_building_id": anchor.building_id,
                }));
            } else {
                let (left, right) = (prev.parsed.0, anchor.parsed.0);
                let count = 0_i64.max((right - left).div_euclid(2) - 1);
                let used = occupied_bases
                    .iter()
                    .filter(|v| v.rem_euclid(2) == parity && left < **v && **v < right)
                    .count();
                let count = 0_i64.max(count - i64::try_from(used).unwrap_or(i64::MAX));
                if count > 0 {
                    gap_total += count;
                    gaps.push(json!({
                        "side": side,
                        "from_number": prev.number,
                        "to_number": anchor.number,
                        "unused_slots": count,
                    }));
                }
            }
        }
    }
    let blockers =
        duplicates.len() + parity_errors.len() + invalid_formats.len() + ordering_errors.len();
    let head =
        |items: &[Value]| -> Vec<Value> { items.iter().take(MAX_DETAILS).cloned().collect() };
    Ok(json!({
        "route_id": route.route_id,
        "existing": numbers.len(),
        "proposed": changes.len(),
        "duplicate_count": duplicates.len(),
        "parity_error_count": parity_errors.len(),
        "invalid_format_count": invalid_formats.len(),
        "ordering_error_count": ordering_errors.len(),
        "gap_count": gap_total,
        "gap_ranges": gaps.len(),
        "blocking_issues": blockers,
        "duplicates": head(&duplicates),
        "parity_errors": head(&parity_errors),
        "invalid_formats": head(&invalid_formats),
        "ordering_errors": head(&ordering_errors),
        "gaps": head(&gaps),
    }))
}

/// Port of `summarize_audit`.
pub fn summarize_audit(rows: &[Value]) -> Value {
    let keys = [
        "existing",
        "proposed",
        "duplicate_count",
        "parity_error_count",
        "invalid_format_count",
        "ordering_error_count",
        "gap_count",
        "gap_ranges",
        "blocking_issues",
    ];
    let mut summary = serde_json::Map::new();
    for key in keys {
        let total: i64 = rows.iter().filter_map(|row| row[key].as_i64()).sum();
        summary.insert(key.to_owned(), json!(total));
    }
    let blocking = summary["blocking_issues"].as_i64().unwrap_or(0);
    summary.insert("routes".to_owned(), json!(rows.len()));
    summary.insert("details".to_owned(), Value::Array(rows.to_vec()));
    summary.insert("can_save".to_owned(), json!(blocking == 0));
    Value::Object(summary)
}

/// One occupant of a full number (`find_tree_collisions` input row).
#[derive(Debug, Clone)]
pub struct Occupant {
    pub route_id: String,
    pub number_text: String,
    pub building_id: Option<String>,
    pub origin: &'static str,
}

/// Port of `find_tree_collisions`: same full number on different routes of one street tree.
pub fn find_tree_collisions(
    parent_by_id: &HashMap<String, Option<String>>,
    occupants: &[Occupant],
) -> Vec<Value> {
    let root = |rid: &str| -> String {
        let mut seen = HashSet::new();
        let mut current = rid.to_owned();
        while let Some(Some(parent)) = parent_by_id.get(&current) {
            if !seen.insert(current.clone()) {
                return rid.to_owned();
            }
            current = parent.clone();
        }
        current
    };
    let mut groups: BTreeMap<(String, String), Vec<Value>> = BTreeMap::new();
    for occupant in occupants {
        groups
            .entry((root(&occupant.route_id), occupant.number_text.clone()))
            .or_default()
            .push(json!({
                "route_id": occupant.route_id,
                "building_id": occupant.building_id,
                "origin": occupant.origin,
            }));
    }
    groups
        .into_iter()
        .filter(|(_, values)| {
            values
                .iter()
                .filter_map(|v| v["route_id"].as_str())
                .collect::<HashSet<_>>()
                .len()
                > 1
        })
        .map(|((scope, number), values)| {
            json!({"root_route_id": scope, "number": number, "occupants": values})
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans_two_houses_on_one_side_without_renumbering() {
        let rule = Rule::new("v1", true, "letters", 50.0, None).unwrap();
        let vertices = [(0.0, 0.0), (100.0, 0.0)];
        let route = RouteSnapshot {
            route_id: "r",
            srid: 3857,
            prefix: "",
            vertices: &vertices,
        };
        let points = vec![
            PointRow {
                id: "a".into(),
                number_id: None,
                number_text: None,
                kind: None,
                x: 20.0,
                y: -10.0,
            },
            PointRow {
                id: "b".into(),
                number_id: None,
                number_text: None,
                kind: None,
                x: 40.0,
                y: -10.0,
            },
        ];
        let targets: Vec<TargetRow> = ["a", "b"]
            .iter()
            .map(|id| TargetRow {
                id: (*id).into(),
                state: "active".into(),
                eligibility: "eligible".into(),
                protected_number: false,
            })
            .collect();
        let (changes, warnings) = build_route_plan(&route, &rule, &points, &[], &targets).unwrap();
        assert!(warnings.is_empty());
        assert_eq!(
            changes
                .iter()
                .map(|c| c.number.as_str())
                .collect::<Vec<_>>(),
            vec!["2", "4"]
        );
        let audit = audit_route_numbering(&route, &rule, &points, &[], &changes).unwrap();
        assert_eq!(audit["blocking_issues"], 0);
    }

    #[test]
    fn finds_cross_route_duplicates_under_one_street() {
        let parents: HashMap<String, Option<String>> = [
            ("s".to_owned(), None),
            ("a".to_owned(), Some("s".to_owned())),
        ]
        .into_iter()
        .collect();
        let occupants = vec![
            Occupant {
                route_id: "s".into(),
                number_text: "2".into(),
                building_id: None,
                origin: "existing",
            },
            Occupant {
                route_id: "a".into(),
                number_text: "2".into(),
                building_id: Some("b".into()),
                origin: "proposal",
            },
        ];
        let issues = find_tree_collisions(&parents, &occupants);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0]["root_route_id"], "s");
    }
}
