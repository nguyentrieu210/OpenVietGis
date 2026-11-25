//! E04 exact typed, resource-bounded filter compiler (zero arbitrary SQL).
//!
//! Mirrors `apps/api/openvietgis_api/domains/queries/domain.py`. Every value,
//! field key and geometry reaches PostgreSQL as a bound parameter; the SQL text
//! only ever contains fixed operators, casts and `$n` placeholders.

use std::cmp::Ordering;
use std::collections::HashSet;
use std::fmt::Write;

use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};
use ovg_core::AppError;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub const MAX_QUERY_LIMIT: i64 = 500;
pub const MAX_QUERY_OFFSET: i64 = 50_000;
pub const MAX_QUERY_FEATURES: i64 = 50_000;
pub const MAX_QUERY_POSITIONS: i64 = 1_000_000;
pub const MAX_REFERENCE_FEATURES: i64 = 500;
pub const MAX_REFERENCE_POSITIONS: i64 = 100_000;
pub const MAX_PAGE_POSITIONS: i64 = 20_000;
pub const MAX_DRAWN_POSITIONS: usize = 2048;
pub const MAX_AST_DEPTH: usize = 6;
pub const MAX_AST_NODES: usize = 100;
pub const MAX_AST_PARAMS: usize = 1000;
pub const QUERY_STATEMENT_TIMEOUT: &str = "5s";
const MAX_POSITIONS: usize = 4096;
const MAX_TEXT_LENGTH: usize = 4096;
pub const READABLE_CRS: [i32; 4] = [4326, 3857, 4756, 5899];

pub(crate) fn invalid_filter(message: &str) -> AppError {
    AppError::validation("INVALID_FILTER", message)
}

pub(crate) fn too_complex(message: &str) -> AppError {
    AppError::validation("QUERY_TOO_COMPLEX", message)
}

