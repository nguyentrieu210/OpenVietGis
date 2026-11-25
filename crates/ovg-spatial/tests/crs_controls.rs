//! Independent R003 CRS control pairs transcribed by the pinned Python fixture.
//! Each direction starts with an independently published coordinate, including inverses.

use ovg_core::crs::{reference_profile_wire, reference_transformation};
use ovg_spatial::{
    Coord2D, PortableTransformKernel, TransformBackend, VerifiedReferenceProvenance,
};
use serde::Deserialize;
use serde_json::Value;

const CONTROLS: &str = include_str!("fixtures/r003_crs_controls.json");

#[derive(Deserialize)]
struct ReferenceFixture {
    profiles: Vec<Value>,
    transformations: Vec<Value>,
    control_sets: Vec<ControlSet>,
}

#[derive(Deserialize)]
struct ControlSet {
    source_code: u32,
    target_code: u32,
    operation_id: String,
    tolerance: f64,
    metric: Metric,
    controls: Vec<Control>,
}

#[derive(Deserialize)]
struct Control {
    point_id: String,
    source_xy: [f64; 2],
    expected_xy: [f64; 2],
    reference_key: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Metric {
    EuclideanMetre,
    GeodesicMetre,
    MaxAxisDegree,
}

impl Metric {
    fn residual(&self, actual: Coord2D, target: Coord2D) -> f64 {
        let dx = actual.x - target.x;
        let dy = actual.y - target.y;
        match self {
            Self::EuclideanMetre => dx.hypot(dy),
            Self::MaxAxisDegree => dx.abs().max(dy.abs()),
            Self::GeodesicMetre => {
                // Local WGS 84 ellipsoid metric. The residuals are measured at
                // the metre scale, where the local linearization is sub-millimetric.
                let latitude = target.y.to_radians();
                let equatorial_radius = 6_378_137.0;
                let eccentricity_squared = 0.006_694_379_990_141_316_5;
                let w = (1.0 - eccentricity_squared * latitude.sin().powi(2)).sqrt();
                let prime_vertical_radius = equatorial_radius / w;
                let meridional_radius =
                    equatorial_radius * (1.0 - eccentricity_squared) / w.powi(3);
                (dx.to_radians() * prime_vertical_radius * latitude.cos())
                    .hypot(dy.to_radians() * meridional_radius)
            }
        }
    }
}

#[test]
fn discovery_matches_the_pinned_all_fields_reference() {
    let reference: ReferenceFixture = serde_json::from_str(CONTROLS).unwrap();
    for expected in &reference.profiles {
        let code = u32::try_from(expected["code"].as_u64().unwrap()).unwrap();
        assert_eq!(reference_profile_wire(code).as_ref(), Some(expected));
    }
    for expected in &reference.transformations {
        let source = u32::try_from(expected["source_code"].as_u64().unwrap()).unwrap();
        let target = u32::try_from(expected["target_code"].as_u64().unwrap()).unwrap();
        let actual = reference_transformation(source, target).unwrap().to_wire();
        assert_eq!(&actual, expected);
    }
}

#[test]
fn approved_operations_match_forty_four_independent_directed_observations() {
    let reference: ReferenceFixture = serde_json::from_str(CONTROLS).unwrap();
    let kernel = PortableTransformKernel::new();
    let mut checked = 0;
    for edge in reference.control_sets {
        for point in edge.controls {
            let input = Coord2D::new(point.source_xy[0], point.source_xy[1]);
            let target = Coord2D::new(point.expected_xy[0], point.expected_xy[1]);
            let provenance = match point.reference_key.as_deref() {
                Some("reference_points_4756" | "reference_points_4326_quang_ninh") => {
                    Some(VerifiedReferenceProvenance::QuangNinh)
                }
                Some(
                    "reference_points_4326_hvs"
                    | "reference_points_4756_hvs"
                    | "reference_points_5899",
                ) => Some(VerifiedReferenceProvenance::QuangNinhHvs),
                _ => None,
            };
            let result = match provenance {
                Some(provenance) => kernel.transform_reference_point(
                    edge.source_code,
                    edge.target_code,
                    input,
                    provenance,
                ),
                None => kernel.transform_point(edge.source_code, edge.target_code, input),
            };
            let actual = result.unwrap_or_else(|error| {
                panic!("{}/{}: {}", edge.operation_id, point.point_id, error);
            });
            let residual = edge.metric.residual(actual, target);
            assert!(
                residual <= edge.tolerance,
                "{}/{}: {} exceeds {}",
                edge.operation_id,
                point.point_id,
                residual,
                edge.tolerance
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 44);
}
