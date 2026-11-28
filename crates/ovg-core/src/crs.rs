//! OpenVietGIS CRS definitions, approved directed operations, and explicit CRS policy.
//!
//! A known definition does not by itself authorize coordinate reads or transforms.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::error::AppError;

pub const CRS_REGISTRY_VERSION: &str = "r003-v1";
pub const WEB_MERCATOR_MAX_LATITUDE: f64 = 85.051_128_779_806_6;
pub const WEB_MERCATOR_MAX_COORDINATE_METRE: f64 = 20_037_508.342_789_244;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrsAxis {
    Longitude,
    Latitude,
    Easting,
    Northing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrsUnit {
    Degree,
    Metre,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrsDisposition {
    Readable,
    DefinitionOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "method", rename_all = "snake_case")]
pub enum ProjectionKind {
    Geographic,
    WebMercator {
        sphere_radius: f64,
    },
    TransverseMercator {
        central_meridian: f64,
        scale_factor: f64,
        false_easting: f64,
        false_northing: f64,
        latitude_of_origin: f64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CoordinateBounds {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

impl CoordinateBounds {
    #[must_use]
    pub const fn new(min_x: f64, min_y: f64, max_x: f64, max_y: f64) -> Self {
        Self {
            min_x,
            min_y,
            max_x,
            max_y,
        }
    }

    #[must_use]
    pub fn contains(&self, x: f64, y: f64, epsilon: f64) -> bool {
        if !x.is_finite() || !y.is_finite() || !epsilon.is_finite() || epsilon < 0.0 {
            return false;
        }
        x >= self.min_x - epsilon
            && x <= self.max_x + epsilon
            && y >= self.min_y - epsilon
            && y <= self.max_y + epsilon
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct CrsProfile {
    pub code: u32,
    pub name: &'static str,
    pub datum: &'static str,
    pub x_axis: CrsAxis,
    pub y_axis: CrsAxis,
    pub unit: CrsUnit,
    pub projection: ProjectionKind,
    pub area_of_use: &'static str,
    pub disposition: CrsDisposition,
    pub native_bounds: Option<CoordinateBounds>,
}

const fn vn2000_tm3(
    code: u32,
    name: &'static str,
    central_meridian: f64,
    area_of_use: &'static str,
) -> CrsProfile {
    CrsProfile {
        code,
        name,
        datum: "Vietnam 2000",
        x_axis: CrsAxis::Easting,
        y_axis: CrsAxis::Northing,
        unit: CrsUnit::Metre,
        projection: ProjectionKind::TransverseMercator {
            central_meridian,
            scale_factor: 0.9999,
            false_easting: 500_000.0,
            false_northing: 0.0,
            latitude_of_origin: 0.0,
        },
        area_of_use,
        disposition: CrsDisposition::Readable,
        native_bounds: None,
    }
}

const fn vn2000_utm(
    code: u32,
    name: &'static str,
    central_meridian: f64,
    area_of_use: &'static str,
) -> CrsProfile {
    CrsProfile {
        code,
        name,
        datum: "Vietnam 2000",
        x_axis: CrsAxis::Easting,
        y_axis: CrsAxis::Northing,
        unit: CrsUnit::Metre,
        projection: ProjectionKind::TransverseMercator {
            central_meridian,
            scale_factor: 0.9996,
            false_easting: 500_000.0,
            false_northing: 0.0,
            latitude_of_origin: 0.0,
        },
        area_of_use,
        disposition: CrsDisposition::DefinitionOnly,
        native_bounds: None,
    }
}

static CRS_PROFILES: &[CrsProfile] = &[
    CrsProfile {
        code: 4326,
        name: "WGS 84",
        datum: "World Geodetic System 1984 ensemble",
        x_axis: CrsAxis::Longitude,
        y_axis: CrsAxis::Latitude,
        unit: CrsUnit::Degree,
        projection: ProjectionKind::Geographic,
        area_of_use: "World.",
        disposition: CrsDisposition::Readable,
        native_bounds: Some(CoordinateBounds::new(-180.0, -90.0, 180.0, 90.0)),
    },
    CrsProfile {
        code: 3857,
        name: "WGS 84 / Pseudo-Mercator",
        datum: "World Geodetic System 1984 ensemble",
        x_axis: CrsAxis::Easting,
        y_axis: CrsAxis::Northing,
        unit: CrsUnit::Metre,
        projection: ProjectionKind::WebMercator {
            sphere_radius: 6_378_137.0,
        },
        area_of_use: "World between 85.06°S and 85.06°N.",
        disposition: CrsDisposition::Readable,
        native_bounds: Some(CoordinateBounds::new(
            -WEB_MERCATOR_MAX_COORDINATE_METRE,
            -WEB_MERCATOR_MAX_COORDINATE_METRE,
            WEB_MERCATOR_MAX_COORDINATE_METRE,
            WEB_MERCATOR_MAX_COORDINATE_METRE,
        )),
    },
    CrsProfile {
        code: 4756,
        name: "VN-2000",
        datum: "Vietnam 2000",
        x_axis: CrsAxis::Longitude,
        y_axis: CrsAxis::Latitude,
        unit: CrsUnit::Degree,
        projection: ProjectionKind::Geographic,
        area_of_use: "Vietnam - onshore.",
        disposition: CrsDisposition::Readable,
        native_bounds: Some(CoordinateBounds::new(-180.0, -90.0, 180.0, 90.0)),
    },
    vn2000_tm3(
        5899,
        "VN-2000 / TM-3 107-45",
        107.75,
        "Vietnam - Quang Ninh province; Da Nang municipality and Quang Nam province; \
         Ba Ria-Vung Tau, Dong Nai and Lam Dong provinces.",
    ),
    vn2000_utm(
        3405,
        "VN-2000 / UTM zone 48N",
        105.0,
        "Vietnam - onshore west of 108°E.",
    ),
    vn2000_utm(
        3406,
        "VN-2000 / UTM zone 49N",
        111.0,
        "Vietnam - onshore east of 108°E.",
    ),
];

/// Mathematical conversion within Vietnam 2000 only. These bounds and
/// parameters are the EPSG projected CRS definitions (PROJ database), not a
/// geodetic datum operation, cadastral certification, or administrative boundary.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct NativeVn2000Zone {
    pub code: u32,
    pub central_meridian: f64,
    pub scale_factor: f64,
    pub bounds: [f64; 4],
}
const fn vn_zone(
    code: u32,
    central_meridian: f64,
    scale_factor: f64,
    bounds: [f64; 4],
) -> NativeVn2000Zone {
    NativeVn2000Zone {
        code,
        central_meridian,
        scale_factor,
        bounds,
    }
}
pub const NATIVE_VN2000_ZONES: &[NativeVn2000Zone] = &[
    vn_zone(3405, 105.0, 0.9996, [102.14, 8.33, 108.0, 23.4]),
    vn_zone(3406, 111.0, 0.9996, [108.0, 10.43, 109.53, 21.56]),
    vn_zone(5896, 102.0, 0.9999, [102.14, 9.2, 103.51, 22.82]),
    vn_zone(5897, 105.0, 0.9999, [103.5, 8.33, 106.51, 23.4]),
    vn_zone(5898, 108.0, 0.9999, [106.5, 8.57, 109.53, 22.95]),
    vn_zone(5899, 107.75, 0.9999, [106.43, 8.57, 108.76, 21.67]),
    vn_zone(9205, 103.0, 0.9999, [102.14, 20.89, 103.99, 22.82]),
    vn_zone(9206, 104.0, 0.9999, [103.21, 20.57, 105.03, 22.04]),
    vn_zone(9207, 104.5, 0.9999, [103.4, 8.33, 105.54, 10.55]),
    vn_zone(9208, 104.75, 0.9999, [103.53, 10.18, 105.86, 22.85]),
    vn_zone(9209, 105.5, 0.9999, [104.33, 9.19, 106.69, 23.4]),
    vn_zone(9210, 105.75, 0.9999, [105.26, 9.75, 107.8, 23.12]),
    vn_zone(9211, 106.0, 0.9999, [104.83, 16.92, 107.03, 22.7]),
    vn_zone(9212, 106.25, 0.9999, [106.41, 11.3, 107.43, 17.22]),
    vn_zone(9213, 106.5, 0.9999, [105.43, 21.32, 106.25, 22.75]),
    vn_zone(9214, 107.0, 0.9999, [105.88, 15.99, 108.24, 21.63]),
    vn_zone(9215, 107.25, 0.9999, [106.09, 21.32, 107.37, 22.47]),
    vn_zone(9216, 107.5, 0.9999, [107.33, 13.92, 108.55, 15.42]),
    vn_zone(9217, 108.25, 0.9999, [108.55, 11.25, 109.53, 14.71]),
    vn_zone(9218, 108.5, 0.9999, [107.2, 10.43, 109.52, 14.61]),
];

pub fn is_extended_native_vn2000(code: u32) -> bool {
    matches!(code, 3405 | 3406 | 5896..=5898 | 9205..=9218)
}
pub fn is_native_vn2000(code: u32) -> bool {
    code == 4756 || code == 5899 || is_extended_native_vn2000(code)
}
pub fn is_processing_crs(code: u32) -> bool {
    matches!(code, 4326 | 3857) || is_native_vn2000(code)
}
/// Keep the R003 registry contract unchanged. Extended native codes are
/// readable only through the native storage/processing path, not WGS84 display.
pub fn native_storage_profile_wire(code: u32) -> Option<Value> {
    if !is_extended_native_vn2000(code) {
        return reference_profile_wire(code);
    }
    let z = NATIVE_VN2000_ZONES.iter().find(|z| z.code == code)?;
    let meridian = z.central_meridian;
    Some(json!({
        "authority":"EPSG","code":code,
        "name":format!("VN-2000 / {} {}",if z.scale_factor==0.9996 {"UTM"}else{"TM-3"},meridian),
        "datum":"Vietnam 2000","native_axes":["easting","northing"],"unit":"metre",
        "coordinate_order":"xy","x_axis":"easting","y_axis":"northing",
        "projection_parameters":{"method":"Transverse Mercator (EPSG:9807)","latitude_of_origin":0.0,
            "central_meridian":meridian,"scale_factor":z.scale_factor,"false_easting":500000.0,"false_northing":0.0},
        "area_of_use":format!("EPSG geographic rectangle {:?}; same datum native processing only.",z.bounds),
        "disposition":"readable"
    }))
}
pub fn native_storage_coordinate_wire(code: u32) -> Option<Value> {
    if !is_extended_native_vn2000(code) {
        return reference_coordinate_wire(code);
    }
    Some(
        json!({"authority":"EPSG","code":code,"coordinate_order":"xy","x_axis":"easting","y_axis":"northing","unit":"metre"}),
    )
}

/// Return all registered CRS profiles.
#[must_use]
pub fn all_crs_profiles() -> &'static [CrsProfile] {
    CRS_PROFILES
}

/// Look up any registered CRS profile by EPSG code.
#[must_use]
pub fn lookup_crs(code: u32) -> Option<&'static CrsProfile> {
    CRS_PROFILES.iter().find(|p| p.code == code)
}

/// Look up a CRS profile that is eligible for coordinate reads.
#[must_use]
pub fn lookup_readable_crs(code: u32) -> Option<&'static CrsProfile> {
    lookup_crs(code).filter(|p| p.disposition == CrsDisposition::Readable)
}

impl CrsProfile {
    fn coordinate_wire(&self) -> Value {
        json!({
            "authority": "EPSG",
            "code": self.code,
            "coordinate_order": "xy",
            "x_axis": self.x_axis,
            "y_axis": self.y_axis,
            "unit": self.unit,
        })
    }

    fn profile_wire(&self) -> Value {
        let (native_axes, projection_parameters) = match self.projection {
            ProjectionKind::Geographic => ([CrsAxis::Latitude, CrsAxis::Longitude], json!({})),
            ProjectionKind::WebMercator { sphere_radius } => (
                [CrsAxis::Easting, CrsAxis::Northing],
                json!({
                    "method": "Popular Visualisation Pseudo Mercator (EPSG:1024)",
                    "latitude_of_origin": 0.0,
                    "central_meridian": 0.0,
                    "false_easting": 0.0,
                    "false_northing": 0.0,
                    "sphere_radius": sphere_radius,
                }),
            ),
            ProjectionKind::TransverseMercator {
                central_meridian,
                scale_factor,
                false_easting,
                false_northing,
                latitude_of_origin,
            } => (
                [CrsAxis::Easting, CrsAxis::Northing],
                json!({
                    "method": "Transverse Mercator (EPSG:9807)",
                    "latitude_of_origin": latitude_of_origin,
                    "central_meridian": central_meridian,
                    "scale_factor": scale_factor,
                    "false_easting": false_easting,
                    "false_northing": false_northing,
                }),
            ),
        };
        json!({
            "authority": "EPSG",
            "code": self.code,
            "name": self.name,
            "datum": self.datum,
            "native_axes": native_axes,
            "unit": self.unit,
            "coordinate_order": "xy",
            "x_axis": self.x_axis,
            "y_axis": self.y_axis,
            "projection_parameters": projection_parameters,
            "area_of_use": self.area_of_use,
            "disposition": self.disposition,
        })
    }
}

/// Public wire description of a known CRS definition.
#[must_use]
pub fn reference_profile_wire(code: u32) -> Option<Value> {
    lookup_crs(code).map(CrsProfile::profile_wire)
}

/// Public coordinate reference for a readable CRS.
#[must_use]
pub fn reference_coordinate_wire(code: u32) -> Option<Value> {
    lookup_readable_crs(code).map(CrsProfile::coordinate_wire)
}

/// Provenance required by a coordinate-changing operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixtureRequirement {
    None,
    QuangNinh,
    QuangNinhHvs,
    DaNangCircular,
    HaiPhongCircular,
    DongNaiCircular,
}

/// Fixed PROJ operation strings, identical to the Python reference. Reads run
/// them inside PostGIS (`ST_TransformPipeline`), never as caller input.
pub const WEB_MERCATOR_PIPELINE: &str =
    "+proj=pipeline +step +proj=unitconvert +xy_in=deg +xy_out=rad +step +proj=webmerc +ellps=WGS84";
pub const EPSG_6960_PIPELINE: &str = "urn:ogc:def:coordinateOperation:EPSG::6960";
pub const TM3_PIPELINE: &str = "+proj=pipeline +step +proj=unitconvert +xy_in=deg +xy_out=rad      +step +proj=tmerc +lat_0=0 +lon_0=107.75 +k=0.9999      +x_0=500000 +y_0=0 +ellps=WGS84";

/// One explicit PostGIS pipeline step of an approved edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PipelineStep {
    pub source_code: u32,
    pub target_code: u32,
    pub pipeline: &'static str,
    pub inverse: bool,
}