pub(crate) fn invalid_spatial(message: &str) -> AppError {
    AppError::validation("INVALID_SPATIAL_FILTER", message)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldType {
    Text,
    Integer,
    Decimal,
    Boolean,
    Timestamp,
}

impl FieldType {
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "text" => Some(Self::Text),
            "integer" => Some(Self::Integer),
            "decimal" => Some(Self::Decimal),
            "boolean" => Some(Self::Boolean),
            "timestamp" => Some(Self::Timestamp),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SourceField {
    pub field_id: String,
    pub field_type: FieldType,
    pub nullable: bool,
}

/// A typed bound parameter. The SQL builder never interpolates these.
#[derive(Clone, Debug)]
pub enum Param {
    Text(String),
    TextArray(Vec<String>),
    Int(i64),
    I32(i32),
    Float(f64),
    Numeric(String),
    Bool(bool),
    Timestamp(DateTime<Utc>),
    Uuid(Uuid),
    Bytes(Vec<u8>),
}

/// Placeholder allocator for one statement.
#[derive(Default)]
pub struct Sql {
    pub params: Vec<Param>,
    values: usize,
}

impl Sql {
    pub fn bind(&mut self, param: Param) -> String {
        let numeric = matches!(param, Param::Numeric(_));
        self.params.push(param);
        let n = self.params.len();
        if numeric {
            format!("CAST(${n} AS numeric)")
        } else {
            format!("${n}")
        }
    }

    fn bind_value(&mut self, value: Typed) -> Result<String, AppError> {
        if self.values >= MAX_AST_PARAMS {
            return Err(too_complex("filter value budget exceeded"));
        }
        self.values += 1;
        Ok(self.bind(value.into_param()))
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Typed {
    Text(String),
    Int(i64),
    Decimal(String),
    Bool(bool),
    Timestamp(DateTime<Utc>),
}

impl Typed {
    fn into_param(self) -> Param {
        match self {
            Self::Text(v) => Param::Text(v),
            Self::Int(v) => Param::Int(v),
            Self::Decimal(v) => Param::Numeric(v),
            Self::Bool(v) => Param::Bool(v),
            Self::Timestamp(v) => Param::Timestamp(v),
        }
    }

    fn compare(&self, other: &Self) -> Option<Ordering> {
        match (self, other) {
            (Self::Text(a), Self::Text(b)) => Some(a.cmp(b)),
            (Self::Int(a), Self::Int(b)) => Some(a.cmp(b)),
            (Self::Decimal(a), Self::Decimal(b)) => Some(compare_decimal(a, b)),
            (Self::Timestamp(a), Self::Timestamp(b)) => Some(a.cmp(b)),
            _ => None,
        }
    }
}

/// Exact canonical value of one attribute; `Ok(None)` is an allowed JSON null.
fn canonical(field: &SourceField, value: &Value) -> Result<Option<Typed>, ()> {
    if value.is_null() {
        return if field.nullable { Ok(None) } else { Err(()) };
    }
    let typed = match field.field_type {
        FieldType::Text => {
            let text = value.as_str().ok_or(())?;
            if text.chars().count() > MAX_TEXT_LENGTH || text.contains('\0') {
                return Err(());
            }
            Typed::Text(text.to_owned())
        }
        FieldType::Integer => {
            let int = value.as_i64().ok_or(())?;
            i32::try_from(int).map_err(|_| ())?;
            Typed::Int(int)
        }
        FieldType::Boolean => Typed::Bool(value.as_bool().ok_or(())?),
        FieldType::Decimal => Typed::Decimal(canonical_decimal(value.as_str().ok_or(())?)?),
        FieldType::Timestamp => Typed::Timestamp(parse_timestamp(value.as_str().ok_or(())?)?),
    };
    Ok(Some(typed))
}

/// Python `type(value) is type(canonical) and value == canonical` for stored rows.
pub(crate) fn is_canonical_attribute(field: &SourceField, value: &Value) -> bool {
    match canonical(field, value) {
        Ok(None) => true,
        Ok(Some(Typed::Decimal(c))) => value.as_str() == Some(c.as_str()),
        Ok(Some(Typed::Timestamp(t))) => value.as_str() == Some(timestamp_string(t).as_str()),
        Ok(Some(_)) => true,
        Err(()) => false,
    }
}

fn typed_value(value: &Value, field: &SourceField) -> Result<Typed, AppError> {
    if value.is_null() {
        return Err(invalid_filter("use is_null, not a null comparison value"));
    }
    match canonical(field, value) {
        Ok(Some(typed)) => Ok(typed),
        _ => Err(invalid_filter(
            "value does not match declared field type or precision",
        )),
    }
}

fn canonical_decimal(value: &str) -> Result<String, ()> {
    if value.len() > 4096 {
        return Err(());
    }
    let (negative, body) = match value.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, value),
    };
    let (integer, fraction) = match body.split_once('.') {
        Some((i, f)) if !f.is_empty() => (i, f),
        Some(_) => return Err(()),
        None => (body, ""),
    };
    let digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    if integer.is_empty() || !digits(integer) || !digits(fraction) {
        return Err(());
    }
    let integer = integer.trim_start_matches('0');
    let fraction = fraction.trim_end_matches('0');
    if integer.len() + fraction.len() > 38 || fraction.len() > 18 {
        return Err(());
    }
    if integer.is_empty() && fraction.is_empty() {
        return Ok("0".into());
    }
    let integer = if integer.is_empty() { "0" } else { integer };
    let sign = if negative { "-" } else { "" };
    let fraction = if fraction.is_empty() {
        String::new()
    } else {
        format!(".{fraction}")
    };
    Ok(format!("{sign}{integer}{fraction}"))
}

/// Numeric order of two canonical decimals.
fn compare_decimal(a: &str, b: &str) -> Ordering {
    let split = |s: &str| {
        let (neg, body) = s.strip_prefix('-').map_or((false, s), |r| (true, r));
        let (i, f) = body.split_once('.').unwrap_or((body, ""));
        (neg, i.to_owned(), f.to_owned())
    };
    let (an, ai, af) = split(a);
    let (bn, bi, bf) = split(b);
    if an != bn {
        return if an {
            Ordering::Less
        } else {
            Ordering::Greater
        };
    }
    let width = af.len().max(bf.len());
    let magnitude = ai
        .len()
        .cmp(&bi.len())
        .then_with(|| ai.cmp(&bi))
        .then_with(|| format!("{af:0<width$}").cmp(&format!("{bf:0<width$}")));
    if an {
        magnitude.reverse()
    } else {
        magnitude
    }
}

/// RFC 3339 subset accepted by the reference `_canonical_timestamp`.
fn parse_timestamp(value: &str) -> Result<DateTime<Utc>, ()> {
    let b = value.as_bytes();
    let digits = |range: std::ops::Range<usize>| -> Result<u32, ()> {
        let part = value.get(range).ok_or(())?;
        if part.is_empty() || !part.bytes().all(|c| c.is_ascii_digit()) {
            return Err(());
        }
        part.parse().map_err(|_| ())
    };
    if b.len() < 20
        || b[4] != b'-'
        || b[7] != b'-'
        || b[10] != b'T'
        || b[13] != b':'
        || b[16] != b':'
        || value.ends_with("-00:00")
    {
        return Err(());
    }
    let year = i32::try_from(digits(0..4)?).map_err(|_| ())?;
    let (month, day) = (digits(5..7)?, digits(8..10)?);
    let (hour, minute, second) = (digits(11..13)?, digits(14..16)?, digits(17..19)?);
    let mut rest = &value[19..];
    let mut micro = 0;
    if let Some(fraction) = rest.strip_prefix('.') {
        let len = fraction.bytes().take_while(u8::is_ascii_digit).count();
        if !(1..=6).contains(&len) {
            return Err(());
        }
        micro = format!("{:0<6}", &fraction[..len])
            .parse()
            .map_err(|_| ())?;
        rest = &fraction[len..];
    }
    let offset = if rest == "Z" {
        0
    } else {
        let rb = rest.as_bytes();
        if rb.len() != 6 || !matches!(rb[0], b'+' | b'-') || rb[3] != b':' {
            return Err(());
        }
        let hh: i64 = rest[1..3].parse().map_err(|_| ())?;
        let mm: i64 = rest[4..6].parse().map_err(|_| ())?;
        if !rest[1..3].bytes().all(|c| c.is_ascii_digit())
            || !rest[4..6].bytes().all(|c| c.is_ascii_digit())
            || hh > 23
            || mm > 59
        {
            return Err(());
        }
        let seconds = hh * 3600 + mm * 60;
        if rb[0] == b'-' {
            -seconds
        } else {
            seconds
        }
    };
    if year < 1 {
        return Err(());
    }
    let local = NaiveDate::from_ymd_opt(year, month, day)
        .and_then(|d| d.and_hms_micro_opt(hour, minute, second, micro))
        .ok_or(())?;
    let utc = local
        .checked_sub_signed(Duration::seconds(offset))
        .ok_or(())?;
    if !(1..=9999).contains(&utc.year()) {
        return Err(());
    }
    Ok(utc.and_utc())
}

/// Python `datetime.isoformat()` in UTC with a `Z` suffix.
fn timestamp_string(value: DateTime<Utc>) -> String {
    if value.timestamp_subsec_micros() == 0 {
        value.format("%Y-%m-%dT%H:%M:%SZ").to_string()
    } else {
        value.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string()
    }
}

pub(crate) fn typed_column(column: &str, field_type: FieldType) -> String {
    match field_type {
        FieldType::Integer => format!("CAST({column} AS bigint)"),
        FieldType::Decimal => format!("CAST({column} AS numeric)"),
        FieldType::Boolean => format!("CAST({column} AS boolean)"),
        FieldType::Timestamp => format!("CAST({column} AS timestamptz)"),
        FieldType::Text => format!("({column} COLLATE \"C\")"),
    }
}

/// Compile a visual filter AST with SQL three-valued Boolean semantics.
pub fn compile_filter(
    node: Option<&Value>,
    fields: &[SourceField],
    sql: &mut Sql,
) -> Result<String, AppError> {
    match node {
        None | Some(Value::Null) => Ok("TRUE".into()),
        Some(node) => compile_node(node, 0, fields, sql, &mut 0),
    }
}

fn compile_node(
    expr: &Value,
    depth: usize,
    fields: &[SourceField],
    sql: &mut Sql,
    nodes: &mut usize,
) -> Result<String, AppError> {
    *nodes += 1;
    if depth > MAX_AST_DEPTH || *nodes > MAX_AST_NODES {
        return Err(too_complex("filter complexity exceeded"));
    }
    let map = expr
        .as_object()
        .ok_or_else(|| invalid_filter("invalid typed filter"))?;
    let op = map
        .get("op")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid_filter("invalid typed filter"))?;
    let keys: HashSet<&str> = map.keys().map(String::as_str).collect();
    let exactly = |want: &[&str]| keys.len() == want.len() && want.iter().all(|k| keys.contains(k));
    match op {
        "and" | "or" => {
            if !exactly(&["op", "conditions"]) {
                return Err(invalid_filter("invalid typed filter"));
            }
            let children = map["conditions"]
                .as_array()
                .filter(|c| !c.is_empty() && c.len() <= MAX_AST_NODES)
                .ok_or_else(|| {
                    invalid_filter("logical group requires bounded non-empty conditions")
                })?;
            let mut parts = Vec::with_capacity(children.len());
            for child in children {
                parts.push(compile_node(child, depth + 1, fields, sql, nodes)?);
            }
            let joiner = if op == "and" { " AND " } else { " OR " };
            return Ok(format!("({})", parts.join(joiner)));
        }
        "not" => {
            if !exactly(&["op", "condition"]) {
                return Err(invalid_filter("invalid typed filter"));
            }
            let inner = compile_node(&map["condition"], depth + 1, fields, sql, nodes)?;
            return Ok(format!("(NOT ({inner}))"));
        }
        _ => {}
    }
    let shape_ok = if op == "is_null" {
        keys.iter().all(|k| matches!(*k, "op" | "field" | "value"))
    } else {
        exactly(&["op", "field", "value"])
    };
    if !shape_ok {
        return Err(invalid_filter("invalid typed filter"));
    }
    let field = map
        .get("field")
        .and_then(Value::as_str)
        .and_then(|id| fields.iter().find(|f| f.field_id == id))
        .ok_or_else(|| AppError::validation("UNKNOWN_QUERY_FIELD", "unknown query field"))?;
    let key = sql.bind(Param::Text(field.field_id.clone()));
    let column = format!("(rf.attributes ->> {key})");
    if op == "is_null" {
        let is_null = match map.get("value") {
            None => true,
            Some(Value::Bool(value)) => *value,
            Some(_) => return Err(invalid_filter("is_null requires a Boolean value")),
        };
        let test = if is_null { "NULL" } else { "NOT NULL" };
        return Ok(format!("({column} IS {test})"));
    }
    let typed = typed_column(&column, field.field_type);
    let value = &map["value"];
    match op {
        "contains" | "prefix" => {
            if field.field_type != FieldType::Text {
                return Err(AppError::validation(
                    "INVALID_OPERATOR",
                    "literal text operator requires text",
                ));
            }
            // No LIKE wildcards, escape syntax, accent folding or case folding.
            let p = sql.bind_value(typed_value(value, field)?)?;
            Ok(if op == "contains" {
                format!("(strpos({typed}, {p}) > 0)")
            } else {
                format!("(starts_with({typed}, {p}))")
            })
        }
        "in" => {
            let items = value
                .as_array()
                .filter(|a| (1..=100).contains(&a.len()))
                .ok_or_else(|| invalid_filter("membership requires 1..100 typed values"))?;
            let mut placeholders = Vec::with_capacity(items.len());
            for item in items {
                let typed_item = typed_value(item, field)?;
                placeholders.push(sql.bind_value(typed_item)?);
            }
            Ok(format!("({typed} IN ({}))", placeholders.join(", ")))
        }
        "between" => {
            let pair = value
                .as_array()
                .filter(|a| a.len() == 2 && field.field_type != FieldType::Boolean)
                .ok_or_else(|| invalid_filter("range requires two ordered non-Boolean values"))?;
            let start = typed_value(&pair[0], field)?;
            let end = typed_value(&pair[1], field)?;
            if start.compare(&end) == Some(Ordering::Greater) {
                return Err(invalid_filter("range endpoints are reversed"));
            }
            let start = sql.bind_value(start)?;
            let end = sql.bind_value(end)?;
            Ok(format!("({typed} BETWEEN {start} AND {end})"))
        }
        _ => {
            let comparator = match op {
                "eq" => "=",
                "neq" => "!=",
                "gt" => ">",
                "gte" => ">=",
                "lt" => "<",
                "lte" => "<=",
                _ => "",
            };
            if comparator.is_empty()
                || (!matches!(op, "eq" | "neq") && field.field_type == FieldType::Boolean)
            {
                return Err(AppError::validation(
                    "INVALID_OPERATOR",
                    "unsupported typed operator",
                ));
            }
            let p = sql.bind_value(typed_value(value, field)?)?;
            Ok(format!("({typed} {comparator} {p})"))
        }
    }
}

/// Order for the bounded page plus the outer re-ordering of the page rows.
pub struct CompiledSort {
    pub inner: String,
    pub key: Option<String>,
    pub outer: String,
}

pub fn compile_sort(
    fields: &[SourceField],
    sort_field: Option<&str>,
    sort_direction: &str,
    sql: &mut Sql,
) -> Result<CompiledSort, AppError> {
    let direction = match sort_direction {
        "asc" => "ASC",
        "desc" => "DESC",
        _ => {
            return Err(AppError::validation(
                "INVALID_SORT",
                "sort direction must be asc or desc",
            ))
        }
    };
    let Some(sort_field) = sort_field else {
        return Ok(CompiledSort {
            inner: format!("rf.feature_id {direction}"),
            key: None,
            outer: format!("feature_id {direction}"),
        });
    };
    let field = fields
        .iter()
        .find(|f| f.field_id == sort_field)
        .ok_or_else(|| AppError::validation("UNKNOWN_SORT_FIELD", "unknown sort field"))?;
    let key = sql.bind(Param::Text(field.field_id.clone()));
    let typed = typed_column(&format!("(rf.attributes ->> {key})"), field.field_type);
    Ok(CompiledSort {
        inner: format!("{typed} {direction} NULLS LAST, rf.feature_id ASC"),
        key: Some(typed),
        outer: format!("sort_key {direction} NULLS LAST, feature_id ASC"),
    })
}

pub(crate) fn valid_revision_token(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

pub type Position = (f64, f64);

/// Readable XY geometry families (the only ones a ready source may hold).
#[derive(Clone, Debug, PartialEq)]
pub enum Geom {
    Point(Position),
    LineString(Vec<Position>),
    Polygon(Vec<Vec<Position>>),
}

impl Geom {
    #[must_use]
    pub fn family(&self) -> &'static str {
        match self {
            Self::Point(_) => "Point",
            Self::LineString(_) => "LineString",
            Self::Polygon(_) => "Polygon",
        }
    }

    fn parts(&self) -> Vec<&[Position]> {
        match self {
            Self::Point(p) => vec![std::slice::from_ref(p)],
            Self::LineString(line) => vec![line.as_slice()],
            Self::Polygon(rings) => rings.iter().map(Vec::as_slice).collect(),
        }
    }

    /// Little-endian ISO WKB (2D).
    #[must_use]
    pub fn to_wkb(&self) -> Vec<u8> {
        fn positions(out: &mut Vec<u8>, line: &[Position]) {
            out.extend_from_slice(&(line.len() as u32).to_le_bytes());
            for (x, y) in line {
                out.extend_from_slice(&x.to_le_bytes());
                out.extend_from_slice(&y.to_le_bytes());
            }
        }
        let mut out = vec![1];
        match self {
            Self::Point((x, y)) => {
                out.extend_from_slice(&1u32.to_le_bytes());
                out.extend_from_slice(&x.to_le_bytes());
                out.extend_from_slice(&y.to_le_bytes());
            }
            Self::LineString(line) => {
                out.extend_from_slice(&2u32.to_le_bytes());
                positions(&mut out, line);
            }
            Self::Polygon(rings) => {
                out.extend_from_slice(&3u32.to_le_bytes());
                out.extend_from_slice(&(rings.len() as u32).to_le_bytes());
                for ring in rings {
                    positions(&mut out, ring);
                }
            }
        }
        out
    }

    /// Bounded little-endian 2D OGC WKB without decimal rounding.
    #[must_use]
    pub fn from_wkb(bytes: &[u8]) -> Option<Self> {
        struct Reader<'b> {
            bytes: &'b [u8],
            offset: usize,
            remaining: usize,
        }
        impl Reader<'_> {
            fn take<const N: usize>(&mut self) -> Option<[u8; N]> {
                let slice = self.bytes.get(self.offset..self.offset + N)?;
                self.offset += N;
                slice.try_into().ok()
            }
            fn count(&mut self) -> Option<usize> {
                usize::try_from(u32::from_le_bytes(self.take()?)).ok()
            }
            fn position(&mut self) -> Option<Position> {
                let x = f64::from_le_bytes(self.take()?);
                let y = f64::from_le_bytes(self.take()?);
                // Normalize negative zero only; never round coordinates.
                (x.is_finite() && y.is_finite()).then_some((x + 0.0, y + 0.0))
            }
            fn positions(&mut self, n: usize) -> Option<Vec<Position>> {
                if n == 0 || n > self.remaining {
                    return None;
                }
                self.remaining -= n;
                (0..n).map(|_| self.position()).collect()
            }
        }
        if bytes.len() < 5 || bytes.len() > 5 + 4 + MAX_POSITIONS * 20 || bytes[0] != 1 {
            return None;
        }
        let mut reader = Reader {
            bytes,
            offset: 1,
            remaining: MAX_POSITIONS,
        };
        let geometry = match u32::from_le_bytes(reader.take()?) {
            1 => Self::Point(reader.positions(1)?[0]),
            2 => {
                let n = reader.count()?;
                Self::LineString(reader.positions(n)?)
            }
            3 => {
                let rings = reader.count()?;
                if rings == 0 || rings > MAX_POSITIONS {
                    return None;
                }
                let mut parts = Vec::with_capacity(rings);
                for _ in 0..rings {
                    let n = reader.count()?;
                    parts.push(reader.positions(n)?);
                }
                Self::Polygon(parts)
            }
            _ => return None,
        };
        (reader.offset == bytes.len()).then_some(geometry)
    }

    #[must_use]
    pub fn coordinates(&self) -> Value {
        let line = |l: &[Position]| Value::Array(l.iter().map(|(x, y)| json!([x, y])).collect());
        match self {
            Self::Point((x, y)) => json!([x, y]),
            Self::LineString(l) => line(l),
            Self::Polygon(rings) => Value::Array(rings.iter().map(|r| line(r)).collect()),
        }
    }
}

