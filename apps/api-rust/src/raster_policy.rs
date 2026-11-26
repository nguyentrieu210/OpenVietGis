//! E09 Rust-native raster control-plane validation.
//!
//! GDAL/PROJ remain worker engines. This module owns bounded OpenVietGIS request
//! semantics before any raster bytes or worker process are touched.

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get};
use axum::{Json, Router};
use ovg_core::AppError;
use ovg_storage::catalog::ProjectReadPolicy;
use ovg_storage::raster;
use serde_json::Value;
use uuid::Uuid;

use crate::catalog::{project_read, CatalogError};
use crate::identity::{require_csrf, HumanSession};
use crate::AppState;

const BASE: &str = "/organizations/{organization_id}/projects/{project_id}/raster";

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route(&format!("{BASE}/layers"), get(list_raster_layers))
        .route(
            &format!("{BASE}/layers/{{layer_id}}"),
            delete(delete_raster_layer),
        )
        .route(
            &format!("{BASE}/layers/{{layer_id}}/tiles/{{z}}/{{x}}/{{tile}}"),
            get(raster_tile),
        )
        .route(
            &format!("{BASE}/layers/{{layer_id}}/file"),
            get(raster_file),
        )
}

async fn list_raster_layers(
    State(state): State<AppState>,
    auth: HumanSession,
    Path((organization_id, project_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>, CatalogError> {
    // The Python raster_scope rejects GIS-scoped actors; unrestricted read
    // reproduces that fail-closed boundary before raster table access.
    let mut read = project_read(
        &state,
        &auth,
        organization_id,
        project_id,
        ProjectReadPolicy::Unrestricted,
    )
    .await?;
    let result = raster::list_layers(&mut read)
        .await
        .map_err(CatalogError::Application)?;
    read.commit().await.map_err(CatalogError::Application)?;
    Ok(Json(result))
}

fn no_cache_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    headers
}

async fn raster_tile(
    State(state): State<AppState>,
    auth: HumanSession,
    Path((organization_id, project_id, layer_id, z, x, tile)): Path<(
        Uuid,
        Uuid,
        Uuid,
        i32,
        i32,
        String,
    )>,
) -> Result<Response, RasterHttpError> {
    let y = tile
        .strip_suffix(".png")
        .and_then(|value| value.parse::<i32>().ok())
        .ok_or(RasterHttpError::NotFound("Tile not found"))?;
    if !(0..=20).contains(&z) {
        return Err(RasterHttpError::NotFound("Tile not found"));
    }
    let limit = 1_i32 << z;
    if x < 0 || y < 0 || x >= limit || y >= limit {
        return Err(RasterHttpError::NotFound("Tile not found"));
    }

    let mut read = project_read(
        &state,
        &auth,
        organization_id,
        project_id,
        ProjectReadPolicy::Unrestricted,
    )
    .await
    .map_err(RasterHttpError::Access)?;
    let png = raster::tile(&mut read, layer_id, z, x, y)
        .await
        .map_err(RasterHttpError::Domain)?;
    read.commit().await.map_err(RasterHttpError::Domain)?;
    let Some(png) = png else {
        return Err(RasterHttpError::NotFound("Tile not found"));
    };

    let mut response = Response::new(Body::from(png));
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static("image/png"));
    for (name, value) in no_cache_headers() {
        if let Some(name) = name {
            response.headers_mut().insert(name, value);
        }
    }
    Ok(response)
}

fn parse_raster_byte_range(header: &str, size: i64) -> Option<(i64, i64)> {
    if size <= 0 || !header.starts_with("bytes=") {
        return None;
    }
    let raw = &header[6..];
    if raw.contains(',') {
        return None;
    }
    let (first, last) = raw.split_once('-')?;
    if first.len() > 19
        || last.len() > 19
        || (!first.is_empty() && !first.bytes().all(|b| b.is_ascii_digit()))
        || (!last.is_empty() && !last.bytes().all(|b| b.is_ascii_digit()))
        || (first.is_empty() && last.is_empty())
    {
        return None;
    }
    if first.is_empty() {
        let suffix = last.parse::<i64>().ok()?;
        if suffix == 0 {
            return None;
        }
        return Some(((size - suffix).max(0), size - 1));
    }
    let start = first.parse::<i64>().ok()?;
    let end = if last.is_empty() {
        size - 1
    } else {
        (last.parse::<i64>().ok()?).min(size - 1)
    };
    (start <= end).then_some((start, end))
}

async fn raster_file(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path((organization_id, project_id, layer_id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<Response, RasterHttpError> {
    let mut read = project_read(
        &state,
        &auth,
        organization_id,
        project_id,
        ProjectReadPolicy::Unrestricted,
    )
    .await
    .map_err(RasterHttpError::Access)?;

    let mut response_headers = no_cache_headers();
    response_headers.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    response_headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"raster-{layer_id}.tif\""))
            .map_err(|_| RasterHttpError::Domain(AppError::internal("invalid raster filename")))?,
    );

    if let Some(range) = headers.get(header::RANGE) {
        let range = range
            .to_str()
            .map_err(|_| RasterHttpError::RangeNotSatisfiable(None, response_headers.clone()))?;
        let size = raster::file_size(&mut read, layer_id)
            .await
            .map_err(RasterHttpError::Domain)?;
        let Some(size) = size else {
            read.commit().await.map_err(RasterHttpError::Domain)?;
            return Err(RasterHttpError::NotFound("Raster layer not found"));
        };
        let Some((start, end)) = parse_raster_byte_range(range, size) else {
            read.commit().await.map_err(RasterHttpError::Domain)?;
            return Err(RasterHttpError::RangeNotSatisfiable(
                Some(size),
                response_headers,
            ));
        };
        let data = raster::file_slice(&mut read, layer_id, start + 1, end - start + 1)
            .await
            .map_err(RasterHttpError::Domain)?
            .ok_or(RasterHttpError::NotFound("Raster layer not found"))?;
        read.commit().await.map_err(RasterHttpError::Domain)?;
        response_headers.insert(
            header::CONTENT_RANGE,
            HeaderValue::from_str(&format!("bytes {start}-{end}/{size}")).map_err(|_| {
                RasterHttpError::Domain(AppError::internal("invalid content range"))
            })?,
        );
        response_headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("image/tiff"));
        let mut response = Response::new(Body::from(data));
        *response.status_mut() = StatusCode::PARTIAL_CONTENT;
        *response.headers_mut() = response_headers;
        return Ok(response);
    }

    let data = raster::file(&mut read, layer_id)
        .await
        .map_err(RasterHttpError::Domain)?;
    read.commit().await.map_err(RasterHttpError::Domain)?;
    let Some(data) = data else {
        return Err(RasterHttpError::NotFound("Raster layer not found"));
    };
    response_headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("image/tiff"));
    let mut response = Response::new(Body::from(data));
    *response.headers_mut() = response_headers;
    Ok(response)
}

