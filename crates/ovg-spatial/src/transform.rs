//! Bounded portable CRS transformations for the R003 directed operation registry.
//!
//! Server reads do not use this kernel: they run the fixed PostGIS
//! `ST_TransformPipeline` steps (`ovg_storage::spatial_transform`) exactly like
//! the Python reference. This kernel is a portable cross-check, differentially
//! tested against recorded PostGIS/PROJ output below.

use std::f64::consts::PI;

use ovg_core::crs::{reference_transformation, FixtureRequirement, TransformationProfile};
use ovg_core::{
    lookup_readable_crs, AppError, ProjectionKind, WEB_MERCATOR_MAX_COORDINATE_METRE,
    WEB_MERCATOR_MAX_LATITUDE,
};
use serde::{Deserialize, Serialize};

const WGS84_A: f64 = 6_378_137.0;
const WGS84_INV_F: f64 = 298.257_223_563;
const WGS84_F: f64 = 1.0 / WGS84_INV_F;
const WGS84_E2: f64 = 2.0 * WGS84_F - WGS84_F * WGS84_F;
const ARCSEC_TO_RAD: f64 = PI / (180.0 * 3600.0);

/// 2D coordinate in `(x, y)` order (`longitude, latitude` for geographic; `easting, northing` for projected).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Coord2D {
    pub x: f64,
    pub y: f64,
}

impl Coord2D {
    #[must_use]
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

/// 7-parameter coordinate-frame Helmert transformation parameters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Helmert7Params {
    pub dx_m: f64,
    pub dy_m: f64,
    pub dz_m: f64,
    pub rx_arcsec: f64,
    pub ry_arcsec: f64,
    pub rz_arcsec: f64,
    pub ds_ppm: f64,
}

/// Official Vietnam Decision 05/2007/QD-BTNMT / EPSG:6960 VN-2000 -> WGS 84 parameters.
pub const EPSG_6960_VN2000_TO_WGS84: Helmert7Params = Helmert7Params {
    dx_m: -191.904_414_29,
    dy_m: -39.303_182_79,
    dz_m: -111.450_328_35,
    rx_arcsec: -0.009_288_36,
    ry_arcsec: 0.019_754_79,
    rz_arcsec: -0.004_273_72,
    ds_ppm: 0.252_906_278,
};

/// Pluggable CRS transformation backend (enables PostGIS/PROJ parity verification).
pub trait TransformBackend: Send + Sync {
    fn backend_name(&self) -> &'static str;
    fn transform_point(
        &self,
        source_epsg: u32,
        target_epsg: u32,
        coord: Coord2D,
    ) -> Result<Coord2D, AppError>;
}

/// Reference fixture provenance validated by the storage adapter.
///
/// These assertions must come from rechecking the source key, version, stored
/// CRS, and manifest digest against the trusted fixture in the current snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifiedReferenceProvenance {
    QuangNinh,
    QuangNinhHvs,
}

/// Portable CRS kernel restricted to operations approved by the reference registry.
#[derive(Debug, Clone, Copy, Default)]
pub struct PortableTransformKernel;

impl PortableTransformKernel {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Transform one coordinate from an independently verified reference fixture.
    pub fn transform_reference_point(
        &self,
        source_epsg: u32,
        target_epsg: u32,
        coord: Coord2D,
        provenance: VerifiedReferenceProvenance,
    ) -> Result<Coord2D, AppError> {
        transform(source_epsg, target_epsg, coord, Some(provenance))
    }
}

impl TransformBackend for PortableTransformKernel {
    fn backend_name(&self) -> &'static str {
        "ovg-spatial-portable-kruger-helmert"
    }

    fn transform_point(
        &self,
        source_epsg: u32,
        target_epsg: u32,
        coord: Coord2D,
    ) -> Result<Coord2D, AppError> {
        transform(source_epsg, target_epsg, coord, None)
    }
}