/// Reference `validate_ready_geometry`: shape, XY domain, bounds and wrapping.
pub(crate) fn validate_ready(
    geometry: &Geom,
    family: Option<&str>,
    srid: i32,
    output_roundoff: bool,
) -> Result<Vec<Position>, ()> {
    if !READABLE_CRS.contains(&srid) || family.is_some_and(|f| f != geometry.family()) {
        return Err(());
    }
    let parts = geometry.parts();
    let distinct = |p: &[Position]| {
        p.iter()
            .map(|(x, y)| (x.to_bits(), y.to_bits()))
            .collect::<HashSet<_>>()
            .len()
    };
    for part in parts.iter().copied() {
        match geometry {
            Geom::Point(_) => {}
            Geom::LineString(_) => {
                if part.len() < 2 || distinct(part) < 2 {
                    return Err(());
                }
            }
            Geom::Polygon(_) => {
                if part.len() < 4
                    || part[0] != part[part.len() - 1]
                    || distinct(&part[..part.len() - 1]) < 3
                {
                    return Err(());
                }
                let twice_area: f64 = part
                    .windows(2)
                    .map(|w| w[0].0 * w[1].1 - w[1].0 * w[0].1)
                    .sum();
                if !twice_area.is_finite() || twice_area == 0.0 {
                    return Err(());
                }
            }
        }
    }
    if parts.iter().map(|p| p.len()).sum::<usize>() > MAX_POSITIONS {
        return Err(());
    }
    let mercator =
        ovg_core::crs::WEB_MERCATOR_MAX_COORDINATE_METRE + if output_roundoff { 1e-6 } else { 0.0 };
    let geographic = matches!(srid, 4326 | 4756);
    for part in parts.iter().copied() {
        for &(x, y) in part {
            if (geographic && !((-180.0..=180.0).contains(&x) && (-90.0..=90.0).contains(&y)))
                || (srid == 3857 && (x.abs() > mercator || y.abs() > mercator))
            {
                return Err(());
            }
        }
        if geographic && part.windows(2).any(|w| (w[0].0 - w[1].0).abs() > 180.0) {
            return Err(());
        }
    }
    Ok(parts.into_iter().flatten().copied().collect())
}