const fn step(
    source_code: u32,
    target_code: u32,
    pipeline: &'static str,
    inverse: bool,
) -> PipelineStep {
    PipelineStep {
        source_code,
        target_code,
        pipeline,
        inverse,
    }
}

const WEB_FORWARD_STEP: PipelineStep = step(4326, 3857, WEB_MERCATOR_PIPELINE, false);
const WEB_INVERSE_STEP: PipelineStep = step(3857, 4326, WEB_MERCATOR_PIPELINE, true);
const DATUM_FORWARD_STEP: PipelineStep = step(4756, 4326, EPSG_6960_PIPELINE, false);
const DATUM_INVERSE_STEP: PipelineStep = step(4326, 4756, EPSG_6960_PIPELINE, true);
/// Inverse TM-3 step, also used to re-check projected outputs against the area.
pub const TM3_INVERSE_STEP: PipelineStep = step(5899, 4756, TM3_PIPELINE, true);
const TM3_FORWARD_STEP: PipelineStep = step(4756, 5899, TM3_PIPELINE, false);

/// An independently validated directed edge in the R003 feature read contract.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TransformationProfile {
    pub source_code: u32,
    pub target_code: u32,
    pub operation_id: &'static str,
    pub method: &'static str,
    pub validated_area: &'static str,
    pub tolerance: f64,
    pub tolerance_unit: CrsUnit,
    pub evidence_id: &'static str,
    pub geographic_bounds: CoordinateBounds,
    pub fixture_requirement: FixtureRequirement,
    /// Explicit PostGIS steps; the only path used by server-side reads.
    pub steps: &'static [PipelineStep],
}