fn transform(
    source_epsg: u32,
    target_epsg: u32,
    coord: Coord2D,
    provenance: Option<VerifiedReferenceProvenance>,
) -> Result<Coord2D, AppError> {
    if !coord.x.is_finite() || !coord.y.is_finite() {
        return Err(AppError::validation(
            "INVALID_COORDINATE",
            "Coordinate values must be finite.",
        ));
    }

    let src =
        lookup_readable_crs(source_epsg).ok_or_else(|| AppError::unsupported_crs(source_epsg))?;
    let dst =
        lookup_readable_crs(target_epsg).ok_or_else(|| AppError::unsupported_crs(target_epsg))?;

    if source_epsg == target_epsg {
        validate_profile_bounds(src.code, coord)?;
        return Ok(coord);
    }

    let profile = reference_transformation(source_epsg, target_epsg)
        .ok_or_else(|| AppError::unsupported_transformation(source_epsg, target_epsg))?;
    let has_required_provenance = match profile.fixture_requirement {
        FixtureRequirement::None => true,
        FixtureRequirement::QuangNinh => provenance.is_some(),
        FixtureRequirement::DaNangCircular | FixtureRequirement::HaiPhongCircular | FixtureRequirement::DongNaiCircular => false,
        FixtureRequirement::QuangNinhHvs => {
            provenance == Some(VerifiedReferenceProvenance::QuangNinhHvs)
        }
    };
    if !has_required_provenance {
        return Err(AppError::transformation_out_of_area(
            "Operation evidence does not cover the source fixture.",
        ));
    }

    let src_geog = match src.projection {
        ProjectionKind::Geographic => coord,
        ProjectionKind::WebMercator { sphere_radius } => {
            web_mercator_to_wgs84(coord, sphere_radius)?
        }
        ProjectionKind::TransverseMercator {
            central_meridian,
            scale_factor,
            false_easting,
            false_northing,
            latitude_of_origin,
        } => tmerc_inverse(
            coord,
            central_meridian,
            scale_factor,
            false_easting,
            false_northing,
            latitude_of_origin,
        )?,
    };
    validate_operation_area(profile, src_geog)?;

    let dst_geog = match (src.datum == "Vietnam 2000", dst.datum == "Vietnam 2000") {
        (true, false) => helmert_vn2000_to_wgs84(src_geog, EPSG_6960_VN2000_TO_WGS84),
        (false, true) => helmert_wgs84_to_vn2000(src_geog, EPSG_6960_VN2000_TO_WGS84),
        _ => src_geog,
    };
    validate_operation_area(profile, dst_geog)?;

    match dst.projection {
        ProjectionKind::Geographic => Ok(dst_geog),
        ProjectionKind::WebMercator { sphere_radius } => {
            wgs84_to_web_mercator(dst_geog, sphere_radius)
        }
        ProjectionKind::TransverseMercator {
            central_meridian,
            scale_factor,
            false_easting,
            false_northing,
            latitude_of_origin,
        } => tmerc_forward(
            dst_geog,
            central_meridian,
            scale_factor,
            false_easting,
            false_northing,
            latitude_of_origin,
        ),
    }
}

fn validate_operation_area(
    profile: &TransformationProfile,
    coord: Coord2D,
) -> Result<(), AppError> {
    if !profile.geographic_bounds.contains(coord.x, coord.y, 0.0) {
        return Err(AppError::transformation_out_of_area(
            "Coordinate lies outside the verified operation area.",
        ));
    }
    Ok(())
}

fn validate_geographic(c: Coord2D) -> Result<(), AppError> {
    if !(-180.0..=180.0).contains(&c.x) || !(-90.0..=90.0).contains(&c.y) {
        return Err(AppError::transformation_out_of_area(
            "Geographic coordinates must lie within [-180, 180] longitude and [-90, 90] latitude.",
        ));
    }
    Ok(())
}

