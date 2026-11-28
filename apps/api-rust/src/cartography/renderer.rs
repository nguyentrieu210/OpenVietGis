//! Pure-Rust cartographic print renderer for 150/300 DPI PNG and PDF outputs.
//!
//! Preserves exact physical page dimensions (`A0..A4`), aspect-preserving scale
//! math, symbology (single/categorical/range), polygon holes, multi-geometries,
//! labels, legend overflow checks, north arrow, scale bar, and attribution.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use ab_glyph::{point, Font, FontRef, Glyph, GlyphId, PxScale, ScaleFont};
use ovg_core::AppError;
use ovg_storage::cartography::PrintLayerSnapshot;
use serde_json::{json, Value};

use super::{pythonish_str, Orientation, PaperSize, PrintFormat};

#[derive(Debug, Clone)]
pub(crate) struct PrintFrameMetrics {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) margin: i32,
    pub(crate) map_x0: i32,
    pub(crate) map_y0: i32,
    pub(crate) map_x1: i32,
    pub(crate) map_y1: i32,
    pub(crate) map_w: i32,
    pub(crate) map_h: i32,
    pub(crate) min_x: f64,
    pub(crate) min_y: f64,
    pub(crate) scale_px: f64,
    pub(crate) pad_x: f64,
    pub(crate) pad_y: f64,
    pub(crate) effective_extent: Value,
    pub(crate) ground_width_m: f64,
    pub(crate) scale_denominator: i32,
    pub(crate) scale_accuracy_note: String,
}

pub(crate) struct RenderedMap {
    pub(crate) bytes: Vec<u8>,
}

