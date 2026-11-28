//! Trusted R003 reference fixture definitions, pin digests, and native PostGIS verification.

use std::collections::BTreeMap;

use chrono::{Duration, SecondsFormat, TimeZone, Utc};
use ovg_core::AppError;
use serde_json::{json, Value};

use crate::editing::domain::sha256_py_json;
use crate::spatial_pins::{schema_revision_digest, FieldSpec};

use super::db_error;

const FIXTURE_VERSION: &str = "r003-reference-v1";

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Provenance {
    Synthetic,
    AnalyticWebMercator,
    QuangNinhCircular,
    QuangNinhHvs,
    DaNangCircular,
    HaiPhongCircular,
    DongNaiCircular,
}

impl Provenance {
    fn as_str(self) -> &'static str {
        match self {
            Self::Synthetic => "synthetic",
            Self::AnalyticWebMercator => "analytic_web_mercator",
            Self::QuangNinhCircular => "quang_ninh_circular_50_2025",
            Self::QuangNinhHvs => "quang_ninh_mae_eia_hvs",
            Self::DaNangCircular => "da_nang_circular_46_2025",
            Self::HaiPhongCircular => "hai_phong_circular_49_2025",
            Self::DongNaiCircular => "dong_nai_circular_02_2026",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ReasonCode {
    NullGeometry,
    EmptyGeometry,
    MissingCrs,
    UnsupportedCrs,
    CrsMismatch,
    UnsupportedGeometry,
    UnsupportedDimensions,
    GeometryFamilyMismatch,
    InvalidGeometry,
    AxisConflict,
    CoordinateOutOfDomain,
    ExtentWrapUnsupported,
}

impl ReasonCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NullGeometry => "NULL_GEOMETRY",
            Self::EmptyGeometry => "EMPTY_GEOMETRY",
            Self::MissingCrs => "MISSING_CRS",
            Self::UnsupportedCrs => "UNSUPPORTED_CRS",
            Self::CrsMismatch => "CRS_MISMATCH",
            Self::UnsupportedGeometry => "UNSUPPORTED_GEOMETRY",
            Self::UnsupportedDimensions => "UNSUPPORTED_DIMENSIONS",
            Self::GeometryFamilyMismatch => "GEOMETRY_FAMILY_MISMATCH",
            Self::InvalidGeometry => "INVALID_GEOMETRY",
            Self::AxisConflict => "AXIS_CONFLICT",
            Self::CoordinateOutOfDomain => "COORDINATE_OUT_OF_DOMAIN",
            Self::ExtentWrapUnsupported => "EXTENT_WRAP_UNSUPPORTED",
        }
    }

    pub fn message(self) -> &'static str {
        match self {
            Self::NullGeometry => "The reference contains absent geometry.",
            Self::EmptyGeometry => "The reference contains an empty geometry.",
            Self::MissingCrs => "Native CRS metadata is missing.",
            Self::UnsupportedCrs => "The native CRS is not readable in this release.",
            Self::CrsMismatch => "Declared CRS differs from verified native CRS.",
            Self::UnsupportedGeometry => "The inspected geometry family is unsupported.",
            Self::UnsupportedDimensions => "Only XY geometry is readable.",
            Self::GeometryFamilyMismatch => "Declared and inspected families differ.",
            Self::InvalidGeometry => "Native geometry failed topology validation.",
            Self::AxisConflict => "Native axes conflict with the independent control.",
            Self::CoordinateOutOfDomain => "Native coordinates outside CRS domain.",
            Self::ExtentWrapUnsupported => "Wrapped geographic extents are unsupported.",
        }
    }

    pub fn to_wire(self) -> Value {
        json!({
            "code": self.as_str(),
            "message": self.message(),
        })
    }
}

#[derive(Clone)]
#[allow(dead_code)]
pub struct FixtureGeometry {
    pub gtype: &'static str,
    pub dimensions: &'static str,
    pub srid: Option<i32>,
    pub coordinates: Value,
    pub wkt: String,
    pub is_empty: bool,
}

#[derive(Clone)]
pub struct FixtureRecord {
    pub feature_id: String,
    pub attributes: Value,
    pub geometry: Option<FixtureGeometry>,
    pub geometry_state: &'static str,
}

#[derive(Clone)]
pub struct ReferenceSpec {
    pub key: &'static str,
    pub declared_crs: Option<i32>,
    pub stored_crs: Option<i32>,
    pub declared_family: &'static str,
    pub declared_dimensions: &'static str,
    pub expected_family: &'static str,
    pub expected_dimensions: &'static str,
    pub expected_count: usize,
    pub expected_bounds: Option<[f64; 4]>,
    pub provenance: Provenance,
    pub expected_reason: Option<ReasonCode>,
    pub axis_control_xy: Option<(f64, f64)>,
    pub correctable_to_crs: Option<i32>,
}

pub fn reference_fields(schema_revision: &str) -> Vec<FieldSpec> {
    [
        ("fld_feature_code", "Feature code", 0, "text", false),
        ("fld_name", "Name", 1, "text", false),
        ("fld_rank", "Rank", 2, "integer", false),
        ("fld_measure", "Measure", 3, "decimal", true),
        ("fld_active", "Active", 4, "boolean", false),
        ("fld_observed_at", "Observed at", 5, "timestamp", true),
        ("fld_note", "Note", 6, "text", true),
    ]
    .into_iter()
    .map(
        |(field_id, name, ordinal, field_type, nullable)| FieldSpec {
            field_id: field_id.to_owned(),
            name: name.to_owned(),
            ordinal,
            field_type: field_type.to_owned(),
            nullable,
            schema_revision: schema_revision.to_owned(),
        },
    )
    .collect()
}

fn canonical_cents(cents: i64) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let abs = cents.unsigned_abs();
    let whole = abs / 100;
    let frac = abs % 100;
    if frac == 0 {
        format!("{sign}{whole}")
    } else if frac % 10 == 0 {
        format!("{sign}{whole}.{}", frac / 10)
    } else {
        format!("{sign}{whole}.{frac:02}")
    }
}

fn build_attributes(sequence: i64, feature_id: &str, label: &str) -> Value {
    let cents = (sequence - 1) * 37 * if sequence % 2 == 0 { -1 } else { 1 };
    let epoch = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
    let observed_ts = epoch + Duration::minutes(sequence - 1);
    let observed = if sequence % 11 == 0 {
        Value::Null
    } else {
        Value::String(observed_ts.to_rfc3339_opts(SecondsFormat::Secs, true))
    };
    let measure = if sequence % 7 == 0 {
        Value::Null
    } else {
        Value::String(canonical_cents(cents))
    };
    let note = if sequence % 5 == 0 {
        Value::Null
    } else if sequence % 5 == 1 {
        Value::String(String::new())
    } else {
        Value::String(format!("note-{sequence:06}"))
    };
    json!({
        "fld_feature_code": feature_id,
        "fld_name": label,
        "fld_rank": sequence % 1000,
        "fld_measure": measure,
        "fld_active": sequence % 2 == 0,
        "fld_observed_at": observed,
        "fld_note": note,
    })
}