async fn delete_raster_layer(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path((organization_id, project_id, layer_id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<StatusCode, RasterHttpError> {
    require_csrf(&state, &auth, &headers)
        .map_err(|error| RasterHttpError::Access(CatalogError::Identity(error)))?;
    let mut edit = project_read(
        &state,
        &auth,
        organization_id,
        project_id,
        ProjectReadPolicy::UnrestrictedEdit,
    )
    .await
    .map_err(RasterHttpError::Access)?;
    let deleted = raster::delete_layer(&mut edit, auth.actor_id(), auth.display_name(), layer_id)
        .await
        .map_err(RasterHttpError::Domain)?;
    if deleted.is_none() {
        edit.commit().await.map_err(RasterHttpError::Domain)?;
        return Err(RasterHttpError::NotFound("Raster layer not found"));
    }
    edit.commit().await.map_err(RasterHttpError::Domain)?;
    Ok(StatusCode::NO_CONTENT)
}

enum RasterHttpError {
    Access(CatalogError),
    Domain(AppError),
    NotFound(&'static str),
    RangeNotSatisfiable(Option<i64>, HeaderMap),
}

impl IntoResponse for RasterHttpError {
    fn into_response(self) -> Response {
        match self {
            Self::Access(error) => error.into_response(),
            Self::NotFound(detail) => (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"detail": detail})),
            )
                .into_response(),
            Self::RangeNotSatisfiable(size, mut headers) => {
                if let Some(size) = size {
                    if let Ok(value) = HeaderValue::from_str(&format!("bytes */{size}")) {
                        headers.insert(header::CONTENT_RANGE, value);
                    }
                }
                let mut response = Response::new(Body::empty());
                *response.status_mut() = StatusCode::RANGE_NOT_SATISFIABLE;
                *response.headers_mut() = headers;
                response
            }
            Self::Domain(AppError::Internal { internal_detail }) => {
                tracing::error!(detail = %internal_detail, "raster request failed");
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

pub const ANALYZE_UPLOAD_LIMIT: u64 = 32 * 1024 * 1024;
pub const PERSIST_UPLOAD_LIMIT: u64 = 64 * 1024 * 1024;
pub const PROJECT_RASTER_LIMIT: u64 = 512 * 1024 * 1024;
pub const PROJECT_LAYER_LIMIT: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RasterMethod {
    Band,
    Ndvi,
    Ndwi,
    Ndbi,
    Hillshade,
    Calculator,
    Rgb,
    Clip,
    Change,
    Slope,
    Aspect,
    Mosaic,
    Resample,
}

#[derive(Debug, Clone, Copy)]
pub struct StoreIntent<'a> {
    pub method: RasterMethod,
    pub name: &'a str,
    pub primary_file: bool,
    pub primary_id: bool,
    pub second_file: bool,
    pub second_id: bool,
    pub third_file: bool,
    pub third_id: bool,
    pub formula: bool,
    pub clip_geojson: bool,
    pub factor: Option<f64>,
    pub resampling: bool,
    pub sentinel_scene: bool,
    pub sentinel_baseline: bool,
    pub band_a: u8,
    pub band_b: u8,
}

fn invalid(message: &'static str) -> AppError {
    AppError::validation("INVALID_RASTER_REQUEST", message)
}

pub fn validate_store_intent(intent: StoreIntent<'_>) -> Result<(), AppError> {
    if intent.name.trim().is_empty() || intent.name.chars().count() > 160 {
        return Err(invalid("Raster name is required"));
    }
    if intent.primary_file == intent.primary_id {
        return Err(invalid("Provide exactly one source file or project raster"));
    }
    if (intent.second_file && intent.second_id) || (intent.third_file && intent.third_id) {
        return Err(invalid("Choose one source per input"));
    }

    let has_second = intent.second_file || intent.second_id;
    let has_third = intent.third_file || intent.third_id;
    if has_second
        && !matches!(
            intent.method,
            RasterMethod::Ndvi
                | RasterMethod::Ndwi
                | RasterMethod::Ndbi
                | RasterMethod::Calculator
                | RasterMethod::Rgb
                | RasterMethod::Change
                | RasterMethod::Mosaic
        )
    {
        return Err(invalid("Second raster is not used by this operation"));
    }
    if has_third && intent.method != RasterMethod::Rgb {
        return Err(invalid("Third raster is only for RGB"));
    }
    if intent.formula != (intent.method == RasterMethod::Calculator)
        || intent.clip_geojson != (intent.method == RasterMethod::Clip)
    {
        return Err(invalid(
            "Formula or clip geometry is required only for its corresponding operation",
        ));
    }
    if intent.method == RasterMethod::Mosaic && !has_second {
        return Err(invalid("Mosaic requires a second raster"));
    }

    let has_factor = intent.factor.is_some();
    if has_factor != (intent.method == RasterMethod::Resample)
        || intent.resampling != (intent.method == RasterMethod::Resample)
    {
        return Err(invalid(
            "Resampling factor and method are required only for resample",
        ));
    }
    if let Some(factor) = intent.factor {
        if !factor.is_finite() || !(0.1..=10.0).contains(&factor) {
            return Err(invalid("Invalid resampling factor"));
        }
    }

    if intent.sentinel_scene != intent.sentinel_baseline {
        return Err(invalid(
            "Sentinel scene and processing baseline are required",
        ));
    }
    if intent.sentinel_scene
        && (!matches!(
            intent.method,
            RasterMethod::Ndvi | RasterMethod::Ndwi | RasterMethod::Ndbi
        ) || !intent.primary_file
            || has_second
            || intent.band_a != 1
            || intent.band_b != 2)
    {
        return Err(invalid(
            "Sentinel analysis uses a three-band spectral crop A/B/SCL",
        ));
    }
    if !(1..=16).contains(&intent.band_a) || !(1..=16).contains(&intent.band_b) {
        return Err(invalid("Raster band must be between 1 and 16"));
    }
    Ok(())
}

pub fn valid_geotiff_magic(magic: [u8; 4]) -> bool {
    matches!(
        magic,
        [b'I', b'I', b'*', 0]
            | [b'M', b'M', 0, b'*']
            | [b'I', b'I', b'+', 0]
            | [b'M', b'M', 0, b'+']
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base(method: RasterMethod) -> StoreIntent<'static> {
        StoreIntent {
            method,
            name: "Raster",
            primary_file: true,
            primary_id: false,
            second_file: false,
            second_id: false,
            third_file: false,
            third_id: false,
            formula: false,
            clip_geojson: false,
            factor: None,
            resampling: false,
            sentinel_scene: false,
            sentinel_baseline: false,
            band_a: 1,
            band_b: 2,
        }
    }

    #[test]
    fn byte_range_parser_matches_python_contract() {
        assert_eq!(parse_raster_byte_range("bytes=0-3", 10), Some((0, 3)));
        assert_eq!(parse_raster_byte_range("bytes=4-", 10), Some((4, 9)));
        assert_eq!(parse_raster_byte_range("bytes=-4", 10), Some((6, 9)));
        assert_eq!(parse_raster_byte_range("bytes=0-999", 10), Some((0, 9)));
        assert_eq!(parse_raster_byte_range("bytes=-99", 10), Some((0, 9)));
        assert_eq!(parse_raster_byte_range("bytes=10-11", 10), None);
        assert_eq!(parse_raster_byte_range("bytes=-0", 10), None);
        assert_eq!(parse_raster_byte_range("bytes=abc-def", 10), None);
        assert_eq!(parse_raster_byte_range("bytes=1-2,4-5", 10), None);
    }

    #[test]
    fn request_policy_matches_python_cross_field_guards() {
        assert!(validate_store_intent(base(RasterMethod::Band)).is_ok());

        let mut mosaic = base(RasterMethod::Mosaic);
        assert!(validate_store_intent(mosaic).is_err());
        mosaic.second_file = true;
        assert!(validate_store_intent(mosaic).is_ok());

        let mut calc = base(RasterMethod::Calculator);
        assert!(validate_store_intent(calc).is_err());
        calc.formula = true;
        assert!(validate_store_intent(calc).is_ok());

        let mut resample = base(RasterMethod::Resample);
        resample.factor = Some(2.0);
        resample.resampling = true;
        assert!(validate_store_intent(resample).is_ok());

        let mut sentinel = base(RasterMethod::Ndvi);
        sentinel.sentinel_scene = true;
        sentinel.sentinel_baseline = true;
        assert!(validate_store_intent(sentinel).is_ok());
        sentinel.second_id = true;
        assert!(validate_store_intent(sentinel).is_err());
    }

    #[test]
    fn geotiff_magic_is_fail_closed() {
        assert!(valid_geotiff_magic(*b"II*\0"));
        assert!(valid_geotiff_magic(*b"MM\0*"));
        assert!(valid_geotiff_magic(*b"II+\0"));
        assert!(valid_geotiff_magic(*b"MM\0+"));
        assert!(!valid_geotiff_magic(*b"PK\x03\x04"));
    }
}
