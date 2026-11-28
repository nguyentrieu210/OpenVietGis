//! E08 Rust-native processing control plane & bounded Rayon execution engine.
//!
//! Geometry SQL remains in replaceable PostGIS workers; this module owns the
//! authenticated OpenVietGIS API contract, algorithm registry, bounded Rayon
//! validation/hashing execution, and durable job lifecycle.

use std::sync::{Arc, Mutex};

use axum::extract::{Path, Query, Request, State};
use axum::http::{StatusCode, Uri};
use axum::routing::{get, post};
use axum::{Json, Router};
use ovg_core::AppError;
use ovg_processing::registry::{validate_algorithm_parameters, AlgorithmId};
use ovg_processing::{JobId, JobStatus};
use ovg_storage::editing::{EditError, Guard, Permission};
use ovg_storage::processing_jobs::{self, ExecutionLockState};
use serde::Deserialize;
use serde_json::{Map, Value};
use uuid::Uuid;

use crate::edit_auth::{
    authenticate_actor, authenticate_json, decode, open, require_csrf, Actor, Failure,
};
use crate::AppState;

const BASE: &str = "/organizations/{organization_id}/projects/{project_id}/processing";

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route(&format!("{BASE}/algorithms"), get(list_algorithms))
        .route(&format!("{BASE}/jobs"), get(list_jobs).post(create_job))
        .route(&format!("{BASE}/jobs/{{job_id}}"), get(get_job))
        .route(&format!("{BASE}/jobs/{{job_id}}/cancel"), post(cancel_job))
        .route(&format!("{BASE}/jobs/{{job_id}}/retry"), post(retry_job))
}

fn algorithm_registry_wire() -> Value {
    serde_json::json!({ "items": ovg_processing::registry::algorithm_registry() })
}

async fn list_algorithms(
    State(state): State<AppState>,
    Path((organization, project)): Path<(String, String)>,
    request: Request,
) -> Result<Json<Value>, Failure> {
    let (mut parts, _) = request.into_parts();
    let actor = authenticate_actor(&state, &mut parts, &organization).await?;
    let Some(mut scope) = open(&state, &actor, &organization, &project, Guard::Read).await? else {
        return Err(EditError::InvalidRequest.into());
    };
    scope.authorize(Permission::Read).await?;
    scope.commit().await?;
    Ok(Json(algorithm_registry_wire()))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateProcessingJobRequest {
    algorithm_id: AlgorithmId,
    input_layer_id: Uuid,
    #[serde(default)]
    overlay_layer_id: Option<Uuid>,
    #[serde(default)]
    expected_input: Option<processing_jobs::ExpectedSource>,
    #[serde(default)]
    expected_overlay: Option<processing_jobs::ExpectedSource>,
    #[serde(default)]
    parameters: Map<String, Value>,
    output_name: String,
    operation_id: String,
    #[serde(default = "default_execute_now")]
    execute_now: bool,
}

const fn default_execute_now() -> bool {
    true
}

fn app_to_edit_error(err: AppError) -> EditError {
    match err {
        AppError::Validation { code, message, .. } => EditError::validation(code, message),
        AppError::Conflict { code, message, .. } => EditError::conflict(code, message),
        AppError::Forbidden { code, message } => EditError::forbidden(code, message),
        AppError::NotFound { message, .. } => EditError::not_found(message),
        AppError::BadRequest { code, message, .. } => EditError::validation(code, message),
        AppError::RateLimited { .. } => EditError::conflict("RATE_LIMITED", "too many requests"),
        AppError::ServiceUnavailable { message, .. } => EditError::detail(503, message),
        AppError::Unauthorized { code, message } => EditError::Platform {
            status: 401,
            code,
            message,
            details: None,
        },
        AppError::Internal { internal_detail } => EditError::Internal(internal_detail),
    }
}

const fn algorithm_str(id: AlgorithmId) -> &'static str {
    match id {
        AlgorithmId::Buffer => "buffer",
        AlgorithmId::Clip => "clip",
        AlgorithmId::Dissolve => "dissolve",
        AlgorithmId::Difference => "difference",
        AlgorithmId::Intersection => "intersection",
    }
}