fn dms(d: i64, m: i64, hundredths: i64) -> f64 {
    (d * 360_000 + m * 6_000 + hundredths) as f64 / 360_000.0
}

fn microdegrees(x: i64, y: i64) -> (f64, f64) {
    (x as f64 / 1_000_000.0, y as f64 / 1_000_000.0)
}

fn point_geom(x: f64, y: f64, srid: Option<i32>) -> FixtureGeometry {
    FixtureGeometry {
        gtype: "Point",
        dimensions: "XY",
        srid,
        coordinates: json!([x, y]),
        wkt: format!("POINT ({x} {y})"),
        is_empty: false,
    }
}

fn line_geom(pts: &[(f64, f64)], srid: Option<i32>) -> FixtureGeometry {
    let coords = Value::Array(pts.iter().map(|&(x, y)| json!([x, y])).collect());
    let wkt_pts = pts
        .iter()
        .map(|&(x, y)| format!("{x} {y}"))
        .collect::<Vec<_>>()
        .join(",");
    FixtureGeometry {
        gtype: "LineString",
        dimensions: "XY",
        srid,
        coordinates: coords,
        wkt: format!("LINESTRING ({wkt_pts})"),
        is_empty: false,
    }
}

fn polygon_geom(ring: &[(f64, f64)], srid: Option<i32>) -> FixtureGeometry {
    let coords = json!([ring.iter().map(|&(x, y)| json!([x, y])).collect::<Vec<_>>()]);
    let wkt_pts = ring
        .iter()
        .map(|&(x, y)| format!("{x} {y}"))
        .collect::<Vec<_>>()
        .join(",");
    FixtureGeometry {
        gtype: "Polygon",
        dimensions: "XY",
        srid,
        coordinates: coords,
        wkt: format!("POLYGON (({wkt_pts}))"),
        is_empty: false,
    }
}

fn record(
    feature_id: &str,
    geometry: Option<FixtureGeometry>,
    sequence: i64,
    label: Option<&str>,
    geometry_state: &'static str,
) -> FixtureRecord {
    let default_label = format!("Reference {feature_id}");
    FixtureRecord {
        feature_id: feature_id.to_owned(),
        attributes: build_attributes(sequence, feature_id, label.unwrap_or(&default_label)),
        geometry,
        geometry_state,
    }
}

