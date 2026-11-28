//! Shared R003 revision pin digests and ready-geometry validators matching
//! `apps/api/openvietgis_api/domains/spatial/domain.py`.

use std::collections::HashSet;

use ovg_core::crs::WEB_MERCATOR_MAX_COORDINATE_METRE;
use ovg_spatial::{parse_wkb, to_geojson, ValidationLimits};
use serde_json::{json, Value};

use crate::editing::domain::sha256_py_json;

pub const MAX_READY_POSITIONS: usize = 4096;

#[derive(Debug, Clone)]
pub struct FieldSpec {
    pub field_id: String,
    pub name: String,
    pub ordinal: i32,
    pub field_type: String,
    pub nullable: bool,
    pub schema_revision: String,
}

#[derive(Debug, Clone)]
pub struct FeaturePinInput {
    pub feature_id: String,
    pub attributes: Value,
    pub geometry: Value,
}

/// Normalize `-0.0` to `0.0` across a GeoJSON coordinate tree, matching Python `_finite`.
#[must_use]
pub fn normalize_negative_zero(value: &Value) -> Value {
    match value {
        Value::Number(n) => match n.as_f64() {
            Some(f) => {
                if f == 0.0 {
                    json!(0.0_f64)
                } else {
                    json!(f)
                }
            }
            None => value.clone(),
        },
        Value::Array(items) => Value::Array(items.iter().map(normalize_negative_zero).collect()),
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, v) in map {
                out.insert(k.clone(), normalize_negative_zero(v));
            }
            Value::Object(out)
        }
        other => other.clone(),
    }
}

/// Exact port of Python `schema_revision_digest` (`r003-schema-v1`).
#[must_use]
pub fn schema_revision_digest(
    fields: &[FieldSpec],
    geometry_family: &str,
    dimensions: &str,
    schema_state: &str,
) -> String {
    let mut ordered: Vec<&FieldSpec> = fields.iter().collect();
    ordered.sort_by(|a, b| a.field_id.cmp(&b.field_id));
    sha256_py_json(&json!({
        "manifest_type": "r003-schema-v1",
        "schema_state": schema_state,
        "geometry_family": geometry_family,
        "dimensions": dimensions,
        "fields": ordered.iter().map(|f| json!({
            "field_id": f.field_id,
            "name": f.name,
            "ordinal": f.ordinal,
            "type": f.field_type,
            "nullable": f.nullable,
        })).collect::<Vec<_>>(),
    }))
}

/// Exact port of Python `content_revision_digest` (`r003-content-v1`).
#[must_use]
pub fn content_revision_digest(
    features: &[FeaturePinInput],
    fields: &[FieldSpec],
    stored_crs: i32,
    source_crs: i32,
    geometry_family: &str,
    dimensions: &str,
) -> String {
    let mut ordered_fields: Vec<&FieldSpec> = fields.iter().collect();
    ordered_fields.sort_by(|a, b| a.field_id.cmp(&b.field_id));
    let mut ordered_features: Vec<&FeaturePinInput> = features.iter().collect();
    ordered_features.sort_by(|a, b| a.feature_id.cmp(&b.feature_id));
    sha256_py_json(&json!({
        "manifest_type": "r003-content-v1",
        "source_crs": source_crs,
        "stored_crs": stored_crs,
        "geometry_family": geometry_family,
        "dimensions": dimensions,
        "field_interpretations": ordered_fields.iter().map(|f| json!({
            "field_id": f.field_id,
            "type": f.field_type,
            "nullable": f.nullable,
        })).collect::<Vec<_>>(),
        "features": ordered_features.iter().map(|feat| json!({
            "feature_id": feat.feature_id,
            "attributes": feat.attributes,
            "geometry": {
                "type": feat.geometry["type"],
                "dimensions": dimensions,
                "srid": stored_crs,
                "coordinates": feat.geometry["coordinates"],
                "members": [],
            },
            "geometry_state": "present",
        })).collect::<Vec<_>>(),
    }))
}

/// Validate a GeoJSON object against OpenVietGIS ready-geometry rules (XY, bounds, non-degenerate).
pub fn validate_ready_geojson(
    geom: &Value,
    expected_family: Option<&str>,
    srid: i32,
) -> Result<Value, &'static str> {
    if !u32::try_from(srid)
        .ok()
        .is_some_and(ovg_core::crs::is_processing_crs)
    {
        return Err("readable CRS required");
    }
    let gtype = geom
        .get("type")
        .and_then(Value::as_str)
        .ok_or("unsupported native geometry representation")?;
    if !matches!(gtype, "Point" | "LineString" | "Polygon") {
        return Err("unsupported readable geometry family");
    }
    if let Some(expected) = expected_family {
        if gtype != expected {
            return Err("geometry family mismatch");
        }
    }
    let coords = geom
        .get("coordinates")
        .ok_or("invalid geometry coordinate structure")?;

    let parts: Vec<Vec<(f64, f64)>> = match gtype {
        "Point" => vec![vec![parse_xy(coords)?]],
        "LineString" => vec![parse_line(coords, false)?],
        "Polygon" => {
            let rings = coords
                .as_array()
                .filter(|arr| !arr.is_empty())
                .ok_or("empty geometry is not a readable feature")?;
            let mut out = Vec::with_capacity(rings.len());
            for ring in rings {
                out.push(parse_line(ring, true)?);
            }
            out
        }
        _ => return Err("unsupported readable geometry family"),
    };

    let total_positions: usize = parts.iter().map(Vec::len).sum();
    if total_positions > MAX_READY_POSITIONS {
        return Err("geometry exceeds bounded position count");
    }

    for part in &parts {
        for &(x, y) in part {
            validate_position_domain(x, y, srid)?;
        }
        if matches!(srid, 4326 | 4756)
            && part
                .windows(2)
                .any(|pair| (pair[0].0 - pair[1].0).abs() > 180.0)
        {
            return Err("unsupported longitude wrapping");
        }
    }

    let normalized_coords = match gtype {
        "Point" => json!([parts[0][0].0, parts[0][0].1]),
        "LineString" => Value::Array(parts[0].iter().map(|&(x, y)| json!([x, y])).collect()),
        "Polygon" => Value::Array(
            parts
                .iter()
                .map(|ring| Value::Array(ring.iter().map(|&(x, y)| json!([x, y])).collect()))
                .collect(),
        ),
        _ => unreachable!(),
    };

    Ok(json!({
        "type": gtype,
        "coordinates": normalized_coords,
    }))
}