async fn create_job(
    State(state): State<AppState>,
    Path((organization, project)): Path<(String, String)>,
    request: Request,
) -> Result<(StatusCode, Json<Value>), Failure> {
    let (actor, headers, body) = authenticate_json(&state, request, &organization).await?;
    let scope = open(&state, &actor, &organization, &project, Guard::ScopedDenied).await?;
    require_csrf(&state, &actor, &headers)?;
    let body: CreateProcessingJobRequest = decode(body)?;
    let Some(mut scope) = scope else {
        return Err(EditError::InvalidRequest.into());
    };
    scope.authorize(Permission::Edit).await?;

    let clean_params = validate_algorithm_parameters(body.algorithm_id, &body.parameters)
        .map_err(app_to_edit_error)?;
    let submitted = processing_jobs::submit_job_pinned(
        &mut scope,
        algorithm_str(body.algorithm_id),
        body.input_layer_id,
        body.overlay_layer_id,
        clean_params,
        &body.output_name,
        &body.operation_id,
        body.expected_input.as_ref(),
        body.expected_overlay.as_ref(),
    )
    .await?;
    let job_id = submitted
        .wire
        .get("id")
        .and_then(Value::as_str)
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| EditError::Internal("missing submitted job id".into()))?;
    scope.commit().await?;

    if submitted.already_existed || !body.execute_now {
        return Ok((StatusCode::CREATED, Json(submitted.wire)));
    }

    let executed = execute_job_bounded(&state, &actor, &organization, &project, job_id).await?;
    Ok((StatusCode::CREATED, Json(executed)))
}

async fn list_jobs(
    State(state): State<AppState>,
    Path((organization, project)): Path<(String, String)>,
    uri: Uri,
    request: Request,
) -> Result<Json<Value>, Failure> {
    let (mut parts, _) = request.into_parts();
    let actor = authenticate_actor(&state, &mut parts, &organization).await?;
    let scope = open(&state, &actor, &organization, &project, Guard::Read).await?;
    let limit = query_limit(&uri)?;
    let Some(mut scope) = scope else {
        return Err(EditError::InvalidRequest.into());
    };
    scope.authorize(Permission::Read).await?;
    let result = processing_jobs::list_jobs_scoped(&mut scope, limit).await?;
    scope.commit().await?;
    Ok(Json(result))
}

async fn get_job(
    State(state): State<AppState>,
    Path((organization, project, job)): Path<(String, String, String)>,
    request: Request,
) -> Result<Json<Value>, Failure> {
    let (mut parts, _) = request.into_parts();
    let actor = authenticate_actor(&state, &mut parts, &organization).await?;
    let scope = open(&state, &actor, &organization, &project, Guard::Read).await?;
    let (Some(mut scope), Ok(job_id)) = (scope, Uuid::parse_str(&job)) else {
        return Err(EditError::InvalidRequest.into());
    };
    scope.authorize(Permission::Read).await?;
    let result = processing_jobs::get_job_scoped(&mut scope, job_id).await?;
    scope.commit().await?;
    Ok(Json(result))
}

async fn cancel_job(
    State(state): State<AppState>,
    Path((organization, project, job)): Path<(String, String, String)>,
    request: Request,
) -> Result<Json<Value>, Failure> {
    let (actor, headers, _) = authenticate_json(&state, request, &organization).await?;
    let scope = open(&state, &actor, &organization, &project, Guard::ScopedDenied).await?;
    require_csrf(&state, &actor, &headers)?;
    let (Some(mut scope), Ok(job_id)) = (scope, Uuid::parse_str(&job)) else {
        return Err(EditError::InvalidRequest.into());
    };
    scope.authorize(Permission::Edit).await?;
    let _ = state.jobs.cancel_job(JobId(job_id));
    let result = processing_jobs::cancel_job_scoped(&mut scope, job_id).await?;
    scope.commit().await?;
    Ok(Json(result))
}

async fn retry_job(
    State(state): State<AppState>,
    Path((organization, project, job)): Path<(String, String, String)>,
    request: Request,
) -> Result<Json<Value>, Failure> {
    let (actor, headers, _) = authenticate_json(&state, request, &organization).await?;
    let scope = open(&state, &actor, &organization, &project, Guard::ScopedDenied).await?;
    require_csrf(&state, &actor, &headers)?;
    let (Some(mut scope), Ok(job_id)) = (scope, Uuid::parse_str(&job)) else {
        return Err(EditError::InvalidRequest.into());
    };
    scope.authorize(Permission::Edit).await?;
    if let Some(succeeded) = processing_jobs::retry_job_prepare(&mut scope, job_id).await? {
        scope.commit().await?;
        return Ok(Json(succeeded));
    }
    scope.commit().await?;

    let executed = execute_job_bounded(&state, &actor, &organization, &project, job_id).await?;
    Ok(Json(executed))
}