impl TransformationProfile {
    #[must_use]
    pub fn to_wire(&self) -> Value {
        json!({
            "source_code": self.source_code,
            "target_code": self.target_code,
            "operation_id": self.operation_id,
            "method": self.method,
            "validated_area": self.validated_area,
            "tolerance": self.tolerance,
            "tolerance_unit": self.tolerance_unit,
            "evidence_id": self.evidence_id,
        })
    }
}

const WEB_FORWARD: TransformationProfile = TransformationProfile {
    source_code: 4326,
    target_code: 3857,
    operation_id: "epsg-1024-forward",
    method: "analytic Pseudo-Mercator",
    validated_area: "Longitude [-180, 180]°, latitude [-85.0511287798066, 85.0511287798066]°; \
                     projected X/Y [-20037508.342789244, 20037508.342789244] m; \
                     1e-6 m output roundoff only, no input clipping.",
    tolerance: 0.001,
    tolerance_unit: CrsUnit::Metre,
    evidence_id: "analytic-webmercator-formula",
    geographic_bounds: CoordinateBounds::new(
        -180.0,
        -WEB_MERCATOR_MAX_LATITUDE,
        180.0,
        WEB_MERCATOR_MAX_LATITUDE,
    ),
    fixture_requirement: FixtureRequirement::None,
    steps: &[WEB_FORWARD_STEP],
};