pub(crate) fn paper_dimensions_px(
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

fn format_with_commas(value: i32) -> String {
    let raw = value.to_string();
    let bytes = raw.as_bytes();
    let mut out = String::with_capacity(raw.len() + 4);
    for (idx, &b) in bytes.iter().enumerate() {
        if idx > 0 && (bytes.len() - idx) % 3 == 0 {
            out.push(',');
        }
        out.push(b as char);
    }
    out
}

pub(crate) fn describe_print_frame(
    paper_size: PaperSize,
    orientation: Orientation,
    dpi: u16,
    crs_code: i32,
    show_legend: bool,
    extent: &BTreeMap<String, f64>,
) -> Result<PrintFrameMetrics, AppError> {
    let (width, height) = paper_dimensions_px(paper_size, orientation, dpi)?;
    let w_i = width as i32;
    let h_i = height as i32;
    let margin = 40.max(w_i / 25);
    let header_h = 60.max(h_i / 14);
    let footer_h = 70.max(h_i / 12);
    let legend_w = if show_legend { w_i / 4 } else { 0 };

    let map_x0 = margin;
    let map_y0 = margin + header_h;
    let map_x1 = w_i - margin - legend_w - if show_legend { margin / 2 } else { 0 };
    let map_y1 = h_i - margin - footer_h;
    let map_w = 100.max(map_x1 - map_x0);
    let map_h = 100.max(map_y1 - map_y0);

    let mut min_x = extent["min_x"];
    let mut min_y = extent["min_y"];
    let mut max_x = extent["max_x"];
    let mut max_y = extent["max_y"];
    let mut span_x = (max_x - min_x).max(1e-6);
    let mut span_y = (max_y - min_y).max(1e-6);
    min_x -= span_x * 0.08;
    max_x += span_x * 0.08;
    min_y -= span_y * 0.08;
    max_y += span_y * 0.08;
    span_x = max_x - min_x;
    span_y = max_y - min_y;

    let scale_px = (f64::from(map_w) / span_x).min(f64::from(map_h) / span_y);
    let pad_x = (f64::from(map_w) - span_x * scale_px) / 2.0;
    let pad_y = (f64::from(map_h) - span_y * scale_px) / 2.0;

    let map_width_meters_paper = (f64::from(map_w) / f64::from(dpi)) * 0.0254;
    let frame_units = f64::from(map_w) / scale_px;
    let (meters_per_unit, scale_reference) = if matches!(crs_code, 4326 | 4756) {
        let mid_lat = (min_y + max_y) / 2.0;
        (
            111_320.0 * mid_lat.to_radians().cos().abs().max(0.001),
            "approximate ground scale at extent center",
        )
    } else if crs_code == 3857 {
        let mid_y = (min_y + max_y) / 2.0;
        (
            1.0 / (mid_y / 6_378_137.0).cosh(),
            "approximate ground scale at extent center (Web Mercator)",
        )
    } else {
        (1.0, "approximate projected grid scale, not ground distance")
    };
    let ground_width_m = frame_units * meters_per_unit;
    let scale_denom = (ground_width_m / map_width_meters_paper.max(1e-4))
        .round()
        .clamp(1.0, f64::from(i32::MAX)) as i32;
    let scale_note = format!(
        "Scale 1:{} in EPSG:{crs_code} ({dpi} DPI): {scale_reference}. \
         Cartographic output only; not survey-grade.",
        format_with_commas(scale_denom)
    );

    Ok(PrintFrameMetrics {
        width,
        height,
        margin,
        map_x0,
        map_y0,
        map_x1,
        map_y1,
        map_w,
        map_h,
        min_x,
        min_y,
        scale_px,
        pad_x,
        pad_y,
        effective_extent: json!({
            "min_x": min_x - pad_x / scale_px,
            "min_y": min_y - pad_y / scale_px,
            "max_x": max_x + pad_x / scale_px,
            "max_y": max_y + pad_y / scale_px,
        }),
        ground_width_m,
        scale_denominator: scale_denom,
        scale_accuracy_note: scale_note,
    })
}

fn hex_to_rgb(hex: &str) -> [u8; 3] {
    let h = hex.trim_start_matches('#');
    if h.len() != 6 {
        return [37, 99, 235];
    }
    let r = u8::from_str_radix(&h[0..2], 16).unwrap_or(37);
    let g = u8::from_str_radix(&h[2..4], 16).unwrap_or(99);
    let b = u8::from_str_radix(&h[4..6], 16).unwrap_or(235);
    [r, g, b]
}

fn resolve_feature_color(style: Option<&Value>, attributes: &Value) -> (String, String) {
    let Some(style) = style else {
        return ("#2563eb".to_owned(), "#1e3a8a".to_owned());
    };
    let mode = style
        .get("mode")
        .and_then(Value::as_str)
        .unwrap_or("single");
    let spec = style.get("style_spec");
    match mode {
        "single" => {
            let fill = spec
                .and_then(|s| s.get("fill_color"))
                .and_then(Value::as_str)
                .unwrap_or("#2563eb");
            let stroke = spec
                .and_then(|s| s.get("stroke_color"))
                .and_then(Value::as_str)
                .unwrap_or("#1e3a8a");
            (fill.to_owned(), stroke.to_owned())
        }
        "categorical" => {
            let field = spec
                .and_then(|s| s.get("field"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let val_str = pythonish_str(attributes.get(field).unwrap_or(&Value::Null));
            if let Some(cats) = spec
                .and_then(|s| s.get("categories"))
                .and_then(Value::as_array)
            {
                for cat in cats {
                    if pythonish_str(cat.get("value").unwrap_or(&Value::Null)) == val_str {
                        let color = cat
                            .get("color")
                            .and_then(Value::as_str)
                            .unwrap_or("#2563eb");
                        return (color.to_owned(), "#111827".to_owned());
                    }
                }
            }
            let fallback = spec
                .and_then(|s| s.get("default_color"))
                .and_then(Value::as_str)
                .unwrap_or("#9ca3af");
            (fallback.to_owned(), "#111827".to_owned())
        }
        "range" => {
            let field = spec
                .and_then(|s| s.get("field"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let num = attributes.get(field).and_then(py_float);
            if let Some(num) = num.filter(|n| n.is_finite()) {
                if let Some(breaks) = spec.and_then(|s| s.get("breaks")).and_then(Value::as_array) {
                    for brk in breaks {
                        let lo = brk.get("min").and_then(py_float);
                        let hi = brk.get("max").and_then(py_float);
                        if let (Some(lo), Some(hi)) = (lo, hi) {
                            if lo <= num && num <= hi {
                                let color = brk
                                    .get("color")
                                    .and_then(Value::as_str)
                                    .unwrap_or("#2563eb");
                                return (color.to_owned(), "#111827".to_owned());
                            }
                        }
                    }
                }
            }
            ("#9ca3af".to_owned(), "#111827".to_owned())
        }
        _ => ("#2563eb".to_owned(), "#1e3a8a".to_owned()),
    }
}

/// Python `float(value)` for JSON scalars: bools are 0/1, strings are trimmed.
fn py_float(value: &Value) -> Option<f64> {
    match value {
        Value::Bool(b) => Some(f64::from(u8::from(*b))),
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_print_map(
    title: &str,
    paper_size: PaperSize,
    orientation: Orientation,
    dpi: u16,
    output_format: PrintFormat,
    crs_code: i32,
    attribution: &str,
    show_north_arrow: bool,
    show_scale_bar: bool,
    show_legend: bool,
    extent: &BTreeMap<String, f64>,
    layers_payload: &[PrintLayerSnapshot],
) -> Result<RenderedMap, AppError> {
    let (canvas, frame) = render_print_canvas(
        title,
        paper_size,
        orientation,
        dpi,
        crs_code,
        attribution,
        show_north_arrow,
        show_scale_bar,
        show_legend,
        extent,
        layers_payload,
    )?;
    let bytes = match output_format {
        PrintFormat::Png => encode_png(frame.width, frame.height, dpi, &canvas.rgb),
        PrintFormat::Pdf => encode_pdf(frame.width, frame.height, dpi, title, &canvas.rgb),
    };
    Ok(RenderedMap { bytes })
}

#[allow(clippy::too_many_arguments)]
fn render_print_canvas(
    title: &str,
    paper_size: PaperSize,
    orientation: Orientation,
    dpi: u16,
    crs_code: i32,
    attribution: &str,
    show_north_arrow: bool,
    show_scale_bar: bool,
    show_legend: bool,
    extent: &BTreeMap<String, f64>,
    layers_payload: &[PrintLayerSnapshot],
) -> Result<(Canvas, PrintFrameMetrics), AppError> {
    let frame = describe_print_frame(paper_size, orientation, dpi, crs_code, show_legend, extent)?;

    let mut legend_entries: Vec<(String, String, String)> = Vec::new();
    for layer in layers_payload {
        if let Some(items) = layer
            .style
            .as_ref()
            .and_then(|s| s.get("legend_items"))
            .and_then(Value::as_array)
            .filter(|items| !items.is_empty())
        {
            for li in items {
                let label = li.get("label").and_then(Value::as_str).unwrap_or("");
                let fill = li
                    .get("fill_color")
                    .and_then(Value::as_str)
                    .unwrap_or("#2563eb");
                let stroke = li
                    .get("stroke_color")
                    .and_then(Value::as_str)
                    .unwrap_or("#1e3a8a");
                legend_entries.push((
                    format!("{}: {label}", layer.name),
                    fill.to_owned(),
                    stroke.to_owned(),
                ));
            }
        } else {
            legend_entries.push((
                layer.name.clone(),
                "#2563eb".to_owned(),
                "#1e3a8a".to_owned(),
            ));
        }
    }

    if show_legend {
        let lx0 = frame.map_x1 + frame.margin / 2;
        let cy = frame.map_y0 + 40;
        if legend_entries.len() > 20 || cy + 26 * (legend_entries.len() as i32) > frame.map_y1 {
            return Err(AppError::validation(
                "PRINT_LEGEND_OVERFLOW",
                "legend does not fit this page",
            ));
        }
        let max_label_x = frame.width as i32 - frame.margin - 8;
        for (label, _, _) in &legend_entries {
            if lx0 + 38 + text_width(label) > max_label_x {
                return Err(AppError::validation(
                    "PRINT_LEGEND_OVERFLOW",
                    "legend label does not fit",
                ));
            }
        }
    }

    let mut canvas = Canvas::new(frame.width, frame.height, [255, 255, 255]);
    let w_i = frame.width as i32;
    let h_i = frame.height as i32;
    let margin = frame.margin;

    // Outer border & header
    canvas.stroke_rect(
        margin / 2,
        margin / 2,
        w_i - margin / 2,
        h_i - margin / 2,
        [30, 41, 59],
        3,
    );
    canvas.draw_text(frame.map_x0, margin, title, [15, 23, 42]);
    let subtitle = format!(
        "CRS: EPSG:{crs_code}  |  Paper: {} {}  |  {dpi} DPI",
        paper_size.wire(),
        orientation.wire()
    );
    canvas.draw_text(frame.map_x0, margin + 24, &subtitle, [71, 85, 105]);

    // Map frame background
    canvas.fill_rect(
        frame.map_x0,
        frame.map_y0,
        frame.map_x1,
        frame.map_y1,
        [248, 250, 252],
        255,
    );
    canvas.stroke_rect(
        frame.map_x0,
        frame.map_y0,
        frame.map_x1,
        frame.map_y1,
        [51, 65, 85],
        2,
    );

    // Subtle graticule grid
    for i in 1..4 {
        let gx = frame.map_x0 + (frame.map_w * i) / 4;
        let gy = frame.map_y0 + (frame.map_h * i) / 4;
        canvas.draw_line(
            gx as f64,
            frame.map_y0 as f64,
            gx as f64,
            frame.map_y1 as f64,
            [226, 232, 240],
            1,
            None,
        );
        canvas.draw_line(
            frame.map_x0 as f64,
            gy as f64,
            frame.map_x1 as f64,
            gy as f64,
            [226, 232, 240],
            1,
            None,
        );
    }

    let clip = [frame.map_x0, frame.map_y0, frame.map_x1, frame.map_y1];
    let to_px = |x: f64, y: f64| -> (f64, f64) {
        let px = f64::from(frame.map_x0) + frame.pad_x + (x - frame.min_x) * frame.scale_px;
        let py = f64::from(frame.map_y1) - frame.pad_y - (y - frame.min_y) * frame.scale_px;
        (px, py)
    };

    for layer in layers_payload {
        let label_field = layer
            .style
            .as_ref()
            .and_then(|s| s.get("label_field"))
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty());

        for feat in &layer.features {
            let Some(gtype) = feat.geometry.get("type").and_then(Value::as_str) else {
                continue;
            };
            let Some(coords) = feat.geometry.get("coordinates") else {
                continue;
            };
            let (fill_hex, stroke_hex) =
                resolve_feature_color(layer.style.as_ref(), &feat.attributes);
            let fill_rgb = hex_to_rgb(&fill_hex);
            let stroke_rgb = hex_to_rgb(&stroke_hex);

            let anchor = paint_geometry(
                &mut canvas,
                gtype,
                coords,
                fill_rgb,
                stroke_rgb,
                frame.width,
                clip,
                &to_px,
            )?;
            if let (Some(lf), Some((ax, ay))) = (label_field, anchor) {
                if let Some(val) = feat.attributes.get(lf).filter(|v| !v.is_null()) {
                    let label = pythonish_str(val);
                    canvas.draw_text_clipped(
                        ax.round() as i32,
                        ay.round() as i32,
                        &label,
                        [15, 23, 42],
                        Some(clip),
                    );
                }
            }
        }
    }

    // Re-draw map frame border so clipped strokes sit cleanly inside.
    canvas.stroke_rect(
        frame.map_x0,
        frame.map_y0,
        frame.map_x1,
        frame.map_y1,
        [51, 65, 85],
        2,
    );

    if show_north_arrow {
        let nx = frame.map_x1 - 36;
        let ny = frame.map_y0 + 54;
        let arrow = [
            (f64::from(nx), f64::from(ny - 28)),
            (f64::from(nx - 12), f64::from(ny + 8)),
            (f64::from(nx), f64::from(ny)),
            (f64::from(nx + 12), f64::from(ny + 8)),
        ];
        canvas.fill_polygon(&[&arrow], [15, 23, 42], 255, None);
        canvas.draw_text(nx - 12, ny - 44, "N (grid)", [15, 23, 42]);
    }

    if show_scale_bar {
        let bar_px = 80.max(frame.map_w / 5);
        let bar_ground_m = (f64::from(bar_px) / f64::from(frame.map_w)) * frame.ground_width_m;
        let label_txt = if bar_ground_m >= 1000.0 {
            format!("{:.2} km", bar_ground_m / 1000.0)
        } else {
            format!("{bar_ground_m:.0} m")
        };
        let bx0 = frame.map_x0 + 20;
        let by0 = frame.map_y1 - 28;
        canvas.fill_rect(bx0, by0, bx0 + bar_px, by0 + 10, [15, 23, 42], 255);
        canvas.fill_rect(
            bx0 + bar_px / 2,
            by0,
            bx0 + bar_px,
            by0 + 10,
            [255, 255, 255],
            255,
        );
        canvas.stroke_rect(bx0, by0, bx0 + bar_px, by0 + 10, [15, 23, 42], 1);
        let scale_label = format!(
            "Scale 1:{} ({label_txt})",
            format_with_commas(frame.scale_denominator)
        );
        canvas.draw_text(bx0, by0 - 18, &scale_label, [15, 23, 42]);
    }

    if show_legend {
        let lx0 = frame.map_x1 + margin / 2;
        let ly0 = frame.map_y0;
        canvas.fill_rect(lx0, ly0, w_i - margin, frame.map_y1, [255, 255, 255], 255);
        canvas.stroke_rect(lx0, ly0, w_i - margin, frame.map_y1, [148, 163, 184], 2);
        canvas.draw_text(lx0 + 12, ly0 + 12, "LEGEND", [15, 23, 42]);
        let mut cy = ly0 + 40;
        for (label, fhex, shex) in &legend_entries {
            canvas.fill_rect(lx0 + 12, cy, lx0 + 30, cy + 16, hex_to_rgb(fhex), 220);
            canvas.stroke_rect(lx0 + 12, cy, lx0 + 30, cy + 16, hex_to_rgb(shex), 1);
            canvas.draw_text(lx0 + 38, cy + 1, label, [30, 41, 59]);
            cy += 26;
        }
    }

    let attr_line = format!("Attribution: {attribution}");
    canvas.draw_text(frame.map_x0, frame.map_y1 + 14, &attr_line, [51, 65, 85]);
    canvas.draw_text(
        frame.map_x0,
        frame.map_y1 + 36,
        &frame.scale_accuracy_note,
        [100, 116, 139],
    );

    Ok((canvas, frame))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_atlas_pdf(
    title: &str,
    paper_size: PaperSize,
    orientation: Orientation,
    dpi: u16,
    crs_code: i32,
    attribution: &str,
    show_north_arrow: bool,
    show_scale_bar: bool,
    show_legend: bool,
    pages: &[(String, BTreeMap<String, f64>)],
    layers: &[PrintLayerSnapshot],
) -> Result<RenderedMap, AppError> {
    let (width, height) = paper_dimensions_px(paper_size, orientation, dpi)?;
    if pages.is_empty()
        || pages.len() > 16
        || u64::from(width) * u64::from(height) * pages.len() as u64 > 40_000_000
    {
        return Err(AppError::validation(
            "ATLAS_LIMIT",
            "Atlas page or pixel budget exceeded",
        ));
    }
    let mut compressed = Vec::with_capacity(pages.len());
    let mut bytes = 0_usize;
    for (index, (fid, extent)) in pages.iter().enumerate() {
        let page_title = format!(
            "{} | {}/{} | {}",
            title.chars().take(80).collect::<String>(),
            index + 1,
            pages.len(),
            fid.chars().take(60).collect::<String>()
        );
        let (canvas, _) = render_print_canvas(
            &page_title,
            paper_size,
            orientation,
            dpi,
            crs_code,
            attribution,
            show_north_arrow,
            show_scale_bar,
            show_legend,
            extent,
            layers,
        )?;
        let page = zlib(&canvas.rgb);
        bytes = bytes.saturating_add(page.len());
        if bytes > 32 * 1024 * 1024 {
            return Err(AppError::validation("ATLAS_LIMIT", "Atlas exceeds 32 MiB"));
        }
        compressed.push(page);
    }
    Ok(RenderedMap {
        bytes: encode_atlas_pdf(width, height, dpi, title, &compressed),
    })
}

fn encode_atlas_pdf(width: u32, height: u32, dpi: u16, title: &str, pages: &[Vec<u8>]) -> Vec<u8> {
    let pt_w = f64::from(width) * 72.0 / f64::from(dpi);
    let pt_h = f64::from(height) * 72.0 / f64::from(dpi);
    let mut pdf = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets = Vec::new();
    fn object(pdf: &mut Vec<u8>, offsets: &mut Vec<usize>, data: &str) {
        offsets.push(pdf.len());
        pdf.extend_from_slice(data.as_bytes());
    }
    object(
        &mut pdf,
        &mut offsets,
        "1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n",
    );
    let kids = (0..pages.len())
        .map(|i| format!("{} 0 R", 3 + 3 * i))
        .collect::<Vec<_>>()
        .join(" ");
    object(
        &mut pdf,
        &mut offsets,
        &format!(
            "2 0 obj\n<< /Type /Pages /Kids [{kids}] /Count {} >>\nendobj\n",
            pages.len()
        ),
    );
    for (i, rgb) in pages.iter().enumerate() {
        let page = 3 + i * 3;
        let image = page + 1;
        let stream = page + 2;
        object(&mut pdf, &mut offsets, &format!(
            "{page} 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {pt_w:.4} {pt_h:.4}] /Resources << /XObject << /Im0 {image} 0 R >> >> /Contents {stream} 0 R >>\nendobj\n"
        ));
        object(&mut pdf, &mut offsets, &format!(
            "{image} 0 obj\n<< /Type /XObject /Subtype /Image /Width {width} /Height {height} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode /Length {} >>\nstream\n", rgb.len()
        ));
        pdf.extend_from_slice(rgb);
        pdf.extend_from_slice(b"\nendstream\nendobj\n");
        let content = format!("q {pt_w:.4} 0 0 {pt_h:.4} 0 0 cm /Im0 Do Q");
        object(
            &mut pdf,
            &mut offsets,
            &format!(
                "{stream} 0 obj\n<< /Length {} >>\nstream\n{content}\nendstream\nendobj\n",
                content.len()
            ),
        );
    }
    let info = 3 + pages.len() * 3;
    object(
        &mut pdf,
        &mut offsets,
        &format!(
            "{info} 0 obj\n<< /Title {} /Producer (OpenVietGIS) >>\nendobj\n",
            pdf_text_string(title)
        ),
    );
    let start = pdf.len();
    let mut xref = format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len() + 1);
    for offset in &offsets {
        let _ = writeln!(xref, "{offset:010} 00000 n ");
    }
    let _ = writeln!(
        xref,
        "trailer\n<< /Size {} /Root 1 0 R /Info {info} 0 R >>\nstartxref\n{start}\n%%EOF",
        offsets.len() + 1
    );
    pdf.extend_from_slice(xref.as_bytes());
    pdf
}

#[allow(clippy::too_many_arguments)]
fn paint_geometry(
    canvas: &mut Canvas,
    kind: &str,
    coords: &Value,
    fill_rgb: [u8; 3],
    stroke_rgb: [u8; 3],
    page_width: u32,
    clip: [i32; 4],
    to_px: &dyn Fn(f64, f64) -> (f64, f64),
) -> Result<Option<(f64, f64)>, AppError> {
    match kind {
        "Point" => {
            let Some((x, y)) = coord_pair(coords) else {
                return Ok(None);
            };
            let (px, py) = to_px(x, y);
            let r = 4.max(page_width as i32 / 240);
            canvas.fill_circle(px, py, r, fill_rgb, 180, Some(clip));
            canvas.stroke_circle(px, py, r, stroke_rgb, 2, Some(clip));
            Ok(Some((px + f64::from(r) + 4.0, py - f64::from(r))))
        }
        "LineString" => {
            let pts = coord_seq(coords, to_px);
            if pts.len() < 2 {
                return Ok(None);
            }
            let width = 2.max(page_width as i32 / 400);
            for pair in pts.windows(2) {
                canvas.draw_line(
                    pair[0].0,
                    pair[0].1,
                    pair[1].0,
                    pair[1].1,
                    stroke_rgb,
                    width,
                    Some(clip),
                );
            }
            Ok(Some(pts[pts.len() / 2]))
        }
        "Polygon" => {
            let Some(ring_vals) = coords.as_array() else {
                return Ok(None);
            };
            let rings: Vec<Vec<(f64, f64)>> = ring_vals
                .iter()
                .map(|r| coord_seq(r, to_px))
                .filter(|r| !r.is_empty())
                .collect();
            if rings.is_empty() {
                return Ok(None);
            }
            let ring_refs: Vec<&[(f64, f64)]> = rings.iter().map(Vec::as_slice).collect();
            canvas.fill_polygon(&ring_refs, fill_rgb, 180, Some(clip));
            for ring in &rings {
                for pair in ring.windows(2) {
                    canvas.draw_line(
                        pair[0].0,
                        pair[0].1,
                        pair[1].0,
                        pair[1].1,
                        stroke_rgb,
                        1,
                        Some(clip),
                    );
                }
            }
            let outer = &rings[0];
            let n = outer.len() as f64;
            let cx = outer.iter().map(|p| p.0).sum::<f64>() / n;
            let cy = outer.iter().map(|p| p.1).sum::<f64>() / n;
            Ok(Some((cx, cy)))
        }
        "MultiPoint" | "MultiLineString" | "MultiPolygon" => {
            let sub = &kind[5..];
            let Some(members) = coords.as_array() else {
                return Ok(None);
            };
            let mut first = None;
            for member in members {
                let anchor = paint_geometry(
                    canvas, sub, member, fill_rgb, stroke_rgb, page_width, clip, to_px,
                )?;
                if first.is_none() {
                    first = anchor;
                }
            }
            Ok(first)
        }
        other => Err(AppError::validation(
            "UNSUPPORTED_PRINT_GEOMETRY",
            format!("unsupported print geometry {other:?}"),
        )),
    }
}

fn coord_pair(value: &Value) -> Option<(f64, f64)> {
    let arr = value.as_array()?;
    let x = arr.first()?.as_f64()?;
    let y = arr.get(1)?.as_f64()?;
    (x.is_finite() && y.is_finite()).then_some((x, y))
}

fn coord_seq(value: &Value, to_px: &dyn Fn(f64, f64) -> (f64, f64)) -> Vec<(f64, f64)> {
    value
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(coord_pair)
                .map(|(x, y)| to_px(x, y))
                .collect()
        })
        .unwrap_or_default()
}

// ----- Pure-Rust RGB Canvas & Primitives -----

struct Canvas {
    width: usize,
    height: usize,
    rgb: Vec<u8>,
}

impl Canvas {
    fn new(width: u32, height: u32, bg: [u8; 3]) -> Self {
        let w = width as usize;
        let h = height as usize;
        let mut rgb = Vec::with_capacity(w * h * 3);
        for _ in 0..(w * h) {
            rgb.extend_from_slice(&bg);
        }
        Self {
            width: w,
            height: h,
            rgb,
        }
    }

    fn blend_pixel(&mut self, x: i32, y: i32, color: [u8; 3], alpha: u8, clip: Option<[i32; 4]>) {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return;
        }
        if let Some([cx0, cy0, cx1, cy1]) = clip {
            if x < cx0 || y < cy0 || x > cx1 || y > cy1 {
                return;
            }
        }
        let idx = (y as usize * self.width + x as usize) * 3;
        if alpha == 255 {
            self.rgb[idx] = color[0];
            self.rgb[idx + 1] = color[1];
            self.rgb[idx + 2] = color[2];
        } else if alpha > 0 {
            let a = u16::from(alpha);
            let inv = 255 - a;
            self.rgb[idx] =
                ((u16::from(color[0]) * a + u16::from(self.rgb[idx]) * inv) / 255) as u8;
            self.rgb[idx + 1] =
                ((u16::from(color[1]) * a + u16::from(self.rgb[idx + 1]) * inv) / 255) as u8;
            self.rgb[idx + 2] =
                ((u16::from(color[2]) * a + u16::from(self.rgb[idx + 2]) * inv) / 255) as u8;
        }
    }

    fn fill_rect(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, color: [u8; 3], alpha: u8) {
        let rx0 = x0.max(0);
        let ry0 = y0.max(0);
        let rx1 = x1.min(self.width as i32 - 1);
        let ry1 = y1.min(self.height as i32 - 1);
        for y in ry0..=ry1 {
            for x in rx0..=rx1 {
                self.blend_pixel(x, y, color, alpha, None);
            }
        }
    }

    fn stroke_rect(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, color: [u8; 3], width: i32) {
        for offset in 0..width {
            let lx0 = x0 + offset;
            let ly0 = y0 + offset;
            let lx1 = x1 - offset;
            let ly1 = y1 - offset;
            if lx0 > lx1 || ly0 > ly1 {
                break;
            }
            for x in lx0..=lx1 {
                self.blend_pixel(x, ly0, color, 255, None);
                self.blend_pixel(x, ly1, color, 255, None);
            }
            for y in ly0..=ly1 {
                self.blend_pixel(lx0, y, color, 255, None);
                self.blend_pixel(lx1, y, color, 255, None);
            }
        }
    }

    fn fill_circle(
        &mut self,
        cx: f64,
        cy: f64,
        radius: i32,
        color: [u8; 3],
        alpha: u8,
        clip: Option<[i32; 4]>,
    ) {
        let r2 = f64::from(radius * radius);
        let ix = cx.round() as i32;
        let iy = cy.round() as i32;
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if f64::from(dx * dx + dy * dy) <= r2 {
                    self.blend_pixel(ix + dx, iy + dy, color, alpha, clip);
                }
            }
        }
    }

    fn stroke_circle(
        &mut self,
        cx: f64,
        cy: f64,
        radius: i32,
        color: [u8; 3],
        width: i32,
        clip: Option<[i32; 4]>,
    ) {
        let outer2 = f64::from(radius * radius);
        let inner = (radius - width).max(0);
        let inner2 = f64::from(inner * inner);
        let ix = cx.round() as i32;
        let iy = cy.round() as i32;
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                let d2 = f64::from(dx * dx + dy * dy);
                if d2 <= outer2 && d2 >= inner2 {
                    self.blend_pixel(ix + dx, iy + dy, color, 255, clip);
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_line(
        &mut self,
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
        color: [u8; 3],
        width: i32,
        clip: Option<[i32; 4]>,
    ) {
        if !x0.is_finite() || !y0.is_finite() || !x1.is_finite() || !y1.is_finite() {
            return;
        }
        let half = (width - 1).max(0) / 2;
        let extra = width - 1 - half;
        // Clip to the visible rect (plus stroke width) so step count is bounded without gaps.
        let [rx0, ry0, rx1, ry1] =
            clip.unwrap_or([0, 0, self.width as i32 - 1, self.height as i32 - 1]);
        let rect = [
            f64::from(rx0 - width),
            f64::from(ry0 - width),
            f64::from(rx1 + width),
            f64::from(ry1 + width),
        ];
        let Some((x0, y0, x1, y1)) = clip_segment(x0, y0, x1, y1, rect) else {
            return;
        };
        let dx = x1 - x0;
        let dy = y1 - y0;
        let steps = dx.abs().max(dy.abs()).ceil() as i32;
        if steps == 0 {
            let ix = x0.round() as i32;
            let iy = y0.round() as i32;
            for oy in -half..=extra {
                for ox in -half..=extra {
                    self.blend_pixel(ix + ox, iy + oy, color, 255, clip);
                }
            }
            return;
        }
        for i in 0..=steps {
            let t = f64::from(i) / f64::from(steps);
            let ix = (x0 + dx * t).round() as i32;
            let iy = (y0 + dy * t).round() as i32;
            for oy in -half..=extra {
                for ox in -half..=extra {
                    self.blend_pixel(ix + ox, iy + oy, color, 255, clip);
                }
            }
        }
    }

    fn fill_polygon(
        &mut self,
        rings: &[&[(f64, f64)]],
        color: [u8; 3],
        alpha: u8,
        clip: Option<[i32; 4]>,
    ) {
        let mut min_y = f64::INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        for ring in rings {
            for &(_, y) in *ring {
                min_y = min_y.min(y);
                max_y = max_y.max(y);
            }
        }
        if !min_y.is_finite() || !max_y.is_finite() {
            return;
        }
        let (cy0, cy1) = match clip {
            Some([_, y0, _, y1]) => (y0.max(0), y1.min(self.height as i32 - 1)),
            None => (0, self.height as i32 - 1),
        };
        let start_y = (min_y.floor() as i32).max(cy0);
        let end_y = (max_y.ceil() as i32).min(cy1);
        let mut intersections = Vec::new();
        for y in start_y..=end_y {
            intersections.clear();
            let fy = f64::from(y) + 0.5;
            for ring in rings {
                if ring.len() < 2 {
                    continue;
                }
                for idx in 0..ring.len() {
                    let (ax, ay) = ring[idx];
                    let (bx, by) = ring[(idx + 1) % ring.len()];
                    if (ay <= fy && by > fy) || (by <= fy && ay > fy) {
                        let x = ax + (fy - ay) * (bx - ax) / (by - ay);
                        if x.is_finite() {
                            intersections.push(x);
                        }
                    }
                }
            }
            intersections.sort_by(f64::total_cmp);
            for pair in intersections.chunks_exact(2) {
                let mut x_start = pair[0].ceil() as i32;
                let mut x_end = pair[1].floor() as i32;
                if let Some([cx0, _, cx1, _]) = clip {
                    x_start = x_start.max(cx0);
                    x_end = x_end.min(cx1);
                } else {
                    x_start = x_start.max(0);
                    x_end = x_end.min(self.width as i32 - 1);
                }
                for x in x_start..=x_end {
                    self.blend_pixel(x, y, color, alpha, clip);
                }
            }
        }
    }

    fn draw_text(&mut self, x: i32, y: i32, text: &str, color: [u8; 3]) {
        self.draw_text_clipped(x, y, text, color, None);
    }

    fn draw_text_clipped(
        &mut self,
        x: i32,
        y: i32,
        text: &str,
        color: [u8; 3],
        clip: Option<[i32; 4]>,
    ) {
        let font = text_font();
        let (glyphs, _) = layout_text(&font, text, x as f32, y as f32);
        for glyph in glyphs {
            let Some(outlined) = font.outline_glyph(glyph) else {
                continue;
            };
            let bounds = outlined.px_bounds();
            let (ox, oy) = (bounds.min.x as i32, bounds.min.y as i32);
            outlined.draw(|gx, gy, coverage| {
                let alpha = (coverage.clamp(0.0, 1.0) * 255.0).round() as u8;
                self.blend_pixel(ox + gx as i32, oy + gy as i32, color, alpha, clip);
            });
        }
    }
}

/// Liang-Barsky: the part of segment (x0,y0)-(x1,y1) inside `[min_x, min_y, max_x, max_y]`.
fn clip_segment(
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    rect: [f64; 4],
) -> Option<(f64, f64, f64, f64)> {
    let (dx, dy) = (x1 - x0, y1 - y0);
    let (mut t0, mut t1) = (0.0_f64, 1.0_f64);
    for (p, q) in [
        (-dx, x0 - rect[0]),
        (dx, rect[2] - x0),
        (-dy, y0 - rect[1]),
        (dy, rect[3] - y0),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
        } else {
            let r = q / p;
            if p < 0.0 {
                t0 = t0.max(r);
            } else {
                t1 = t1.min(r);
            }
            if t0 > t1 {
                return None;
            }
        }
    }
    Some((x0 + t0 * dx, y0 + t0 * dy, x0 + t1 * dx, y0 + t1 * dy))
}

// Noto Sans Regular (SIL OFL 1.1, see assets/fonts/OFL.txt): covers Vietnamese.
const FONT_BYTES: &[u8] = include_bytes!("../../assets/fonts/NotoSans-Regular.ttf");
/// Pillow's default `ImageDraw.text` font is a 10 px em FreeType font.
const TEXT_EM_PX: f32 = 10.0;

fn text_font() -> FontRef<'static> {
    FontRef::try_from_slice(FONT_BYTES).expect("bundled font parses")
}

/// Lays out `text` with its top (ascender) at `y`, like Pillow's default "la" anchor.
/// Returns positioned glyphs and the caret x after the last glyph.
fn layout_text(font: &FontRef<'_>, text: &str, x: f32, y: f32) -> (Vec<Glyph>, f32) {
    let upem = font.units_per_em().unwrap_or(1000.0);
    let scaled = font.as_scaled(PxScale::from(TEXT_EM_PX * font.height_unscaled() / upem));
    let baseline = y + scaled.ascent();
    let mut caret = x;
    let mut prev: Option<GlyphId> = None;
    let mut glyphs = Vec::with_capacity(text.len());
    for ch in text.chars().filter(|c| !c.is_control()) {
        let id = scaled.glyph_id(ch);
        if let Some(prev) = prev {
            caret += scaled.kern(prev, id);
        }
        glyphs.push(id.with_scale_and_position(scaled.scale(), point(caret, baseline)));
        caret += scaled.h_advance(id);
        prev = Some(id);
    }
    (glyphs, caret)
}

fn text_width(text: &str) -> i32 {
    layout_text(&text_font(), text, 0.0, 0.0).1.ceil() as i32
}

// ----- PNG and PDF encoders -----

/// RFC 1950 zlib stream (DEFLATE + Adler-32); level 6 is zlib's and Pillow's default.
fn zlib(raw: &[u8]) -> Vec<u8> {
    miniz_oxide::deflate::compress_to_vec_zlib(raw, 6)
}

fn push_png_chunk(out: &mut Vec<u8>, tag: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(tag);
    out.extend_from_slice(data);
    let crc = crc32fast::hash(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}

fn encode_png(width: u32, height: u32, dpi: u16, rgb: &[u8]) -> Vec<u8> {
    let row_bytes = width as usize * 3;
    let mut scanlines = Vec::with_capacity((row_bytes + 1) * height as usize);
    for row in rgb.chunks_exact(row_bytes) {
        scanlines.push(0_u8);
        scanlines.extend_from_slice(row);
    }
    let idat = zlib(&scanlines);

    let mut out = Vec::with_capacity(128 + idat.len());
    out.extend_from_slice(b"\x89PNG\r\n\x1a\n");

    let mut ihdr = [0_u8; 13];
    ihdr[0..4].copy_from_slice(&width.to_be_bytes());
    ihdr[4..8].copy_from_slice(&height.to_be_bytes());
    ihdr[8] = 8; // bit depth
    ihdr[9] = 2; // RGB
    push_png_chunk(&mut out, b"IHDR", &ihdr);

    let ppm = ((f64::from(dpi) / 0.0254).round()) as u32;
    let mut phys = [0_u8; 9];
    phys[0..4].copy_from_slice(&ppm.to_be_bytes());
    phys[4..8].copy_from_slice(&ppm.to_be_bytes());
    phys[8] = 1; // meter
    push_png_chunk(&mut out, b"pHYs", &phys);

    push_png_chunk(&mut out, b"IDAT", &idat);
    push_png_chunk(&mut out, b"IEND", &[]);
    out
}

/// PDF text string: a literal for printable ASCII, else UTF-16BE hex with a BOM.
fn pdf_text_string(text: &str) -> String {
    if text.chars().any(|c| !c.is_ascii() || c.is_ascii_control()) {
        let mut out = String::from("<FEFF");
        for unit in text.encode_utf16() {
            let _ = write!(out, "{unit:04X}");
        }
        out.push('>');
        return out;
    }
    let mut out = String::from("(");
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '(' => out.push_str("\\("),
            ')' => out.push_str("\\)"),
            c => out.push(c),
        }
    }
    out.push(')');
    out
}

