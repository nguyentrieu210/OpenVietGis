//! E06 cartography repository over canonical RLS tables (`layer_styles`,
//! `print_layouts`, `print_exports`, and `designer_layouts`).

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;

use chrono::{DateTime, Utc};
use ovg_core::crs::{
    reference_transformation, WEB_MERCATOR_MAX_COORDINATE_METRE, WEB_MERCATOR_MAX_LATITUDE,
    WEB_MERCATOR_PIPELINE,
};
use ovg_core::AppError;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::postgres::PgRow;
use sqlx::Row;
use uuid::Uuid;

use crate::catalog::ProjectRead;

fn db_error(error: sqlx::Error) -> AppError {
    tracing::error!(%error, "cartography database operation failed");
    AppError::internal("cartography database operation failed")
}

/// Python `datetime.isoformat()` of a UTC timestamp.
#[must_use]
pub fn iso(ts: DateTime<Utc>) -> String {
    if ts.timestamp_subsec_micros() == 0 {
        ts.format("%Y-%m-%dT%H:%M:%S+00:00").to_string()
    } else {
        ts.format("%Y-%m-%dT%H:%M:%S%.6f+00:00").to_string()
    }
}

/// Append Python's `repr(float)` (shortest round-trip, Python exponent rules).
fn push_py_float(out: &mut String, value: f64) {
    if value == 0.0 {
        out.push_str(if value.is_sign_negative() {
            "-0.0"
        } else {
            "0.0"
        });
        return;
    }
    let sci = format!("{value:e}");
    let (mantissa, exponent) = sci.split_once('e').unwrap_or((sci.as_str(), "0"));
    let exponent: i32 = exponent.parse().unwrap_or(0);
    let mantissa = match mantissa.strip_prefix('-') {
        Some(rest) => {
            out.push('-');
            rest
        }
        None => mantissa,
    };
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    if (-4..16).contains(&exponent) {
        let point = exponent + 1;
        if point <= 0 {
            out.push_str("0.");
            for _ in 0..-point {
                out.push('0');
            }
            out.push_str(&digits);
        } else {
            let point = point as usize;
            if point >= digits.len() {
                out.push_str(&digits);
                for _ in digits.len()..point {
                    out.push('0');
                }
                out.push_str(".0");
            } else {
                out.push_str(&digits[..point]);
                out.push('.');
                out.push_str(&digits[point..]);
            }
        }
    } else {
        out.push_str(&digits[..1]);
        if digits.len() > 1 {
            out.push('.');
            out.push_str(&digits[1..]);
        }
        let sign = if exponent < 0 { '-' } else { '+' };
        let _ = write!(out, "e{sign}{:02}", exponent.unsigned_abs());
    }
}

fn push_utf8_json_str(out: &mut String, text: &str) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Canonical JSON matching Python:
/// `json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False, allow_nan=False)`.
pub fn push_canonical_json_utf8(out: &mut String, value: &Value) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => match (n.as_i64(), n.as_u64()) {
            (Some(i), _) => {
                let _ = write!(out, "{i}");
            }
            (None, Some(u)) => {
                let _ = write!(out, "{u}");
            }
            _ => push_py_float(out, n.as_f64().unwrap_or(0.0)),
        },
        Value::String(s) => push_utf8_json_str(out, s),
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                push_canonical_json_utf8(out, item);
            }
            out.push(']');
        }
        Value::Object(map) => {
            out.push('{');
            for (i, (key, item)) in map.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                push_utf8_json_str(out, key);
                out.push(':');
                push_canonical_json_utf8(out, item);
            }
            out.push('}');
        }
    }
}

