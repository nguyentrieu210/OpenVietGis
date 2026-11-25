//! Pure R006 editing rules mirroring `domains/editing/domain.py` and the attribute
//! canonicalization of `domains/spatial/domain.py`. Python `repr`/`json.dumps`
//! formatting is reproduced because it feeds persisted revision digests.

use chrono::{Duration, NaiveDate, NaiveTime};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// Wire-exact failure of an editing request.
#[derive(Debug)]
pub enum EditError {
    /// FastAPI `HTTPException` and tenancy errors: `{"detail": ...}`.
    Detail(u16, String),
    /// `PlatformDomainError` and the OV409 guard: `{"code", "message", "details"?}`.
    Platform {
        status: u16,
        code: &'static str,
        message: String,
        details: Option<Value>,
    },
    /// Fixed spatial error table: `{"code", "detail"}`.
    Spatial(u16, &'static str, &'static str),
    /// Redacted FastAPI request validation envelope (422).
    InvalidRequest,
    /// Never exposed to clients.
    Internal(String),
}

impl EditError {
    fn platform(status: u16, code: &'static str, message: impl Into<String>) -> Self {
        Self::Platform {
            status,
            code,
            message: message.into(),
            details: None,
        }
    }

    pub fn validation(code: &'static str, message: impl Into<String>) -> Self {
        Self::platform(422, code, message)
    }

    pub fn conflict(code: &'static str, message: impl Into<String>) -> Self {
        Self::platform(409, code, message)
    }

    pub fn forbidden(code: &'static str, message: impl Into<String>) -> Self {
        Self::platform(403, code, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::platform(404, "NOT_FOUND", message)
    }

    #[must_use]
    pub fn with_details(self, details: Value) -> Self {
        match self {
            Self::Platform {
                status,
                code,
                message,
                ..
            } => Self::Platform {
                status,
                code,
                message,
                details: Some(details),
            },
            other => other,
        }
    }

    pub fn detail(status: u16, detail: impl Into<String>) -> Self {
        Self::Detail(status, detail.into())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OpType {
    Create,
    Update,
    Delete,
}

impl OpType {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Update => "update",
            Self::Delete => "delete",
        }
    }
}

/// One public edit operation (`EditOperationRequest`, extra fields forbidden).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditOperation {
    pub op_type: OpType,
    pub feature_id: String,
    #[serde(default)]
    pub geometry: Option<Map<String, Value>>,
    #[serde(default)]
    pub properties: Option<Map<String, Value>>,
}

/// Persisted field identity used for typed attribute validation.
pub struct FieldDef {
    pub field_id: String,
    pub field_type: String,
    pub nullable: bool,
    pub schema_revision: String,
}

/// Python `str.strip()` whitespace (Rust `White_Space` plus U+001C..U+001F).
#[must_use]
pub fn py_strip(value: &str) -> &str {
    value.trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
}

fn ascii_token(value: &str, max: usize, extra: &[u8]) -> bool {
    (1..=max).contains(&value.len())
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || extra.contains(&b))
}

pub fn validate_operation_id(raw: &str) -> Result<String, EditError> {
    let cleaned = py_strip(raw);
    if ascii_token(cleaned, 128, b"_.:-") {
        Ok(cleaned.to_owned())
    } else {
        Err(EditError::validation(
            "INVALID_OPERATION_ID",
            "operation_id must be 1..128 URL-safe characters",
        ))
    }
}

pub fn validate_feature_id(raw: &str) -> Result<String, EditError> {
    let cleaned = py_strip(raw);
    if ascii_token(cleaned, 128, b"_-") {
        Ok(cleaned.to_owned())
    } else {
        Err(EditError::validation(
            "INVALID_FEATURE_ID",
            format!("invalid feature_id {}", py_str_repr(raw)),
        ))
    }
}

