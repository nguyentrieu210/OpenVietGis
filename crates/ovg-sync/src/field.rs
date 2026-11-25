//! R013 offline intent candidate. Only use with an authenticated, integrity-verified
//! package and native CRS. This is not an authorization or persistence layer.
use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldError {
    Validation,
    InvalidPackage,
    InvalidOperationId,
    InvalidFeatureId,
    SchemaMismatch,
    OutsidePackageScope,
}
impl FieldError {
    pub fn code(self) -> &'static str {
        match self {
            Self::Validation => "VALIDATION_ERROR",
            Self::InvalidPackage => "PACKAGE_INVALID",
            Self::InvalidOperationId => "INVALID_OFFLINE_OPERATION_ID",
            Self::InvalidFeatureId => "INVALID_FEATURE_ID",
            Self::SchemaMismatch => "SCHEMA_MISMATCH",
            Self::OutsidePackageScope => "OUTSIDE_PACKAGE_SCOPE",
        }
    }
}
pub fn validate_bbox(value: &Value) -> Result<[f64; 4], FieldError> {
    let obj = value.as_object().ok_or(FieldError::Validation)?;
    if obj.len() != 4 {
        return Err(FieldError::Validation);
    }
    let mut bounds = [0.; 4];
    for (i, key) in ["min_x", "min_y", "max_x", "max_y"].iter().enumerate() {
        bounds[i] = obj.get(*key).and_then(Value::as_f64)
            .filter(|n| n.is_finite()).ok_or(FieldError::Validation)?;
    }
    if bounds[0] >= bounds[2] || bounds[1] >= bounds[3] {
        return Err(FieldError::Validation);
    }
    Ok(bounds)
}

/// Scope order is part of the existing Python wire contract.
pub fn offline_core_feature_id(scope: [Uuid; 6]) -> String {
    let text = format!("ovg-field-feature-v1:{}:{}:{}:{}:{}:{}",
        scope[0], scope[1], scope[2], scope[3], scope[4], scope[5]);
    let digest = Sha256::digest(text.as_bytes());
    format!("field-core-{:x}", digest)[..59].to_owned()
}

