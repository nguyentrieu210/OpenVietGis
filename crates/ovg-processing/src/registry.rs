//! Metadata and parameter normalization for the R008 processing algorithms.
//!
//! The job service must separately authorize the project and input layers.

use ovg_core::AppError;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

const MIN_DISTANCE_METERS: f64 = 0.1;
const MAX_DISTANCE_METERS: f64 = 500_000.0;
const MIN_QUAD_SEGS: i64 = 2;
const MAX_QUAD_SEGS: i64 = 16;
const DEFAULT_QUAD_SEGS: i64 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlgorithmId {
    Buffer,
    Clip,
    Dissolve,
    Difference,
    Intersection,
    Union,
    Reproject,
    SpatialJoin,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct AlgorithmDescriptor {
    pub id: AlgorithmId,
    pub provider_id: &'static str,
    pub provider_name: &'static str,
    pub category: &'static str,
    pub capabilities: &'static [&'static str],
    pub name: &'static str,
    pub description: &'static str,
    pub required_inputs: &'static [&'static str],
    pub parameters_schema: Value,
}

/// Descriptors in the order returned by the existing processing catalog.
#[must_use]
pub fn algorithm_registry() -> [AlgorithmDescriptor; 8] {
    [
        AlgorithmDescriptor {
            id: AlgorithmId::Buffer,
            provider_id: "openvietgis",
            provider_name: "OpenVietGIS Core",
            category: "vector",
            capabilities: &["vector.read", "geometry.buffer"],
            name: "Buffer",
            description: "Creates projected-distance buffers (Web Mercator for geographic inputs) and publishes a new Polygon layer.",
            required_inputs: &["input_layer_id"],
            parameters_schema: json!({
                "distance_meters": {
                    "type": "number",
                    "min": MIN_DISTANCE_METERS,
                    "max": MAX_DISTANCE_METERS
                },
                "quad_segs": {
                    "type": "integer",
                    "min": MIN_QUAD_SEGS,
                    "max": MAX_QUAD_SEGS,
                    "default": DEFAULT_QUAD_SEGS
                }
            }),
        },
        AlgorithmDescriptor {
            id: AlgorithmId::Clip,
            provider_id: "openvietgis",
            provider_name: "OpenVietGIS Core",
            category: "vector",
            capabilities: &["vector.read", "geometry.clip", "vector.overlay"],
            name: "Clip",
            description: "Clips input layer features to an authorized overlay polygon layer.",
            required_inputs: &["input_layer_id", "overlay_layer_id"],
            parameters_schema: json!({}),
        },
        AlgorithmDescriptor {
            id: AlgorithmId::Dissolve,
            provider_id: "openvietgis",
            provider_name: "OpenVietGIS Core",
            category: "vector",
            capabilities: &["vector.read", "geometry.dissolve"],
            name: "Dissolve",
            description: "Unions features globally or grouped by an attribute field into a new layer.",
            required_inputs: &["input_layer_id"],
            parameters_schema: json!({
                "group_by_field": {"type": "string", "nullable": true}
            }),
        },
        AlgorithmDescriptor {
            id: AlgorithmId::Difference,
            provider_id: "openvietgis",
            provider_name: "OpenVietGIS Core",
            category: "vector",
            capabilities: &["vector.read", "geometry.difference", "vector.overlay"],
            name: "Difference",
            description: "Subtracts the union of polygon overlay geometries; output includes source feature lineage.",
            required_inputs: &["input_layer_id", "overlay_layer_id"],
            parameters_schema: json!({}),
        },
        AlgorithmDescriptor {
            id: AlgorithmId::Intersection,
            provider_id: "openvietgis",
            provider_name: "OpenVietGIS Core",
            category: "vector",
            capabilities: &["vector.read", "geometry.intersection", "vector.overlay"],
            name: "Intersection",
            description: "Intersects each input feature with each overlapping polygon; output includes both feature IDs, not a full attribute join.",
            required_inputs: &["input_layer_id", "overlay_layer_id"],
            parameters_schema: json!({}),
        },
        AlgorithmDescriptor {
            id: AlgorithmId::Union,
            provider_id: "openvietgis",
            provider_name: "OpenVietGIS Core",
            category: "vector",
            capabilities: &["vector.read", "geometry.union", "vector.overlay"],
            name: "Union",
            description: "Polygon overlay union with both input schemas, including unmatched parts.",
            required_inputs: &["input_layer_id", "overlay_layer_id"],
            parameters_schema: json!({}),
        },
        AlgorithmDescriptor {
            id: AlgorithmId::Reproject,
            provider_id: "openvietgis",
            provider_name: "OpenVietGIS Core",
            category: "vector",
            capabilities: &["vector.read", "geometry.reproject"],
            name: "Reproject",
            description: "Transforms geometries using approved directed CRS pipelines or bounded native VN-2000 projections. Datum changes require trusted regional evidence. Retains attributes.",
            required_inputs: &["input_layer_id"],
            parameters_schema: json!({"target_srid":{"type":"integer","enum":[4326,3857,4756,3405,3406,5896,5897,5898,5899,9205,9206,9207,9208,9209,9210,9211,9212,9213,9214,9215,9216,9217,9218]}}),
        },
        AlgorithmDescriptor {
            id: AlgorithmId::SpatialJoin,
            provider_id: "openvietgis",
            provider_name: "OpenVietGIS Core",
            category: "vector",
            capabilities: &["vector.read", "geometry.spatial_join", "vector.overlay"],
            name: "Spatial Join",
            description: "Join attributes by a spatial predicate, retaining input geometry with inner or left join and deterministic first or all matches.",
            required_inputs: &["input_layer_id", "overlay_layer_id"],
            parameters_schema: json!({
                "predicate":{"type":"string","enum":["intersects","within","contains","touches","overlaps","crosses","equals"],"default":"intersects"},
                "join_type":{"type":"string","enum":["left","inner"],"default":"left"},
                "match_mode":{"type":"string","enum":["all","first"],"default":"all"}
            }),
        },
    ]
}