fn validate_profile_bounds(code: u32, c: Coord2D) -> Result<(), AppError> {
    match code {
        4326 | 4756 => validate_geographic(c),
        3857 => {
            if c.x.abs() > WEB_MERCATOR_MAX_COORDINATE_METRE
                || c.y.abs() > WEB_MERCATOR_MAX_COORDINATE_METRE
            {
                return Err(AppError::transformation_out_of_area(
                    "Web Mercator coordinate exceeds valid extent.",
                ));
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn wgs84_to_web_mercator(c: Coord2D, radius: f64) -> Result<Coord2D, AppError> {
    if !(-180.0..=180.0).contains(&c.x)
        || !(-WEB_MERCATOR_MAX_LATITUDE..=WEB_MERCATOR_MAX_LATITUDE).contains(&c.y)
    {
        return Err(AppError::transformation_out_of_area(
            "Coordinate lies outside valid Web Mercator latitude range.",
        ));
    }
    let x = radius * c.x.to_radians();
    let limit = WEB_MERCATOR_MAX_COORDINATE_METRE;
    // Inputs were checked above. Keep floating-point roundoff at the closed boundary
    // inside the mathematically equivalent projected extent.
    let y = (radius * c.y.to_radians().tan().asinh()).clamp(-limit, limit);
    Ok(Coord2D::new(x, y))
}

fn web_mercator_to_wgs84(c: Coord2D, radius: f64) -> Result<Coord2D, AppError> {
    if c.x.abs() > WEB_MERCATOR_MAX_COORDINATE_METRE
        || c.y.abs() > WEB_MERCATOR_MAX_COORDINATE_METRE
    {
        return Err(AppError::transformation_out_of_area(
            "Web Mercator coordinate lies outside valid projected extent.",
        ));
    }
    let lon = (c.x / WEB_MERCATOR_MAX_COORDINATE_METRE) * 180.0;
    let latitude_limit = WEB_MERCATOR_MAX_LATITUDE;
    let lat = (c.y / radius)
        .sinh()
        .atan()
        .to_degrees()
        .clamp(-latitude_limit, latitude_limit);
    Ok(Coord2D::new(lon, lat))
}

fn geodetic_to_ecef(lon_deg: f64, lat_deg: f64, h: f64) -> (f64, f64, f64) {
    let lon = lon_deg.to_radians();
    let lat = lat_deg.to_radians();
    let sin_lat = lat.sin();
    let cos_lat = lat.cos();
    let n = WGS84_A / (1.0 - WGS84_E2 * sin_lat * sin_lat).sqrt();
    let x = (n + h) * cos_lat * lon.cos();
    let y = (n + h) * cos_lat * lon.sin();
    let z = (n * (1.0 - WGS84_E2) + h) * sin_lat;
    (x, y, z)
}

fn ecef_to_geodetic(x: f64, y: f64, z: f64) -> Coord2D {
    let p = x.hypot(y);
    let lon = y.atan2(x);
    let mut lat = z.atan2(p * (1.0 - WGS84_E2));
    for _ in 0..8 {
        let sin_lat = lat.sin();
        let n = WGS84_A / (1.0 - WGS84_E2 * sin_lat * sin_lat).sqrt();
        lat = (z + WGS84_E2 * n * sin_lat).atan2(p);
    }
    Coord2D::new(lon.to_degrees(), lat.to_degrees())
}

fn helmert_vn2000_to_wgs84(c: Coord2D, p: Helmert7Params) -> Coord2D {
    let (xs, ys, zs) = geodetic_to_ecef(c.x, c.y, 0.0);
    let rx = p.rx_arcsec * ARCSEC_TO_RAD;
    let ry = p.ry_arcsec * ARCSEC_TO_RAD;
    let rz = p.rz_arcsec * ARCSEC_TO_RAD;
    let scale = 1.0 + p.ds_ppm * 1e-6;

    // EPSG:6960 uses the coordinate-frame rotation convention.
    let xt = p.dx_m + scale * (xs + rz * ys - ry * zs);
    let yt = p.dy_m + scale * (-rz * xs + ys + rx * zs);
    let zt = p.dz_m + scale * (ry * xs - rx * ys + zs);

    ecef_to_geodetic(xt, yt, zt)
}

fn helmert_wgs84_to_vn2000(c: Coord2D, p: Helmert7Params) -> Coord2D {
    let (xt, yt, zt) = geodetic_to_ecef(c.x, c.y, 0.0);
    let rx = p.rx_arcsec * ARCSEC_TO_RAD;
    let ry = p.ry_arcsec * ARCSEC_TO_RAD;
    let rz = p.rz_arcsec * ARCSEC_TO_RAD;
    let scale = 1.0 + p.ds_ppm * 1e-6;

    // Exact 3x3 linear system solve for (scale * R) * v = [xt - dx, yt - dy, zt - dz]
    let m = [
        [scale, scale * rz, -scale * ry],
        [-scale * rz, scale, scale * rx],
        [scale * ry, -scale * rx, scale],
    ];
    let rhs = [xt - p.dx_m, yt - p.dy_m, zt - p.dz_m];
    let [xs, ys, zs] = solve_3x3(m, rhs);

    ecef_to_geodetic(xs, ys, zs)
}

fn solve_3x3(m: [[f64; 3]; 3], b: [f64; 3]) -> [f64; 3] {
    let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);

    let inv = [
        [
            (m[1][1] * m[2][2] - m[1][2] * m[2][1]) / det,
            (m[0][2] * m[2][1] - m[0][1] * m[2][2]) / det,
            (m[0][1] * m[1][2] - m[0][2] * m[1][1]) / det,
        ],
        [
            (m[1][2] * m[2][0] - m[1][0] * m[2][2]) / det,
            (m[0][0] * m[2][2] - m[0][2] * m[2][0]) / det,
            (m[0][2] * m[1][0] - m[0][0] * m[1][2]) / det,
        ],
        [
            (m[1][0] * m[2][1] - m[1][1] * m[2][0]) / det,
            (m[0][1] * m[2][0] - m[0][0] * m[2][1]) / det,
            (m[0][0] * m[1][1] - m[0][1] * m[1][0]) / det,
        ],
    ];

    [
        inv[0][0] * b[0] + inv[0][1] * b[1] + inv[0][2] * b[2],
        inv[1][0] * b[0] + inv[1][1] * b[1] + inv[1][2] * b[2],
        inv[2][0] * b[0] + inv[2][1] * b[1] + inv[2][2] * b[2],
    ]
}

fn kruger_constants() -> (f64, [f64; 4], [f64; 4], [f64; 4]) {
    let n = WGS84_F / (2.0 - WGS84_F);
    let n2 = n * n;
    let n3 = n2 * n;
    let n4 = n2 * n2;

    let a_rect = (WGS84_A / (1.0 + n)) * (1.0 + n2 / 4.0 + n4 / 64.0);

    let alpha = [
        n / 2.0 - (2.0 / 3.0) * n2 + (5.0 / 16.0) * n3 + (41.0 / 180.0) * n4,
        (13.0 / 48.0) * n2 - (3.0 / 5.0) * n3 + (557.0 / 1440.0) * n4,
        (61.0 / 240.0) * n3 - (103.0 / 140.0) * n4,
        (49_561.0 / 161_280.0) * n4,
    ];

    let beta = [
        n / 2.0 - (2.0 / 3.0) * n2 + (37.0 / 96.0) * n3 - (1.0 / 360.0) * n4,
        (1.0 / 48.0) * n2 + (1.0 / 15.0) * n3 - (437.0 / 1440.0) * n4,
        (17.0 / 480.0) * n3 - (37.0 / 840.0) * n4,
        (4_397.0 / 161_280.0) * n4,
    ];

    let delta = [
        2.0 * n - (2.0 / 3.0) * n2 - 2.0 * n3 + (116.0 / 45.0) * n4,
        (7.0 / 3.0) * n2 - (8.0 / 5.0) * n3 - (227.0 / 45.0) * n4,
        (56.0 / 15.0) * n3 - (136.0 / 35.0) * n4,
        (4_279.0 / 630.0) * n4,
    ];

    (a_rect, alpha, beta, delta)
}

fn tmerc_forward(
    geog: Coord2D,
    central_meridian_deg: f64,
    scale_factor: f64,
    false_easting: f64,
    false_northing: f64,
    latitude_of_origin_deg: f64,
) -> Result<Coord2D, AppError> {
    validate_geographic(geog)?;
    let (a_rect, alpha, _, _) = kruger_constants();
    let e = WGS84_E2.sqrt();
    let lat = geog.y.to_radians();
    let d_lon = (geog.x - central_meridian_deg).to_radians();

    let t = (lat.tan().asinh() - e * (e * lat.sin()).atanh()).sinh();
    let xi_p = t.atan2(d_lon.cos());
    let eta_p = (d_lon.sin() / (t * t + d_lon.cos().powi(2)).sqrt()).asinh();

    let mut xi = xi_p;
    let mut eta = eta_p;
    for (idx, &a_j) in alpha.iter().enumerate() {
        let k = 2.0 * (idx + 1) as f64;
        xi += a_j * (k * xi_p).sin() * (k * eta_p).cosh();
        eta += a_j * (k * xi_p).cos() * (k * eta_p).sinh();
    }

    let origin_northing = if latitude_of_origin_deg.abs() > 0.0 {
        let lat0 = latitude_of_origin_deg.to_radians();
        let t0 = (lat0.tan().asinh() - e * (e * lat0.sin()).atanh()).sinh();
        let xi0_p = t0.atan();
        let mut xi0 = xi0_p;
        for (idx, &a_j) in alpha.iter().enumerate() {
            let k = 2.0 * (idx + 1) as f64;
            xi0 += a_j * (k * xi0_p).sin();
        }
        scale_factor * a_rect * xi0
    } else {
        0.0
    };

    let easting = false_easting + scale_factor * a_rect * eta;
    let northing = false_northing + scale_factor * a_rect * xi - origin_northing;
    Ok(Coord2D::new(easting, northing))
}

fn tmerc_inverse(
    proj: Coord2D,
    central_meridian_deg: f64,
    scale_factor: f64,
    false_easting: f64,
    false_northing: f64,
    latitude_of_origin_deg: f64,
) -> Result<Coord2D, AppError> {
    let (a_rect, alpha, beta, delta) = kruger_constants();
    let e = WGS84_E2.sqrt();
    let origin_northing = if latitude_of_origin_deg.abs() > 0.0 {
        let lat0 = latitude_of_origin_deg.to_radians();
        let t0 = (lat0.tan().asinh() - e * (e * lat0.sin()).atanh()).sinh();
        let xi0_p = t0.atan();
        let mut xi0 = xi0_p;
        for (idx, &a_j) in alpha.iter().enumerate() {
            let k = 2.0 * (idx + 1) as f64;
            xi0 += a_j * (k * xi0_p).sin();
        }
        scale_factor * a_rect * xi0
    } else {
        0.0
    };

    let xi = (proj.y - false_northing + origin_northing) / (scale_factor * a_rect);
    let eta = (proj.x - false_easting) / (scale_factor * a_rect);

    let mut xi_p = xi;
    let mut eta_p = eta;
    for (idx, &b_j) in beta.iter().enumerate() {
        let k = 2.0 * (idx + 1) as f64;
        xi_p -= b_j * (k * xi).sin() * (k * eta).cosh();
        eta_p -= b_j * (k * xi).cos() * (k * eta).sinh();
    }

    let sin_chi = (xi_p.sin() / eta_p.cosh()).clamp(-1.0, 1.0);
    let chi = sin_chi.asin();
    let mut lat = chi;
    for (idx, &d_j) in delta.iter().enumerate() {
        let k = 2.0 * (idx + 1) as f64;
        lat += d_j * (k * chi).sin();
    }
    let lon = central_meridian_deg.to_radians() + eta_p.sinh().atan2(xi_p.cos());

    let out = Coord2D::new(lon.to_degrees(), lat.to_degrees());
    validate_geographic(out)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unknown_crs_and_unapproved_transformation_edges() {
        let kernel = PortableTransformKernel::new();
        let input = Coord2D::new(107.165, 20.715);
        for code in [3405, 3406, 9209, 99999] {
            let error = kernel.transform_point(4326, code, input).unwrap_err();
            assert_eq!(error.code(), "UNSUPPORTED_CRS");
        }
        for (source, target) in [(5899, 3857), (3857, 4756), (4756, 3857)] {
            let error = kernel.transform_point(source, target, input).unwrap_err();
            assert_eq!(error.code(), "UNSUPPORTED_TRANSFORMATION");
        }
    }

    #[test]
    fn regional_operations_require_matching_provenance_and_geographic_bounds() {
        let kernel = PortableTransformKernel::new();
        let inside = Coord2D::new(107.165, 20.715);
        assert_eq!(
            kernel
                .transform_point(4326, 5899, inside)
                .unwrap_err()
                .code(),
            "TRANSFORMATION_OUT_OF_AREA"
        );
        assert_eq!(
            kernel
                .transform_reference_point(
                    4326,
                    5899,
                    inside,
                    VerifiedReferenceProvenance::QuangNinh
                )
                .unwrap_err()
                .code(),
            "TRANSFORMATION_OUT_OF_AREA"
        );
        let provenance = VerifiedReferenceProvenance::QuangNinhHvs;
        for (source, target, point) in [
            (4326, 5899, Coord2D::new(107.181, 20.715)),
            (5899, 4756, Coord2D::new(500_000.0, 0.0)),
            // The source is inside, but the EPSG:6960 output crosses the approved region.
            (4756, 4326, Coord2D::new(108.07, 21.0)),
        ] {
            let error = kernel
                .transform_reference_point(source, target, point, provenance)
                .unwrap_err();
            assert_eq!(error.code(), "TRANSFORMATION_OUT_OF_AREA");
        }
    }

    /// Differential fixture recorded from PostGIS 3.7.0 / PROJ 9.9.0.
    const POSTGIS_FIXTURE: &str =
        include_str!("../tests/fixtures/vn2000_postgis_differential.json");

    /// Central meridian and scale of every VN-2000 zone in the fixture, as
    /// reported by `postgis_srs('EPSG', code)`.
    fn zone(epsg: u64) -> (f64, f64) {
        let central_meridian = match epsg {
            3405 | 5897 => 105.0,
            3406 => 111.0,
            5896 => 102.0,
            5898 => 108.0,
            5899 => 107.75,
            9205 => 103.0,
            9206 => 104.0,
            9207 => 104.5,
            9208 => 104.75,
            9209 => 105.5,
            9210 => 105.75,
            9211 => 106.0,
            9212 => 106.25,
            9213 => 106.5,
            9214 => 107.0,
            9215 => 107.25,
            9216 => 107.5,
            9217 => 108.25,
            9218 => 108.5,
            other => panic!("no zone parameters for EPSG:{other}"),
        };
        let scale = if matches!(epsg, 3405 | 3406) {
            0.9996
        } else {
            0.9999
        };
        (central_meridian, scale)
    }

    fn xy(value: &serde_json::Value) -> Coord2D {
        Coord2D::new(value[0].as_f64().unwrap(), value[1].as_f64().unwrap())
    }

    fn euclidean_m(a: Coord2D, b: Coord2D) -> f64 {
        (a.x - b.x).hypot(a.y - b.y)
    }

    /// Local ellipsoidal metric; sub-millimetric at the residual scales tested.
    fn geodesic_m(a: Coord2D, b: Coord2D) -> f64 {
        let latitude = b.y.to_radians();
        let w = (1.0 - WGS84_E2 * latitude.sin().powi(2)).sqrt();
        let prime_vertical = WGS84_A / w;
        let meridional = WGS84_A * (1.0 - WGS84_E2) / w.powi(3);
        ((a.x - b.x).to_radians() * prime_vertical * latitude.cos())
            .hypot((a.y - b.y).to_radians() * meridional)
    }

    #[test]
    fn portable_kernel_matches_postgis_explicit_operations_within_one_centimetre() {
        let fixture: serde_json::Value = serde_json::from_str(POSTGIS_FIXTURE).unwrap();
        let points = fixture["points"].as_array().unwrap();
        assert_eq!(points.len(), 39);
        let p = EPSG_6960_VN2000_TO_WGS84;
        let mut worst = [0.0_f64; 6];
        for point in points {
            let id = point["id"].as_str().unwrap();
            let (cm, k) = zone(point["epsg"].as_u64().unwrap());
            let geographic = xy(&point["vn2000_lonlat"]);
            let wgs84 = xy(&point["wgs84_input"]);
            let projected = xy(&point["projected_input"]);
            let inverse = tmerc_inverse(projected, cm, k, 500_000.0, 0.0, 0.0).unwrap();
            let forward = tmerc_forward(geographic, cm, k, 500_000.0, 0.0, 0.0).unwrap();
            let datum_inverse = helmert_wgs84_to_vn2000(wgs84, p);
            let reprojected = tmerc_forward(datum_inverse, cm, k, 500_000.0, 0.0, 0.0).unwrap();
            let checks = [
                (
                    "EPSG:6960 forward",
                    geodesic_m(
                        helmert_vn2000_to_wgs84(geographic, p),
                        xy(&point["wgs84_pipeline_6960"]),
                    ),
                ),
                (
                    "EPSG:6960 inverse",
                    geodesic_m(
                        datum_inverse,
                        xy(&point["wgs84_input_to_vn2000_pipeline_6960"]),
                    ),
                ),
                ("TM forward", euclidean_m(forward, xy(&point["projected"]))),
                (
                    "TM inverse",
                    geodesic_m(inverse, xy(&point["projected_input_to_vn2000"])),
                ),
                (
                    "TM inverse then EPSG:6960",
                    geodesic_m(
                        helmert_vn2000_to_wgs84(inverse, p),
                        xy(&point["projected_input_to_wgs84"]),
                    ),
                ),
                (
                    "inverse EPSG:6960 then TM",
                    euclidean_m(reprojected, xy(&point["wgs84_input_to_projected"])),
                ),
            ];
            for (index, (name, residual)) in checks.iter().enumerate() {
                assert!(*residual <= 0.01, "{id} {name}: {residual} m");
                worst[index] = worst[index].max(*residual);
            }
        }
        println!("maximum residuals in metres: {worst:?}");
    }

    #[test]
    fn plain_st_transform_is_not_the_reference_operation() {
        // Reads use ST_TransformPipeline: PROJ's default VN-2000 -> WGS 84
        // choice departs from EPSG:6960 by about 0.8 m in southern provinces.
        let fixture: serde_json::Value = serde_json::from_str(POSTGIS_FIXTURE).unwrap();
        let worst = fixture["points"]
            .as_array()
            .unwrap()
            .iter()
            .map(|point| {
                geodesic_m(
                    xy(&point["wgs84_st_transform"]),
                    xy(&point["wgs84_pipeline_6960"]),
                )
            })
            .fold(0.0, f64::max);
        assert!(worst > 0.5, "{worst}");
    }

    #[test]
    fn web_mercator_accepts_exact_limits_and_rejects_values_just_outside() {
        let kernel = PortableTransformKernel::new();
        for sign in [-1.0, 1.0] {
            let geographic = Coord2D::new(sign * 180.0, sign * WEB_MERCATOR_MAX_LATITUDE);
            let projected = kernel.transform_point(4326, 3857, geographic).unwrap();
            let round_trip = kernel.transform_point(3857, 4326, projected).unwrap();
            assert!((round_trip.x - geographic.x).abs() < 1e-8);
            assert!((round_trip.y - geographic.y).abs() < 1e-8);

            let next_latitude = f64::from_bits(WEB_MERCATOR_MAX_LATITUDE.to_bits() + 1);
            let invalid = Coord2D::new(0.0, sign * next_latitude);
            assert_eq!(
                kernel
                    .transform_point(4326, 3857, invalid)
                    .unwrap_err()
                    .code(),
                "TRANSFORMATION_OUT_OF_AREA"
            );

            let next_easting = f64::from_bits(WEB_MERCATOR_MAX_COORDINATE_METRE.to_bits() + 1);
            let invalid = Coord2D::new(sign * next_easting, 0.0);
            assert_eq!(
                kernel
                    .transform_point(3857, 4326, invalid)
                    .unwrap_err()
                    .code(),
                "TRANSFORMATION_OUT_OF_AREA"
            );
        }
    }
}