/// SHA-256 hex digest matching Python `print_revision_fingerprint`.
#[must_use]
pub fn print_revision_fingerprint(value: &Value) -> String {
    let mut out = String::new();
    push_canonical_json_utf8(&mut out, value);
    let digest = Sha256::digest(out.as_bytes());
    let mut hex = String::with_capacity(64);
    for byte in digest {
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

fn style_wire(row: &PgRow) -> Result<Value, AppError> {
    let label_field = row
        .try_get::<Option<String>, _>("label_field")
        .map_err(db_error)?
        .filter(|s| !s.is_empty());
    Ok(json!({
        "id": row.try_get::<Uuid, _>("id").map_err(db_error)?,
        "layer_id": row.try_get::<Uuid, _>("layer_id").map_err(db_error)?,
        "mode": row.try_get::<String, _>("mode").map_err(db_error)?,
        "style_spec": row.try_get::<Value, _>("style_spec").map_err(db_error)?,
        "label_field": label_field,
        "legend_items": row.try_get::<Value, _>("legend_items").map_err(db_error)?,
        "revision": row.try_get::<i32, _>("revision").map_err(db_error)?,
        "updated_at": iso(row.try_get::<DateTime<Utc>, _>("updated_at").map_err(db_error)?),
    }))
}

fn layout_wire(row: &PgRow) -> Result<Value, AppError> {
    Ok(json!({
        "id": row.try_get::<Uuid, _>("id").map_err(db_error)?,
        "name": row.try_get::<String, _>("name").map_err(db_error)?,
        "title": row.try_get::<String, _>("title").map_err(db_error)?,
        "paper_size": row.try_get::<String, _>("paper_size").map_err(db_error)?,
        "orientation": row.try_get::<String, _>("orientation").map_err(db_error)?,
        "crs_code": row.try_get::<i32, _>("crs_code").map_err(db_error)?,
        "attribution": row.try_get::<String, _>("attribution").map_err(db_error)?,
        "show_north_arrow": row.try_get::<bool, _>("show_north_arrow").map_err(db_error)?,
        "show_scale_bar": row.try_get::<bool, _>("show_scale_bar").map_err(db_error)?,
        "show_legend": row.try_get::<bool, _>("show_legend").map_err(db_error)?,
        "layer_ids": row.try_get::<Value, _>("layer_ids").map_err(db_error)?,
        "extent": row.try_get::<Option<Value>, _>("extent").map_err(db_error)?,
        "created_at": iso(row.try_get::<DateTime<Utc>, _>("created_at").map_err(db_error)?),
    }))
}

pub async fn get_layer_style(
    read: &mut ProjectRead<'_>,
    layer_id: Uuid,
) -> Result<Value, AppError> {
    let row = sqlx::query(
        "SELECT id, layer_id, mode, style_spec, label_field, legend_items, revision, updated_at
         FROM cartography.layer_styles
         WHERE organization_id = $1 AND project_id = $2 AND layer_id = $3",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(layer_id)
    .fetch_optional(&mut **read.transaction())
    .await
    .map_err(db_error)?;
    let style = row.as_ref().map(style_wire).transpose()?;
    Ok(json!({ "style": style }))
}

pub async fn list_layouts(read: &mut ProjectRead<'_>) -> Result<Value, AppError> {
    let rows = sqlx::query(
        "SELECT id, name, title, paper_size, orientation, crs_code, attribution,
                show_north_arrow, show_scale_bar, show_legend, layer_ids, extent, created_at
         FROM cartography.print_layouts
         WHERE organization_id = $1 AND project_id = $2
         ORDER BY created_at DESC",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(db_error)?;
    let items = rows
        .iter()
        .map(layout_wire)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({ "items": items }))
}

#[derive(Debug, Clone)]
pub struct StyleTarget {
    pub name: String,
    pub source_id: Uuid,
}

pub async fn style_target(
    edit: &mut ProjectRead<'_>,
    layer_id: Uuid,
) -> Result<StyleTarget, AppError> {
    let row = sqlx::query(
        "SELECT id, name, source_id
         FROM spatial.layers
         WHERE organization_id = $1 AND project_id = $2 AND id = $3
           AND lifecycle = 'active'
         FOR UPDATE",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(layer_id)
    .fetch_optional(&mut **edit.transaction())
    .await
    .map_err(db_error)?
    .ok_or_else(|| AppError::not_found("layer not found"))?;
    Ok(StyleTarget {
        name: row.try_get("name").map_err(db_error)?,
        source_id: row.try_get("source_id").map_err(db_error)?,
    })
}

#[derive(Debug, Clone)]
pub struct StyleInsert {
    pub actor_id: Uuid,
    pub layer_id: Uuid,
    pub source_id: Uuid,
    pub mode: String,
    pub style_spec: Value,
    pub label_field: Option<String>,
    pub legend_items: Value,
}

pub async fn upsert_layer_style(
    edit: &mut ProjectRead<'_>,
    input: StyleInsert,
) -> Result<Value, AppError> {
    if let Some(field_id) = input.label_field.as_deref().filter(|s| !s.is_empty()) {
        let exists = sqlx::query_scalar::<_, i32>(
            "SELECT 1 FROM spatial.source_fields
             WHERE organization_id = $1 AND project_id = $2
               AND source_id = $3 AND field_id = $4",
        )
        .bind(edit.organization_id())
        .bind(edit.project_id())
        .bind(input.source_id)
        .bind(field_id)
        .fetch_optional(&mut **edit.transaction())
        .await
        .map_err(db_error)?;
        if exists.is_none() {
            return Err(AppError::validation(
                "UNKNOWN_FIELD",
                format!("label_field '{field_id}' not in layer schema"),
            ));
        }
    }

    let now = Utc::now();
    let row = sqlx::query(
        "INSERT INTO cartography.layer_styles (
            id, organization_id, project_id, layer_id, mode, style_spec,
            label_field, legend_items, revision, updated_by_actor_id,
            created_at, updated_at
         ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,1,$9,$10,$10)
         ON CONFLICT (organization_id, project_id, layer_id) DO UPDATE
         SET mode = EXCLUDED.mode,
             style_spec = EXCLUDED.style_spec,
             label_field = EXCLUDED.label_field,
             legend_items = EXCLUDED.legend_items,
             revision = cartography.layer_styles.revision + 1,
             updated_by_actor_id = EXCLUDED.updated_by_actor_id,
             updated_at = EXCLUDED.updated_at
         RETURNING id, layer_id, mode, style_spec, label_field, legend_items,
                   revision, updated_at",
    )
    .bind(Uuid::new_v4())
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(input.layer_id)
    .bind(input.mode)
    .bind(input.style_spec)
    .bind(input.label_field)
    .bind(input.legend_items)
    .bind(input.actor_id)
    .bind(now)
    .fetch_one(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    style_wire(&row)
}

#[derive(Debug, Clone)]
pub struct LayoutInsert {
    pub name: String,
    pub title: String,
    pub paper_size: String,
    pub orientation: String,
    pub crs_code: i32,
    pub attribution: String,
    pub show_north_arrow: bool,
    pub show_scale_bar: bool,
    pub show_legend: bool,
    pub layer_ids: Vec<Uuid>,
    pub extent: Option<Value>,
}

pub async fn create_layout(
    edit: &mut ProjectRead<'_>,
    actor_id: Uuid,
    input: LayoutInsert,
) -> Result<Value, AppError> {
    for layer_id in &input.layer_ids {
        let available = sqlx::query_scalar::<_, Uuid>(
            "SELECT l.id
             FROM spatial.layers l
             JOIN spatial.data_sources s
               ON l.source_id = s.id
              AND l.project_id = s.project_id
              AND l.organization_id = s.organization_id
             WHERE l.organization_id = $1
               AND l.project_id = $2
               AND l.id = $3
               AND l.lifecycle = 'active'
               AND s.lifecycle = 'active'
               AND s.validation_state = 'ready'",
        )
        .bind(edit.organization_id())
        .bind(edit.project_id())
        .bind(layer_id)
        .fetch_optional(&mut **edit.transaction())
        .await
        .map_err(db_error)?;
        if available.is_none() {
            return Err(AppError::not_found("layout layer unavailable"));
        }
    }

    let now = Utc::now();
    let layer_ids = Value::Array(
        input
            .layer_ids
            .iter()
            .map(|id| Value::String(id.to_string()))
            .collect(),
    );
    let row = sqlx::query(
        "INSERT INTO cartography.print_layouts (
            id, organization_id, project_id, name, title, paper_size, orientation,
            crs_code, attribution, show_north_arrow, show_scale_bar, show_legend,
            layer_ids, extent, created_by_actor_id, created_at, updated_at
         ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$16)
         RETURNING id, name, title, paper_size, orientation, crs_code, attribution,
                   show_north_arrow, show_scale_bar, show_legend, layer_ids, extent,
                   created_at",
    )
    .bind(Uuid::new_v4())
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(input.name)
    .bind(input.title)
    .bind(input.paper_size)
    .bind(input.orientation)
    .bind(input.crs_code)
    .bind(input.attribution)
    .bind(input.show_north_arrow)
    .bind(input.show_scale_bar)
    .bind(input.show_legend)
    .bind(layer_ids)
    .bind(input.extent)
    .bind(actor_id)
    .bind(now)
    .fetch_one(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    layout_wire(&row)
}

fn export_wire(row: &PgRow) -> Result<Value, AppError> {
    Ok(json!({
        "id": row.try_get::<Uuid, _>("id").map_err(db_error)?,
        "layout_id": row.try_get::<Uuid, _>("layout_id").map_err(db_error)?,
        "output_format": row.try_get::<String, _>("output_format").map_err(db_error)?,
        "dpi": row.try_get::<i32, _>("dpi").map_err(db_error)?,
        "scale_denominator": row.try_get::<i32, _>("scale_denominator").map_err(db_error)?,
        "scale_accuracy_note": row.try_get::<String, _>("scale_accuracy_note").map_err(db_error)?,
        "revision_manifest": row.try_get::<Value, _>("revision_manifest").map_err(db_error)?,
        "sha256": row.try_get::<String, _>("sha256").map_err(db_error)?,
        "byte_size": row.try_get::<i64, _>("byte_size").map_err(db_error)?,
        "created_at": iso(row.try_get::<DateTime<Utc>, _>("created_at").map_err(db_error)?),
    }))
}

pub async fn list_exports(read: &mut ProjectRead<'_>) -> Result<Value, AppError> {
    let rows = sqlx::query(
        "SELECT id, layout_id, output_format, dpi, scale_denominator,
                scale_accuracy_note, revision_manifest, sha256, byte_size, created_at
         FROM cartography.print_exports
         WHERE organization_id = $1 AND project_id = $2
         ORDER BY created_at DESC
         LIMIT 50",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(db_error)?;
    let items = rows
        .iter()
        .map(export_wire)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({ "items": items }))
}

#[derive(Debug, Clone)]
pub struct PrintLayoutRow {
    pub id: Uuid,
    pub name: String,
    pub title: String,
    pub paper_size: String,
    pub orientation: String,
    pub crs_code: i32,
    pub attribution: String,
    pub show_north_arrow: bool,
    pub show_scale_bar: bool,
    pub show_legend: bool,
    pub layer_ids: Vec<String>,
    pub extent: Option<BTreeMap<String, f64>>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct PrintFeatureRow {
    pub feature_id: String,
    pub attributes: Value,
    pub geometry: Value,
}

#[derive(Debug, Clone)]
pub struct PrintLayerSnapshot {
    pub id: Uuid,
    pub name: String,
    pub layer_revision: i64,
    pub source_id: Uuid,
    pub content_revision: String,
    pub schema_revision: String,
    pub source_crs: i32,
    pub transformation_operation: Option<String>,
    pub style: Option<Value>,
    pub style_id: Option<Uuid>,
    pub style_revision: i32,
    pub style_fingerprint: Option<String>,
    pub features: Vec<PrintFeatureRow>,
}

#[derive(Debug, Clone)]
pub struct PrintSnapshotData {
    pub layout: PrintLayoutRow,
    pub layers: Vec<PrintLayerSnapshot>,
    pub data_bounds: Option<[f64; 4]>,
}

fn parse_extent_map(value: Option<Value>) -> Result<Option<BTreeMap<String, f64>>, AppError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let obj = value
        .as_object()
        .ok_or_else(|| AppError::validation("INVALID_PRINT_EXTENT", "invalid print extent"))?;
    let mut map = BTreeMap::new();
    for (k, v) in obj {
        let num = v
            .as_f64()
            .filter(|n| n.is_finite())
            .ok_or_else(|| AppError::validation("INVALID_PRINT_EXTENT", "invalid print extent"))?;
        map.insert(k.clone(), num);
    }
    Ok(Some(map))
}

async fn require_web_domain(
    read: &mut ProjectRead<'_>,
    source_id: Uuid,
    native_srid: i32,
) -> Result<(), AppError> {
    if !matches!(native_srid, 4326 | 3857) {
        return Err(AppError::validation(
            "UNSUPPORTED_TRANSFORMATION",
            "unsupported transform CRS",
        ));
    }
    let (min_x, min_y, max_x, max_y) = if native_srid == 4326 {
        (
            -180.0_f64,
            -WEB_MERCATOR_MAX_LATITUDE,
            180.0_f64,
            WEB_MERCATOR_MAX_LATITUDE,
        )
    } else {
        (
            -WEB_MERCATOR_MAX_COORDINATE_METRE,
            -WEB_MERCATOR_MAX_COORDINATE_METRE,
            WEB_MERCATOR_MAX_COORDINATE_METRE,
            WEB_MERCATOR_MAX_COORDINATE_METRE,
        )
    };
    let invalid: bool = sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1
            FROM spatial.reference_features
            WHERE organization_id = $1 AND project_id = $2 AND source_id = $3
            AND (
                CASE WHEN geometry_state = 'present' AND geometry IS NOT NULL
                    AND ST_SRID(geometry) = $4 AND ST_IsValid(geometry)
                    AND ST_Zmflag(geometry) = 0 AND NOT ST_IsEmpty(geometry)
                THEN (
                    ST_XMin(geometry) >= $5
                    AND ST_XMax(geometry) <= $6
                    AND ST_YMin(geometry) >= $7
                    AND ST_YMax(geometry) <= $8
                ) ELSE false END
            ) IS NOT TRUE
        )",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(source_id)
    .bind(native_srid)
    .bind(min_x)
    .bind(max_x)
    .bind(min_y)
    .bind(max_y)
    .fetch_one(&mut **read.transaction())
    .await
    .map_err(db_error)?;
    if invalid {
        return Err(AppError::validation(
            "TRANSFORMATION_OUT_OF_AREA",
            "geometry is outside the verified transformation domain",
        ));
    }
    Ok(())
}

/// Lock and snapshot layout, active ready layers, styles, and features inside the
/// active RLS-bound project transaction.
pub async fn prepare_print_snapshot(
    read: &mut ProjectRead<'_>,
    layout_id: Uuid,
) -> Result<PrintSnapshotData, AppError> {
    let lrow = sqlx::query(
        "SELECT id, name, title, paper_size, orientation, crs_code, attribution,
                show_north_arrow, show_scale_bar, show_legend, layer_ids, extent, updated_at
         FROM cartography.print_layouts
         WHERE organization_id = $1 AND project_id = $2 AND id = $3
         FOR SHARE",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(layout_id)
    .fetch_optional(&mut **read.transaction())
    .await
    .map_err(db_error)?
    .ok_or_else(|| AppError::not_found("print layout not found"))?;

    let raw_layer_ids: Value = lrow.try_get("layer_ids").map_err(db_error)?;
    let layer_ids: Vec<String> = raw_layer_ids
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|v| v.as_str().map(ToOwned::to_owned))
                .collect()
        })
        .unwrap_or_default();
    let unique_count = layer_ids
        .iter()
        .collect::<std::collections::HashSet<_>>()
        .len();
    if layer_ids.is_empty() || unique_count != layer_ids.len() || layer_ids.len() > 20 {
        return Err(AppError::validation(
            "LAYOUT_LAYER_LIMIT",
            "invalid layout layer set",
        ));
    }

    let layout = PrintLayoutRow {
        id: lrow.try_get("id").map_err(db_error)?,
        name: lrow.try_get("name").map_err(db_error)?,
        title: lrow.try_get("title").map_err(db_error)?,
        paper_size: lrow.try_get("paper_size").map_err(db_error)?,
        orientation: lrow.try_get("orientation").map_err(db_error)?,
        crs_code: lrow.try_get("crs_code").map_err(db_error)?,
        attribution: lrow.try_get("attribution").map_err(db_error)?,
        show_north_arrow: lrow.try_get("show_north_arrow").map_err(db_error)?,
        show_scale_bar: lrow.try_get("show_scale_bar").map_err(db_error)?,
        show_legend: lrow.try_get("show_legend").map_err(db_error)?,
        layer_ids: layer_ids.clone(),
        extent: parse_extent_map(lrow.try_get("extent").map_err(db_error)?)?,
        updated_at: lrow.try_get("updated_at").map_err(db_error)?,
    };

    let mut sorted_ids = layer_ids.clone();
    sorted_ids.sort();

    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    let mut by_id = HashMap::with_capacity(sorted_ids.len());
    for lid_str in &sorted_ids {
        let lid = Uuid::parse_str(lid_str)
            .map_err(|_| AppError::validation("LAYOUT_LAYER_LIMIT", "invalid layout layer set"))?;
        let meta = sqlx::query(
            "SELECT l.id AS layer_id, l.name AS layer_name, l.layer_revision::bigint AS layer_revision,
                    s.id AS source_id, s.stored_crs, s.content_revision, s.schema_revision,
                    s.feature_count::bigint AS feature_count, s.geometry_family
             FROM spatial.layers l
             JOIN spatial.data_sources s
               ON s.organization_id = l.organization_id
              AND s.project_id = l.project_id
              AND s.id = l.source_id
             WHERE l.organization_id = $1 AND l.project_id = $2 AND l.id = $3
               AND l.lifecycle = 'active' AND s.lifecycle = 'active'
               AND s.validation_state = 'ready'
             FOR SHARE OF l, s",
        )
        .bind(read.organization_id())
        .bind(read.project_id())
        .bind(lid)
        .fetch_optional(&mut **read.transaction())
        .await
        .map_err(db_error)?
        .ok_or_else(|| AppError::not_found("one or more layout layers are unavailable"))?;

        let source_id: Uuid = meta.try_get("source_id").map_err(db_error)?;
        let source_srid: i32 = meta
            .try_get::<Option<i32>, _>("stored_crs")
            .map_err(db_error)?
            .ok_or_else(|| {
                AppError::validation("INVALID_PRINT_SOURCE", "print source has no stored CRS")
            })?;
        let content_rev: String = meta.try_get("content_revision").map_err(db_error)?;
        let schema_rev: String = meta.try_get("schema_revision").map_err(db_error)?;
        let feature_count: Option<i64> = meta.try_get("feature_count").map_err(db_error)?;
        let geometry_family: String = meta.try_get("geometry_family").map_err(db_error)?;
        let layer_name: String = meta.try_get("layer_name").map_err(db_error)?;
        let layer_revision: i64 = meta.try_get("layer_revision").map_err(db_error)?;

        let style_row = sqlx::query(
            "SELECT id AS style_id, mode, style_spec, label_field, legend_items, revision
             FROM cartography.layer_styles
             WHERE organization_id = $1 AND project_id = $2 AND layer_id = $3
             FOR SHARE",
        )
        .bind(read.organization_id())
        .bind(read.project_id())
        .bind(lid)
        .fetch_optional(&mut **read.transaction())
        .await
        .map_err(db_error)?;

        let (style_dict, style_id, style_revision, style_fingerprint) = match style_row {
            Some(row) => {
                let style_id: Uuid = row.try_get("style_id").map_err(db_error)?;
                let mode: String = row.try_get("mode").map_err(db_error)?;
                let style_spec: Value = row.try_get("style_spec").map_err(db_error)?;
                let label_field: Option<String> = row.try_get("label_field").map_err(db_error)?;
                let legend_items: Value = row.try_get("legend_items").map_err(db_error)?;
                let revision: i32 = row.try_get("revision").map_err(db_error)?;
                let dict = json!({
                    "mode": mode,
                    "style_spec": style_spec,
                    "label_field": label_field,
                    "legend_items": legend_items,
                    "revision": revision,
                });
                let fp = print_revision_fingerprint(&dict);
                (Some(dict), Some(style_id), revision, Some(fp))
            }
            None => (None, None, 0, None),
        };

        let mut transform_operation: Option<String> = None;
        let mut use_transform = false;
        let mut transform_inverse = false;
        if source_srid != layout.crs_code {
            let src_u32 = u32::try_from(source_srid).ok();
            let dst_u32 = u32::try_from(layout.crs_code).ok();
            let profile = match (src_u32, dst_u32) {
                (Some(s), Some(d)) if matches!((s, d), (4326, 3857) | (3857, 4326)) => {
                    reference_transformation(s, d)
                }
                _ => None,
            }
            .filter(|p| {
                !matches!(
                    p.fixture_requirement,
                    ovg_core::crs::FixtureRequirement::QuangNinh
                        | ovg_core::crs::FixtureRequirement::QuangNinhHvs
                ) && p.steps.len() == 1
            });

            let Some(profile) = profile else {
                return Err(AppError::validation(
                    "UNSUPPORTED_TRANSFORMATION",
                    "no approved transformation for this operation; use a supported native CRS",
                ));
            };
            transform_inverse = profile.steps[0].inverse;
            transform_operation = Some(profile.operation_id.to_owned());
            use_transform = true;
            require_web_domain(read, source_id, source_srid).await?;
        }

        let geom_expr = if use_transform {
            if transform_inverse {
                "ST_InverseTransformPipeline(rf.geometry, $8, $9)"
            } else {
                "ST_TransformPipeline(rf.geometry, $8, $9)"
            }
        } else {
            "rf.geometry"
        };

        let sql = format!(
            "SELECT rf.feature_id, rf.attributes,
                    (rf.content_revision = $4 AND rf.schema_revision = $5
                     AND rf.geometry_state = 'present' AND rf.geometry IS NOT NULL
                     AND ST_SRID(rf.geometry) = $6 AND ST_Zmflag(rf.geometry) = 0
                     AND ST_GeometryType(rf.geometry) = $7
                     AND NOT ST_IsEmpty(rf.geometry) AND ST_IsValid(rf.geometry)
                     AND ST_NPoints(rf.geometry) BETWEEN 1 AND 4096) IS TRUE AS valid,
                    ST_AsGeoJSON({geom_expr}, 15)::jsonb AS geom,
                    ST_XMin({geom_expr}) AS x0, ST_YMin({geom_expr}) AS y0,
                    ST_XMax({geom_expr}) AS x1, ST_YMax({geom_expr}) AS y1
             FROM spatial.reference_features rf
             WHERE rf.organization_id = $1 AND rf.project_id = $2
               AND rf.source_id = $3
             ORDER BY rf.feature_id
             LIMIT 2501"
        );
        let mut query = sqlx::query(&sql)
            .bind(read.organization_id())
            .bind(read.project_id())
            .bind(source_id)
            .bind(&content_rev)
            .bind(&schema_rev)
            .bind(source_srid)
            .bind(format!("ST_{geometry_family}"));
        if use_transform {
            query = query.bind(WEB_MERCATOR_PIPELINE).bind(layout.crs_code);
        }
        let frows = query
            .fetch_all(&mut **read.transaction())
            .await
            .map_err(db_error)?;

        if frows.len() > 2500 {
            return Err(AppError::validation(
                "PRINT_FEATURE_LIMIT",
                "print layer exceeds the supported 2500-feature limit; narrow the source",
            ));
        }
        if feature_count != Some(frows.len() as i64) {
            return Err(AppError::validation(
                "INVALID_PRINT_SOURCE",
                "print source does not match its ready revision",
            ));
        }
        let mut features = Vec::with_capacity(frows.len());
        for fr in &frows {
            let valid: bool = fr.try_get("valid").map_err(db_error)?;
            let geom: Option<Value> = fr.try_get("geom").map_err(db_error)?;
            let (true, Some(geom)) = (valid, geom) else {
                return Err(AppError::validation(
                    "INVALID_PRINT_SOURCE",
                    "print source does not match its ready revision",
                ));
            };
            let x0: Option<f64> = fr.try_get("x0").map_err(db_error)?;
            let y0: Option<f64> = fr.try_get("y0").map_err(db_error)?;
            let x1: Option<f64> = fr.try_get("x1").map_err(db_error)?;
            let y1: Option<f64> = fr.try_get("y1").map_err(db_error)?;
            if let (Some(x0), Some(y0), Some(x1), Some(y1)) = (x0, y0, x1, y1) {
                min_x = min_x.min(x0);
                min_y = min_y.min(y0);
                max_x = max_x.max(x1);
                max_y = max_y.max(y1);
            }
            features.push(PrintFeatureRow {
                feature_id: fr.try_get("feature_id").map_err(db_error)?,
                attributes: fr.try_get("attributes").map_err(db_error)?,
                geometry: geom,
            });
        }

        by_id.insert(
            lid_str.clone(),
            PrintLayerSnapshot {
                id: lid,
                name: layer_name,
                layer_revision,
                source_id,
                content_revision: content_rev,
                schema_revision: schema_rev,
                source_crs: source_srid,
                transformation_operation,
                style: style_dict,
                style_id,
                style_revision,
                style_fingerprint,
                features,
            },
        );
    }

    let mut layers = Vec::with_capacity(layer_ids.len());
    for lid_str in &layer_ids {
        if let Some(layer) = by_id.remove(lid_str) {
            layers.push(layer);
        }
    }

    let data_bounds = if min_x < f64::INFINITY {
        Some([min_x, min_y, max_x, max_y])
    } else {
        None
    };
    Ok(PrintSnapshotData {
        layout,
        layers,
        data_bounds,
    })
}

#[derive(Debug, Clone)]
pub struct PrintExportInsert {
    pub id: Uuid,
    pub layout_id: Uuid,
    pub output_format: String,
    pub dpi: i32,
    pub scale_denominator: i32,
    pub scale_accuracy_note: String,
    pub revision_manifest: Value,
    pub storage_key: String,
    pub sha256: String,
    pub byte_size: i64,
    pub created_by_actor_id: Uuid,
    pub created_at: DateTime<Utc>,
}

pub async fn insert_print_export(
    read: &mut ProjectRead<'_>,
    input: PrintExportInsert,
) -> Result<Value, AppError> {
    let row = sqlx::query(
        "INSERT INTO cartography.print_exports (
            id, organization_id, project_id, layout_id, output_format, dpi,
            scale_denominator, scale_accuracy_note, revision_manifest,
            storage_key, sha256, byte_size, created_by_actor_id, created_at
         ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)
         RETURNING id, layout_id, output_format, dpi, scale_denominator,
                   scale_accuracy_note, revision_manifest, sha256, byte_size, created_at",
    )
    .bind(input.id)
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(input.layout_id)
    .bind(input.output_format)
    .bind(input.dpi)
    .bind(input.scale_denominator)
    .bind(input.scale_accuracy_note)
    .bind(input.revision_manifest)
    .bind(input.storage_key)
    .bind(input.sha256)
    .bind(input.byte_size)
    .bind(input.created_by_actor_id)
    .bind(input.created_at)
    .fetch_one(&mut **read.transaction())
    .await
    .map_err(db_error)?;
    export_wire(&row)
}