/// Reference `parse_drawn_area`: a bounded native XY Polygon with explicit CRS.
pub fn parse_drawn_area(value: &Value, crs_code: &Value) -> Result<(Geom, i32), AppError> {
    let srid = crs_code
        .as_i64()
        .and_then(|c| i32::try_from(c).ok())
        .filter(|c| READABLE_CRS.contains(c))
        .ok_or_else(|| {
            AppError::validation(
                "UNSUPPORTED_CRS",
                "drawn area requires explicit supported CRS",
            )
        })?;
    let map = value
        .as_object()
        .filter(|m| {
            m.len() == 2
                && m.contains_key("coordinates")
                && m.get("type").and_then(Value::as_str) == Some("Polygon")
        })
        .ok_or_else(|| invalid_spatial("drawn geometry must be a Polygon"))?;
    let rings = map["coordinates"]
        .as_array()
        .filter(|r| (1..=128).contains(&r.len()))
        .ok_or_else(|| invalid_spatial("invalid drawn polygon rings"))?;
    let mut total = 0;
    for ring in rings {
        let ring = ring
            .as_array()
            .filter(|r| r.len() >= 4)
            .ok_or_else(|| invalid_spatial("invalid drawn polygon ring"))?;
        total += ring.len();
        if total > MAX_DRAWN_POSITIONS {
            return Err(too_complex("drawn polygon exceeds position limit"));
        }
        if ring
            .iter()
            .any(|p| p.as_array().is_none_or(|p| p.len() != 2))
        {
            return Err(invalid_spatial("drawn geometry requires XY coordinates"));
        }
    }
    let domain = || invalid_spatial("invalid drawn polygon or CRS domain");
    let mut parsed = Vec::with_capacity(rings.len());
    for ring in rings.iter().filter_map(Value::as_array) {
        let mut positions = Vec::with_capacity(ring.len());
        for position in ring.iter().filter_map(Value::as_array) {
            let coord = |v: &Value| v.as_f64().filter(|c| c.is_finite()).map(|c| c + 0.0);
            match (coord(&position[0]), coord(&position[1])) {
                (Some(x), Some(y)) => positions.push((x, y)),
                _ => return Err(domain()),
            }
        }
        parsed.push(positions);
    }
    let geometry = Geom::Polygon(parsed);
    validate_ready(&geometry, Some("Polygon"), srid, false).map_err(|()| domain())?;
    Ok((geometry, srid))
}