/// Validate parameters and return the canonical job payload.
pub fn validate_algorithm_parameters(
    algorithm_id: AlgorithmId,
    parameters: &Map<String, Value>,
) -> Result<Value, AppError> {
    match algorithm_id {
        AlgorithmId::Buffer => validate_buffer(parameters),
        AlgorithmId::Clip if parameters.is_empty() => Ok(json!({})),
        AlgorithmId::Clip => Err(AppError::validation(
            "INVALID_PARAMETERS",
            "unknown clip parameter",
        )),
        AlgorithmId::Dissolve => validate_dissolve(parameters),
        AlgorithmId::Difference | AlgorithmId::Intersection | AlgorithmId::Union
            if parameters.is_empty() =>
        {
            Ok(json!({}))
        }
        AlgorithmId::Difference | AlgorithmId::Intersection | AlgorithmId::Union => Err(
            AppError::validation("INVALID_PARAMETERS", "unknown overlay parameter"),
        ),
        AlgorithmId::Reproject => {
            let srid = parameters.get("target_srid").and_then(Value::as_i64);
            if parameters.len() != 1 || !srid.and_then(|v|u32::try_from(v).ok()).is_some_and(ovg_core::crs::is_processing_crs) {
                return Err(AppError::validation("INVALID_PARAMETERS", "target_srid must be an approved web or native VN-2000 CRS; regional evidence is checked at execution"));
            }
            Ok(json!({"target_srid": srid}))
        }
        AlgorithmId::SpatialJoin => {
            if parameters
                .keys()
                .any(|k| !matches!(k.as_str(), "predicate" | "join_type" | "match_mode"))
            {
                return Err(AppError::validation(
                    "INVALID_PARAMETERS",
                    "unknown spatial join parameter",
                ));
            }
            let pick = |key: &str, default: &str, allowed: &[&str]| -> Result<String, AppError> {
                let value = match parameters.get(key) {
                    None => default,
                    Some(v) => v.as_str().ok_or_else(|| {
                        AppError::validation(
                            "INVALID_PARAMETERS",
                            "spatial join parameter must be a string",
                        )
                    })?,
                };
                if !allowed.contains(&value) {
                    return Err(AppError::validation(
                        "INVALID_PARAMETERS",
                        "unsupported spatial join option",
                    ));
                }
                Ok(value.to_owned())
            };
            Ok(json!({
                "predicate":pick("predicate","intersects",&["intersects","within","contains","touches","overlaps","crosses","equals"])?,
                "join_type":pick("join_type","left",&["left","inner"])?,
                "match_mode":pick("match_mode","all",&["all","first"])?
            }))
        }
    }
}

fn validate_buffer(parameters: &Map<String, Value>) -> Result<Value, AppError> {
    if parameters
        .keys()
        .any(|key| !matches!(key.as_str(), "distance_meters" | "quad_segs"))
    {
        return Err(AppError::validation(
            "INVALID_PARAMETERS",
            "unknown buffer parameter",
        ));
    }

    let distance = parameters
        .get("distance_meters")
        .and_then(Value::as_f64)
        .filter(|value| (MIN_DISTANCE_METERS..=MAX_DISTANCE_METERS).contains(value))
        .ok_or_else(|| {
            AppError::validation(
                "INVALID_PARAMETERS",
                "distance_meters must be a number between 0.1 and 500000",
            )
        })?;

    let quad_segs = match parameters.get("quad_segs") {
        None => DEFAULT_QUAD_SEGS,
        Some(value) => value
            .as_i64()
            .filter(|value| (MIN_QUAD_SEGS..=MAX_QUAD_SEGS).contains(value))
            .ok_or_else(|| {
                AppError::validation("INVALID_PARAMETERS", "quad_segs must be between 2 and 16")
            })?,
    };

    Ok(json!({ "distance_meters": distance, "quad_segs": quad_segs }))
}