/// `validate_geometry_payload` followed by `geometry_to_wkt`.
pub fn geometry_wkt(
    geometry: Option<&Map<String, Value>>,
    family: &str,
) -> Result<String, EditError> {
    let invalid = |message: &str| EditError::validation("INVALID_GEOMETRY", message);
    let geometry = geometry
        .ok_or_else(|| EditError::validation("MISSING_GEOMETRY", "geometry object is required"))?;
    let gtype = geometry.get("type").unwrap_or(&Value::Null);
    if gtype.as_str() != Some(family) {
        return Err(EditError::validation(
            "GEOMETRY_FAMILY_MISMATCH",
            format!(
                "geometry type {} does not match source family {}",
                py_repr(gtype),
                py_str_repr(family)
            ),
        ));
    }
    let coords = geometry.get("coordinates").unwrap_or(&Value::Null);
    match family {
        "Point" => Ok(format!("POINT({})", position(coords)?.0)),
        "LineString" => {
            let points = match coords {
                Value::Array(points) if points.len() >= 2 => points,
                _ => return Err(invalid("LineString requires at least 2 positions")),
            };
            let parts = points
                .iter()
                .map(|p| position(p).map(|(text, _)| text))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(format!("LINESTRING({})", parts.join(", ")))
        }
        "Polygon" => {
            let rings = match coords {
                Value::Array(rings) if !rings.is_empty() => rings,
                _ => return Err(invalid("Polygon requires at least 1 linear ring")),
            };
            let mut wkt_rings = Vec::with_capacity(rings.len());
            for ring in rings {
                let points = match ring {
                    Value::Array(points) if points.len() >= 4 => points,
                    _ => return Err(invalid("Polygon ring requires at least 4 positions")),
                };
                let parsed = points.iter().map(position).collect::<Result<Vec<_>, _>>()?;
                if parsed.first().map(|p| p.1) != parsed.last().map(|p| p.1) {
                    return Err(invalid("Polygon ring must be closed (no silent repair)"));
                }
                let text: Vec<_> = parsed.into_iter().map(|(text, _)| text).collect();
                wkt_rings.push(format!("({})", text.join(", ")));
            }
            Ok(format!("POLYGON({})", wkt_rings.join(", ")))
        }
        _ => Err(EditError::validation(
            "UNSUPPORTED_GEOMETRY",
            format!("unsupported geometry type {}", py_repr(gtype)),
        )),
    }
}

/// `_validate_pos`; returns the WKT pair and the numeric value for ring closure.
fn position(value: &Value) -> Result<(String, (f64, f64)), EditError> {
    let pair = match value {
        Value::Array(pair) if pair.len() == 2 => pair,
        _ => {
            return Err(EditError::validation(
                "UNSUPPORTED_DIMENSIONS",
                "only 2D XY coordinates are supported",
            ))
        }
    };
    let number = |v: &Value| {
        v.as_f64().filter(|f| f.is_finite()).ok_or_else(|| {
            EditError::validation("INVALID_GEOMETRY", "coordinates must be finite numbers")
        })
    };
    let (x, y) = (number(&pair[0])?, number(&pair[1])?);
    Ok((format!("{} {}", py_float(x), py_float(y)), (x, y)))
}

/// `validate_typed_attributes`: rejects unknown fields and canonicalizes each value.
pub fn typed_attributes(
    properties: Option<&Map<String, Value>>,
    fields: &[FieldDef],
) -> Result<Map<String, Value>, EditError> {
    let empty = Map::new();
    let raw = properties.unwrap_or(&empty);
    let mut unknown: Vec<&String> = raw
        .keys()
        .filter(|key| !fields.iter().any(|f| f.field_id == **key))
        .collect();
    if !unknown.is_empty() {
        unknown.sort();
        let names: Vec<&str> = unknown.iter().map(|k| k.as_str()).collect();
        return Err(EditError::validation(
            "SCHEMA_MISMATCH",
            format!("unknown attribute fields: {}", names.join(", ")),
        )
        .with_details(json!({ "unknown_fields": names })));
    }
    let mut out = Map::new();
    for field in fields {
        let value = raw.get(&field.field_id).unwrap_or(&Value::Null);
        let canonical = canonical_attribute(field, value).map_err(|reason| {
            EditError::validation(
                "SCHEMA_MISMATCH",
                format!(
                    "field '{}' does not match {}: {reason}",
                    field.field_id, field.field_type
                ),
            )
            .with_details(json!({ "field": field.field_id }))
        })?;
        out.insert(field.field_id.clone(), canonical);
    }
    Ok(out)
}