/// SHA-256 of Python `json.dumps(sort_keys=True, ensure_ascii=True, separators=(",", ":"))`.
#[must_use]
pub fn canonical_manifest_digest(manifest: &Value) -> String {
    let mut out = String::new();
    write_canonical(manifest, &mut out);
    format!("{:x}", Sha256::digest(out.as_bytes()))
}

fn write_canonical(value: &Value, out: &mut String) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                let _ = write!(out, "{i}");
            } else if let Some(u) = n.as_u64() {
                let _ = write!(out, "{u}");
            } else {
                out.push_str(&python_float_repr(n.as_f64().unwrap_or(0.0)));
            }
        }
        Value::String(s) => write_python_string(s, out),
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_canonical(item, out);
            }
            out.push(']');
        }
        Value::Object(map) => {
            let mut entries: Vec<_> = map.iter().collect();
            entries.sort_by(|a, b| a.0.cmp(b.0));
            out.push('{');
            for (i, (key, item)) in entries.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_python_string(key, out);
                out.push(':');
                write_canonical(item, out);
            }
            out.push('}');
        }
    }
}

fn write_python_string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            ' '..='~' => out.push(c),
            _ => {
                for unit in c.encode_utf16(&mut [0; 2]) {
                    let _ = write!(out, "\\u{unit:04x}");
                }
            }
        }
    }
    out.push('"');
}