async fn execute_job_bounded(
    state: &AppState,
    actor: &Actor,
    organization: &str,
    project: &str,
    job_id: Uuid,
) -> Result<Value, Failure> {
    let Some(mut scope) = open(state, actor, organization, project, Guard::ScopedDenied).await?
    else {
        return Err(EditError::InvalidRequest.into());
    };
    scope.authorize(Permission::Edit).await?;

    let inputs = match processing_jobs::lock_and_prepare_execution(&mut scope, job_id).await? {
        ExecutionLockState::Terminal(wire) => {
            scope.commit().await?;
            return Ok(wire);
        }
        ExecutionLockState::Ready(inputs) => inputs,
    };

    let cpu_slot = Arc::new(Mutex::new(None));
    let cpu_slot_worker = Arc::clone(&cpu_slot);
    let inputs_for_worker = inputs.clone();
    let submit_res = state.jobs.submit_with_id(
        JobId(job_id),
        inputs.organization_id,
        format!("processing.{}", inputs.algorithm_id),
        None,
        move |token| {
            let outcome = processing_jobs::validate_and_hash_cpu(&inputs_for_worker, &|| {
                token.is_cancelled()
            });
            let ok = outcome.is_ok();
            *cpu_slot_worker.lock().expect("cpu slot mutex poisoned") = Some(outcome);
            if ok {
                Ok(Value::Null)
            } else {
                Err("cpu validation failed".to_owned())
            }
        },
    );

    let handle = match submit_res {
        Ok((_, handle)) => handle,
        Err(_) => {
            let failed = processing_jobs::fail_job(
                &mut scope,
                job_id,
                "PROCESSING_BUSY",
                "processing executor queue is full",
            )
            .await?;
            scope.commit().await?;
            return Ok(failed);
        }
    };

    let wire = match handle.await {
        Ok(record) if record.status == JobStatus::Succeeded => {
            let cpu_out = cpu_slot
                .lock()
                .expect("cpu slot mutex poisoned")
                .take()
                .unwrap_or_else(|| {
                    Err(EditError::Internal(
                        "missing cpu output from succeeded worker".into(),
                    ))
                });
            match cpu_out {
                Ok(output) => {
                    processing_jobs::publish_execution(&mut scope, &inputs, &output).await?
                }
                Err(err) => {
                    let (code, msg) = processing_jobs::failure_code_message(&err);
                    processing_jobs::fail_job(&mut scope, job_id, &code, &msg).await?
                }
            }
        }
        Ok(record) => {
            let cpu_err = cpu_slot
                .lock()
                .expect("cpu slot mutex poisoned")
                .take()
                .and_then(Result::err);
            let (code, msg) = if let Some(err) = cpu_err {
                processing_jobs::failure_code_message(&err)
            } else if record.error.as_deref() == Some("job timed out") {
                ("PROCESSING_TIMEOUT".to_owned(), "job timed out".to_owned())
            } else {
                (
                    "PROCESSING_FAILED".to_owned(),
                    record
                        .error
                        .unwrap_or_else(|| "processing could not safely publish output".to_owned()),
                )
            };
            processing_jobs::fail_job(&mut scope, job_id, &code, &msg).await?
        }
        Err(_) => {
            processing_jobs::fail_job(
                &mut scope,
                job_id,
                "PROCESSING_FAILED",
                "worker terminated unexpectedly",
            )
            .await?
        }
    };

    scope.commit().await?;
    Ok(wire)
}

fn query_limit(uri: &Uri) -> Result<i64, Failure> {
    let Query(pairs) = axum::extract::Query::<Vec<(String, String)>>::try_from_uri(uri)
        .map_err(|_| EditError::InvalidRequest)?;
    match pairs.into_iter().rev().find(|(key, _)| key == "limit") {
        None => Ok(50),
        Some((_, value)) => value
            .parse::<i64>()
            .ok()
            .filter(|limit| (1..=200).contains(limit))
            .ok_or(Failure::Edit(EditError::InvalidRequest)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_matches_the_frozen_python_contract() {
        let body = algorithm_registry_wire();
        let items = body["items"].as_array().unwrap();
        assert_eq!(items.len(), 5);
        assert_eq!(items[0]["id"], "buffer");
        assert_eq!(items[0]["parameters_schema"]["quad_segs"]["default"], 8);
        assert_eq!(items[1]["required_inputs"][1], "overlay_layer_id");
        assert_eq!(
            items[2]["parameters_schema"]["group_by_field"]["nullable"],
            true
        );
    }
}