#[derive(Debug, Clone)]
pub struct PrintExportMeta {
    pub id: Uuid,
    pub output_format: String,
    pub storage_key: String,
    pub sha256: String,
}

pub async fn get_print_export(
    read: &mut ProjectRead<'_>,
    export_id: Uuid,
) -> Result<PrintExportMeta, AppError> {
    let row = sqlx::query(
        "SELECT id, output_format, storage_key, sha256
         FROM cartography.print_exports
         WHERE organization_id = $1 AND project_id = $2 AND id = $3",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(export_id)
    .fetch_optional(&mut **read.transaction())
    .await
    .map_err(db_error)?
    .ok_or_else(|| AppError::not_found("print export not found"))?;
    Ok(PrintExportMeta {
        id: row.try_get("id").map_err(db_error)?,
        output_format: row.try_get("output_format").map_err(db_error)?,
        storage_key: row.try_get("storage_key").map_err(db_error)?,
        sha256: row.try_get("sha256").map_err(db_error)?,
    })
}

// ----- Layout Designer repository (`cartography.designer_layouts`) -----

#[derive(Debug)]
pub enum DesignerStoreError {
    Quota,
    Conflict,
    Database,
}

fn designer_db_error(error: sqlx::Error) -> DesignerStoreError {
    tracing::error!(%error, "designer layout database operation failed");
    DesignerStoreError::Database
}

fn designer_wire(row: &PgRow) -> Result<Value, DesignerStoreError> {
    Ok(json!({
        "id": row.try_get::<Uuid, _>("id").map_err(designer_db_error)?,
        "name": row.try_get::<String, _>("name").map_err(designer_db_error)?,
        "document": row.try_get::<Value, _>("document").map_err(designer_db_error)?,
        "revision": row.try_get::<i32, _>("revision").map_err(designer_db_error)?,
        "created_at": iso(row.try_get::<DateTime<Utc>, _>("created_at").map_err(designer_db_error)?),
        "updated_at": iso(row.try_get::<DateTime<Utc>, _>("updated_at").map_err(designer_db_error)?),
    }))
}

async fn designer_audit(
    edit: &mut ProjectRead<'_>,
    actor_id: Uuid,
    actor_display: &str,
    resource_id: Uuid,
    action: &str,
    name: &str,
) -> Result<(), DesignerStoreError> {
    sqlx::query(
        "INSERT INTO audit.audit_events (
            id, organization_id, actor_type, actor_id, actor_display_snapshot,
            action, resource_type, resource_id, outcome, metadata, created_at
         ) VALUES ($1,$2,'human',$3,$4,$5,'designer_layout',$6,'success',$7,$8)",
    )
    .bind(Uuid::new_v4())
    .bind(edit.organization_id())
    .bind(actor_id)
    .bind(actor_display)
    .bind(action)
    .bind(resource_id)
    .bind(json!({"name": name}))
    .bind(Utc::now())
    .execute(&mut **edit.transaction())
    .await
    .map_err(designer_db_error)?;
    Ok(())
}

