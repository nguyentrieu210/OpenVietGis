//! Exact port of the pure rules in `modules/house_numbering/domain.py`.
//!
//! Inputs are already scoped to one route and measured in its projected CRS.
//! There is no database, identity or Core dependency here.

use std::collections::{BTreeSet, HashSet};

use crate::prefix::canonical_prefix;

/// User-actionable invalid or conflicting domain input (Python `DomainError`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomainError(pub String);

impl DomainError {
    fn new(message: &str) -> Self {
        Self(message.to_owned())
    }
}

impl std::fmt::Display for DomainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

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

fn finite(value: f64, name: &str) -> Result<(), DomainError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(DomainError(format!("{name} must be a finite number")))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    pub version: String,
    pub left_odd: bool,
    pub suffix_style: String,
    pub max_distance: f64,
    pub authority: Option<String>,
}

impl Rule {
    pub fn new(
        version: &str,
        left_odd: bool,
        suffix_style: &str,
        max_distance: f64,
        authority: Option<&str>,
    ) -> Result<Self, DomainError> {
        if version.trim().is_empty() {
            return Err(DomainError::new("rule version required"));
        }
        if !matches!(suffix_style, "letters" | "dash") {
            return Err(DomainError::new("unknown numbering policy"));
        }
        finite(max_distance, "max_distance")?;
        if max_distance <= 0.0 {
            return Err(DomainError::new("max_distance must be positive"));
        }
        if authority.is_some_and(|value| value.trim().is_empty()) {
            return Err(DomainError::new("authority must be a nonempty reference"));
        }
        Ok(Self {
            version: version.to_owned(),
            left_odd,
            suffix_style: suffix_style.to_owned(),
            max_distance,
            authority: authority.map(str::to_owned),
        })
    }