type Dms = (i64, i64, i64);
type CircularRow = (&'static str, Dms, Dms, Dms, Dms);
type HvsRow = (&'static str, (f64, f64), Dms, Dms, Dms, Dms);

// Public coordinate pairs from the appendices of Circular 46/2025/TT-BXD
// (DN1-DN3) and Circular 49/2025/TT-BXD (HP1-HP3). XY = longitude, latitude.
const DA_NANG: [CircularRow; 3] = [
    (
        "DN1",
        (108, 12, 1740),
        (16, 12, 3770),
        (108, 12, 2400),
        (16, 12, 3400),
    ),
    (
        "DN2",
        (108, 14, 466),
        (16, 13, 5642),
        (108, 14, 1122),
        (16, 13, 5274),
    ),
    (
        "DN3",
        (108, 16, 3714),
        (16, 10, 4240),
        (108, 16, 4370),
        (16, 10, 3871),
    ),
];
const HAI_PHONG: [CircularRow; 3] = [
    (
        "HP1",
        (106, 48, 4020),
        (20, 46, 4070),
        (106, 48, 4690),
        (20, 46, 3720),
    ),
    (
        "HP2",
        (106, 48, 4020),
        (20, 37, 5340),
        (106, 48, 4690),
        (20, 37, 4980),
    ),
    (
        "HP3",
        (106, 44, 3480),
        (20, 41, 3980),
        (106, 44, 4160),
        (20, 41, 3620),
    ),
];
// Circular 02/2026/TT-BXD: selected independently matching control pairs.
// HD1 is excluded: its published pair differs by approximately 19 m.
const DONG_NAI: [CircularRow; 3] = [
    (
        "HD2",
        (106, 51, 3777),
        (10, 38, 3407),
        (106, 51, 4420),
        (10, 38, 3040),
    ),
    (
        "HD5",
        (106, 45, 3094),
        (10, 41, 4808),
        (106, 45, 3736),
        (10, 41, 4441),
    ),
    (
        "HD6",
        (106, 45, 1337),
        (10, 42, 937),
        (106, 45, 1980),
        (10, 42, 571),
    ),
];
pub(super) fn regional_controls(p: Provenance) -> Option<Vec<((f64, f64), (f64, f64))>> {
    let rows = match p {
        Provenance::DaNangCircular => &DA_NANG,
        Provenance::HaiPhongCircular => &HAI_PHONG,
        Provenance::DongNaiCircular => &DONG_NAI,
        _ => return None,
    };
    let xy = |lon: Dms, lat: Dms| (dms(lon.0, lon.1, lon.2), dms(lat.0, lat.1, lat.2));
    Some(
        rows.iter()
            .map(|&(_, a, b, c, d)| (xy(a, b), xy(c, d)))
            .collect(),
    )
}
const CIRCULAR: [CircularRow; 5] = [
    (
        "VH1",
        (108, 1, 1000),
        (21, 24, 4000),
        (108, 1, 1680),
        (21, 24, 3640),
    ),
    (
        "VH2",
        (108, 3, 3530),
        (21, 24, 5210),
        (108, 3, 4210),
        (21, 24, 4850),
    ),
    (
        "VH10",
        (107, 56, 4600),
        (21, 14, 1050),
        (107, 56, 5280),
        (21, 14, 690),
    ),
    (
        "CT1",
        (107, 45, 4080),
        (20, 58, 1430),
        (107, 45, 4760),
        (20, 58, 1070),
    ),
    (
        "QY1",
        (106, 52, 5920),
        (20, 50, 780),
        (106, 53, 600),
        (20, 50, 420),
    ),
];

const HVS: [HvsRow; 4] = [
    (
        "HVS-QN1",
        (438442.69, 2291039.83),
        (107, 9, 3244),
        (20, 42, 4209),
        (107, 9, 3919),
        (20, 42, 3848),
    ),
    (
        "HVS-QN2",
        (437957.55, 2291878.33),
        (107, 9, 1556),
        (20, 43, 930),
        (107, 9, 2231),
        (20, 43, 569),
    ),
    (
        "HVS-QN3",
        (438891.96, 2292185.83),
        (107, 9, 4782),
        (20, 43, 1941),
        (107, 9, 5457),
        (20, 43, 1580),
    ),
    (
        "HVS-QN4",
        (439365.92, 2291378.72),
        (107, 10, 430),
        (20, 42, 5322),
        (107, 10, 1105),
        (20, 42, 4961),
    ),
];

fn point_bounds(pts: &[(&str, (f64, f64))]) -> [f64; 4] {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for &(_, (x, y)) in pts {
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    [min_x, min_y, max_x, max_y]
}

fn point_records(pts: &[(&str, (f64, f64))], srid: i32) -> Vec<FixtureRecord> {
    pts.iter()
        .enumerate()
        .map(|(i, &(id, (x, y)))| {
            record(
                id,
                Some(point_geom(x, y, Some(srid))),
                i as i64 + 1,
                None,
                "present",
            )
        })
        .collect()
}

pub fn get_reference_spec(key: &str) -> Option<(ReferenceSpec, Vec<FixtureRecord>)> {
    let hvs_qn1_wgs = (dms(107, 9, 3919), dms(20, 42, 3848));
    let good_point = point_geom(106.6, 10.7, Some(4326));
    let good_line = line_geom(&[(106.6, 10.7), (106.601, 10.701)], Some(4326));

    match key {
        "reference_points_4326" => {
            let mut recs = Vec::with_capacity(4000);
            for seq in 1..=4000_i64 {
                let row = (seq - 1) / 100;
                let col = (seq - 1) % 100;
                let (x, y) = microdegrees(106_600_000 + 1_000 * col, 10_700_000 + 1_000 * row);
                recs.push(record(
                    &format!("point-{seq:06}"),
                    Some(point_geom(x, y, Some(4326))),
                    seq,
                    Some(&format!("Point {seq:06}")),
                    "present",
                ));
            }
            Some((
                ReferenceSpec {
                    key: "reference_points_4326",
                    declared_crs: Some(4326),
                    stored_crs: Some(4326),
                    declared_family: "Point",
                    declared_dimensions: "XY",
                    expected_family: "Point",
                    expected_dimensions: "XY",
                    expected_count: 4000,
                    expected_bounds: Some([106.6, 10.7, 106.699, 10.739]),
                    provenance: Provenance::Synthetic,
                    expected_reason: None,
                    axis_control_xy: None,
                    correctable_to_crs: None,
                },
                recs,
            ))
        }
        "reference_lines_4326" => {
            let mut recs = Vec::with_capacity(3000);
            for seq in 1..=3000_i64 {
                let row = (seq - 1) / 100;
                let col = (seq - 1) % 100;
                let x0 = 106_600_000 + 1_000 * col;
                let y0 = 10_700_000 + 1_000 * row;
                let pts = [
                    microdegrees(x0, y0),
                    microdegrees(x0 + 300, y0 + 200),
                    microdegrees(x0 + 600, y0 + 100),
                ];
                recs.push(record(
                    &format!("line-{seq:06}"),
                    Some(line_geom(&pts, Some(4326))),
                    seq,
                    Some(&format!("Line {seq:06}")),
                    "present",
                ));
            }
            Some((
                ReferenceSpec {
                    key: "reference_lines_4326",
                    declared_crs: Some(4326),
                    stored_crs: Some(4326),
                    declared_family: "LineString",
                    declared_dimensions: "XY",
                    expected_family: "LineString",
                    expected_dimensions: "XY",
                    expected_count: 3000,
                    expected_bounds: Some([106.6, 10.7, 106.6996, 10.7292]),
                    provenance: Provenance::Synthetic,
                    expected_reason: None,
                    axis_control_xy: None,
                    correctable_to_crs: None,
                },
                recs,
            ))
        }
        "reference_polygons_4326" => {
            let mut recs = Vec::with_capacity(3000);
            for seq in 1..=3000_i64 {
                let row = (seq - 1) / 100;
                let col = (seq - 1) % 100;
                let x0 = 106_600_000 + 1_000 * col;
                let y0 = 10_700_000 + 1_000 * row;
                let ring = [
                    microdegrees(x0, y0),
                    microdegrees(x0 + 600, y0),
                    microdegrees(x0 + 600, y0 + 400),
                    microdegrees(x0, y0 + 400),
                    microdegrees(x0, y0),
                ];
                recs.push(record(
                    &format!("polygon-{seq:06}"),
                    Some(polygon_geom(&ring, Some(4326))),
                    seq,
                    Some(&format!("Polygon {seq:06}")),
                    "present",
                ));
            }
            Some((
                ReferenceSpec {
                    key: "reference_polygons_4326",
                    declared_crs: Some(4326),
                    stored_crs: Some(4326),
                    declared_family: "Polygon",
                    declared_dimensions: "XY",
                    expected_family: "Polygon",
                    expected_dimensions: "XY",
                    expected_count: 3000,
                    expected_bounds: Some([106.6, 10.7, 106.6996, 10.7294]),
                    provenance: Provenance::Synthetic,
                    expected_reason: None,
                    axis_control_xy: None,
                    correctable_to_crs: None,
                },
                recs,
            ))
        }
        "reference_points_3857" => {
            #[allow(clippy::excessive_precision)]
            let pts = [
                ("merc-000001", (11866657.718562961, 1198103.040560261)),
                ("merc-000002", (11877745.139845973, 1201433.906852360)),
                ("merc-000003", (11928662.674934816, 2357927.646019978)),
            ];
            let mut recs = point_records(&pts, 3857);
            recs.sort_by(|a, b| a.feature_id.cmp(&b.feature_id));
            Some((
                ReferenceSpec {
                    key: "reference_points_3857",
                    declared_crs: Some(3857),
                    stored_crs: Some(3857),
                    declared_family: "Point",
                    declared_dimensions: "XY",
                    expected_family: "Point",
                    expected_dimensions: "XY",
                    expected_count: pts.len(),
                    expected_bounds: Some(point_bounds(&pts)),
                    provenance: Provenance::AnalyticWebMercator,
                    expected_reason: None,
                    axis_control_xy: None,
                    correctable_to_crs: None,
                },
                recs,
            ))
        }
        "reference_points_4756_da_nang" => {
            let pts: Vec<(&str, (f64, f64))> = DA_NANG
                .iter()
                .map(|&(id, lon, lat, _, _)| {
                    (id, (dms(lon.0, lon.1, lon.2), dms(lat.0, lat.1, lat.2)))
                })
                .collect();
            let mut recs = point_records(&pts, 4756);
            recs.sort_by(|a, b| a.feature_id.cmp(&b.feature_id));
            Some((
                ReferenceSpec {
                    key: "reference_points_4756_da_nang",
                    declared_crs: Some(4756),
                    stored_crs: Some(4756),
                    declared_family: "Point",
                    declared_dimensions: "XY",
                    expected_family: "Point",
                    expected_dimensions: "XY",
                    expected_count: pts.len(),
                    expected_bounds: Some(point_bounds(&pts)),
                    provenance: Provenance::DaNangCircular,
                    expected_reason: None,
                    axis_control_xy: None,
                    correctable_to_crs: None,
                },
                recs,
            ))
        }
        "reference_points_4326_da_nang" => {
            let pts: Vec<(&str, (f64, f64))> = DA_NANG
                .iter()
                .map(|&(id, _, _, lon, lat)| {
                    (id, (dms(lon.0, lon.1, lon.2), dms(lat.0, lat.1, lat.2)))
                })
                .collect();
            let mut recs = point_records(&pts, 4326);
            recs.sort_by(|a, b| a.feature_id.cmp(&b.feature_id));
            Some((
                ReferenceSpec {
                    key: "reference_points_4326_da_nang",
                    declared_crs: Some(4326),
                    stored_crs: Some(4326),
                    declared_family: "Point",
                    declared_dimensions: "XY",
                    expected_family: "Point",
                    expected_dimensions: "XY",
                    expected_count: pts.len(),
                    expected_bounds: Some(point_bounds(&pts)),
                    provenance: Provenance::DaNangCircular,
                    expected_reason: None,
                    axis_control_xy: None,
                    correctable_to_crs: None,
                },
                recs,
            ))
        }
        "reference_points_4756_hai_phong" => {
            let pts: Vec<(&str, (f64, f64))> = HAI_PHONG
                .iter()
                .map(|&(id, lon, lat, _, _)| {
                    (id, (dms(lon.0, lon.1, lon.2), dms(lat.0, lat.1, lat.2)))
                })
                .collect();
            let mut recs = point_records(&pts, 4756);
            recs.sort_by(|a, b| a.feature_id.cmp(&b.feature_id));
            Some((
                ReferenceSpec {
                    key: "reference_points_4756_hai_phong",
                    declared_crs: Some(4756),
                    stored_crs: Some(4756),
                    declared_family: "Point",
                    declared_dimensions: "XY",
                    expected_family: "Point",
                    expected_dimensions: "XY",
                    expected_count: pts.len(),
                    expected_bounds: Some(point_bounds(&pts)),
                    provenance: Provenance::HaiPhongCircular,
                    expected_reason: None,
                    axis_control_xy: None,
                    correctable_to_crs: None,
                },
                recs,
            ))
        }
        "reference_points_4326_hai_phong" => {
            let pts: Vec<(&str, (f64, f64))> = HAI_PHONG
                .iter()
                .map(|&(id, _, _, lon, lat)| {
                    (id, (dms(lon.0, lon.1, lon.2), dms(lat.0, lat.1, lat.2)))
                })
                .collect();
            let mut recs = point_records(&pts, 4326);
            recs.sort_by(|a, b| a.feature_id.cmp(&b.feature_id));
            Some((
                ReferenceSpec {
                    key: "reference_points_4326_hai_phong",
                    declared_crs: Some(4326),
                    stored_crs: Some(4326),
                    declared_family: "Point",
                    declared_dimensions: "XY",
                    expected_family: "Point",
                    expected_dimensions: "XY",
                    expected_count: pts.len(),
                    expected_bounds: Some(point_bounds(&pts)),
                    provenance: Provenance::HaiPhongCircular,
                    expected_reason: None,
                    axis_control_xy: None,
                    correctable_to_crs: None,
                },
                recs,
            ))
        }
        "reference_points_4756_dong_nai" => {
            let pts: Vec<(&str, (f64, f64))> = DONG_NAI
                .iter()
                .map(|&(id, lon, lat, _, _)| {
                    (id, (dms(lon.0, lon.1, lon.2), dms(lat.0, lat.1, lat.2)))
                })
                .collect();
            let mut recs = point_records(&pts, 4756);
            recs.sort_by(|a, b| a.feature_id.cmp(&b.feature_id));
            Some((
                ReferenceSpec {
                    key: "reference_points_4756_dong_nai",
                    declared_crs: Some(4756),
                    stored_crs: Some(4756),
                    declared_family: "Point",
                    declared_dimensions: "XY",
                    expected_family: "Point",
                    expected_dimensions: "XY",
                    expected_count: pts.len(),
                    expected_bounds: Some(point_bounds(&pts)),
                    provenance: Provenance::DongNaiCircular,
                    expected_reason: None,
                    axis_control_xy: None,
                    correctable_to_crs: None,
                },
                recs,
            ))
        }
        "reference_points_4326_dong_nai" => {
            let pts: Vec<(&str, (f64, f64))> = DONG_NAI
                .iter()
                .map(|&(id, _, _, lon, lat)| {
                    (id, (dms(lon.0, lon.1, lon.2), dms(lat.0, lat.1, lat.2)))
                })
                .collect();
            let mut recs = point_records(&pts, 4326);
            recs.sort_by(|a, b| a.feature_id.cmp(&b.feature_id));
            Some((
                ReferenceSpec {
                    key: "reference_points_4326_dong_nai",
                    declared_crs: Some(4326),
                    stored_crs: Some(4326),
                    declared_family: "Point",
                    declared_dimensions: "XY",
                    expected_family: "Point",
                    expected_dimensions: "XY",
                    expected_count: pts.len(),
                    expected_bounds: Some(point_bounds(&pts)),
                    provenance: Provenance::DongNaiCircular,
                    expected_reason: None,
                    axis_control_xy: None,
                    correctable_to_crs: None,
                },
                recs,
            ))
        }
        "reference_points_4756" => {
            let pts: Vec<(&str, (f64, f64))> = CIRCULAR
                .iter()
                .map(|&(id, vlon, vlat, _, _)| {
                    (
                        id,
                        (dms(vlon.0, vlon.1, vlon.2), dms(vlat.0, vlat.1, vlat.2)),
                    )
                })
                .collect();
            let mut recs = point_records(&pts, 4756);
            recs.sort_by(|a, b| a.feature_id.cmp(&b.feature_id));
            Some((
                ReferenceSpec {
                    key: "reference_points_4756",
                    declared_crs: Some(4756),
                    stored_crs: Some(4756),
                    declared_family: "Point",
                    declared_dimensions: "XY",
                    expected_family: "Point",
                    expected_dimensions: "XY",
                    expected_count: pts.len(),
                    expected_bounds: Some(point_bounds(&pts)),
                    provenance: Provenance::QuangNinhCircular,
                    expected_reason: None,
                    axis_control_xy: None,
                    correctable_to_crs: None,
                },
                recs,
            ))
        }
        "reference_points_4326_quang_ninh" => {
            let pts: Vec<(&str, (f64, f64))> = CIRCULAR
                .iter()
                .map(|&(id, _, _, wlon, wlat)| {
                    (
                        id,
                        (dms(wlon.0, wlon.1, wlon.2), dms(wlat.0, wlat.1, wlat.2)),
                    )
                })
                .collect();
            let mut recs = point_records(&pts, 4326);
            recs.sort_by(|a, b| a.feature_id.cmp(&b.feature_id));
            Some((
                ReferenceSpec {
                    key: "reference_points_4326_quang_ninh",
                    declared_crs: Some(4326),
                    stored_crs: Some(4326),
                    declared_family: "Point",
                    declared_dimensions: "XY",
                    expected_family: "Point",
                    expected_dimensions: "XY",
                    expected_count: pts.len(),
                    expected_bounds: Some(point_bounds(&pts)),
                    provenance: Provenance::QuangNinhCircular,
                    expected_reason: None,
                    axis_control_xy: None,
                    correctable_to_crs: None,
                },
                recs,
            ))
        }
        "reference_points_4756_hvs" => {
            let pts: Vec<(&str, (f64, f64))> = HVS
                .iter()
                .map(|&(id, _, vlon, vlat, _, _)| {
                    (
                        id,
                        (dms(vlon.0, vlon.1, vlon.2), dms(vlat.0, vlat.1, vlat.2)),
                    )
                })
                .collect();
            let mut recs = point_records(&pts, 4756);
            recs.sort_by(|a, b| a.feature_id.cmp(&b.feature_id));
            Some((
                ReferenceSpec {
                    key: "reference_points_4756_hvs",
                    declared_crs: Some(4756),
                    stored_crs: Some(4756),
                    declared_family: "Point",
                    declared_dimensions: "XY",
                    expected_family: "Point",
                    expected_dimensions: "XY",
                    expected_count: pts.len(),
                    expected_bounds: Some(point_bounds(&pts)),
                    provenance: Provenance::QuangNinhHvs,
                    expected_reason: None,
                    axis_control_xy: None,
                    correctable_to_crs: None,
                },
                recs,
            ))
        }
        "reference_points_4326_hvs" => {
            let pts: Vec<(&str, (f64, f64))> = HVS
                .iter()
                .map(|&(id, _, _, _, wlon, wlat)| {
                    (
                        id,
                        (dms(wlon.0, wlon.1, wlon.2), dms(wlat.0, wlat.1, wlat.2)),
                    )
                })
                .collect();
            let mut recs = point_records(&pts, 4326);
            recs.sort_by(|a, b| a.feature_id.cmp(&b.feature_id));
            Some((
                ReferenceSpec {
                    key: "reference_points_4326_hvs",
                    declared_crs: Some(4326),
                    stored_crs: Some(4326),
                    declared_family: "Point",
                    declared_dimensions: "XY",
                    expected_family: "Point",
                    expected_dimensions: "XY",
                    expected_count: pts.len(),
                    expected_bounds: Some(point_bounds(&pts)),
                    provenance: Provenance::QuangNinhHvs,
                    expected_reason: None,
                    axis_control_xy: None,
                    correctable_to_crs: None,
                },
                recs,
            ))
        }
        "reference_points_5899" => {
            let pts: Vec<(&str, (f64, f64))> =
                HVS.iter().map(|&(id, xy, _, _, _, _)| (id, xy)).collect();
            let mut recs = point_records(&pts, 5899);
            recs.sort_by(|a, b| a.feature_id.cmp(&b.feature_id));
            Some((
                ReferenceSpec {
                    key: "reference_points_5899",
                    declared_crs: Some(5899),
                    stored_crs: Some(5899),
                    declared_family: "Point",
                    declared_dimensions: "XY",
                    expected_family: "Point",
                    expected_dimensions: "XY",
                    expected_count: pts.len(),
                    expected_bounds: Some(point_bounds(&pts)),
                    provenance: Provenance::QuangNinhHvs,
                    expected_reason: None,
                    axis_control_xy: None,
                    correctable_to_crs: None,
                },
                recs,
            ))
        }
        "reference_points_4326_webmerc_out_of_area" => {
            let pts = [("point-000001", (0.0, 86.0))];
            let recs = point_records(&pts, 4326);
            Some((
                ReferenceSpec {
                    key: "reference_points_4326_webmerc_out_of_area",
                    declared_crs: Some(4326),
                    stored_crs: Some(4326),
                    declared_family: "Point",
                    declared_dimensions: "XY",
                    expected_family: "Point",
                    expected_dimensions: "XY",
                    expected_count: 1,
                    expected_bounds: Some([0.0, 86.0, 0.0, 86.0]),
                    provenance: Provenance::Synthetic,
                    expected_reason: None,
                    axis_control_xy: None,
                    correctable_to_crs: None,
                },
                recs,
            ))
        }
        "reference_empty_4326" => Some((
            ReferenceSpec {
                key: "reference_empty_4326",
                declared_crs: Some(4326),
                stored_crs: Some(4326),
                declared_family: "Point",
                declared_dimensions: "XY",
                expected_family: "Point",
                expected_dimensions: "XY",
                expected_count: 0,
                expected_bounds: None,
                provenance: Provenance::Synthetic,
                expected_reason: None,
                axis_control_xy: None,
                correctable_to_crs: None,
            },
            Vec::new(),
        )),
        "reference_unknown_crs" => Some((
            ReferenceSpec {
                key: "reference_unknown_crs",
                declared_crs: None,
                stored_crs: None,
                declared_family: "Point",
                declared_dimensions: "XY",
                expected_family: "Point",
                expected_dimensions: "XY",
                expected_count: 1,
                expected_bounds: None,
                provenance: Provenance::Synthetic,
                expected_reason: Some(ReasonCode::MissingCrs),
                axis_control_xy: None,
                correctable_to_crs: None,
            },
            vec![record(
                "point-000001",
                Some(point_geom(106.6, 10.7, None)),
                1,
                None,
                "present",
            )],
        )),
        "reference_conflicting_metadata" => Some((
            ReferenceSpec {
                key: "reference_conflicting_metadata",
                declared_crs: Some(4326),
                stored_crs: Some(4326),
                declared_family: "Point",
                declared_dimensions: "XY",
                expected_family: "LineString",
                expected_dimensions: "XY",
                expected_count: 1,
                expected_bounds: None,
                provenance: Provenance::Synthetic,
                expected_reason: Some(ReasonCode::GeometryFamilyMismatch),
                axis_control_xy: None,
                correctable_to_crs: None,
            },
            vec![record("line-000001", Some(good_line), 1, None, "present")],
        )),
        "reference_unsupported_dimensions" => Some((
            ReferenceSpec {
                key: "reference_unsupported_dimensions",
                declared_crs: Some(4326),
                stored_crs: Some(4326),
                declared_family: "Point",
                declared_dimensions: "XYZ",
                expected_family: "Point",
                expected_dimensions: "XYZ",
                expected_count: 1,
                expected_bounds: None,
                provenance: Provenance::Synthetic,
                expected_reason: Some(ReasonCode::UnsupportedDimensions),
                axis_control_xy: None,
                correctable_to_crs: None,
            },
            vec![record(
                "point-000001",
                Some(FixtureGeometry {
                    gtype: "Point",
                    dimensions: "XYZ",
                    srid: Some(4326),
                    coordinates: json!([106.6, 10.7, 5.0]),
                    wkt: "POINT Z (106.6 10.7 5.0)".to_owned(),
                    is_empty: false,
                }),
                1,
                None,
                "unsupported",
            )],
        )),
        "reference_unsupported_dimensions_m" => Some((
            ReferenceSpec {
                key: "reference_unsupported_dimensions_m",
                declared_crs: Some(4326),
                stored_crs: Some(4326),
                declared_family: "Point",
                declared_dimensions: "XYM",
                expected_family: "Point",
                expected_dimensions: "XYM",
                expected_count: 1,
                expected_bounds: None,
                provenance: Provenance::Synthetic,
                expected_reason: Some(ReasonCode::UnsupportedDimensions),
                axis_control_xy: None,
                correctable_to_crs: None,
            },
            vec![record(
                "point-000001",
                Some(FixtureGeometry {
                    gtype: "Point",
                    dimensions: "XYM",
                    srid: Some(4326),
                    coordinates: json!([106.6, 10.7, 7.0]),
                    wkt: "POINT M (106.6 10.7 7.0)".to_owned(),
                    is_empty: false,
                }),
                1,
                None,
                "unsupported",
            )],
        )),
        "reference_mixed_geometry" => Some((
            ReferenceSpec {
                key: "reference_mixed_geometry",
                declared_crs: Some(4326),
                stored_crs: Some(4326),
                declared_family: "Mixed",
                declared_dimensions: "XY",
                expected_family: "Mixed",
                expected_dimensions: "XY",
                expected_count: 2,
                expected_bounds: None,
                provenance: Provenance::Synthetic,
                expected_reason: Some(ReasonCode::UnsupportedGeometry),
                axis_control_xy: None,
                correctable_to_crs: None,
            },
            vec![
                record("line-000001", Some(good_line), 1, None, "present"),
                record("point-000001", Some(good_point), 2, None, "present"),
            ],
        )),
        "reference_unsupported_family" => Some((
            ReferenceSpec {
                key: "reference_unsupported_family",
                declared_crs: Some(4326),
                stored_crs: Some(4326),
                declared_family: "Unsupported",
                declared_dimensions: "XY",
                expected_family: "Unsupported",
                expected_dimensions: "XY",
                expected_count: 1,
                expected_bounds: None,
                provenance: Provenance::Synthetic,
                expected_reason: Some(ReasonCode::UnsupportedGeometry),
                axis_control_xy: None,
                correctable_to_crs: None,
            },
            vec![record(
                "multipoint-000001",
                Some(FixtureGeometry {
                    gtype: "MultiPoint",
                    dimensions: "XY",
                    srid: Some(4326),
                    coordinates: json!([[106.6, 10.7], [106.601, 10.701]]),
                    wkt: "MULTIPOINT ((106.6 10.7),(106.601 10.701))".to_owned(),
                    is_empty: false,
                }),
                1,
                None,
                "unsupported",
            )],
        )),
        "reference_null_geometry" => Some((
            ReferenceSpec {
                key: "reference_null_geometry",
                declared_crs: Some(4326),
                stored_crs: None,
                declared_family: "Point",
                declared_dimensions: "XY",
                expected_family: "Unknown",
                expected_dimensions: "Unknown",
                expected_count: 1,
                expected_bounds: None,
                provenance: Provenance::Synthetic,
                expected_reason: Some(ReasonCode::NullGeometry),
                axis_control_xy: None,
                correctable_to_crs: None,
            },
            vec![record("point-000001", None, 1, None, "absent")],
        )),
        "reference_empty_geometry" => Some((
            ReferenceSpec {
                key: "reference_empty_geometry",
                declared_crs: Some(4326),
                stored_crs: Some(4326),
                declared_family: "Point",
                declared_dimensions: "XY",
                expected_family: "Point",
                expected_dimensions: "XY",
                expected_count: 1,
                expected_bounds: None,
                provenance: Provenance::Synthetic,
                expected_reason: Some(ReasonCode::EmptyGeometry),
                axis_control_xy: None,
                correctable_to_crs: None,
            },
            vec![record(
                "point-000001",
                Some(FixtureGeometry {
                    gtype: "Point",
                    dimensions: "XY",
                    srid: Some(4326),
                    coordinates: json!([]),
                    wkt: "POINT EMPTY".to_owned(),
                    is_empty: true,
                }),
                1,
                None,
                "empty",
            )],
        )),
        "reference_invalid_geometry" => {
            let ring = [
                (106.6, 10.7),
                (106.601, 10.701),
                (106.6, 10.701),
                (106.601, 10.7),
                (106.6, 10.7),
            ];
            Some((
                ReferenceSpec {
                    key: "reference_invalid_geometry",
                    declared_crs: Some(4326),
                    stored_crs: Some(4326),
                    declared_family: "Polygon",
                    declared_dimensions: "XY",
                    expected_family: "Polygon",
                    expected_dimensions: "XY",
                    expected_count: 1,
                    expected_bounds: None,
                    provenance: Provenance::Synthetic,
                    expected_reason: Some(ReasonCode::InvalidGeometry),
                    axis_control_xy: None,
                    correctable_to_crs: None,
                },
                vec![record(
                    "polygon-000001",
                    Some(polygon_geom(&ring, Some(4326))),
                    1,
                    None,
                    "invalid",
                )],
            ))
        }
        "reference_axis_conflict" => Some((
            ReferenceSpec {
                key: "reference_axis_conflict",
                declared_crs: Some(4326),
                stored_crs: Some(4326),
                declared_family: "Point",
                declared_dimensions: "XY",
                expected_family: "Point",
                expected_dimensions: "XY",
                expected_count: 1,
                expected_bounds: None,
                provenance: Provenance::QuangNinhHvs,
                expected_reason: Some(ReasonCode::AxisConflict),
                axis_control_xy: Some(hvs_qn1_wgs),
                correctable_to_crs: None,
            },
            vec![record(
                "HVS-QN1",
                Some(point_geom(hvs_qn1_wgs.1, hvs_qn1_wgs.0, Some(4326))),
                1,
                None,
                "invalid",
            )],
        )),
        "reference_out_of_domain" => Some((
            ReferenceSpec {
                key: "reference_out_of_domain",
                declared_crs: Some(4326),
                stored_crs: Some(4326),
                declared_family: "Point",
                declared_dimensions: "XY",
                expected_family: "Point",
                expected_dimensions: "XY",
                expected_count: 1,
                expected_bounds: None,
                provenance: Provenance::Synthetic,
                expected_reason: Some(ReasonCode::CoordinateOutOfDomain),
                axis_control_xy: None,
                correctable_to_crs: None,
            },
            vec![record(
                "point-000001",
                Some(point_geom(106.6, 91.0, Some(4326))),
                1,
                None,
                "invalid",
            )],
        )),
        "reference_definition_only_crs" => Some((
            ReferenceSpec {
                key: "reference_definition_only_crs",
                declared_crs: Some(3405),
                stored_crs: Some(3405),
                declared_family: "Point",
                declared_dimensions: "XY",
                expected_family: "Point",
                expected_dimensions: "XY",
                expected_count: 1,
                expected_bounds: None,
                provenance: Provenance::Synthetic,
                expected_reason: Some(ReasonCode::UnsupportedCrs),
                axis_control_xy: None,
                correctable_to_crs: None,
            },
            vec![record(
                "point-000001",
                Some(point_geom(500_000.0, 1_200_000.0, Some(3405))),
                1,
                None,
                "present",
            )],
        )),
        "reference_wrapping_extent" => Some((
            ReferenceSpec {
                key: "reference_wrapping_extent",
                declared_crs: Some(4326),
                stored_crs: Some(4326),
                declared_family: "LineString",
                declared_dimensions: "XY",
                expected_family: "LineString",
                expected_dimensions: "XY",
                expected_count: 1,
                expected_bounds: None,
                provenance: Provenance::Synthetic,
                expected_reason: Some(ReasonCode::ExtentWrapUnsupported),
                axis_control_xy: None,
                correctable_to_crs: None,
            },
            vec![record(
                "line-000001",
                Some(line_geom(&[(179.9, 10.0), (-179.9, 10.0)], Some(4326))),
                1,
                None,
                "present",
            )],
        )),
        "reference_correctable_crs" => Some((
            ReferenceSpec {
                key: "reference_correctable_crs",
                declared_crs: Some(3857),
                stored_crs: Some(4326),
                declared_family: "Point",
                declared_dimensions: "XY",
                expected_family: "Point",
                expected_dimensions: "XY",
                expected_count: 1,
                expected_bounds: Some([hvs_qn1_wgs.0, hvs_qn1_wgs.1, hvs_qn1_wgs.0, hvs_qn1_wgs.1]),
                provenance: Provenance::QuangNinhHvs,
                expected_reason: Some(ReasonCode::CrsMismatch),
                axis_control_xy: Some(hvs_qn1_wgs),
                correctable_to_crs: Some(4326),
            },
            vec![record(
                "HVS-QN1",
                Some(point_geom(hvs_qn1_wgs.0, hvs_qn1_wgs.1, Some(4326))),
                1,
                None,
                "present",
            )],
        )),
        _ => None,
    }
}

pub fn content_digest_records(
    records: &[FixtureRecord],
    fields: &[FieldSpec],
    stored_crs: Option<i32>,
    source_crs: Option<i32>,
    geometry_family: &str,
    dimensions: &str,
) -> String {
    let mut ordered_fields: Vec<&FieldSpec> = fields.iter().collect();
    ordered_fields.sort_by(|a, b| a.field_id.cmp(&b.field_id));
    let mut ordered_recs: Vec<&FixtureRecord> = records.iter().collect();
    ordered_recs.sort_by(|a, b| a.feature_id.cmp(&b.feature_id));
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
        "features": ordered_recs.iter().map(|r| {
            let geom = r.geometry.as_ref().map(|g| json!({
                "type": g.gtype,
                "dimensions": g.dimensions,
                "srid": g.srid,
                "coordinates": g.coordinates,
                "members": [],
            }));
            json!({
                "feature_id": r.feature_id,
                "attributes": r.attributes,
                "geometry": geom,
                "geometry_state": r.geometry_state,
            })
        }).collect::<Vec<_>>(),
    }))
}