const DATUM_FORWARD: TransformationProfile = TransformationProfile {
    source_code: 4756,
    target_code: 4326,
    operation_id: "epsg-6960-forward",
    method: "VN-2000 to WGS 84 (2)",
    validated_area: "Trusted Quang Ninh reference fixtures only; longitude [106.88, 108.07]°, \
                     latitude [20.70, 21.42]°. Port-water control results, not an official boundary \
                     or extension of EPSG:6960 onshore scope. No survey-accuracy claim.",
    tolerance: 3.0,
    tolerance_unit: CrsUnit::Metre,
    evidence_id: "circular-50-2025-tt-bxd-quang-ninh",
    geographic_bounds: CoordinateBounds::new(106.88, 20.70, 108.07, 21.42),
    fixture_requirement: FixtureRequirement::QuangNinh,
    steps: &[DATUM_FORWARD_STEP],
};

const TM3_FORWARD: TransformationProfile = TransformationProfile {
    source_code: 4756,
    target_code: 5899,
    operation_id: "epsg-5899-forward-tm3",
    method: "native EPSG TM-3 107-45 projection",
    validated_area: "Trusted Quang Ninh anchorage reference fixtures only; longitude \
                     [107.15, 107.18]°, latitude [20.70, 20.73]°. Product control rectangle, \
                     not an official boundary, extension of EPSG:6960 onshore scope, \
                     or survey-accuracy claim.",
    tolerance: 1.0,
    tolerance_unit: CrsUnit::Metre,
    evidence_id: "mae-eia-quang-ninh-table-1-4-qh-04",
    geographic_bounds: CoordinateBounds::new(107.15, 20.70, 107.18, 20.73),
    fixture_requirement: FixtureRequirement::QuangNinhHvs,
    steps: &[TM3_FORWARD_STEP],
};