fn canonical_attribute(field: &FieldDef, value: &Value) -> Result<Value, &'static str> {
    if value.is_null() {
        return if field.nullable {
            Ok(Value::Null)
        } else {
            Err("nonnullable field cannot be null")
        };
    }
    match field.field_type.as_str() {
        "text" => match value {
            Value::String(s) if s.chars().count() <= 4096 && !s.contains('\0') => Ok(value.clone()),
            Value::String(_) => Err("text length or encoding outside bounds"),
            _ => Err("text value required"),
        },
        "integer" => match value.as_i64() {
            Some(i) if value.is_number() && i32::try_from(i).is_ok() => Ok(value.clone()),
            _ => Err("integer outside bounds"),
        },
        "boolean" => match value {
            Value::Bool(_) => Ok(value.clone()),
            _ => Err("boolean value required"),
        },
        "decimal" => canonical_decimal(value).map(Value::String),
        "timestamp" => canonical_timestamp(value).map(Value::String),
        _ => Err("unsupported field type"),
    }
}

const EXACT_DECIMAL: &str = "exact base-10 decimal required";
const DECIMAL_BOUNDS: &str = "decimal outside precision/scale bounds";

fn canonical_decimal(value: &Value) -> Result<String, &'static str> {
    let text = match value {
        // `Decimal(str(val))` then `format(value, "f")` for JSON numbers.
        Value::Number(n) => {
            let repr = match n.as_f64() {
                Some(f) if n.is_f64() => py_float(f),
                _ => n.to_string(),
            };
            match decimal_plain(&repr)? {
                Some(plain) => plain,
                None => return Ok("0".to_owned()),
            }
        }
        Value::String(s) => s.clone(),
        _ => return Err(EXACT_DECIMAL),
    };
    let body = text.strip_prefix('-').unwrap_or(&text);
    let (integer, fraction) = body.split_once('.').unwrap_or((body, ""));
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if text.len() > 4096 || !digits(integer) || (body.contains('.') && !digits(fraction)) {
        return Err(EXACT_DECIMAL);
    }
    let integer = match integer.trim_start_matches('0') {
        "" => "0",
        stripped => stripped,
    };
    let fraction = fraction.trim_end_matches('0');
    let precision = integer.trim_start_matches('0').len() + fraction.len();
    if precision > 38 || fraction.len() > 18 {
        return Err(DECIMAL_BOUNDS);
    }
    if integer == "0" && fraction.is_empty() {
        return Ok("0".to_owned());
    }
    let sign = if text.starts_with('-') { "-" } else { "" };
    let fraction = if fraction.is_empty() {
        String::new()
    } else {
        format!(".{fraction}")
    };
    Ok(format!("{sign}{integer}{fraction}"))
}

/// Python `format(Decimal(repr), "f")`; `None` means the decimal is zero.
fn decimal_plain(repr: &str) -> Result<Option<String>, &'static str> {
    let (negative, body) = match repr.strip_prefix('-') {
        Some(body) => (true, body),
        None => (false, repr),
    };
    let (mantissa, exponent) = match body.split_once('e') {
        Some((m, e)) => (m, e.parse::<i64>().map_err(|_| EXACT_DECIMAL)?),
        None => (body, 0),
    };
    let (int_part, frac_part) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let all_digits = format!("{int_part}{frac_part}");
    let exponent = exponent - frac_part.len() as i64;
    let digits = all_digits.trim_start_matches('0');
    if digits.is_empty() {
        return Ok(None);
    }
    let adjusted = exponent + digits.len() as i64 - 1;
    if !(-18..=37).contains(&adjusted) || digits.len() > 4096 {
        return Err(DECIMAL_BOUNDS);
    }
    let plain = if exponent >= 0 {
        format!("{digits}{}", "0".repeat(exponent as usize))
    } else {
        let scale = exponent.unsigned_abs() as usize;
        let padded = if digits.len() <= scale {
            format!("{}{digits}", "0".repeat(scale + 1 - digits.len()))
        } else {
            digits.to_owned()
        };
        let split = padded.len() - scale;
        format!("{}.{}", &padded[..split], &padded[split..])
    };
    Ok(Some(if negative { format!("-{plain}") } else { plain }))
}