pub struct MaterializedFixture {
    pub spec: ReferenceSpec,
    pub fields: Vec<FieldSpec>,
    pub records: Vec<FixtureRecord>,
    pub schema_revision: String,
    pub content_revision: String,
    pub manifest_digest: String,
    pub validation_state: &'static str,
    pub validation_reason: Option<ReasonCode>,
    pub extent_state: &'static str,
    pub extent: Option<Value>,
}

pub fn materialize_fixture(
    key: &str,
    declared_crs_override: Option<i32>,
) -> Result<MaterializedFixture, AppError> {
    let (spec, records) = get_reference_spec(key).ok_or_else(|| {
        AppError::validation(
            "UNKNOWN_REFERENCE_KEY",
            "Select a supported platform reference fixture.",
        )
    })?;
    let source_crs = declared_crs_override.or(spec.declared_crs);
    let schema_revision = schema_revision_digest(
        &reference_fields("0"),
        spec.expected_family,
        spec.expected_dimensions,
        "known",
    );
    let fields = reference_fields(&schema_revision);
    let content_revision = content_digest_records(
        &records,
        &fields,
        spec.stored_crs,
        source_crs,
        spec.expected_family,
        spec.expected_dimensions,
    );
    let orig_content = if declared_crs_override.is_some() {
        content_digest_records(
            &records,
            &fields,
            spec.stored_crs,
            spec.declared_crs,
            spec.expected_family,
            spec.expected_dimensions,
        )
    } else {
        content_revision.clone()
    };

    let manifest_digest = sha256_py_json(&json!({
        "manifest_type": "r003-reference-fixture-v1",
        "key": spec.key,
        "version": FIXTURE_VERSION,
        "declared_crs": spec.declared_crs,
        "stored_crs": spec.stored_crs,
        "declared_family": spec.declared_family,
        "declared_dimensions": spec.declared_dimensions,
        "expected_family": spec.expected_family,
        "expected_dimensions": spec.expected_dimensions,
        "expected_count": spec.expected_count,
        "expected_native_bounds": spec.expected_bounds,
        "provenance": spec.provenance.as_str(),
        "expected_reason": spec.expected_reason.map(ReasonCode::as_str),
        "axis_control_xy": spec.axis_control_xy.map(|(x, y)| json!([x, y])),
        "correctable_to_crs": spec.correctable_to_crs,
        "schema_revision": schema_revision,
        "content_revision": orig_content,
    }));

    let reason = if declared_crs_override.is_some() {
        None
    } else {
        spec.expected_reason
    };
    let validation_state = match reason {
        None => "ready",
        Some(ReasonCode::MissingCrs) => "not_ready",
        Some(_) => "invalid",
    };
    let (extent_state, extent) = if reason.is_none() {
        if let (Some([min_x, min_y, max_x, max_y]), Some(crs_code)) =
            (spec.expected_bounds, spec.stored_crs)
        {
            (
                "available",
                Some(json!({
                    "min_x": min_x,
                    "min_y": min_y,
                    "max_x": max_x,
                    "max_y": max_y,
                    "crs_code": crs_code,
                })),
            )
        } else {
            ("empty", None)
        }
    } else {
        ("unavailable", None)
    };

    Ok(MaterializedFixture {
        spec,
        fields,
        records,
        schema_revision,
        content_revision,
        manifest_digest,
        validation_state,
        validation_reason: reason,
        extent_state,
        extent,
    })
}