/// Parse OGC/EWKB bytes and validate as ready XY geometry for `(expected_family, expected_srid)`.
pub fn validate_ready_wkb(
    wkb: &[u8],
    expected_family: &str,
    expected_srid: i32,
) -> Result<Value, &'static str> {
    let parsed = parse_wkb(wkb, &ValidationLimits::default())
        .map_err(|_| "invalid native geometry encoding")?;
    if let Some(srid) = parsed.srid {
        if i32::try_from(srid).ok() != Some(expected_srid) {
            return Err("native CRS mismatch");
        }
    }
    let geojson = to_geojson(&parsed.geometry);
    validate_ready_geojson(&geojson, Some(expected_family), expected_srid)
}

fn parse_xy(value: &Value) -> Result<(f64, f64), &'static str> {
    let pair = value.as_array().ok_or("XY position required")?;
    if pair.len() != 2 {
        return Err("XY position required");
    }
    let x = pair[0]
        .as_f64()
        .filter(|v| v.is_finite())
        .ok_or("finite coordinate required")?;
    let y = pair[1]
        .as_f64()
        .filter(|v| v.is_finite())
        .ok_or("finite coordinate required")?;
    Ok((
        if x == 0.0 { 0.0 } else { x },
        if y == 0.0 { 0.0 } else { y },
    ))
}

fn parse_line(value: &Value, ring: bool) -> Result<Vec<(f64, f64)>, &'static str> {
    let items = value
        .as_array()
        .ok_or("geometry coordinate sequence required")?;
    let mut pts = Vec::with_capacity(items.len());
    for item in items {
        pts.push(parse_xy(item)?);
    }
    if ring {
        if pts.len() < 4 || pts.first() != pts.last() {
            return Err("polygon ring must be closed and nondegenerate");
        }
        let distinct: HashSet<(u64, u64)> = pts[..pts.len() - 1]
            .iter()
            .map(|&(x, y)| (x.to_bits(), y.to_bits()))
            .collect();
        if distinct.len() < 3 {
            return Err("polygon ring must be closed and nondegenerate");
        }
        let twice_area: f64 = pts
            .windows(2)
            .map(|w| w[0].0 * w[1].1 - w[1].0 * w[0].1)
            .sum();
        if !twice_area.is_finite() || twice_area == 0.0 {
            return Err("polygon ring must have nonzero finite area");
        }
    } else {
        if pts.len() < 2 {
            return Err("line requires at least two distinct positions");
        }
        let distinct: HashSet<(u64, u64)> = pts
            .iter()
            .map(|&(x, y)| (x.to_bits(), y.to_bits()))
            .collect();
        if distinct.len() < 2 {
            return Err("line requires at least two distinct positions");
        }
    }
    Ok(pts)
}

fn validate_position_domain(x: f64, y: f64, srid: i32) -> Result<(), &'static str> {
    match srid {
        4326 | 4756 => {
            if !(-180.0..=180.0).contains(&x) || !(-90.0..=90.0).contains(&y) {
                return Err("geographic position outside XY domain");
            }
        }
        3857 => {
            if x.abs() > WEB_MERCATOR_MAX_COORDINATE_METRE
                || y.abs() > WEB_MERCATOR_MAX_COORDINATE_METRE
            {
                return Err("Web Mercator position outside domain");
            }
        }
        _ if u32::try_from(srid)
            .ok()
            .is_some_and(ovg_core::crs::is_extended_native_vn2000) =>
        {
            ovg_spatial::transform::project_native_vn2000(
                srid as u32,
                4756,
                ovg_spatial::Coord2D::new(x, y),
            )
            .map_err(|_| "projected VN-2000 position outside declared zone")?;
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_ready_geojson_and_normalizes_negative_zero() {
        let pt = validate_ready_geojson(
            &json!({"type": "Point", "coordinates": [-0.0, 21.0]}),
            Some("Point"),
            4326,
        )
        .unwrap();
        assert_eq!(
            pt["coordinates"][0].as_f64().unwrap().to_bits(),
            0.0_f64.to_bits()
        );

        assert!(validate_ready_geojson(
            &json!({"type": "Point", "coordinates": [181.0, 21.0]}),
            Some("Point"),
            4326
        )
        .is_err());

        assert!(validate_ready_geojson(
            &json!({"type": "LineString", "coordinates": [[-179.0, 0.0], [179.0, 0.0]]}),
            Some("LineString"),
            4326
        )
        .is_err());

        assert!(validate_ready_geojson(
            &json!({"type": "Polygon", "coordinates": [[[0.0, 0.0], [1.0, 0.0], [2.0, 0.0], [0.0, 0.0]]]}),
            Some("Polygon"),
            4326
        )
        .is_err());
    }
}