/// Python `float.__repr__` (shortest round-trip digits, `e±NN` outside [1e-4, 1e16)).
fn python_float_repr(x: f64) -> String {
    if x == 0.0 {
        return if x.is_sign_negative() { "-0.0" } else { "0.0" }.into();
    }
    let scientific = format!("{x:e}");
    let (mantissa, exponent) = scientific.split_once('e').unwrap_or((&scientific, "0"));
    let exponent: i32 = exponent.parse().unwrap_or(0);
    let (sign, mantissa) = mantissa
        .strip_prefix('-')
        .map_or(("", mantissa), |m| ("-", m));
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let point = exponent + 1;
    if !(-3..=16).contains(&point) {
        let (first, rest) = digits.split_at(1);
        let rest = if rest.is_empty() {
            String::new()
        } else {
            format!(".{rest}")
        };
        let exp_sign = if exponent < 0 { '-' } else { '+' };
        return format!("{sign}{first}{rest}e{exp_sign}{:02}", exponent.abs());
    }
    if point <= 0 {
        let zeros = "0".repeat(point.unsigned_abs() as usize);
        return format!("{sign}0.{zeros}{digits}");
    }
    let point = point as usize;
    if point >= digits.len() {
        format!("{sign}{digits}{}.0", "0".repeat(point - digits.len()))
    } else {
        format!("{sign}{}.{}", &digits[..point], &digits[point..])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields() -> Vec<SourceField> {
        [
            ("name", FieldType::Text, true),
            ("rank", FieldType::Integer, false),
            ("measure", FieldType::Decimal, true),
            ("active", FieldType::Boolean, false),
            ("observed", FieldType::Timestamp, true),
        ]
        .into_iter()
        .map(|(id, field_type, nullable)| SourceField {
            field_id: id.into(),
            field_type,
            nullable,
        })
        .collect()
    }

    fn compile(node: Value) -> Result<(String, Vec<Param>), AppError> {
        let mut sql = Sql::default();
        let text = compile_filter(Some(&node), &fields(), &mut sql)?;
        Ok((text, sql.params))
    }

    fn code(node: Value) -> &'static str {
        compile(node).unwrap_err().code()
    }

    #[test]
    fn compiles_values_and_keys_to_bound_parameters_only() {
        let (text, params) = compile(json!({"op": "and", "conditions": [
            {"field": "name", "op": "contains", "value": "%_'; DROP TABLE x; --"},
            {"op": "not", "condition": {"field": "rank", "op": "in", "value": [1, 2]}},
            {"field": "measure", "op": "between", "value": ["-1.50", "2"]},
            {"field": "observed", "op": "is_null", "value": false}
        ]}))
        .unwrap();
        assert!(!text.contains("DROP") && !text.contains('\'') && !text.contains("name"));
        assert_eq!(
            text,
            "((strpos(((rf.attributes ->> $1) COLLATE \"C\"), $2) > 0) AND \
             (NOT ((CAST((rf.attributes ->> $3) AS bigint) IN ($4, $5)))) AND \
             (CAST((rf.attributes ->> $6) AS numeric) BETWEEN CAST($7 AS numeric) \
             AND CAST($8 AS numeric)) AND ((rf.attributes ->> $9) IS NOT NULL))"
        );
        assert!(matches!(&params[6], Param::Numeric(v) if v == "-1.5"));
        assert_eq!(params.len(), 9);
    }

    #[test]
    fn rejects_invalid_typed_values_and_bad_ast() {
        assert_eq!(
            code(json!({"field": "rank", "op": "eq", "value": 1.5})),
            "INVALID_FILTER"
        );
        assert_eq!(
            code(json!({"field": "rank", "op": "eq", "value": true})),
            "INVALID_FILTER"
        );
        assert_eq!(
            code(json!({"field": "rank", "op": "eq", "value": null})),
            "INVALID_FILTER"
        );
        assert_eq!(
            code(json!({"field": "rank", "op": "eq", "value": 2_147_483_648_i64})),
            "INVALID_FILTER"
        );
        assert_eq!(
            code(json!({"field": "measure", "op": "eq", "value": 1.1})),
            "INVALID_FILTER"
        );
        assert_eq!(
            code(json!({"field": "measure", "op": "eq", "value": "1.0000000000000000001"})),
            "INVALID_FILTER"
        );
        assert_eq!(
            code(json!({"field": "observed", "op": "eq", "value": "2026-11-21T00:00:00-00:00"})),
            "INVALID_FILTER"
        );
        assert_eq!(
            code(json!({"field": "nope", "op": "eq", "value": 1})),
            "UNKNOWN_QUERY_FIELD"
        );
        assert_eq!(
            code(json!({"field": "rank", "op": "contains", "value": "1"})),
            "INVALID_OPERATOR"
        );
        assert_eq!(
            code(json!({"field": "active", "op": "gt", "value": true})),
            "INVALID_OPERATOR"
        );
        assert_eq!(
            code(json!({"field": "rank", "op": "like", "value": 1})),
            "INVALID_OPERATOR"
        );
        assert_eq!(
            code(json!({"field": "rank", "op": "between", "value": [3, 1]})),
            "INVALID_FILTER"
        );
        assert_eq!(
            code(json!({"field": "rank", "op": "eq", "value": 1, "x": 1})),
            "INVALID_FILTER"
        );
        assert_eq!(
            code(json!({"op": "and", "conditions": []})),
            "INVALID_FILTER"
        );
        assert_eq!(
            code(json!({"field": "name", "op": "is_null", "value": "yes"})),
            "INVALID_FILTER"
        );
        let mut deep = json!({"field": "rank", "op": "eq", "value": 1});
        for _ in 0..7 {
            deep = json!({"op": "not", "condition": deep});
        }
        assert_eq!(code(deep), "QUERY_TOO_COMPLEX");
        let wide: Vec<_> = (0..101)
            .map(|_| json!({"field": "rank", "op": "eq", "value": 1}))
            .collect();
        assert_eq!(
            code(json!({"op": "or", "conditions": wide})),
            "INVALID_FILTER"
        );
        let values: Vec<_> = (0..100).collect();
        let many: Vec<_> = (0..11)
            .map(|_| json!({"field": "rank", "op": "in", "value": values}))
            .collect();
        assert_eq!(
            code(json!({"op": "or", "conditions": many})),
            "QUERY_TOO_COMPLEX"
        );
    }

    #[test]
    fn canonicalizes_decimals_and_timestamps_like_python() {
        assert_eq!(canonical_decimal("-000.500").unwrap(), "-0.5");
        assert_eq!(canonical_decimal("-0.000").unwrap(), "0");
        assert!(canonical_decimal("1.").is_err() && canonical_decimal(".5").is_err());
        assert_eq!(compare_decimal("-2", "-10"), Ordering::Greater);
        assert_eq!(
            compare_decimal("1.000000000000000002", "1.000000000000000001"),
            Ordering::Greater
        );
        let t = parse_timestamp("2026-11-21T07:00:00+07:00").unwrap();
        assert_eq!(timestamp_string(t), "2026-11-21T00:00:00Z");
        let t = parse_timestamp("2026-11-21T00:00:00.12Z").unwrap();
        assert_eq!(timestamp_string(t), "2026-11-21T00:00:00.120000Z");
        assert!(parse_timestamp("2026-11-21T00:00:60Z").is_err());
        assert!(parse_timestamp("2026-02-30T00:00:00Z").is_err());
        assert!(parse_timestamp("0001-01-01T00:00:00+01:00").is_err());
        let field = &fields()[2];
        assert!(is_canonical_attribute(field, &json!("1.5")));
        assert!(!is_canonical_attribute(field, &json!("1.50")));
    }

    #[test]
    fn sort_is_typed_and_tie_broken_by_primary_key() {
        let mut sql = Sql::default();
        let sort = compile_sort(&fields(), Some("rank"), "desc", &mut sql).unwrap();
        assert_eq!(
            sort.inner,
            "CAST((rf.attributes ->> $1) AS bigint) DESC NULLS LAST, rf.feature_id ASC"
        );
        let plain = compile_sort(&fields(), None, "asc", &mut sql).unwrap();
        assert_eq!(plain.inner, "rf.feature_id ASC");
        assert!(compile_sort(&fields(), None, "up", &mut sql).is_err());
        assert!(compile_sort(&fields(), Some("x"), "asc", &mut sql).is_err());
    }

    #[test]
    fn drawn_polygon_shape_bounds() {
        let square = json!({"type": "Polygon", "coordinates": [[[105, 21], [105.5, 21], [105.5, 21.5], [105, 21]]]});
        let (geometry, srid) = parse_drawn_area(&square, &json!(4326)).unwrap();
        assert_eq!(srid, 4326);
        assert_eq!(Geom::from_wkb(&geometry.to_wkb()), Some(geometry));
        let code = |g: Value, c: Value| parse_drawn_area(&g, &c).unwrap_err().code();
        assert_eq!(code(square.clone(), json!(4326.0)), "UNSUPPORTED_CRS");
        assert_eq!(code(square.clone(), json!(3405)), "UNSUPPORTED_CRS");
        let open = json!({"type": "Polygon", "coordinates": [[[105, 21], [106, 21], [106, 22], [105, 22.5]]]});
        assert_eq!(code(open, json!(4326)), "INVALID_SPATIAL_FILTER");
        let outside = json!({"type": "Polygon", "coordinates": [[[185, 21], [186, 21], [186, 22], [185, 21]]]});
        assert_eq!(code(outside, json!(4326)), "INVALID_SPATIAL_FILTER");
        let big: Vec<_> = (0..2049).map(|i| json!([f64::from(i) * 1e-6, 0])).collect();
        assert_eq!(
            code(
                json!({"type": "Polygon", "coordinates": [big]}),
                json!(4326)
            ),
            "QUERY_TOO_COMPLEX"
        );
        assert_eq!(
            code(json!({"type": "Point", "coordinates": [1, 2]}), json!(4326)),
            "INVALID_SPATIAL_FILTER"
        );
    }

    #[test]
    fn canonical_json_matches_python_json_dumps() {
        for (value, expected) in [
            (1e16, "1e+16"),
            (1e15, "1000000000000000.0"),
            (1.5e-5, "1.5e-05"),
            (0.0001, "0.0001"),
            (105.65, "105.65"),
            (-0.0, "-0.0"),
            (12_345_678_901_234_567.0, "1.2345678901234568e+16"),
            (100.0, "100.0"),
        ] {
            assert_eq!(python_float_repr(value), expected);
        }
        let mut out = String::new();
        write_canonical(
            &json!({"b": [1, 2.5, null], "a": "Đất \"x\"\n😀\u{7f}"}),
            &mut out,
        );
        assert_eq!(out, r#"{"a":"Đất \"x\"\n😀\u007f","b":[1,2.5,null]}"#);
    }
}