static REFERENCE_TRANSFORMATIONS: &[TransformationProfile] = &[
    WEB_FORWARD,
    TransformationProfile {
        source_code: 3857,
        target_code: 4326,
        operation_id: "epsg-1024-inverse",
        method: "analytic Pseudo-Mercator inverse",
        tolerance: 1e-8,
        tolerance_unit: CrsUnit::Degree,
        steps: &[WEB_INVERSE_STEP],
        ..WEB_FORWARD
    },
    DATUM_FORWARD,
    TransformationProfile {
        source_code: 4326,
        target_code: 4756,
        operation_id: "epsg-6960-inverse",
        method: "inverse EPSG:6960",
        steps: &[DATUM_INVERSE_STEP],
        ..DATUM_FORWARD
    },
    TransformationProfile {
        source_code: 5899,
        target_code: 4756,
        operation_id: "epsg-5899-inverse-tm3",
        method: "inverse native EPSG TM-3 107-45 projection",
        steps: &[TM3_INVERSE_STEP],
        ..TM3_FORWARD
    },
    TM3_FORWARD,
    TransformationProfile {
        source_code: 5899,
        target_code: 4326,
        operation_id: "epsg-5899-to-4326-via-6960",
        method: "explicit inverse TM-3 then EPSG:6960 approved as one registry edge",
        steps: &[TM3_INVERSE_STEP, DATUM_FORWARD_STEP],
        ..TM3_FORWARD
    },
    TransformationProfile {
        source_code: 4326,
        target_code: 5899,
        operation_id: "epsg-4326-to-5899-via-6960",
        method: "explicit inverse EPSG:6960 then TM-3 approved as one registry edge",
        steps: &[DATUM_INVERSE_STEP, TM3_FORWARD_STEP],
        ..TM3_FORWARD
    },
];