fn encode_pdf(width: u32, height: u32, dpi: u16, title: &str, rgb: &[u8]) -> Vec<u8> {
    let pt_w = (f64::from(width) * 72.0) / f64::from(dpi);
    let pt_h = (f64::from(height) * 72.0) / f64::from(dpi);
    let compressed_rgb = zlib(rgb);
    let content_stream = format!("q {pt_w:.4} 0 0 {pt_h:.4} 0 0 cm /Im0 Do Q");

    let mut pdf = Vec::with_capacity(1024 + compressed_rgb.len());
    pdf.extend_from_slice(b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n");

    let mut offsets = Vec::with_capacity(6);

    // 1: Catalog
    offsets.push(pdf.len());
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    // 2: Pages
    offsets.push(pdf.len());
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");

    // 3: Page
    offsets.push(pdf.len());
    let page = format!(
        "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {pt_w:.4} {pt_h:.4}] \
         /Resources << /XObject << /Im0 4 0 R >> >> /Contents 5 0 R >>\nendobj\n"
    );
    pdf.extend_from_slice(page.as_bytes());

    // 4: Image XObject
    offsets.push(pdf.len());
    let img_head = format!(
        "4 0 obj\n<< /Type /XObject /Subtype /Image /Width {width} /Height {height} \
         /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode /Length {} >>\nstream\n",
        compressed_rgb.len()
    );
    pdf.extend_from_slice(img_head.as_bytes());
    pdf.extend_from_slice(&compressed_rgb);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");

    // 5: Page contents
    offsets.push(pdf.len());
    let contents = format!(
        "5 0 obj\n<< /Length {} >>\nstream\n{content_stream}\nendstream\nendobj\n",
        content_stream.len()
    );
    pdf.extend_from_slice(contents.as_bytes());

    // 6: Info
    offsets.push(pdf.len());
    let info = format!(
        "6 0 obj\n<< /Title {} /Producer (OpenVietGIS) /Creator (OpenVietGIS) >>\nendobj\n",
        pdf_text_string(title)
    );
    pdf.extend_from_slice(info.as_bytes());

    let xref_offset = pdf.len();
    let mut xref = String::new();
    let _ = writeln!(xref, "xref\n0 {}", offsets.len() + 1);
    let _ = writeln!(xref, "0000000000 65535 f ");
    for off in offsets {
        let _ = writeln!(xref, "{off:010} 00000 n ");
    }
    let _ = writeln!(
        xref,
        "trailer\n<< /Size 7 /Root 1 0 R /Info 6 0 R >>\nstartxref\n{xref_offset}\n%%EOF"
    );
    pdf.extend_from_slice(xref.as_bytes());
    pdf
}

#[cfg(test)]
mod tests {
    use super::*;
    use ovg_storage::cartography::PrintFeatureRow;
    use uuid::Uuid;

    fn render_test(
        features: Vec<PrintFeatureRow>,
        extent: Option<BTreeMap<String, f64>>,
        legend: bool,
        layers: Option<Vec<PrintLayerSnapshot>>,
        output_format: PrintFormat,
    ) -> Result<(RenderedMap, PrintFrameMetrics), AppError> {
        let extent = extent.unwrap_or_else(|| {
            BTreeMap::from([
                ("min_x".to_owned(), 0.0),
                ("min_y".to_owned(), 0.0),
                ("max_x".to_owned(), 100.0),
                ("max_y".to_owned(), 100.0),
            ])
        });
        let layers = layers.unwrap_or_else(|| {
            vec![PrintLayerSnapshot {
                id: Uuid::new_v4(),
                name: "control".to_owned(),
                layer_revision: 1,
                source_id: Uuid::new_v4(),
                content_revision: "a".repeat(64),
                schema_revision: "b".repeat(64),
                source_crs: 5899,
                transformation_operation: None,
                style: None,
                style_id: None,
                style_revision: 0,
                style_fingerprint: None,
                features,
            }]
        });
        let frame = describe_print_frame(
            PaperSize::A4,
            Orientation::Landscape,
            150,
            5899,
            legend,
            &extent,
        )?;
        let rendered = render_print_map(
            "Synthetic map",
            PaperSize::A4,
            Orientation::Landscape,
            150,
            output_format,
            5899,
            "Synthetic",
            false,
            false,
            legend,
            &extent,
            &layers,
        )?;
        Ok((rendered, frame))
    }

    #[test]
    fn atlas_pdf_has_independent_page_objects_and_xref() {
        let data = zlib(&[255, 255, 255]);
        let pdf = encode_atlas_pdf(1, 1, 150, "Atlas", &[data.clone(), data]);
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.contains("/Kids [3 0 R 6 0 R] /Count 2"));
        assert!(text.contains("/Contents 5 0 R"));
        assert!(text.contains("/Contents 8 0 R"));
        assert!(text.contains("/Size 10"));
        assert!(pdf.ends_with(b"%%EOF\n"));
    }

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
        assert_eq!(
            paper_dimensions_px(PaperSize::A3, Orientation::Portrait, 300).unwrap(),
            (3508, 4961)
        );
        assert!(paper_dimensions_px(PaperSize::A2, Orientation::Portrait, 300).is_err());
        assert!(paper_dimensions_px(PaperSize::A0, Orientation::Portrait, 150).is_ok());
    }

    #[test]
    fn scale_uses_actual_aspect_preserving_transform() {
        let (_, narrow) = render_test(
            vec![],
            Some(BTreeMap::from([
                ("min_x".to_owned(), 0.0),
                ("min_y".to_owned(), 0.0),
                ("max_x".to_owned(), 10.0),
                ("max_y".to_owned(), 100.0),
            ])),
            false,
            None,
            PrintFormat::Png,
        )
        .unwrap();
        let (_, wide) = render_test(
            vec![],
            Some(BTreeMap::from([
                ("min_x".to_owned(), 0.0),
                ("min_y".to_owned(), 0.0),
                ("max_x".to_owned(), 30.0),
                ("max_y".to_owned(), 100.0),
            ])),
            false,
            None,
            PrintFormat::Png,
        )
        .unwrap();
        assert_eq!(narrow.scale_denominator, wide.scale_denominator);
        assert!(narrow.scale_accuracy_note.contains("grid scale"));
        assert!(narrow.scale_accuracy_note.contains("not ground distance"));
    }

    #[test]
    fn legend_overflow_rejected_before_export() {
        let layers = (0..21)
            .map(|i| PrintLayerSnapshot {
                id: Uuid::new_v4(),
                name: format!("control-{i}"),
                layer_revision: 1,
                source_id: Uuid::new_v4(),
                content_revision: "a".repeat(64),
                schema_revision: "b".repeat(64),
                source_crs: 5899,
                transformation_operation: None,
                style: None,
                style_id: None,
                style_revision: 0,
                style_fingerprint: None,
                features: vec![],
            })
            .collect();
        let err = render_test(vec![], None, true, Some(layers), PrintFormat::Png)
            .err()
            .unwrap();
        assert_eq!(err.code(), "PRINT_LEGEND_OVERFLOW");
    }

    #[test]
    fn polygon_holes_and_pdf_png_containers_encode_cleanly() {
        let poly = PrintFeatureRow {
            feature_id: "poly-1".to_owned(),
            attributes: json!({}),
            geometry: json!({
                "type": "Polygon",
                "coordinates": [
                    [[0, 0], [100, 0], [100, 100], [0, 100], [0, 0]],
                    [[40, 40], [60, 40], [60, 60], [40, 60], [40, 40]]
                ]
            }),
        };
        let (png, frame) = render_test(vec![poly], None, false, None, PrintFormat::Png).unwrap();
        assert!(png.bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert_eq!(
            u32::from_be_bytes(png.bytes[16..20].try_into().unwrap()),
            frame.width
        );
        assert_eq!(
            u32::from_be_bytes(png.bytes[20..24].try_into().unwrap()),
            frame.height
        );

        let (pdf, _) = render_test(vec![], None, false, None, PrintFormat::Pdf).unwrap();
        assert!(pdf.bytes.starts_with(b"%PDF-1.4\n"));
        assert!(pdf.bytes.ends_with(b"%%EOF\n"));
    }

    #[test]
    fn blank_a4_outputs_are_deflate_compressed() {
        let (png, _) = render_test(vec![], None, true, None, PrintFormat::Png).unwrap();
        let (pdf, _) = render_test(vec![], None, true, None, PrintFormat::Pdf).unwrap();
        assert!(png.bytes.len() < 1_000_000, "png {} bytes", png.bytes.len());
        assert!(pdf.bytes.len() < 1_000_000, "pdf {} bytes", pdf.bytes.len());

        let raw: Vec<u8> = (0..200_000_u32).map(|i| (i % 7) as u8).collect();
        let packed = zlib(&raw);
        assert!(packed.len() < raw.len() / 10);
        let unpacked = miniz_oxide::inflate::decompress_to_vec_zlib(&packed).unwrap();
        assert_eq!(unpacked, raw);
        assert_eq!(crc32fast::hash(b"IEND"), 0xAE42_6082);
    }

    #[test]
    fn vietnamese_text_renders_real_glyphs() {
        let font = text_font();
        for ch in "Bản đồ Việt Nam ạỹ".chars().filter(|c| !c.is_whitespace()) {
            assert_ne!(font.glyph_id(ch).0, 0, "missing glyph for {ch}");
        }
        let mut canvas = Canvas::new(200, 30, [255, 255, 255]);
        canvas.draw_text(2, 2, "Bản đồ Việt Nam", [0, 0, 0]);
        let ink = canvas.rgb.iter().filter(|&&v| v < 128).count();
        assert!(ink > 50, "only {ink} dark channels");
        assert!(text_width("Bản đồ") > 0);
        assert_eq!(pdf_text_string("Bản"), "<FEFF00421EA3006E>");
        assert_eq!(pdf_text_string("Map (1)"), "(Map \\(1\\))");
    }

    #[test]
    fn long_clipped_segment_is_continuous() {
        let mut canvas = Canvas::new(200, 100, [255, 255, 255]);
        let clip = Some([10, 10, 189, 89]);
        canvas.draw_line(-1.0e7, 50.0, 1.0e7, 50.0, [0, 0, 0], 1, clip);
        for x in 0..200 {
            let idx = (50 * 200 + x) * 3;
            let inked = canvas.rgb[idx] == 0;
            assert_eq!(inked, (10..=189).contains(&x), "x={x}");
        }
        // Entirely outside the clip: nothing drawn.
        canvas.draw_line(-1.0e7, 5.0, 1.0e7, 5.0, [0, 0, 0], 1, clip);
        assert_eq!(canvas.rgb[(5 * 200 + 100) * 3], 255);
    }

    #[test]
    fn style_matching_follows_python_coercions() {
        let range = json!({
            "mode": "range",
            "style_spec": {"field": "v", "breaks": [{"min": 1, "max": 1, "color": "#ff0000"}]}
        });
        let hit = resolve_feature_color(Some(&range), &json!({"v": true}));
        assert_eq!(hit.0, "#ff0000");
        let hit = resolve_feature_color(Some(&range), &json!({"v": " 1 "}));
        assert_eq!(hit.0, "#ff0000");
        let miss = resolve_feature_color(Some(&range), &json!({"v": false}));
        assert_eq!(miss.0, "#9ca3af");

        let categorical = json!({
            "mode": "categorical",
            "style_spec": {"field": "v", "categories": [
                {"value": "1.0", "color": "#00ff00"},
                {"value": "1e-05", "color": "#0000ff"},
                {"value": "True", "color": "#123456"}
            ]}
        });
        let hit = resolve_feature_color(Some(&categorical), &json!({"v": 1.0}));
        assert_eq!(hit.0, "#00ff00");
        let hit = resolve_feature_color(Some(&categorical), &json!({"v": 0.00001}));
        assert_eq!(hit.0, "#0000ff");
        let hit = resolve_feature_color(Some(&categorical), &json!({"v": true}));
        assert_eq!(hit.0, "#123456");
    }
}