fn canonical_timestamp(value: &Value) -> Result<String, &'static str> {
    const RFC: &str = "RFC3339 timestamp with known timezone required";
    let Value::String(text) = value else {
        return Err("timezone-aware timestamp required");
    };
    let b = text.as_bytes();
    let digits = |range: std::ops::Range<usize>| {
        b.get(range.clone())
            .filter(|s| s.iter().all(u8::is_ascii_digit))
            .map(|_| text[range].parse::<u32>().unwrap_or(0))
    };
    let shape = b.len() >= 20
        && b[4] == b'-'
        && b[7] == b'-'
        && b[10] == b'T'
        && b[13] == b':'
        && b[16] == b':';
    let (Some(year), Some(month), Some(day), Some(hour), Some(minute), Some(second)) = (
        digits(0..4),
        digits(5..7),
        digits(8..10),
        digits(11..13),
        digits(14..16),
        digits(17..19),
    ) else {
        return Err(RFC);
    };
    if !shape {
        return Err(RFC);
    }
    let mut rest = &text[19..];
    let mut micros = 0;
    if let Some(after) = rest.strip_prefix('.') {
        let count = after.bytes().take_while(u8::is_ascii_digit).count();
        if !(1..=6).contains(&count) {
            return Err(RFC);
        }
        micros = format!("{:0<6}", &after[..count])
            .parse::<u32>()
            .unwrap_or(0);
        rest = &after[count..];
    }
    let offset_minutes: i64 = if rest == "Z" {
        0
    } else {
        let ob = rest.as_bytes();
        let valid = ob.len() == 6
            && matches!(ob[0], b'+' | b'-')
            && ob[3] == b':'
            && [1, 2, 4, 5].iter().all(|&i| ob[i].is_ascii_digit())
            && (ob[1] < b'2' || (ob[1] == b'2' && ob[2] <= b'3'))
            && ob[4] <= b'5';
        if !valid || rest == "-00:00" {
            return Err(RFC);
        }
        let hh: i64 = rest[1..3].parse().unwrap_or(0);
        let mm: i64 = rest[4..6].parse().unwrap_or(0);
        if ob[0] == b'-' {
            -(hh * 60 + mm)
        } else {
            hh * 60 + mm
        }
    };
    let date = NaiveDate::from_ymd_opt(year as i32, month, day).filter(|_| year >= 1);
    let time = NaiveTime::from_hms_micro_opt(hour, minute, second, micros);
    let (Some(date), Some(time)) = (date, time) else {
        return Err("invalid timestamp");
    };
    let utc = date.and_time(time) - Duration::minutes(offset_minutes);
    if !(1..=9999).contains(&chrono::Datelike::year(&utc)) {
        return Err("timestamp outside bounds");
    }
    let format = if micros == 0 {
        "%Y-%m-%dT%H:%M:%SZ"
    } else {
        "%Y-%m-%dT%H:%M:%S%.6fZ"
    };
    Ok(utc.format(format).to_string())
}

/// `changeset_request_digest`; validates every feature ID in request order.
pub fn request_digest(
    base_content_revision: &str,
    base_schema_revision: Option<&str>,
    summary: &str,
    operations: &[EditOperation],
    reverted_from: Option<Uuid>,
) -> Result<String, EditError> {
    let mut ops = Vec::with_capacity(operations.len());
    for op in operations {
        ops.push(json!({
            "op_type": op.op_type.as_str(),
            "feature_id": validate_feature_id(&op.feature_id)?,
            "geometry": op.geometry,
            "properties": op.properties,
        }));
    }
    let mut payload = json!({
        "base": base_content_revision,
        "summary": summary,
        "reverted_from": reverted_from.map(|id| id.to_string()),
        "operations": ops,
    });
    if let Some(schema) = base_schema_revision {
        payload["base_schema"] = json!(schema);
    }
    Ok(sha256_py_json(&payload))
}

/// `compute_content_revision`.
#[must_use]
pub fn content_revision(source_id: Uuid, base: &str, fingerprint: Vec<Value>) -> String {
    sha256_py_json(&json!({
        "source_id": source_id.to_string(),
        "base": base,
        "ops": fingerprint,
    }))
}