// These narrowly bounded alternatives share the datum operation but not the
// evidence or permitted region. They are selected only after immutable source
// verification. Keep the legacy R003 registry shape for existing clients.
const DA_NANG_FORWARD: TransformationProfile = TransformationProfile {
    validated_area: "Trusted Da Nang bay points DN1-DN3, Circular 46/2025/TT-BXD; not a province boundary or survey claim.",
    evidence_id: "circular-46-2025-tt-bxd-da-nang",
    geographic_bounds: CoordinateBounds::new(108.20,16.17,108.29,16.24),
    tolerance: 3.0,
    fixture_requirement: FixtureRequirement::DaNangCircular,
    ..DATUM_FORWARD
};
const HAI_PHONG_FORWARD: TransformationProfile = TransformationProfile {
    validated_area: "Trusted Hai Phong port points HP1-HP3, Circular 49/2025/TT-BXD; not a province boundary or survey claim.",
    evidence_id: "circular-49-2025-tt-bxd-hai-phong",
    geographic_bounds: CoordinateBounds::new(106.73,20.62,106.82,20.79),
    tolerance: 5.0,
    fixture_requirement: FixtureRequirement::HaiPhongCircular,
    ..DATUM_FORWARD
};
const DONG_NAI_FORWARD: TransformationProfile = TransformationProfile {
    validated_area: "Trusted Dong Nai river points HD2, HD5, HD6, Circular 02/2026/TT-BXD; not a province boundary or survey claim.",
    evidence_id: "circular-02-2026-tt-bxd-dong-nai",
    geographic_bounds: CoordinateBounds::new(106.74, 10.63, 106.87, 10.71),
    tolerance: 3.0,
    fixture_requirement: FixtureRequirement::DongNaiCircular,
    ..DATUM_FORWARD
};
static REGIONAL_DATUM_TRANSFORMATIONS: &[TransformationProfile] = &[
    DA_NANG_FORWARD,
    TransformationProfile {
        source_code: 4326,
        target_code: 4756,
        operation_id: "epsg-6960-inverse",
        method: "inverse EPSG:6960",
        steps: &[DATUM_INVERSE_STEP],
        ..DA_NANG_FORWARD
    },
    DONG_NAI_FORWARD,
    TransformationProfile {
        source_code: 4326,
        target_code: 4756,
        operation_id: "epsg-6960-inverse",
        method: "inverse EPSG:6960",
        steps: &[DATUM_INVERSE_STEP],
        ..DONG_NAI_FORWARD
    },
    HAI_PHONG_FORWARD,
    TransformationProfile {
        source_code: 4326,
        target_code: 4756,
        operation_id: "epsg-6960-inverse",
        method: "inverse EPSG:6960",
        steps: &[DATUM_INVERSE_STEP],
        ..HAI_PHONG_FORWARD
    },
];
#[must_use]
pub fn regional_datum_transformation(
    source: u32,
    target: u32,
    requirement: FixtureRequirement,
) -> Option<&'static TransformationProfile> {
    REGIONAL_DATUM_TRANSFORMATIONS.iter().find(|p| {
        p.source_code == source && p.target_code == target && p.fixture_requirement == requirement
    })
}