/// Verify native PostGIS geometry observations match the trusted fixture specification.
pub async fn verify_native_postgis(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    mat: &MaterializedFixture,
    records_json: &str,
) -> Result<(), AppError> {
    if mat.records.is_empty() {
        return Ok(());
    }
    let row: (i64, bool) = sqlx::query_as(
        "WITH expected AS (
             SELECT r.feature_id, r.geometry_state,
                    CASE WHEN r.wkt IS NULL THEN NULL
                         ELSE ST_GeomFromText(r.wkt, COALESCE(r.srid, 0)) END AS geometry
             FROM jsonb_to_recordset(CAST($1 AS jsonb)) AS r(
                 feature_id text, attributes jsonb, wkt text, srid integer, geometry_state text
             )
         )
         SELECT count(*)::bigint,
                bool_and(
                    CASE WHEN geometry IS NULL THEN geometry_state = 'absent'
                         WHEN ST_IsEmpty(geometry) THEN geometry_state = 'empty'
                         WHEN ST_Zmflag(geometry) <> 0 THEN geometry_state = 'unsupported'
                         WHEN ST_GeometryType(geometry) NOT IN ('ST_Point','ST_LineString','ST_Polygon')
                              THEN geometry_state = 'unsupported'
                         ELSE true END
                )
         FROM expected",
    )
    .bind(records_json)
    .fetch_one(&mut **tx)
    .await
    .map_err(db_error)?;
    if row.0 as usize != mat.records.len() || !row.1 {
        return Err(AppError::internal(
            "reference fixture native verification failed",
        ));
    }
    Ok(())
}