fn sha256_py_json(value: &Value) -> String {
    let mut text = String::new();
    py_json(value, &mut text);
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

/// Python `json.dumps(sort_keys=True, separators=(",", ":"))` with `ensure_ascii`.
pub fn py_json(value: &Value, out: &mut String) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => match n.as_f64() {
            Some(f) if n.is_f64() => out.push_str(&py_float(f)),
            _ => out.push_str(&n.to_string()),
        },
        Value::String(s) => py_json_str(s, out),
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                py_json(item, out);
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
                py_json_str(key, out);
                out.push(':');
                py_json(item, out);
            }
            out.push('}');
        }
    }
}

fn py_json_str(s: &str, out: &mut String) {
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
                    out.push_str(&format!("\\u{unit:04x}"));
                }
            }
        }
    }
    out.push('"');
}

/// Python `float.__repr__`.
#[must_use]
pub fn py_float(x: f64) -> String {
    if x.is_nan() {
        return "nan".to_owned();
    }
    if x.is_infinite() {
        return if x > 0.0 { "inf" } else { "-inf" }.to_owned();
    }
    if x == 0.0 {
        return if x.is_sign_negative() { "-0.0" } else { "0.0" }.to_owned();
    }
    let scientific = format!("{x:e}");
    let (mantissa, exponent) = scientific.split_once('e').unwrap_or((&scientific, "0"));
    let exponent: i32 = exponent.parse().unwrap_or(0);
    if (-4..16).contains(&exponent) {
        let plain = format!("{x}");
        if plain.contains('.') {
            plain
        } else {
            format!("{plain}.0")
        }
    } else {
        let sign = if exponent < 0 { '-' } else { '+' };
        format!("{mantissa}e{sign}{:02}", exponent.unsigned_abs())
    }
}

/// Python `repr` of a JSON-decoded value (used only inside error messages).
fn py_repr(value: &Value) -> String {
    match value {
        Value::Null => "None".to_owned(),
        Value::Bool(true) => "True".to_owned(),
        Value::Bool(false) => "False".to_owned(),
        Value::Number(n) => match n.as_f64() {
            Some(f) if n.is_f64() => py_float(f),
            _ => n.to_string(),
        },
        Value::String(s) => py_str_repr(s),
        Value::Array(items) => {
            let parts: Vec<_> = items.iter().map(py_repr).collect();
            format!("[{}]", parts.join(", "))
        }
        Value::Object(map) => {
            let parts: Vec<_> = map
                .iter()
                .map(|(k, v)| format!("{}: {}", py_str_repr(k), py_repr(v)))
                .collect();
            format!("{{{}}}", parts.join(", "))
        }
    }
}

/// Python `repr(str)`.
// shortcut: escapes ASCII controls and common non-printables only, not every
// non-printable Unicode category; upgrade if feature IDs with exotic code points matter.
fn py_str_repr(s: &str) -> String {
    let quote = if s.contains('\'') && !s.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::with_capacity(s.len() + 2);
    out.push(quote);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if c.is_control()
                || matches!(c, '\u{a0}' | '\u{ad}' | '\u{3000}' | '\u{feff}')
                || ('\u{2000}'..='\u{200f}').contains(&c)
                || ('\u{2028}'..='\u{202f}').contains(&c)
                || ('\u{205f}'..='\u{206f}').contains(&c) =>
            {
                let code = u32::from(c);
                if code < 0x100 {
                    out.push_str(&format!("\\x{code:02x}"));
                } else {
                    out.push_str(&format!("\\u{code:04x}"));
                }
            }
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}

/// Python `==` on JSON-decoded values (`1 == 1.0 == True`).
#[must_use]
pub fn py_eq(a: &Value, b: &Value) -> bool {
    let numeric = |v: &Value| match v {
        Value::Bool(b) => Some(f64::from(u8::from(*b))),
        Value::Number(n) => n.as_f64(),
        _ => None,
    };
    match (a, b) {
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(l, r)| py_eq(l, r))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len() && x.iter().all(|(k, l)| y.get(k).is_some_and(|r| py_eq(l, r)))
        }
        (Value::Number(x), Value::Number(y)) if !x.is_f64() && !y.is_f64() => x == y,
        _ => match (numeric(a), numeric(b)) {
            (Some(l), Some(r)) => l == r,
            _ => a == b,
        },
    }
}