fn validate_dissolve(parameters: &Map<String, Value>) -> Result<Value, AppError> {
    if parameters.keys().any(|key| key != "group_by_field") {
        return Err(AppError::validation(
            "INVALID_PARAMETERS",
            "unknown dissolve parameter",
        ));
    }

    let group_field = match parameters.get("group_by_field") {
        None | Some(Value::Null) => None,
        Some(Value::String(value)) => {
            // Python str.strip also treats these ASCII separators as whitespace.
            let trimmed = value.trim_matches(|character: char| {
                character.is_whitespace() || matches!(character, '\u{001c}'..='\u{001f}')
            });
            Some(trimmed).filter(|field| !field.is_empty())
        }
        Some(_) => {
            return Err(AppError::validation(
                "INVALID_PARAMETERS",
                "group_by_field must be a field id string or null",
            ));
        }
    };

    Ok(json!({ "group_by_field": group_field }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn validate(algorithm: AlgorithmId, parameters: Value) -> Result<Value, AppError> {
        validate_algorithm_parameters(algorithm, parameters.as_object().unwrap())
    }

    #[test]
    fn matches_the_legacy_catalog_wire_shape_and_order() {
        assert_eq!(
            json!({ "items": &algorithm_registry()[..3] }),
            json!({
                "items": [
                    {
                        "id": "buffer",
                        "provider_id": "openvietgis",
                        "provider_name": "OpenVietGIS Core",
                        "category": "vector",
                        "capabilities": ["vector.read", "geometry.buffer"],
                        "name": "Buffer",
                        "description": "Creates projected-distance buffers (Web Mercator for geographic inputs) and publishes a new Polygon layer.",
                        "required_inputs": ["input_layer_id"],
                        "parameters_schema": {
                            "distance_meters": {"type": "number", "min": 0.1, "max": 500_000.0},
                            "quad_segs": {"type": "integer", "min": 2, "max": 16, "default": 8}
                        }
                    },
                    {
                        "id": "clip",
                        "provider_id": "openvietgis",
                        "provider_name": "OpenVietGIS Core",
                        "category": "vector",
                        "capabilities": ["vector.read", "geometry.clip", "vector.overlay"],
                        "name": "Clip",
                        "description": "Clips input layer features to an authorized overlay polygon layer.",
                        "required_inputs": ["input_layer_id", "overlay_layer_id"],
                        "parameters_schema": {}
                    },
                    {
                        "id": "dissolve",
                        "provider_id": "openvietgis",
                        "provider_name": "OpenVietGIS Core",
                        "category": "vector",
                        "capabilities": ["vector.read", "geometry.dissolve"],
                        "name": "Dissolve",
                        "description": "Unions features globally or grouped by an attribute field into a new layer.",
                        "required_inputs": ["input_layer_id"],
                        "parameters_schema": {
                            "group_by_field": {"type": "string", "nullable": true}
                        }
                    }
                ]
            })
        );
        assert_eq!(
            serde_json::from_value::<AlgorithmId>(json!("clip")).unwrap(),
            AlgorithmId::Clip
        );
        assert!(serde_json::from_value::<AlgorithmId>(json!("Clip")).is_err());
    }

    #[test]
    fn expanded_overlay_catalog() {
        let catalog = algorithm_registry();
        assert_eq!(catalog[3].id, AlgorithmId::Difference);
        assert_eq!(catalog[4].id, AlgorithmId::Intersection);
        for algorithm in [AlgorithmId::Difference, AlgorithmId::Intersection] {
            assert_eq!(validate(algorithm, json!({})).unwrap(), json!({}));
            assert!(validate(algorithm, json!({"unexpected": 1})).is_err());
        }
    }

    #[test]
    fn validates_new_vector_algorithms() {
        assert_eq!(algorithm_registry().len(), 8);
        assert_eq!(validate(AlgorithmId::Union, json!({})).unwrap(), json!({}));
        assert_eq!(
            validate(AlgorithmId::Reproject, json!({"target_srid":3857})).unwrap(),
            json!({"target_srid":3857})
        );
        assert!(validate(AlgorithmId::Reproject, json!({"target_srid":"3857"})).is_err());
        assert!(validate(AlgorithmId::Reproject, json!({"target_srid":3405})).is_ok());
        assert!(validate(AlgorithmId::Reproject, json!({"target_srid":9999})).is_err());
        assert_eq!(
            validate(AlgorithmId::Reproject, json!({"target_srid":5899})).unwrap(),
            json!({"target_srid":5899})
        );
        assert_eq!(
            validate(AlgorithmId::SpatialJoin, json!({})).unwrap(),
            json!({"predicate":"intersects","join_type":"left","match_mode":"all"})
        );
        assert!(validate(AlgorithmId::SpatialJoin, json!({"predicate":"ST_Anything"})).is_err());
        assert!(validate(AlgorithmId::SpatialJoin, json!({"match_mode":"random"})).is_err());
    }

    #[test]
    fn normalizes_buffer_distance_and_optional_segment_count() {
        let distances = [json!(0.1), json!(12.5), json!(250), json!(500_000)];
        for distance in distances {
            let normalized = distance.as_f64().unwrap();
            assert_eq!(
                validate(AlgorithmId::Buffer, json!({"distance_meters": distance})).unwrap(),
                json!({"distance_meters": normalized, "quad_segs": 8})
            );
        }
        assert_eq!(
            validate(
                AlgorithmId::Buffer,
                json!({"distance_meters": 0.1, "quad_segs": 2})
            )
            .unwrap(),
            json!({"distance_meters": 0.1, "quad_segs": 2})
        );
        assert_eq!(
            validate(
                AlgorithmId::Buffer,
                json!({"distance_meters": 500_000, "quad_segs": 16})
            )
            .unwrap(),
            json!({"distance_meters": 500_000.0, "quad_segs": 16})
        );
    }

    #[test]
    fn rejects_buffer_parameters_without_coercion() {
        let cases = [
            (json!({"unexpected": 1}), "unknown buffer parameter"),
            (
                json!({}),
                "distance_meters must be a number between 0.1 and 500000",
            ),
            (
                json!({"distance_meters": true}),
                "distance_meters must be a number between 0.1 and 500000",
            ),
            (
                json!({"distance_meters": "1000"}),
                "distance_meters must be a number between 0.1 and 500000",
            ),
            (
                json!({"distance_meters": 0.09}),
                "distance_meters must be a number between 0.1 and 500000",
            ),
            (
                json!({"distance_meters": 500_001}),
                "distance_meters must be a number between 0.1 and 500000",
            ),
            (
                json!({"distance_meters": 100, "quad_segs": 2.0}),
                "quad_segs must be between 2 and 16",
            ),
            (
                json!({"distance_meters": 100, "quad_segs": "8"}),
                "quad_segs must be between 2 and 16",
            ),
            (
                json!({"distance_meters": 100, "quad_segs": null}),
                "quad_segs must be between 2 and 16",
            ),
            (
                json!({"distance_meters": 100, "quad_segs": 1}),
                "quad_segs must be between 2 and 16",
            ),
            (
                json!({"distance_meters": 100, "quad_segs": 17}),
                "quad_segs must be between 2 and 16",
            ),
        ];

        for (parameters, message) in cases {
            let error = validate(AlgorithmId::Buffer, parameters).unwrap_err();
            assert_eq!(error.code(), "INVALID_PARAMETERS");
            assert_eq!(error.to_string(), message);
        }
    }

    #[test]
    fn validates_clip_and_normalizes_optional_dissolve_grouping() {
        assert_eq!(validate(AlgorithmId::Clip, json!({})).unwrap(), json!({}));
        let dissolve_cases = [
            (json!({}), Value::Null),
            (json!({"group_by_field": null}), Value::Null),
            (json!({"group_by_field": " \t  "}), Value::Null),
            (
                json!({"group_by_field": "\u{001c} Quận \u{00a0}"}),
                json!("Quận"),
            ),
        ];
        for (parameters, group_field) in dissolve_cases {
            assert_eq!(
                validate(AlgorithmId::Dissolve, parameters).unwrap(),
                json!({"group_by_field": group_field})
            );
        }

        let invalid_cases = [
            (
                AlgorithmId::Clip,
                json!({"unexpected": 1}),
                "unknown clip parameter",
            ),
            (
                AlgorithmId::Dissolve,
                json!({"unexpected": 1}),
                "unknown dissolve parameter",
            ),
            (
                AlgorithmId::Dissolve,
                json!({"group_by_field": 1}),
                "group_by_field must be a field id string or null",
            ),
        ];
        for (algorithm, parameters, message) in invalid_cases {
            let error = validate(algorithm, parameters).unwrap_err();
            assert_eq!(error.code(), "INVALID_PARAMETERS");
            assert_eq!(error.to_string(), message);
        }
    }
}
