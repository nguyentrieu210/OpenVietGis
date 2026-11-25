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
}

#[derive(Debug, PartialEq, Serialize)]
pub struct AlgorithmDescriptor {
    pub id: AlgorithmId,
    pub name: &'static str,
    pub description: &'static str,
    pub required_inputs: &'static [&'static str],
    pub parameters_schema: Value,
}

/// Descriptors in the order returned by the existing processing catalog.
#[must_use]
pub fn algorithm_registry() -> [AlgorithmDescriptor; 3] {
    [
        AlgorithmDescriptor {
            id: AlgorithmId::Buffer,
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
            name: "Clip",
            description: "Clips input layer features to an authorized overlay polygon layer.",
            required_inputs: &["input_layer_id", "overlay_layer_id"],
            parameters_schema: json!({}),
        },
        AlgorithmDescriptor {
            id: AlgorithmId::Dissolve,
            name: "Dissolve",
            description: "Unions features globally or grouped by an attribute field into a new layer.",
            required_inputs: &["input_layer_id"],
            parameters_schema: json!({
                "group_by_field": {"type": "string", "nullable": true}
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
            json!({ "items": algorithm_registry() }),
            json!({
                "items": [
                    {
                        "id": "buffer",
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
                        "name": "Clip",
                        "description": "Clips input layer features to an authorized overlay polygon layer.",
                        "required_inputs": ["input_layer_id", "overlay_layer_id"],
                        "parameters_schema": {}
                    },
                    {
                        "id": "dissolve",
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