/// `redact_audit_metadata` for an operation ID: secret-shaped values are hidden.
#[must_use]
pub fn redact_audit_value(value: &str) -> &str {
    let token = |s: &str, min: usize| {
        s.len() >= min
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    };
    let hex32 = |s: Option<&str>| {
        s.is_some_and(|s| s.len() == 32 && s.bytes().all(|b| b.is_ascii_hexdigit()))
    };
    let trimmed = py_strip(value);
    let session = trimmed
        .strip_prefix("ovgs_")
        .and_then(|rest| rest.split_once(".s_"))
        .is_some_and(|(hex, tail)| hex32(Some(hex)) && token(tail, 16));
    let invitation = trimmed.strip_prefix("inv_").is_some_and(|t| token(t, 16));
    let service = trimmed.strip_prefix("ovgsvc").is_some_and(|rest| {
        hex32(rest.get(..32))
            && rest.get(32..33) == Some("_")
            && rest.get(33..).is_some_and(|t| token(t, 16))
    });
    let parts: Vec<_> = trimmed.split('.').collect();
    let jwt = parts.len() == 3 && parts.iter().all(|p| token(*p, 1));
    if session || invitation || service || jwt {
        "[REDACTED]"
    } else {
        value
    }
}

/// A 2D position.
pub type Xy = (f64, f64);
/// A 2D segment between two positions.
pub type Segment = (Xy, Xy);