pub struct SourceSnapshotArgs<'a> {
    pub name: &'a str,
    pub kind: &'a str,
    pub reference_key: &'a str,
    pub fixture_version: &'a str,
    pub manifest_digest: &'a str,
    pub lifecycle: &'a str,
    pub validation_state: &'a str,
    pub reason: Option<(&'a str, &'a str)>,
    pub source_crs: Option<i32>,
    pub stored_crs: Option<i32>,
    pub schema_state: &'a str,
    pub geometry_family: &'a str,
    pub dimensions: &'a str,
    pub feature_count_state: &'a str,
    pub feature_count: i64,
    pub extent_state: &'a str,
    pub catalog_revision: i64,
    pub content_revision: &'a str,
    pub schema_revision: &'a str,
}

/// Compute scalar snapshot map matching Python `_source_snapshot` for catalog_changes diffing.
pub fn source_snapshot(s: &SourceSnapshotArgs<'_>) -> BTreeMap<&'static str, Value> {
    let mut map = BTreeMap::new();
    map.insert("name", json!(s.name));
    map.insert("kind", json!(s.kind));
    map.insert("reference_key", json!(s.reference_key));
    map.insert("fixture_version", json!(s.fixture_version));
    map.insert("manifest_digest", json!(s.manifest_digest));
    map.insert("source_asset_id", Value::Null);
    map.insert("lifecycle", json!(s.lifecycle));
    map.insert("validation_state", json!(s.validation_state));
    map.insert(
        "validation_reason.code",
        s.reason.map_or(Value::Null, |(c, _)| json!(c)),
    );
    map.insert(
        "validation_reason.message",
        s.reason.map_or(Value::Null, |(_, m)| json!(m)),
    );
    map.insert("source_crs", s.source_crs.map_or(Value::Null, |c| json!(c)));
    map.insert("stored_crs", s.stored_crs.map_or(Value::Null, |c| json!(c)));
    map.insert("schema_state", json!(s.schema_state));
    map.insert("geometry_family", json!(s.geometry_family));
    map.insert("dimensions", json!(s.dimensions));
    map.insert("feature_count_state", json!(s.feature_count_state));
    map.insert("feature_count", json!(s.feature_count));
    map.insert("extent_state", json!(s.extent_state));
    map.insert("catalog_revision", json!(s.catalog_revision));
    map.insert("content_revision", json!(s.content_revision));
    map.insert("schema_revision", json!(s.schema_revision));
    map
}

pub fn diff_snapshots(
    prior: &BTreeMap<&'static str, Value>,
    current: &BTreeMap<&'static str, Value>,
) -> Value {
    let items: Vec<Value> = current
        .iter()
        .filter_map(|(&field, new_val)| {
            let old_val = prior.get(field).cloned().unwrap_or(Value::Null);
            if old_val == *new_val {
                None
            } else {
                Some(json!({
                    "field": field,
                    "old_value": old_val,
                    "new_value": new_val,
                }))
            }
        })
        .collect();
    Value::Array(items)
}
