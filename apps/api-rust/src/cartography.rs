//! E06 renderer-neutral cartography control plane.
//!
//! MapLibre/QGIS/renderer engines stay below this boundary. Rust owns validation
//! of portable style/layout contracts and canonical project persistence.

use std::collections::{BTreeMap, HashSet};

use axum::extract::rejection::JsonRejection;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use ovg_core::AppError;
use ovg_storage::cartography as cartography_store;
use ovg_storage::catalog::ProjectReadPolicy;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use uuid::Uuid;

use crate::catalog::{project_read, CatalogError};
use crate::identity::{require_csrf, HumanSession};
use crate::AppState;

const BASE: &str = "/organizations/{organization_id}/projects/{project_id}/cartography";

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route(
            &format!("{BASE}/layers/{{layer_id}}/style"),
            get(get_layer_style).put(upsert_layer_style),
        )
        .route(
            &format!("{BASE}/layouts"),
            get(list_layouts).post(create_layout),
        )
        .route(&format!("{BASE}/exports"), get(list_exports))
}

async fn get_layer_style(
    State(state): State<AppState>,
    auth: HumanSession,
    Path((organization_id, project_id, layer_id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<Json<Value>, CatalogError> {
    let mut read = project_read(
        &state,
        &auth,
        organization_id,
        project_id,
        ProjectReadPolicy::Unrestricted,
    )
    .await?;
    let result = cartography_store::get_layer_style(&mut read, layer_id)
        .await
        .map_err(CatalogError::Application)?;
    read.commit().await.map_err(CatalogError::Application)?;
    Ok(Json(result))
}

async fn list_layouts(
    State(state): State<AppState>,
    auth: HumanSession,
    Path((organization_id, project_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>, CatalogError> {
    let mut read = project_read(
        &state,
        &auth,
        organization_id,
        project_id,
        ProjectReadPolicy::Unrestricted,
    )
    .await?;
    let result = cartography_store::list_layouts(&mut read)
        .await
        .map_err(CatalogError::Application)?;
    read.commit().await.map_err(CatalogError::Application)?;
    Ok(Json(result))
}

async fn list_exports(
    State(state): State<AppState>,
    auth: HumanSession,
    Path((organization_id, project_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>, CatalogError> {
    let mut read = project_read(
        &state,
        &auth,
        organization_id,
        project_id,
        ProjectReadPolicy::Unrestricted,
    )
    .await?;
    let result = cartography_store::list_exports(&mut read)
        .await
        .map_err(CatalogError::Application)?;
    read.commit().await.map_err(CatalogError::Application)?;
    Ok(Json(result))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
enum StyleMode {
    Single,
    Categorical,
    Range,
}

impl StyleMode {
    const fn wire(self) -> &'static str {
        match self {
            Self::Single => "single",
            Self::Categorical => "categorical",
            Self::Range => "range",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct UpsertStyleRequest {
    mode: StyleMode,
    style_spec: Map<String, Value>,
    #[serde(default)]
    label_field: Option<String>,
}

async fn upsert_layer_style(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path((organization_id, project_id, layer_id)): Path<(Uuid, Uuid, Uuid)>,
    body: Result<Json<UpsertStyleRequest>, JsonRejection>,
) -> Result<Json<Value>, CartographyError> {
    let Json(body) = body.map_err(|_| CartographyError::InvalidRequest)?;
    require_csrf(&state, &auth, &headers)
        .map_err(|error| CartographyError::Access(CatalogError::Identity(error)))?;
    let mut edit = project_read(
        &state,
        &auth,
        organization_id,
        project_id,
        ProjectReadPolicy::UnrestrictedEdit,
    )
    .await
    .map_err(CartographyError::Access)?;

    let target = cartography_store::style_target(&mut edit, layer_id)
        .await
        .map_err(CartographyError::Domain)?;
    let (style_spec, legend_items) =
        validate_and_build_legend(body.mode, &body.style_spec, &target.name)
            .map_err(CartographyError::Domain)?;

    let result = cartography_store::upsert_layer_style(
        &mut edit,
        cartography_store::StyleInsert {
            actor_id: auth.actor_id(),
            layer_id,
            source_id: target.source_id,
            mode: body.mode.wire().to_owned(),
            style_spec,
            label_field: body.label_field,
            legend_items,
        },
    )
    .await
    .map_err(CartographyError::Domain)?;
    edit.commit().await.map_err(CartographyError::Domain)?;
    Ok(Json(result))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
enum PaperSize {
    A4,
    A3,
    A2,
    A1,
    A0,
}

impl PaperSize {
    const fn wire(self) -> &'static str {
        match self {
            Self::A4 => "A4",
            Self::A3 => "A3",
            Self::A2 => "A2",
            Self::A1 => "A1",
            Self::A0 => "A0",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Orientation {
    Portrait,
    Landscape,
}

impl Orientation {
    const fn wire(self) -> &'static str {
        match self {
            Self::Portrait => "portrait",
            Self::Landscape => "landscape",
        }
    }
}

fn default_paper() -> PaperSize {
    PaperSize::A4
}

fn default_orientation() -> Orientation {
    Orientation::Landscape
}

const fn default_crs() -> i32 {
    4326
}

fn default_attribution() -> String {
    "OpenVietGIS Project Data".to_owned()
}

const fn yes() -> bool {
    true
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateLayoutRequest {
    name: String,
    title: String,
    #[serde(default = "default_paper")]
    paper_size: PaperSize,
    #[serde(default = "default_orientation")]
    orientation: Orientation,
    #[serde(default = "default_crs")]
    crs_code: i32,
    #[serde(default = "default_attribution")]
    attribution: String,
    layer_ids: Vec<Uuid>,
    #[serde(default = "yes")]
    show_north_arrow: bool,
    #[serde(default = "yes")]
    show_scale_bar: bool,
    #[serde(default = "yes")]
    show_legend: bool,
    #[serde(default)]
    extent: Option<BTreeMap<String, f64>>,
}

async fn create_layout(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path((organization_id, project_id)): Path<(Uuid, Uuid)>,
    body: Result<Json<CreateLayoutRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<Value>), CartographyError> {
    let Json(body) = body.map_err(|_| CartographyError::InvalidRequest)?;
    require_csrf(&state, &auth, &headers)
        .map_err(|error| CartographyError::Access(CatalogError::Identity(error)))?;

    validate_layout_request(&body).map_err(CartographyError::Domain)?;
    let extent = body
        .extent
        .as_ref()
        .map(|extent| serde_json::to_value(extent).expect("finite layout extent"));

    let mut edit = project_read(
        &state,
        &auth,
        organization_id,
        project_id,
        ProjectReadPolicy::UnrestrictedEdit,
    )
    .await
    .map_err(CartographyError::Access)?;
    let result = cartography_store::create_layout(
        &mut edit,
        auth.actor_id(),
        cartography_store::LayoutInsert {
            name: normalized_text(&body.name, 200, "Print Layout"),
            title: normalized_text(&body.title, 200, "OpenVietGIS Map"),
            paper_size: body.paper_size.wire().to_owned(),
            orientation: body.orientation.wire().to_owned(),
            crs_code: body.crs_code,
            attribution: normalized_text(&body.attribution, 300, "OpenVietGIS Project Data"),
            show_north_arrow: body.show_north_arrow,
            show_scale_bar: body.show_scale_bar,
            show_legend: body.show_legend,
            layer_ids: body.layer_ids,
            extent,
        },
    )
    .await
    .map_err(CartographyError::Domain)?;
    edit.commit().await.map_err(CartographyError::Domain)?;
    Ok((StatusCode::CREATED, Json(result)))
}

fn normalized_text(value: &str, max_chars: usize, fallback: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return fallback.to_owned();
    }
    trimmed.chars().take(max_chars).collect()
}

fn validate_layout_request(body: &CreateLayoutRequest) -> Result<(), AppError> {
    if !matches!(body.crs_code, 4326 | 3857 | 4756 | 5899) {
        return Err(AppError::validation(
            "UNSUPPORTED_CRS",
            format!("unsupported layout CRS {}", body.crs_code),
        ));
    }
    if body.layer_ids.is_empty() {
        return Err(AppError::validation(
            "EMPTY_LAYOUT",
            "layout requires at least one layer",
        ));
    }
    let unique: HashSet<_> = body.layer_ids.iter().copied().collect();
    if unique.len() != body.layer_ids.len() || body.layer_ids.len() > 20 {
        return Err(AppError::validation(
            "LAYOUT_LAYER_LIMIT",
            "layout requires 1..20 unique layers",
        ));
    }
    if let Some(extent) = &body.extent {
        validate_print_extent(extent, body.crs_code)?;
    }
    Ok(())
}

fn validate_print_extent(extent: &BTreeMap<String, f64>, crs_code: i32) -> Result<(), AppError> {
    const KEYS: [&str; 4] = ["max_x", "max_y", "min_x", "min_y"];
    if extent.len() != 4 || !KEYS.iter().all(|key| extent.contains_key(*key)) {
        return Err(AppError::validation(
            "INVALID_PRINT_EXTENT",
            "invalid print extent",
        ));
    }
    let min_x = extent["min_x"];
    let min_y = extent["min_y"];
    let max_x = extent["max_x"];
    let max_y = extent["max_y"];
    let values = [min_x, min_y, max_x, max_y];
    let invalid = values
        .iter()
        .any(|value| !value.is_finite() || value.abs() > 1e9)
        || min_x >= max_x
        || min_y >= max_y
        || (matches!(crs_code, 4326 | 4756)
            && !(-180.0 <= min_x && max_x <= 180.0 && -90.0 <= min_y && max_y <= 90.0))
        || (crs_code == 3857 && values.iter().any(|value| value.abs() > 20_037_509.0));
    if invalid {
        return Err(AppError::validation(
            "INVALID_PRINT_EXTENT",
            "invalid print extent",
        ));
    }
    Ok(())
}

fn check_color(value: Option<&Value>, default: &str) -> Result<String, AppError> {
    let value = match value {
        None | Some(Value::Null) => default.to_owned(),
        Some(Value::String(value)) => value.clone(),
        Some(other) => pythonish_str(other),
    };
    validate_hex_color(&value)?;
    Ok(value)
}

fn as_float(value: &Value) -> Result<f64, AppError> {
    let parsed = match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.parse::<f64>().ok(),
        Value::Bool(value) => Some(if *value { 1.0 } else { 0.0 }),
        _ => None,
    }
    .filter(|value| value.is_finite())
    .ok_or_else(|| AppError::validation("INVALID_STYLE", "invalid range value"))?;
    Ok(parsed)
}

fn python_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(number) => number.as_f64().is_some_and(|value| value != 0.0),
        Value::String(text) => !text.is_empty(),
        Value::Array(items) => !items.is_empty(),
        Value::Object(object) => !object.is_empty(),
    }
}

fn pythonish_str(value: &Value) -> String {
    match value {
        Value::Null => "None".to_owned(),
        Value::Bool(true) => "True".to_owned(),
        Value::Bool(false) => "False".to_owned(),
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        Value::Array(items) => {
            let body = items
                .iter()
                .map(pythonish_repr)
                .collect::<Vec<_>>()
                .join(", ");
            format!("[{body}]")
        }
        Value::Object(object) => {
            let body = object
                .iter()
                .map(|(key, value)| {
                    format!("{}: {}", python_string_repr(key), pythonish_repr(value))
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("{{{body}}}")
        }
    }
}

fn pythonish_repr(value: &Value) -> String {
    match value {
        Value::String(text) => python_string_repr(text),
        _ => pythonish_str(value),
    }
}

fn python_string_repr(text: &str) -> String {
    format!("'{}'", text.replace('\\', "\\\\").replace('\'', "\\'"))
}

fn python_g(value: f64) -> String {
    if value == 0.0 {
        return "0".to_owned();
    }
    let abs = value.abs();
    let exponent = abs.log10().floor() as i32;
    if !(-4..6).contains(&exponent) {
        let raw = format!("{value:.5e}");
        let (mantissa, exp) = raw.split_once('e').expect("scientific format");
        let mantissa = mantissa.trim_end_matches('0').trim_end_matches('.');
        let exp_value = exp.parse::<i32>().expect("scientific exponent");
        return format!("{mantissa}e{exp_value:+03}");
    }
    let decimals = (5 - exponent).max(0) as usize;
    let raw = format!("{value:.decimals$}");
    raw.trim_end_matches('0').trim_end_matches('.').to_owned()
}

fn validate_and_build_legend(
    mode: StyleMode,
    style_spec: &Map<String, Value>,
    layer_name: &str,
) -> Result<(Value, Value), AppError> {
    match mode {
        StyleMode::Single => {
            let fill = check_color(style_spec.get("fill_color"), "#2563eb")?;
            let stroke = check_color(style_spec.get("stroke_color"), "#1e3a8a")?;
            let width = style_spec
                .get("stroke_width")
                .map(as_float)
                .transpose()?
                .unwrap_or(1.5);
            let opacity = style_spec
                .get("opacity")
                .map(as_float)
                .transpose()?
                .unwrap_or(0.75);
            validate_single_style(&fill, &stroke, width, opacity)?;
            Ok((
                json!({
                    "fill_color": fill,
                    "stroke_color": stroke,
                    "stroke_width": width,
                    "opacity": opacity,
                }),
                json!([{
                    "label": layer_name,
                    "fill_color": fill,
                    "stroke_color": stroke,
                }]),
            ))
        }
        StyleMode::Categorical => {
            let field = style_spec
                .get("field")
                .filter(|value| python_truthy(value))
                .map(pythonish_str)
                .unwrap_or_default()
                .trim()
                .to_owned();
            let categories = style_spec
                .get("categories")
                .and_then(Value::as_array)
                .filter(|items| !items.is_empty())
                .ok_or_else(|| {
                    AppError::validation(
                        "INVALID_STYLE",
                        "categorical style requires field and non-empty categories",
                    )
                })?;
            if field.is_empty() {
                return Err(AppError::validation(
                    "INVALID_STYLE",
                    "categorical style requires field and non-empty categories",
                ));
            }
            let mut clean = Vec::with_capacity(categories.len());
            let mut legend = Vec::with_capacity(categories.len());
            for (idx, item) in categories.iter().enumerate() {
                let object = item
                    .as_object()
                    .filter(|object| object.contains_key("value"))
                    .ok_or_else(|| {
                        AppError::validation(
                            "INVALID_STYLE",
                            format!("category #{idx} requires value"),
                        )
                    })?;
                let color = check_color(object.get("color"), "#1f78b4")?;
                let value = object["value"].clone();
                let label = object
                    .get("label")
                    .filter(|value| python_truthy(value))
                    .map(pythonish_str)
                    .unwrap_or_else(|| pythonish_str(&value));
                clean.push(json!({"value": value, "color": color, "label": label}));
                legend.push(json!({
                    "label": label,
                    "fill_color": color,
                    "stroke_color": "#111827",
                }));
            }
            let fallback = check_color(style_spec.get("default_color"), "#9ca3af")?;
            Ok((
                json!({"field": field, "categories": clean, "default_color": fallback}),
                Value::Array(legend),
            ))
        }
        StyleMode::Range => {
            let field = style_spec
                .get("field")
                .filter(|value| python_truthy(value))
                .map(pythonish_str)
                .unwrap_or_default()
                .trim()
                .to_owned();
            let breaks = style_spec
                .get("breaks")
                .and_then(Value::as_array)
                .filter(|items| !items.is_empty())
                .ok_or_else(|| {
                    AppError::validation(
                        "INVALID_STYLE",
                        "range style requires field and non-empty breaks",
                    )
                })?;
            if field.is_empty() {
                return Err(AppError::validation(
                    "INVALID_STYLE",
                    "range style requires field and non-empty breaks",
                ));
            }
            let mut clean = Vec::with_capacity(breaks.len());
            let mut legend = Vec::with_capacity(breaks.len());
            for (idx, item) in breaks.iter().enumerate() {
                let object = item
                    .as_object()
                    .filter(|object| object.contains_key("min") && object.contains_key("max"))
                    .ok_or_else(|| {
                        AppError::validation(
                            "INVALID_STYLE",
                            format!("range break #{idx} requires min and max"),
                        )
                    })?;
                let lo = as_float(&object["min"])?;
                let hi = as_float(&object["max"])?;
                if lo > hi {
                    return Err(AppError::validation(
                        "INVALID_STYLE",
                        "range break min > max",
                    ));
                }
                let color = check_color(object.get("color"), "#1f78b4")?;
                let label = object
                    .get("label")
                    .filter(|value| python_truthy(value))
                    .map(pythonish_str)
                    .unwrap_or_else(|| format!("{} – {}", python_g(lo), python_g(hi)));
                clean.push(json!({"min": lo, "max": hi, "color": color, "label": label}));
                legend.push(json!({
                    "label": label,
                    "fill_color": color,
                    "stroke_color": "#111827",
                }));
            }
            Ok((
                json!({"field": field, "breaks": clean}),
                Value::Array(legend),
            ))
        }
    }
}

pub fn validate_hex_color(value: &str) -> Result<&str, AppError> {
    let bytes = value.as_bytes();
    if bytes.len() == 7 && bytes[0] == b'#' && bytes[1..].iter().all(u8::is_ascii_hexdigit) {
        Ok(value)
    } else {
        Err(AppError::validation(
            "INVALID_STYLE",
            format!("invalid hex color {value:?}"),
        ))
    }
}

pub fn validate_single_style(
    fill_color: &str,
    stroke_color: &str,
    stroke_width: f64,
    opacity: f64,
) -> Result<(), AppError> {
    validate_hex_color(fill_color)?;
    validate_hex_color(stroke_color)?;
    if !stroke_width.is_finite()
        || !opacity.is_finite()
        || !(0.5..=12.0).contains(&stroke_width)
        || !(0.0..=1.0).contains(&opacity)
    {
        return Err(AppError::validation(
            "INVALID_STYLE",
            "invalid opacity or stroke_width",
        ));
    }
    Ok(())
}

#[cfg(test)]
fn paper_dimensions_px(
    paper_size: PaperSize,
    orientation: Orientation,
    dpi: u16,
) -> Result<(u32, u32), AppError> {
    if !matches!(dpi, 150 | 300) {
        return Err(AppError::validation(
            "INVALID_DPI",
            "dpi must be 150 or 300",
        ));
    }
    if matches!(paper_size, PaperSize::A0 | PaperSize::A1 | PaperSize::A2) && dpi != 150 {
        return Err(AppError::validation(
            "INVALID_DPI",
            "Large paper formats require 150 dpi",
        ));
    }

    let (mut mm_w, mut mm_h) = match paper_size {
        PaperSize::A4 => (210.0_f64, 297.0_f64),
        PaperSize::A3 => (297.0, 420.0),
        PaperSize::A2 => (420.0, 594.0),
        PaperSize::A1 => (594.0, 841.0),
        PaperSize::A0 => (841.0, 1189.0),
    };
    if orientation == Orientation::Landscape {
        std::mem::swap(&mut mm_w, &mut mm_h);
    }
    let px_w = ((mm_w / 25.4) * f64::from(dpi)).round() as u32;
    let px_h = ((mm_h / 25.4) * f64::from(dpi)).round() as u32;
    Ok((px_w, px_h))
}

enum CartographyError {
    InvalidRequest,
    Access(CatalogError),
    Domain(AppError),
}

impl IntoResponse for CartographyError {
    fn into_response(self) -> Response {
        match self {
            Self::InvalidRequest => CatalogError::InvalidRequest.into_response(),
            Self::Access(error) => error.into_response(),
            Self::Domain(AppError::Internal { internal_detail }) => {
                tracing::error!(detail = %internal_detail, "cartography request failed");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response()
            }
            Self::Domain(error) => {
                let status = StatusCode::from_u16(error.status_code())
                    .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
                (status, Json(error.to_envelope(None))).into_response()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn print_dimensions_match_python_physical_page_contract() {
        assert_eq!(
            paper_dimensions_px(PaperSize::A4, Orientation::Portrait, 300).unwrap(),
            (2480, 3508)
        );
        assert_eq!(
            paper_dimensions_px(PaperSize::A4, Orientation::Landscape, 300).unwrap(),
            (3508, 2480)
        );
        assert!(paper_dimensions_px(PaperSize::A2, Orientation::Portrait, 300).is_err());
        assert!(paper_dimensions_px(PaperSize::A0, Orientation::Portrait, 150).is_ok());
    }

    #[test]
    fn all_style_modes_match_reference_normalization() {
        let single = serde_json::from_value::<Map<String, Value>>(json!({})).unwrap();
        let (spec, legend) =
            validate_and_build_legend(StyleMode::Single, &single, "Layer A").unwrap();
        assert_eq!(spec["fill_color"], "#2563eb");
        assert_eq!(legend[0]["label"], "Layer A");

        let categorical = serde_json::from_value::<Map<String, Value>>(json!({
            "field": "kind",
            "categories": [
                {"value": "road", "color": "#112233"},
                {"value": 2, "label": "Two"}
            ]
        }))
        .unwrap();
        let (spec, legend) =
            validate_and_build_legend(StyleMode::Categorical, &categorical, "x").unwrap();
        assert_eq!(spec["categories"][0]["label"], "road");
        assert_eq!(spec["categories"][1]["color"], "#1f78b4");
        assert_eq!(legend.as_array().unwrap().len(), 2);

        let range = serde_json::from_value::<Map<String, Value>>(json!({
            "field": "height",
            "breaks": [
                {"min": 0, "max": 12.5, "color": "#445566"},
                {"min": 1000000, "max": 2000000}
            ]
        }))
        .unwrap();
        let (spec, _) = validate_and_build_legend(StyleMode::Range, &range, "x").unwrap();
        assert_eq!(spec["breaks"][0]["label"], "0 – 12.5");
        assert_eq!(spec["breaks"][1]["label"], "1e+06 – 2e+06");
    }

    #[test]
    fn styles_and_extents_fail_closed() {
        assert!(validate_single_style("#2563eb", "#1e3a8a", 1.5, 0.75).is_ok());
        for bad in ["#123", "2563eb", "#GG0000", "#1234567"] {
            assert!(validate_hex_color(bad).is_err(), "{bad}");
        }
        assert!(validate_single_style("#2563eb", "#1e3a8a", f64::NAN, 0.5).is_err());
        assert!(validate_single_style("#2563eb", "#1e3a8a", 1.0, f64::INFINITY).is_err());

        let valid = BTreeMap::from([
            ("min_x".to_owned(), 105.0),
            ("min_y".to_owned(), 10.0),
            ("max_x".to_owned(), 106.0),
            ("max_y".to_owned(), 11.0),
        ]);
        assert!(validate_print_extent(&valid, 4326).is_ok());
        let mut invalid = valid.clone();
        invalid.insert("max_x".to_owned(), 181.0);
        assert!(validate_print_extent(&invalid, 4326).is_err());
    }
}