/// Look up a directed operation. Identity reads and inferred paths are excluded.
#[must_use]
pub fn reference_transformation(
    source_code: u32,
    target_code: u32,
) -> Option<&'static TransformationProfile> {
    REFERENCE_TRANSFORMATIONS
        .iter()
        .find(|edge| edge.source_code == source_code && edge.target_code == target_code)
}

/// Serialize the exact scoped CRS discovery contract after project authorization.
#[must_use]
pub fn reference_registry_wire(organization_id: Uuid, project_id: Uuid) -> Value {
    let profiles: Vec<_> = CRS_PROFILES.iter().map(CrsProfile::profile_wire).collect();
    let transformations: Vec<_> = REFERENCE_TRANSFORMATIONS
        .iter()
        .map(TransformationProfile::to_wire)
        .collect();
    json!({
        "organization_id": organization_id,
        "project_id": project_id,
        "profiles": profiles,
        "transformations": transformations,
    })
}

/// Explicit source/storage/display CRS policy (Constitution IV: never silently infer CRS).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrsPolicy {
    pub source_epsg: u32,
    pub storage_epsg: u32,
    pub display_epsg: u32,
}

impl CrsPolicy {
    /// Create and validate an explicit CRS policy; rejects any unknown or unreadable EPSG code.
    pub fn new(source_epsg: u32, storage_epsg: u32, display_epsg: u32) -> Result<Self, AppError> {
        let policy = Self {
            source_epsg,
            storage_epsg,
            display_epsg,
        };
        policy.validate()?;
        Ok(policy)
    }

    pub fn validate(&self) -> Result<(), AppError> {
        for code in [self.source_epsg, self.storage_epsg, self.display_epsg] {
            if lookup_readable_crs(code).is_none() {
                return Err(AppError::unsupported_crs(code));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_discovery_reports_reference_profiles_and_directed_operations() {
        let organization_id = Uuid::from_u128(1);
        let project_id = Uuid::from_u128(2);
        let wire = reference_registry_wire(organization_id, project_id);
        assert_eq!(wire["organization_id"], organization_id.to_string());
        assert_eq!(wire["project_id"], project_id.to_string());
        let profiles = wire["profiles"].as_array().unwrap();
        let codes: Vec<_> = profiles
            .iter()
            .map(|profile| profile["code"].as_u64().unwrap())
            .collect();
        assert_eq!(codes, [4326, 3857, 4756, 5899, 3405, 3406]);
        assert_eq!(profiles[0]["native_axes"], json!(["latitude", "longitude"]));
        assert_eq!(profiles[4]["disposition"], "definition_only");
        assert_eq!(profiles[5]["disposition"], "definition_only");
        assert_eq!(wire["transformations"].as_array().unwrap().len(), 8);
        assert!(reference_transformation(4326, 4326).is_none());
        assert!(reference_transformation(5899, 3857).is_none());
        assert!(reference_profile_wire(3405).is_some());
        assert!(reference_coordinate_wire(3405).is_none());
        assert!(reference_profile_wire(9209).is_none());
        for edge in REFERENCE_TRANSFORMATIONS {
            let first = edge.steps.first().unwrap();
            let last = edge.steps.last().unwrap();
            assert_eq!(first.source_code, edge.source_code);
            assert_eq!(last.target_code, edge.target_code);
            assert!(edge
                .steps
                .windows(2)
                .all(|pair| pair[0].target_code == pair[1].source_code));
        }
    }

    #[test]
    fn crs_policy_rejects_unknown_or_definition_only_codes() {
        assert!(CrsPolicy::new(5899, 4326, 3857).is_ok());
        for code in [0, 3405, 3406, 9209] {
            let error = CrsPolicy::new(code, 4326, 3857).unwrap_err();
            assert_eq!(error.code(), "UNSUPPORTED_CRS");
            assert_eq!(error.status_code(), 422);
        }
    }
}