pub async fn list_designer_layouts(
    read: &mut ProjectRead<'_>,
) -> Result<Value, DesignerStoreError> {
    let rows = sqlx::query(
        "SELECT id, name, document, revision, created_at, updated_at
         FROM cartography.designer_layouts
         WHERE organization_id = $1 AND project_id = $2
         ORDER BY updated_at DESC, id",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(designer_db_error)?;
    let items = rows
        .iter()
        .map(designer_wire)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({ "items": items }))
}

pub struct DesignerWrite {
    pub name: String,
    pub document: Value,
}

pub async fn create_designer_layout(
    edit: &mut ProjectRead<'_>,
    actor_id: Uuid,
    actor_display: &str,
    input: DesignerWrite,
) -> Result<Value, DesignerStoreError> {
    let lock_key = format!("layout-designer:{}", edit.project_id());
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(lock_key)
        .execute(&mut **edit.transaction())
        .await
        .map_err(designer_db_error)?;
    let count = sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM cartography.designer_layouts
         WHERE organization_id = $1 AND project_id = $2",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .fetch_one(&mut **edit.transaction())
    .await
    .map_err(designer_db_error)?;
    if count >= 50 {
        return Err(DesignerStoreError::Quota);
    }

    let id = Uuid::new_v4();
    let row = sqlx::query(
        "INSERT INTO cartography.designer_layouts (organization_id, project_id, id, name, document)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING id, name, document, revision, created_at, updated_at",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(id)
    .bind(&input.name)
    .bind(input.document)
    .fetch_one(&mut **edit.transaction())
    .await
    .map_err(designer_db_error)?;
    designer_audit(
        edit,
        actor_id,
        actor_display,
        id,
        "cartography.designer.create",
        &input.name,
    )
    .await?;
    designer_wire(&row)
}

pub async fn update_designer_layout(
    edit: &mut ProjectRead<'_>,
    actor_id: Uuid,
    actor_display: &str,
    layout_id: Uuid,
    revision: i32,
    input: DesignerWrite,
) -> Result<Value, DesignerStoreError> {
    let row = sqlx::query(
        "UPDATE cartography.designer_layouts
         SET name = $5, document = $6, revision = revision + 1, updated_at = now()
         WHERE organization_id = $1 AND project_id = $2 AND id = $3 AND revision = $4
         RETURNING id, name, document, revision, created_at, updated_at",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(layout_id)
    .bind(revision)
    .bind(&input.name)
    .bind(input.document)
    .fetch_optional(&mut **edit.transaction())
    .await
    .map_err(designer_db_error)?
    .ok_or(DesignerStoreError::Conflict)?;
    designer_audit(
        edit,
        actor_id,
        actor_display,
        layout_id,
        "cartography.designer.update",
        &input.name,
    )
    .await?;
    designer_wire(&row)
}

pub async fn delete_designer_layout(
    edit: &mut ProjectRead<'_>,
    actor_id: Uuid,
    actor_display: &str,
    layout_id: Uuid,
    revision: i32,
) -> Result<(), DesignerStoreError> {
    let name = sqlx::query_scalar::<_, String>(
        "DELETE FROM cartography.designer_layouts
         WHERE organization_id = $1 AND project_id = $2 AND id = $3 AND revision = $4
         RETURNING name",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(layout_id)
    .bind(revision)
    .fetch_optional(&mut **edit.transaction())
    .await
    .map_err(designer_db_error)?
    .ok_or(DesignerStoreError::Conflict)?;
    designer_audit(
        edit,
        actor_id,
        actor_display,
        layout_id,
        "cartography.designer.delete",
        &name,
    )
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn python_isoformat_uses_explicit_utc_offset() {
        let value = DateTime::parse_from_rfc3339("2026-11-26T01:02:03+00:00")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(iso(value), "2026-11-26T01:02:03+00:00");
        let with_micros = DateTime::parse_from_rfc3339("2026-11-26T01:02:03.123000+00:00")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(iso(with_micros), "2026-11-26T01:02:03.123000+00:00");
    }

    #[test]
    fn print_fingerprint_matches_python_ensure_ascii_false() {
        let mut out = String::new();
        push_canonical_json_utf8(
            &mut out,
            &json!({"b": [1, 2.0, "Đất\n"], "a": null, "c": true}),
        );
        assert_eq!(out, r#"{"a":null,"b":[1,2.0,"Đất\n"],"c":true}"#);
        // python: hashlib.sha256(json.dumps({"b":[1,2.0,"Đất\n"],"a":None,"c":True},sort_keys=True,separators=(",",":"),ensure_ascii=False).encode()).hexdigest()
        let digest = print_revision_fingerprint(&json!({"a": 1}));
        assert_eq!(
            digest,
            "015abd7f5cc57a2dd94b7590f04ad8084273905ee33ec5cebeae62276a97f862"
        );
    }
}
