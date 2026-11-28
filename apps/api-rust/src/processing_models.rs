//! E08 Rust-native reusable processing DAG control plane.

use std::collections::{HashMap, HashSet};

use axum::extract::rejection::QueryRejection;
use axum::extract::{Path, Query, Request, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use ovg_processing::registry::{validate_algorithm_parameters, AlgorithmId as VectorAlgorithm};
use ovg_storage::editing::{EditError, Guard, Permission};
use ovg_storage::processing_models::{self as store, ModelStoreError, ModelWrite};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use uuid::Uuid;

use crate::catalog::CatalogError;
use crate::edit_auth::{
    authenticate_actor, authenticate_json, decode, open, require_csrf, Failure,
};
use crate::AppState;

const BASE: &str = "/organizations/{organization_id}/projects/{project_id}/processing/models";

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route(BASE, get(list_models).post(create_model))
        .route(
            &format!("{BASE}/{{model_id}}"),
            axum::routing::put(update_model).delete(delete_model),
        )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
enum RefType {
    Layer,
    Step,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct InputRef {
    #[serde(rename = "type")]
    kind: RefType,
    id: Uuid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum Algorithm {
    Buffer,
    Clip,
    Dissolve,
    Difference,
    Intersection,
    Union,
    Reproject,
    SpatialJoin,
    Band,
    Ndvi,
    Ndwi,
    Ndbi,
    Hillshade,
    Slope,
    Aspect,
    Resample,
    Mosaic,
    Change,
    Rgb,
    Calculator,
}

impl Algorithm {
    const fn vector(self) -> bool {
        matches!(
            self,
            Self::Buffer
                | Self::Clip
                | Self::Dissolve
                | Self::Difference
                | Self::Intersection
                | Self::Union
                | Self::Reproject
                | Self::SpatialJoin
        )
    }

    const fn always_two_inputs(self) -> bool {
        matches!(
            self,
            Self::Clip
                | Self::Difference
                | Self::Intersection
                | Self::Union
                | Self::SpatialJoin
                | Self::Ndvi
                | Self::Ndwi
                | Self::Ndbi
                | Self::Mosaic
                | Self::Change
                | Self::Rgb
        )
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Node {
    id: Uuid,
    algorithm: Algorithm,
    name: String,
    a: InputRef,
    #[serde(default)]
    b: Option<InputRef>,
    #[serde(default)]
    c: Option<InputRef>,
    #[serde(default)]
    parameters: Map<String, Value>,
    #[serde(default)]
    x: i32,
    #[serde(default)]
    y: i32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Graph {
    #[serde(default = "graph_version")]
    version: i32,
    nodes: Vec<Node>,
}

const fn graph_version() -> i32 {
    1
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ModelRequest {
    name: String,
    graph: Graph,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateRequest {
    name: String,
    graph: Graph,
    revision: i32,
}

#[derive(Debug, Deserialize)]
struct DeleteQuery {
    revision: i32,
}

async fn list_models(
    State(state): State<AppState>,
    Path((organization, project)): Path<(String, String)>,
    request: Request,
) -> Result<Json<Value>, ModelError> {
    let (mut parts, _) = request.into_parts();
    let actor = authenticate_actor(&state, &mut parts, &organization)
        .await
        .map_err(ModelError::Failure)?;
    let Some(mut scope) = open(&state, &actor, &organization, &project, Guard::Read)
        .await
        .map_err(ModelError::Failure)?
    else {
        return Err(ModelError::InvalidRequest);
    };
    scope
        .authorize(Permission::Read)
        .await
        .map_err(|e| ModelError::Failure(Failure::Edit(e)))?;
    let result = store::list_models_scoped(&mut scope)
        .await
        .map_err(ModelError::Store)?;
    scope
        .commit()
        .await
        .map_err(|_| ModelError::Store(ModelStoreError::Database))?;
    Ok(Json(result))
}

async fn create_model(
    State(state): State<AppState>,
    Path((organization, project)): Path<(String, String)>,
    request: Request,
) -> Result<(StatusCode, Json<Value>), ModelError> {
    let (actor, headers, body) = authenticate_json(&state, request, &organization)
        .await
        .map_err(ModelError::Failure)?;
    let scope = open(&state, &actor, &organization, &project, Guard::ScopedDenied)
        .await
        .map_err(ModelError::Failure)?;
    require_csrf(&state, &actor, &headers).map_err(ModelError::Failure)?;
    let body: ModelRequest = decode(body).map_err(ModelError::Failure)?;
    let input = normalize_model(body.name, body.graph)?;

    let Some(mut scope) = scope else {
        return Err(ModelError::InvalidRequest);
    };
    scope
        .authorize(Permission::Edit)
        .await
        .map_err(|e| ModelError::Failure(Failure::Edit(e)))?;
    let result = store::create_model_scoped(&mut scope, input)
        .await
        .map_err(ModelError::Store)?;
    scope
        .commit()
        .await
        .map_err(|_| ModelError::Store(ModelStoreError::Database))?;
    Ok((StatusCode::CREATED, Json(result)))
}

async fn update_model(
    State(state): State<AppState>,
    Path((organization, project, model)): Path<(String, String, String)>,
    request: Request,
) -> Result<Json<Value>, ModelError> {
    let (actor, headers, body) = authenticate_json(&state, request, &organization)
        .await
        .map_err(ModelError::Failure)?;
    let scope = open(&state, &actor, &organization, &project, Guard::ScopedDenied)
        .await
        .map_err(ModelError::Failure)?;
    require_csrf(&state, &actor, &headers).map_err(ModelError::Failure)?;
    let body: UpdateRequest = decode(body).map_err(ModelError::Failure)?;
    if body.revision < 1 {
        return Err(ModelError::InvalidRequest);
    }
    let input = normalize_model(body.name, body.graph)?;

    let (Some(mut scope), Ok(model_id)) = (scope, Uuid::parse_str(&model)) else {
        return Err(ModelError::InvalidRequest);
    };
    scope
        .authorize(Permission::Edit)
        .await
        .map_err(|e| ModelError::Failure(Failure::Edit(e)))?;
    let result = store::update_model_scoped(&mut scope, model_id, body.revision, input)
        .await
        .map_err(ModelError::Store)?;
    scope
        .commit()
        .await
        .map_err(|_| ModelError::Store(ModelStoreError::Database))?;
    Ok(Json(result))
}

async fn delete_model(
    State(state): State<AppState>,
    Path((organization, project, model)): Path<(String, String, String)>,
    query: Result<Query<DeleteQuery>, QueryRejection>,
    request: Request,
) -> Result<StatusCode, ModelError> {
    let (actor, headers, _) = authenticate_json(&state, request, &organization)
        .await
        .map_err(ModelError::Failure)?;
    let scope = open(&state, &actor, &organization, &project, Guard::ScopedDenied)
        .await
        .map_err(ModelError::Failure)?;
    require_csrf(&state, &actor, &headers).map_err(ModelError::Failure)?;
    let Query(query) = query.map_err(|_| ModelError::InvalidRequest)?;
    let (Some(mut scope), Ok(model_id)) = (scope, Uuid::parse_str(&model)) else {
        return Err(ModelError::InvalidRequest);
    };
    scope
        .authorize(Permission::Edit)
        .await
        .map_err(|e| ModelError::Failure(Failure::Edit(e)))?;
    store::delete_model_scoped(&mut scope, model_id, query.revision)
        .await
        .map_err(ModelError::Store)?;
    scope
        .commit()
        .await
        .map_err(|_| ModelError::Store(ModelStoreError::Database))?;
    let _ = EditError::InvalidRequest;
    Ok(StatusCode::NO_CONTENT)
}

fn normalize_model(name: String, mut graph: Graph) -> Result<ModelWrite, ModelError> {
    if name.chars().count() > 120 || name.is_empty() {
        return Err(ModelError::InvalidRequest);
    }
    let name = name.trim().to_owned();
    if name.is_empty() {
        return Err(ModelError::Http(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Model name is required",
        ));
    }
    validate_graph(&mut graph).map_err(|_| ModelError::InvalidRequest)?;
    let graph = serde_json::to_value(graph).map_err(|_| ModelError::InvalidRequest)?;
    Ok(ModelWrite { name, graph })
}

fn validate_graph(graph: &mut Graph) -> Result<(), ()> {
    if graph.version != 1 || graph.nodes.is_empty() || graph.nodes.len() > 12 {
        return Err(());
    }
    let ids: HashSet<_> = graph.nodes.iter().map(|node| node.id).collect();
    if ids.len() != graph.nodes.len() {
        return Err(());
    }

    for node in &mut graph.nodes {
        validate_node(node)?;
    }

    let by_id: HashMap<_, _> = graph
        .nodes
        .iter()
        .map(|node| (node.id, node.algorithm))
        .collect();
    for node in &graph.nodes {
        for reference in [
            &node.a,
            node.b.as_ref().unwrap_or(&node.a),
            node.c.as_ref().unwrap_or(&node.a),
        ] {
            if reference.kind != RefType::Step {
                continue;
            }
            let Some(algorithm) = by_id.get(&reference.id) else {
                return Err(());
            };
            if algorithm.vector() != node.algorithm.vector() {
                return Err(());
            }
        }
        for reference in node.b.iter().chain(node.c.iter()) {
            if reference.kind == RefType::Step && !by_id.contains_key(&reference.id) {
                return Err(());
            }
        }
        if node.a.kind == RefType::Step && !by_id.contains_key(&node.a.id) {
            return Err(());
        }
    }

    let mut done = HashSet::new();
    let mut pending: HashSet<_> = by_id.keys().copied().collect();
    while !pending.is_empty() {
        let ready = pending
            .iter()
            .copied()
            .filter(|id| {
                let node = graph
                    .nodes
                    .iter()
                    .find(|node| node.id == *id)
                    .expect("known id");
                [
                    &node.a,
                    node.b.as_ref().unwrap_or(&node.a),
                    node.c.as_ref().unwrap_or(&node.a),
                ]
                .into_iter()
                .all(|reference| reference.kind != RefType::Step || done.contains(&reference.id))
            })
            .collect::<Vec<_>>();
        if ready.is_empty() {
            return Err(());
        }
        for id in ready {
            pending.remove(&id);
            done.insert(id);
        }
    }
    Ok(())
}

fn validate_node(node: &mut Node) -> Result<(), ()> {
    if node.name.is_empty()
        || node.name.chars().count() > 120
        || !(0..=5000).contains(&node.x)
        || !(0..=5000).contains(&node.y)
    {
        return Err(());
    }
    node.name = node.name.trim().to_owned();
    if node.name.is_empty() {
        return Err(());
    }
    if node
        .parameters
        .values()
        .any(|value| !matches!(value, Value::Null | Value::String(_) | Value::Number(_)))
    {
        return Err(());
    }

    let uses_b = node.algorithm.always_two_inputs()
        || (node.algorithm == Algorithm::Calculator
            && node
                .parameters
                .get("formula")
                .and_then(Value::as_str)
                .is_some_and(|formula| contains_word(formula, 'B')));
    if node.b.is_some() != uses_b || node.c.is_some() != (node.algorithm == Algorithm::Rgb) {
        return Err(());
    }

    if node.algorithm.vector() {
        let algorithm = match node.algorithm {
            Algorithm::Buffer => VectorAlgorithm::Buffer,
            Algorithm::Clip => VectorAlgorithm::Clip,
            Algorithm::Dissolve => VectorAlgorithm::Dissolve,
            Algorithm::Difference => VectorAlgorithm::Difference,
            Algorithm::Intersection => VectorAlgorithm::Intersection,
            Algorithm::Union => VectorAlgorithm::Union,
            Algorithm::Reproject => VectorAlgorithm::Reproject,
            Algorithm::SpatialJoin => VectorAlgorithm::SpatialJoin,
            _ => unreachable!(),
        };
        let normalized =
            validate_algorithm_parameters(algorithm, &node.parameters).map_err(|_| ())?;
        node.parameters = normalized.as_object().cloned().ok_or(())?;
        return Ok(());
    }

    validate_raster_parameters(node)
}

fn validate_raster_parameters(node: &mut Node) -> Result<(), ()> {
    let uses_b = node.b.is_some();
    let mut allowed = HashSet::from(["band_a"]);
    if uses_b {
        allowed.insert("band_b");
    }
    if node.algorithm == Algorithm::Rgb {
        allowed.insert("band_c");
    }
    if node.algorithm == Algorithm::Resample {
        allowed.insert("factor");
        allowed.insert("resampling");
    }
    if node.algorithm == Algorithm::Calculator {
        allowed.insert("formula");
    }
    if node
        .parameters
        .keys()
        .any(|key| !allowed.contains(key.as_str()))
    {
        return Err(());
    }

    for key in ["band_a", "band_b", "band_c"] {
        if !allowed.contains(key) {
            continue;
        }
        let value = node
            .parameters
            .entry(key.to_owned())
            .or_insert(Value::from(1));
        let band = value
            .as_i64()
            .filter(|band| (1..=16).contains(band))
            .ok_or(())?;
        *value = Value::from(band);
    }

    if node.algorithm == Algorithm::Resample {
        let factor = node
            .parameters
            .get("factor")
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite() && (0.1..=10.0).contains(value))
            .ok_or(())?;
        let resampling = node
            .parameters
            .get("resampling")
            .and_then(Value::as_str)
            .filter(|value| matches!(*value, "near" | "bilinear"))
            .ok_or(())?
            .to_owned();
        let _ = factor;
        node.parameters
            .insert("resampling".to_owned(), Value::String(resampling));
    }

    if node.algorithm == Algorithm::Calculator {
        let formula = node
            .parameters
            .get("formula")
            .and_then(Value::as_str)
            .filter(|formula| {
                let len = formula.chars().count();
                (1..=200).contains(&len) && contains_word(formula, 'A')
            })
            .ok_or(())?
            .to_owned();
        node.parameters
            .insert("formula".to_owned(), Value::String(formula));
    }
    Ok(())
}

fn contains_word(text: &str, needle: char) -> bool {
    let chars = text.chars().collect::<Vec<_>>();
    chars.iter().enumerate().any(|(index, ch)| {
        *ch == needle
            && (index == 0 || !is_word(chars[index - 1]))
            && (index + 1 == chars.len() || !is_word(chars[index + 1]))
    })
}

fn is_word(value: char) -> bool {
    value == '_' || value.is_alphanumeric()
}

enum ModelError {
    InvalidRequest,
    Failure(Failure),
    Store(ModelStoreError),
    Http(StatusCode, &'static str),
}

impl IntoResponse for ModelError {
    fn into_response(self) -> Response {
        match self {
            Self::InvalidRequest => CatalogError::InvalidRequest.into_response(),
            Self::Failure(failure) => failure.into_response(),
            Self::Store(ModelStoreError::Quota) => (
                StatusCode::PAYLOAD_TOO_LARGE,
                Json(serde_json::json!({"detail": "Maximum 50 models per project"})),
            )
                .into_response(),
            Self::Store(ModelStoreError::Conflict) => (
                StatusCode::CONFLICT,
                Json(serde_json::json!({"detail": "Model changed or removed; reload the catalog"})),
            )
                .into_response(),
            Self::Store(ModelStoreError::Database) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response()
            }
            Self::Http(status, detail) => {
                (status, Json(serde_json::json!({"detail": detail}))).into_response()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn uuid(value: u128) -> Uuid {
        Uuid::from_u128(value)
    }

    #[test]
    fn validates_and_normalizes_vector_and_raster_dag() {
        let mut graph: Graph = serde_json::from_value(json!({
            "version": 1,
            "nodes": [
                {
                    "id": uuid(1),
                    "algorithm": "buffer",
                    "name": " Buffer ",
                    "a": {"type": "layer", "id": uuid(10)},
                    "parameters": {"distance_meters": 5}
                },
                {
                    "id": uuid(2),
                    "algorithm": "resample",
                    "name": "Raster",
                    "a": {"type": "layer", "id": uuid(11)},
                    "parameters": {"factor": 2, "resampling": "near"}
                },
                {
                    "id": uuid(3),
                    "algorithm": "calculator",
                    "name": "Calc",
                    "a": {"type": "step", "id": uuid(2)},
                    "b": {"type": "layer", "id": uuid(12)},
                    "parameters": {"formula": "A + B"}
                }
            ]
        }))
        .unwrap();
        validate_graph(&mut graph).unwrap();
        assert_eq!(graph.nodes[0].name, "Buffer");
        assert_eq!(graph.nodes[0].parameters["quad_segs"], 8);
        assert_eq!(graph.nodes[1].parameters["band_a"], 1);
        assert_eq!(graph.nodes[2].parameters["band_b"], 1);
    }

    #[test]
    fn rejects_cycles_cross_family_ports_and_bad_port_counts() {
        let cases = [
            json!({
                "nodes": [
                    {"id": uuid(1), "algorithm":"buffer","name":"A",
                     "a":{"type":"step","id":uuid(2)}, "parameters":{"distance_meters":1}},
                    {"id": uuid(2), "algorithm":"buffer","name":"B",
                     "a":{"type":"step","id":uuid(1)}, "parameters":{"distance_meters":1}}
                ]
            }),
            json!({
                "nodes": [
                    {"id": uuid(1), "algorithm":"band","name":"Raster",
                     "a":{"type":"layer","id":uuid(10)}},
                    {"id": uuid(2), "algorithm":"buffer","name":"Vector",
                     "a":{"type":"step","id":uuid(1)}, "parameters":{"distance_meters":1}}
                ]
            }),
            json!({
                "nodes": [
                    {"id": uuid(1), "algorithm":"ndvi","name":"Missing B",
                     "a":{"type":"layer","id":uuid(10)}}
                ]
            }),
        ];
        for value in cases {
            let mut graph: Graph = serde_json::from_value(value).unwrap();
            assert!(validate_graph(&mut graph).is_err());
        }
    }
}
