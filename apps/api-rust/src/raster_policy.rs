//! E09 Rust-native raster control-plane validation.
//!
//! GDAL/PROJ remain worker engines. This module owns bounded OpenVietGIS request
//! semantics before any raster bytes or worker process are touched.

use ovg_core::AppError;

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