/// `snap_point_to_candidates`: deterministic 2D vertex-then-segment snapping.
#[must_use]
pub fn snap(
    target: Xy,
    vertices: &[Xy],
    segments: &[Segment],
    tolerance: f64,
) -> Value {
    let (tx, ty) = target;
    let mut best_dist = tolerance;
    let mut best = target;
    let mut kind = "none";
    for &(vx, vy) in vertices {
        let d = (tx - vx).hypot(ty - vy);
        if d <= best_dist {
            best_dist = d;
            best = (vx, vy);
            kind = "vertex";
        }
    }
    if kind == "none" {
        for &((ax, ay), (bx, by)) in segments {
            let (dx, dy) = (bx - ax, by - ay);
            let len_sq = dx * dx + dy * dy;
            if len_sq == 0.0 {
                continue;
            }
            let t = (((tx - ax) * dx + (ty - ay) * dy) / len_sq).clamp(0.0, 1.0);
            let (px, py) = (ax + t * dx, ay + t * dy);
            let d = (tx - px).hypot(ty - py);
            if d <= best_dist {
                best_dist = d;
                best = (px, py);
                kind = "segment";
            }
        }
    }
    let snapped = kind != "none";
    json!({
        "snapped": snapped,
        "kind": kind,
        "point": [best.0, best.1],
        "distance": if snapped { Some(best_dist) } else { None },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn python_float_repr() {
        for (value, expected) in [
            (21.0, "21.0"),
            (105.84, "105.84"),
            (1e-5, "1e-05"),
            (1.5e16, "1.5e+16"),
            (1e16, "1e+16"),
            (1e15, "1000000000000000.0"),
            (0.0001, "0.0001"),
            (-0.0, "-0.0"),
            (1.2345e100, "1.2345e+100"),
            (-2.5e-7, "-2.5e-07"),
        ] {
            assert_eq!(py_float(value), expected);
        }
    }

    #[test]
    fn python_json_dumps_sorted_ascii() {
        let mut out = String::new();
        py_json(
            &json!({"b": [1, 1.0, 1e-5], "a": "Mã\n\"x\"😀\u{7f}", "c": null}),
            &mut out,
        );
        assert_eq!(
            out,
            r#"{"a":"Mã\n\"x\"😀\u007f","b":[1,1.0,1e-05],"c":null}"#
        );
    }

    #[test]
    fn decimal_and_timestamp_canonicalization() {
        let field = |t: &str| FieldDef {
            field_id: "v".into(),
            field_type: t.into(),
            nullable: true,
            schema_revision: String::new(),
        };
        let dec = field("decimal");
        for (input, expected) in [
            (json!(1000.0), "1000"),
            (json!(1e-5), "0.00001"),
            (json!(-0.5), "-0.5"),
            (json!("007.250"), "7.25"),
            (json!("-0.000"), "0"),
            (json!(1.5e16), "15000000000000000"),
        ] {
            assert_eq!(canonical_attribute(&dec, &input).unwrap(), json!(expected));
        }
        assert!(canonical_attribute(&dec, &json!(1e40)).is_err());
        assert!(canonical_attribute(&dec, &json!("1e5")).is_err());
        let ts = field("timestamp");
        assert_eq!(
            canonical_attribute(&ts, &json!("2026-11-26T07:30:00.5+07:00")).unwrap(),
            json!("2026-11-26T00:30:00.500000Z")
        );
        assert_eq!(
            canonical_attribute(&ts, &json!("2026-11-26T00:00:00Z")).unwrap(),
            json!("2026-11-26T00:00:00Z")
        );
        for bad in [
            "2026-11-26T00:00:60Z",
            "2026-02-30T00:00:00Z",
            "2026-11-26T00:00:00-00:00",
        ] {
            assert!(canonical_attribute(&ts, &json!(bad)).is_err(), "{bad}");
        }
        let int = field("integer");
        assert!(canonical_attribute(&int, &json!(1.0)).is_err());
        assert!(canonical_attribute(&int, &json!(2_147_483_648_i64)).is_err());
        assert!(canonical_attribute(&int, &json!(true)).is_err());
    }

    #[test]
    fn geometry_validation_matches_python_codes() {
        let wkt = |kind: &str, coordinates: Value, family: &str| {
            let geometry = json!({ "type": kind, "coordinates": coordinates });
            geometry_wkt(geometry.as_object(), family)
        };
        let code = |r: Result<String, EditError>| match r {
            Err(EditError::Platform { code, .. }) => code,
            _ => "OK",
        };
        let ring = |last: Value| json!([[[0, 0], [1, 0], [1, 1], last]]);
        assert_eq!(
            wkt("Point", json!([105, 21.5]), "Point").unwrap(),
            "POINT(105.0 21.5)"
        );
        assert_eq!(code(geometry_wkt(None, "Point")), "MISSING_GEOMETRY");
        assert_eq!(
            code(wkt("Point", json!([1, 2, 3]), "Point")),
            "UNSUPPORTED_DIMENSIONS"
        );
        assert_eq!(
            code(wkt("LineString", json!([[0, 0], [1, 1]]), "Point")),
            "GEOMETRY_FAMILY_MISMATCH"
        );
        assert_eq!(
            code(wkt("Polygon", ring(json!([0.5, 0])), "Polygon")),
            "INVALID_GEOMETRY"
        );
        assert_eq!(
            wkt("Polygon", ring(json!([0.0, 0])), "Polygon").unwrap(),
            "POLYGON((0.0 0.0, 1.0 0.0, 1.0 1.0, 0.0 0.0))"
        );
    }

    #[test]
    fn identifiers_redaction_and_snap() {
        assert_eq!(validate_operation_id(" op-1:a.b ").unwrap(), "op-1:a.b");
        assert!(validate_operation_id("bad id").is_err());
        assert!(validate_feature_id("x".repeat(129).as_str()).is_err());
        assert_eq!(redact_audit_value("op.with.three"), "[REDACTED]");
        assert_eq!(redact_audit_value("plain-op-1"), "plain-op-1");
        assert!(py_eq(
            &json!({"a": 1, "b": true}),
            &json!({"b": 1.0, "a": 1.0})
        ));
        assert!(!py_eq(&json!({"a": "1"}), &json!({"a": 1})));
        let vertex = snap((0.0, 0.0), &[(3.0, 4.0)], &[], 5.0);
        assert_eq!(vertex["kind"], "vertex");
        assert_eq!(vertex["distance"], 5.0);
        let segment = snap((1.0, 1.0), &[], &[((0.0, 0.0), (2.0, 0.0))], 2.0);
        assert_eq!(segment["point"], json!([1.0, 0.0]));
        let none = snap((9.0, 9.0), &[(0.0, 0.0)], &[], 1.0);
        assert_eq!(none["distance"], Value::Null);
        assert_eq!(none["point"], json!([9.0, 9.0]));
    }
}