fn decimal(s: &str) -> bool {
    if s.len() > 4096 { return false; }
    let unsigned = s.strip_prefix('-').unwrap_or(s);
    let (whole, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
    !whole.is_empty() && whole.bytes().all(|b| b.is_ascii_digit())
        && (whole == "0" || !whole.starts_with('0'))
        && (!unsigned.contains('.') || (!fraction.is_empty() && !fraction.ends_with('0')))
        && fraction.bytes().all(|b| b.is_ascii_digit()) && fraction.len() <= 18
        && whole.trim_start_matches('0').len() + fraction.len() <= 38 && s != "-0"
}
fn timestamp(s: &str) -> bool {
    let Ok(dt) = DateTime::parse_from_rfc3339(s) else { return false };
    let utc = dt.with_timezone(&Utc);
    if s.len() < 4 || s.starts_with("0000") || utc.timestamp_subsec_nanos() % 1000 != 0 {
        return false;
    }
    let format = if utc.timestamp_subsec_nanos() == 0 { SecondsFormat::Secs } else { SecondsFormat::Micros };
    // Python datetime rejects leap seconds and only accepts years 1..9999.
    utc.timestamp_subsec_nanos() < 1_000_000_000 && utc.to_rfc3339_opts(format, true) == s
}
fn canonical_value(value: &Value, kind: &str, nullable: bool) -> bool {
    if value.is_null() { return nullable; }
    match kind {
        "text" => value.as_str().is_some_and(|s| !s.contains('\0') && s.chars().count() <= 4096),
        "integer" => value.as_i64().is_some_and(|n| i32::try_from(n).is_ok()),
        "boolean" => value.is_boolean(),
        "decimal" => value.as_str().is_some_and(decimal),
        "timestamp" => value.as_str().is_some_and(timestamp),
        _ => false,
    }
}

pub fn validate_offline_ops(snapshot: &Value, operations: &[Value]) -> Result<(), FieldError> {
    use FieldError::*;
    if operations.is_empty() || operations.len() > 2000 { return Err(Validation); }
    let b = validate_bbox(&snapshot["aoi_bbox"]).map_err(|_| InvalidPackage)?;
    let features = snapshot["features"].as_array().ok_or(InvalidPackage)?;
    let mut ids = HashSet::with_capacity(features.len());
    for f in features {
        if !ids.insert(f["feature_id"].as_str().ok_or(InvalidPackage)?) { return Err(InvalidPackage); }
    }
    let mut fields = HashMap::new();
    for f in snapshot["fields"].as_array().ok_or(InvalidPackage)? {
        let id = f["field_id"].as_str().ok_or(InvalidPackage)?;
        let kind = f["type"].as_str().ok_or(InvalidPackage)?;
        let nullable = f["nullable"].as_bool().ok_or(InvalidPackage)?;
        if !matches!(kind, "text" | "integer" | "decimal" | "boolean" | "timestamp")
            || fields.insert(id, (kind, nullable)).is_some() { return Err(InvalidPackage); }
    }
    let mut seen = HashSet::new();
    let mut operation_ids = HashSet::new();
    for op in operations {
        let obj = op.as_object().ok_or(Validation)?;
        if obj.keys().any(|k| !matches!(k.as_str(), "op_type" | "feature_id" | "geometry" | "properties" | "client_operation_id")) {
            return Err(Validation);
        }
        let id = op["feature_id"].as_str().ok_or(Validation)?;
        if !seen.insert(id) { return Err(Validation); }
        if let Some(raw) = obj.get("client_operation_id") {
            let raw = raw.as_str().ok_or(InvalidOperationId)?;
            let uuid = Uuid::parse_str(raw).map_err(|_| InvalidOperationId)?;
            if uuid.is_nil() || uuid.to_string() != raw || !operation_ids.insert(uuid) {
                return Err(InvalidOperationId);
            }
            if id.is_empty() || id.len() > 128 || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-') {
                return Err(InvalidFeatureId);
            }
        }
        let props = op["properties"].as_object().ok_or(Validation)?;
        if props.len() != fields.len() || props.keys().any(|k| !fields.contains_key(k.as_str())) { return Err(Validation); }
        for (key, value) in props {
            let (kind, nullable) = fields[key.as_str()];
            if !canonical_value(value, kind, nullable) { return Err(SchemaMismatch); }
        }
        match op["op_type"].as_str() {
            Some("update") if ids.contains(id) && op["geometry"].is_null() => {},
            Some("create") if snapshot["geometry_family"] == "Point" && !ids.contains(id)
                && op["geometry"]["type"] == "Point" => {
                let c = op["geometry"]["coordinates"].as_array().ok_or(OutsidePackageScope)?;
                if c.len() != 2 { return Err(OutsidePackageScope); }
                let x = c[0].as_f64().ok_or(OutsidePackageScope)?;
                let y = c[1].as_f64().ok_or(OutsidePackageScope)?;
                if !x.is_finite() || !y.is_finite() || x < b[0] || x > b[2] || y < b[1] || y > b[3] {
                    return Err(OutsidePackageScope);
                }
            }
            _ => return Err(OutsidePackageScope),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn bounds_and_exact_decimal() {
        assert!(validate_bbox(&json!({"min_x":0,"min_y":0,"max_x":1,"max_y":1})).is_ok());
        assert!(validate_bbox(&json!({"min_x":true,"min_y":0,"max_x":1,"max_y":1})).is_err());
        for invalid in ["01", "-0", "12.50", "1e2", "0.1234567890123456789", "1."] { assert!(!decimal(invalid)); }
        assert!(decimal("123456789.123456789"));
        assert!(timestamp("2026-11-21T00:00:00.123000Z"));
        assert!(!timestamp("2026-11-21T00:00:00.123Z"));
    }
    #[test]
    fn fixture_parity() {
        let fixture: Value = serde_json::from_str(include_str!("../../../specs/064-rust-platform/evidence/e13-fixtures.json")).unwrap();
        for case in fixture["cases"].as_array().unwrap() {
            let actual = validate_offline_ops(&fixture["snapshot"], case["ops"].as_array().unwrap());
            let code = actual.err().map(FieldError::code).unwrap_or("OK");
            assert_eq!(code, case["expected"].as_str().unwrap(), "{}", case["id"]);
        }
        let scope: Vec<Uuid> = fixture["scope"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().parse().unwrap()).collect();
        assert_eq!(offline_core_feature_id(scope.try_into().unwrap()), fixture["core_id"]);
    }
}