    /// 1 when the side carries odd numbers, 0 for even.
    pub fn parity(&self, side: &str) -> Result<i64, DomainError> {
        match side {
            "left" => Ok(i64::from(self.left_odd)),
            "right" => Ok(i64::from(!self.left_odd)),
            _ => Err(DomainError::new("side must be left or right")),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Address {
    pub id: String,
    pub number: String,
    pub measure: f64,
    pub side: String,
    pub state: String,
}

impl Address {
    pub fn new(
        id: &str,
        number: &str,
        measure: f64,
        side: &str,
        state: &str,
    ) -> Result<Self, DomainError> {
        if id.trim().is_empty() {
            return Err(DomainError::new("address ID required"));
        }
        if number.trim().is_empty() {
            return Err(DomainError::new("number must preserve nonempty raw text"));
        }
        finite(measure, "measure")?;
        if measure < 0.0 || !matches!(side, "left" | "right") || !STATES.contains(&state) {
            return Err(DomainError::new("invalid address measure, side or state"));
        }
        Ok(Self {
            id: id.to_owned(),
            number: number.to_owned(),
            measure,
            side: side.to_owned(),
            state: state.to_owned(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    pub number: Option<String>,
    pub reason: &'static str,
    pub rule_version: String,
}

fn letters_rank(suffix: &str) -> i64 {
    suffix
        .bytes()
        .fold(0, |n, letter| n * 26 + i64::from(letter - b'A') + 1)
}

fn rank_letters(mut rank: i64) -> String {
    let mut out = Vec::new();
    while rank > 0 {
        let rem = (rank - 1) % 26;
        rank = (rank - 1) / 26;
        out.push(b'A' + u8::try_from(rem).unwrap_or(0));
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

/// Canonical `PREFIX/` or empty (Python `_prefix`).
pub fn route_prefix(prefix: &str) -> Result<String, DomainError> {
    if prefix.is_empty() {
        return Ok(String::new());
    }
    canonical_prefix(prefix)
        .map(|value| format!("{value}/"))
        .map_err(|_| DomainError::new("invalid alley prefix"))
}

/// Python `_parse`: `(base, rank)` of a raw number under the route prefix and style.
pub fn parse_number(number: &str, prefix: &str, style: &str) -> Option<(i64, i64)> {
    let rest = number.strip_prefix(prefix)?;
    let bytes = rest.as_bytes();
    if bytes.first().is_none_or(|b| !(b'1'..=b'9').contains(b)) {
        return None;
    }
    let digits = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
    let base = &rest[..digits];
    let tail = &rest[digits..];
    let (letters, dash) = if tail.is_empty() {
        ("", "")
    } else if tail.bytes().all(|b| b.is_ascii_uppercase()) {
        (tail, "")
    } else if let Some(value) = tail.strip_prefix('-') {
        let vb = value.as_bytes();
        if vb.first().is_none_or(|b| !(b'1'..=b'9').contains(b))
            || !vb.iter().all(u8::is_ascii_digit)
        {
            return None;
        }
        ("", value)
    } else {
        return None;
    };
    if (!letters.is_empty() && style != "letters") || (!dash.is_empty() && style != "dash") {
        return None;
    }
    if base.len() > 12 || dash.len() > 9 || letters.len() > 6 {
        return None;
    }
    let rank = if !letters.is_empty() {
        letters_rank(letters)
    } else if !dash.is_empty() {
        dash.parse().ok()?
    } else {
        0
    };
    Some((base.parse().ok()?, rank))
}

/// Collision identity only: preserve original evidence text, but do not
/// allocate another number that differs only in ASCII case or outer spaces.
pub fn number_collision_key(number: &str) -> String {
    number.trim_matches(' ').to_ascii_uppercase()
}

const MAX_BASE: i64 = 999_999_999_999;

pub fn format_number(base: i64, rank: i64, prefix: &str, style: &str) -> String {
    let suffix = if rank == 0 {
        String::new()
    } else if style == "letters" {
        rank_letters(rank)
    } else {
        format!("-{rank}")
    };
    format!("{prefix}{base}{suffix}")
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SuggestOptions<'a> {
    pub prefix: &'a str,
    pub distance: f64,
    pub existing_id: Option<&'a str>,
    pub batch_before_first: bool,
}

/// Port of `suggest_number`: propose a route-side number without modifying inputs.
pub fn suggest_number(
    addresses: &[Address],
    measure: f64,
    side: &str,
    rule: &Rule,
    options: SuggestOptions<'_>,
) -> Result<Suggestion, DomainError> {
    let parity = rule.parity(side)?;
    finite(measure, "measure")?;
    finite(options.distance, "distance")?;
    if measure < 0.0 || options.distance < 0.0 {
        return Err(DomainError::new("measure and distance must be nonnegative"));
    }
    let prefix = route_prefix(options.prefix)?;
    let ids: HashSet<&str> = addresses.iter().map(|row| row.id.as_str()).collect();
    if ids.len() != addresses.len() {
        return Err(DomainError::new("duplicate address ID"));
    }
    let result = |number: Option<String>, reason: &'static str| Suggestion {
        number,
        reason,
        rule_version: rule.version.clone(),
    };
    if let Some(existing_id) = options.existing_id {
        let existing = addresses
            .iter()
            .find(|row| row.id == existing_id)
            .ok_or_else(|| DomainError::new("unknown existing address ID"))?;
        return Ok(result(Some(existing.number.clone()), "existing_number"));
    }
    if options.distance > rule.max_distance {
        return Ok(result(None, "outside_route_distance"));
    }
    let mut anchors: Vec<&Address> = addresses
        .iter()
        .filter(|row| row.side == side && row.state != "reserved")
        .collect();
    anchors.sort_by(|a, b| {
        a.measure
            .partial_cmp(&b.measure)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.id.cmp(&b.id))
    });
    let mut parsed: Vec<(&Address, (i64, i64))> = Vec::new();
    for row in anchors {
        match parse_number(&row.number, &prefix, &rule.suffix_style) {
            Some(value) if value.0 % 2 == parity => parsed.push((row, value)),
            _ => return Ok(result(None, "ambiguous_or_inconsistent_anchor")),
        }
    }
    for pair in parsed.windows(2) {
        let ((a, av), (b, bv)) = (&pair[0], &pair[1]);
        if a.measure >= b.measure || av >= bv {
            return Ok(result(None, "nonmonotonic_or_duplicate_anchor"));
        }
    }
    if parsed.iter().any(|(row, _)| row.measure == measure) {
        return Ok(result(None, "ambiguous_location"));
    }
    let previous = parsed
        .iter()
        .rev()
        .find(|(row, _)| row.measure < measure)
        .map(|(_, value)| *value);
    let following = parsed
        .iter()
        .find(|(row, _)| row.measure > measure)
        .map(|(_, value)| *value);
    let occupied: BTreeSet<String> = addresses.iter().map(|row| number_collision_key(&row.number)).collect();
    let style = rule.suffix_style.as_str();
    let free_base = |start: i64, stop: Option<i64>| -> Option<String> {
        let mut candidate = start;
        for _ in 0..=occupied.len() {
            if candidate <= 0 || candidate > MAX_BASE || stop.is_some_and(|stop| candidate >= stop) {
                return None;
            }
            let number = format_number(candidate, 0, &prefix, style);
            if !occupied.contains(number.as_str()) {
                return Some(number);
            }
            candidate += 2;
        }
        None
    };
    let start = if parity == 1 { 1 } else { 2 };
    match (previous, following) {
        (None, None) => Ok(result(free_base(start, None), "empty_route_side")),
        (None, Some(following)) => {
            if options.batch_before_first {
                let number = free_base(start, Some(following.0));
                let reason = if number.is_some() {
                    "before_first"
                } else {
                    "no_number_before_first"
                };
                return Ok(result(number, reason));
            }
            let mut candidate = following.0 - 2;
            for _ in 0..=occupied.len() {
                if candidate <= 0 { break; }
                let number = format_number(candidate, 0, &prefix, style);
                if !occupied.contains(number.as_str()) {
                    return Ok(result(Some(number), "before_first"));
                }
                candidate -= 2;
            }
            Ok(result(None, "no_number_before_first"))
        }
        (Some(previous), None) => {
            let number=free_base(previous.0 + 2, None);
            let reason=if number.is_some(){"after_last"}else{"numbering_range_exhausted"};
            Ok(result(number,reason))
        },
        (Some(previous), Some(following)) => {
            if let Some(gap) = free_base(previous.0 + 2, Some(following.0)) {
                return Ok(result(Some(gap), "unused_gap"));
            }
            let mut rank = previous.1 + 1;
            for _ in 0..=occupied.len() {
                if (previous.0, rank) >= following {
                    return Ok(result(None, "no_slot_between_anchors"));
                }
                let number = format_number(previous.0, rank, &prefix, style);
                if parse_number(&number, &prefix, style).is_none() {
                    return Ok(result(None, "numbering_range_exhausted"));
                }
                if !occupied.contains(number.as_str()) {
                    return Ok(result(Some(number), "subnumber"));
                }
                rank += 1;
            }
            Ok(result(None, "no_free_slot"))
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RouteLocation {
    pub measure: f64,
    pub distance: f64,
    pub side: Option<&'static str>,
    pub crs: String,
}

/// Port of `locate_on_route` in a caller-verified projected metre CRS.
pub fn locate_on_route(
    point: (f64, f64),
    vertices: &[(f64, f64)],
    crs: &str,
) -> Result<RouteLocation, DomainError> {
    if crs.trim().is_empty() {
        return Err(DomainError::new("verified projected CRS required"));
    }
    if vertices.len() < 2 {
        return Err(DomainError::new("route needs at least two vertices"));
    }
    for (x, y) in std::iter::once(&point).chain(vertices.iter()) {
        finite(*x, "x")?;
        finite(*y, "y")?;
    }
    let (px, py) = point;
    let mut best: Option<RouteLocation> = None;
    let mut cumulative = 0.0;
    for pair in vertices.windows(2) {
        let ((ax, ay), (bx, by)) = (pair[0], pair[1]);
        let (dx, dy) = (bx - ax, by - ay);
        let length = dx.hypot(dy);
        if length <= 0.0 || !length.is_finite() {
            return Err(DomainError::new(
                "route contains invalid or repeated vertices",
            ));
        }
        let (unit_x, unit_y) = (dx / length, dy / length);
        let along = 0.0_f64.max(length.min((px - ax) * unit_x + (py - ay) * unit_y));
        let (qx, qy) = (ax + along * unit_x, ay + along * unit_y);
        let distance = (px - qx).hypot(py - qy);
        let cross = unit_x * (py - ay) - unit_y * (px - ax);
        let side = if cross.abs() <= 1e-8 {
            None
        } else if cross > 0.0 {
            Some("left")
        } else {
            Some("right")
        };
        let candidate = RouteLocation {
            measure: cumulative + along,
            distance,
            side,
            crs: crs.to_owned(),
        };
        best = match best {
            None => Some(candidate),
            Some(current) if distance < current.distance - 1e-8 => Some(candidate),
            Some(current)
                if (distance - current.distance).abs() <= 1e-8
                    && ((candidate.measure - current.measure).abs() > 1e-8
                        || candidate.side != current.side) =>
            {
                Some(RouteLocation {
                    side: None,
                    ..current
                })
            }
            keep => keep,
        };
        cumulative += length;
    }
    best.ok_or_else(|| DomainError::new("no usable route"))
}

/// Port of `classify_reconciliation`.
pub fn classify_reconciliation(
    certificate: Option<&str>,
    cad: Option<&str>,
    survey: Option<&str>,
    surveyed: bool,
    exists: bool,
) -> Result<&'static str, DomainError> {
    let certificate = certificate.filter(|value| !value.is_empty());
    let cad_value = cad.filter(|value| !value.is_empty());
    if !surveyed && (survey.is_some() || !exists) {
        return Err(DomainError::new("survey result requires a survey"));
    }
    if surveyed && !exists {
        return Ok("no_longer_exists");
    }
    if surveyed && certificate.is_none() && cad_value.is_none() {
        return Ok("new_building");
    }
    if let (Some(certificate), Some(cad)) = (certificate, cad_value) {
        if certificate != cad {
            return Ok("cad_certificate_mismatch");
        }
    }
    if !surveyed {
        return Ok("not_surveyed");
    }
    // Python `certificate or cad` keeps an empty certificate falsy.
    let reference = certificate.or(cad);
    match survey {
        Some(value) if Some(value) == reference => Ok("matched"),
        _ => Ok("number_mismatch"),
    }
}

#[derive(Debug, Clone)]
pub struct EvidenceCandidate {
    pub building_id: String,
    pub route_id: String,
    pub old_number: Option<String>,
    pub new_number: Option<String>,
}

/// Port of `match_certificate`; returns `(building_id, reason)`.
pub fn match_certificate(
    candidates: &[EvidenceCandidate],
    route_id: &str,
    old_number: Option<&str>,
    new_number: Option<&str>,
) -> Result<(Option<String>, &'static str), DomainError> {
    if route_id.is_empty() {
        return Err(DomainError::new("route required"));
    }
    let rows: Vec<&EvidenceCandidate> = candidates
        .iter()
        .filter(|row| row.route_id == route_id)
        .collect();
    let matches = |old: bool, number: Option<&str>| -> BTreeSet<&str> {
        let Some(number)=number.filter(|v|!v.trim().is_empty()) else{return BTreeSet::new();};
        let key=number_collision_key(number);
        rows.iter().filter(|row|{
            let value=if old{&row.old_number}else{&row.new_number};
            value.as_ref().is_some_and(|v|number_collision_key(v)==key)
        }).map(|row|row.building_id.as_str()).collect()
    };
    let old=matches(true,old_number);
    let new=matches(false,new_number);
    if old.len()>1{return Ok((None,"ambiguous_old_number"));}
    if new.len()>1{return Ok((None,"ambiguous_new_number"));}
    if !old.is_empty()&&!new.is_empty()&&old!=new{return Ok((None,"conflicting_old_new_number"));}
    if let Some(id)=old.iter().next(){return Ok((Some((*id).to_owned()),"unique_old_number"));}
    if let Some(id)=new.iter().next(){return Ok((Some((*id).to_owned()),"unique_new_number"));}
    Ok((None, "no_match"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule() -> Rule {
        Rule::new("v1", true, "letters", 50.0, Some("QD")).unwrap()
    }

    #[test]
    fn parses_and_formats_like_python() {
        assert_eq!(parse_number("12/3A/5B", "12/3A/", "letters"), Some((5, 2)));
        assert_eq!(parse_number("7-2", "", "dash"), Some((7, 2)));
        assert_eq!(parse_number("7-2", "", "letters"), None);
        assert_eq!(parse_number("07", "", "letters"), None);
        assert_eq!(format_number(9, 27, "", "letters"), "9AA");
        assert_eq!(letters_rank("AA"), 27);
    }

    #[test]
    fn suggests_empty_side_gap_and_subnumber() {
        let r = rule();
        let empty = suggest_number(&[], 10.0, "right", &r, SuggestOptions::default()).unwrap();
        assert_eq!(
            (empty.number.as_deref(), empty.reason),
            (Some("2"), "empty_route_side")
        );
        let rows = vec![
            Address::new("a", "1", 5.0, "left", "approved").unwrap(),
            Address::new("b", "3", 15.0, "left", "approved").unwrap(),
        ];
        let sub = suggest_number(&rows, 10.0, "left", &r, SuggestOptions::default()).unwrap();
        assert_eq!(
            (sub.number.as_deref(), sub.reason),
            (Some("1A"), "subnumber")
        );
    }

    #[test]
    fn fills_numeric_gaps_then_suffixes_without_using_reserved_slots() {
        let r=rule();
        let mut rows=vec![Address::new("a","100",0.0,"right","certificate").unwrap(),
                          Address::new("b","106",100.0,"right","approved").unwrap()];
        let next=|rows:&[Address]|suggest_number(rows,50.0,"right",&r,SuggestOptions::default()).unwrap().number;
        assert_eq!(next(&rows).as_deref(),Some("102"));
        rows.push(Address::new("c","102",0.0,"right","reserved").unwrap());
        assert_eq!(next(&rows).as_deref(),Some("104"));
        rows.push(Address::new("d","104",0.0,"right","reserved").unwrap());
        assert_eq!(next(&rows).as_deref(),Some("100A"));
        rows.push(Address::new("e"," 100a ",0.0,"left","reserved").unwrap());
        assert_eq!(next(&rows).as_deref(),Some("100B"));
    }

    #[test]
    fn nested_alley_dash_and_reverse_parity() {
        let r=Rule::new("v2",false,"dash",35.0,None).unwrap();
        let rows=vec![Address::new("a","12/3A/100",1.0,"left","approved").unwrap(),
                      Address::new("b","12/3A/102",20.0,"left","certificate").unwrap(),
                      Address::new("c","12/3a/100-1",0.0,"right","reserved").unwrap()];
        let s=suggest_number(&rows,10.0,"left",&r,SuggestOptions{prefix:"12/3a",..Default::default()}).unwrap();
        assert_eq!(s.number.as_deref(),Some("12/3A/100-2"));
        let rev=locate_on_route((20.0,10.0),&[(100.0,0.0),(0.0,0.0)],"EPSG:3857").unwrap();
        assert_eq!(rev.side,Some("right"));assert_eq!(rev.measure,80.0);
    }

    #[test]
    fn rejects_ambiguous_anchors_and_exhausted_range() {
        let r=rule();
        let rows=vec![Address::new("a","106",10.0,"right","approved").unwrap(),
                      Address::new("b","100",20.0,"right","certificate").unwrap()];
        assert_eq!(suggest_number(&rows,15.0,"right",&r,SuggestOptions::default()).unwrap().reason,"nonmonotonic_or_duplicate_anchor");
        let rows=vec![Address::new("a","999999999999",1.0,"left","approved").unwrap()];
        let s=suggest_number(&rows,2.0,"left",&r,SuggestOptions::default()).unwrap();
        assert_eq!(s.number,None);assert_eq!(s.reason,"numbering_range_exhausted");
        let rows=vec![Address::new("a","1ZZZZZZ",1.0,"left","approved").unwrap(),Address::new("b","3",3.0,"left","approved").unwrap()];
        assert_eq!(suggest_number(&rows,2.0,"left",&r,SuggestOptions::default()).unwrap().number,None);
    }

    #[test]
    fn certificate_match_checks_every_source_and_refuses_conflicting_identities(){
        let candidate=|id:&str,old:Option<&str>,new:Option<&str>|EvidenceCandidate{building_id:id.into(),route_id:"r".into(),old_number:old.map(str::to_owned),new_number:new.map(str::to_owned)};
        let mut rows=vec![candidate("a",Some("98"),None),candidate("a",Some(" 100a "),None),candidate("a",None,Some("100A"))];
        assert_eq!(match_certificate(&rows,"r",Some("100A"),Some("100A")).unwrap(),(Some("a".into()),"unique_old_number"));
        rows.push(candidate("b",None,Some("100A")));
        assert_eq!(match_certificate(&rows,"r",Some("100A"),Some("100A")).unwrap(),(None,"ambiguous_new_number"));
        rows.remove(2);
        assert_eq!(match_certificate(&rows,"r",Some("100A"),Some("100A")).unwrap(),(None,"conflicting_old_new_number"));
        rows.push(candidate("b",Some("100A"),None));
        assert_eq!(match_certificate(&rows,"r",Some("100A"),None).unwrap(),(None,"ambiguous_old_number"));
    }

    #[test]
    fn locates_side_and_measure() {
        let loc = locate_on_route((20.0, -10.0), &[(0.0, 0.0), (100.0, 0.0)], "EPSG:3857").unwrap();
        assert_eq!(loc.side, Some("right"));
        assert!((loc.measure - 20.0).abs() < 1e-9 && (loc.distance - 10.0).abs() < 1e-9);
    }

    #[test]
    fn classifies_reconciliation() {
        assert_eq!(
            classify_reconciliation(Some("2"), None, Some("2"), true, true).unwrap(),
            "matched"
        );
        assert_eq!(
            classify_reconciliation(None, None, None, false, true).unwrap(),
            "not_surveyed"
        );
        assert_eq!(
            classify_reconciliation(Some("1"), Some("2"), None, false, true).unwrap(),
            "cad_certificate_mismatch"
        );
    }
}
