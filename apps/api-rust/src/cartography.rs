//! E06 renderer-neutral cartography policy.
//!
//! MapLibre/QGIS/renderer engines stay below this boundary. Rust owns validation
//! of portable page/style contracts so invalid state never reaches a renderer.

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use ovg_core::AppError;
use ovg_storage::cartography as cartography_store;
use ovg_storage::catalog::ProjectReadPolicy;
use serde_json::Value;
use uuid::Uuid;

use crate::catalog::{project_read, CatalogError};
use crate::identity::HumanSession;
use crate::AppState;

const BASE: &str = "/organizations/{organization_id}/projects/{project_id}/cartography";

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route(
            &format!("{BASE}/layers/{{layer_id}}/style"),
            get(get_layer_style),
        )
        .route(&format!("{BASE}/layouts"), get(list_layouts))
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaperSize {
    A4,
    A3,
    A2,
    A1,
    A0,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    Portrait,
    Landscape,
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

pub fn paper_dimensions_px(
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
    fn styles_reject_nonfinite_or_noncanonical_values() {
        assert!(validate_single_style("#2563eb", "#1e3a8a", 1.5, 0.75).is_ok());
        for bad in ["#123", "2563eb", "#GG0000", "#1234567"] {
            assert!(validate_hex_color(bad).is_err(), "{bad}");
        }
        assert!(validate_single_style("#2563eb", "#1e3a8a", f64::NAN, 0.5).is_err());
        assert!(validate_single_style("#2563eb", "#1e3a8a", 1.0, f64::INFINITY).is_err());
    }
}
