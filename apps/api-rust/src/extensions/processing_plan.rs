//! Typed proposal for canonical processing, not a second geometry engine.
use ovg_core::AppError;
use ovg_processing::registry::{validate_algorithm_parameters, AlgorithmId};
use ovg_storage::{
    catalog::ProjectRead,
    editing::EditError,
    processing_jobs::{self, ExpectedSource},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use uuid::Uuid;
fn invalid() -> AppError {
    AppError::validation("INVALID_PLUGIN_PROCESSING", "invalid processing request")
}
pub(super) fn edit_error(e: EditError) -> AppError {
    match e {
        EditError::Platform {
            status: 404,
            message,
            ..
        } => AppError::not_found(message),
        EditError::Platform {
            status: 403,
            message,
            ..
        } => AppError::forbidden(message),
        EditError::Platform {
            status: 409,
            message,
            ..
        } => AppError::conflict("PLUGIN_PROCESSING_CONFLICT", message),
        EditError::Platform {
            status: 422,
            message,
            ..
        } => AppError::validation("PLUGIN_PROCESSING_INVALID", message),
        _ => AppError::internal("canonical processing operation failed"),
    }
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Request {
    pub algorithm_id: AlgorithmId,
    pub input_layer_id: Uuid,
    #[serde(default)]
    pub overlay_layer_id: Option<Uuid>,
    #[serde(default)]
    pub parameters: Map<String, Value>,
    pub output_name: String,
}
impl Request {
    fn normalize(mut self) -> Result<Self, AppError> {
        self.output_name = self.output_name.trim().to_owned();
        let overlay = matches!(
            self.algorithm_id,
            AlgorithmId::Clip
                | AlgorithmId::Difference
                | AlgorithmId::Intersection
                | AlgorithmId::Union
                | AlgorithmId::SpatialJoin
        );
        if self.input_layer_id.is_nil()
            || self
                .overlay_layer_id
                .is_some_and(|id| id.is_nil() || id == self.input_layer_id)
            || overlay != self.overlay_layer_id.is_some()
            || self.output_name.is_empty()
            || self.output_name.chars().count() > 200
        {
            return Err(invalid());
        }
        self.parameters = validate_algorithm_parameters(self.algorithm_id, &self.parameters)?
            .as_object()
            .cloned()
            .ok_or_else(invalid)?;
        Ok(self)
    }
}
pub(super) async fn preview(read: &mut ProjectRead<'_>, raw: &str) -> Result<Value, AppError> {
    if raw.len() > 16384 {
        return Err(invalid());
    }
    let req: Request = serde_json::from_str(raw).map_err(|_| invalid())?;
    let req = req.normalize()?;
    let input = processing_jobs::plugin_source_pin(read, req.input_layer_id)
        .await
        .map_err(edit_error)?;
    let overlay = match req.overlay_layer_id {
        Some(id) => Some(
            processing_jobs::plugin_source_pin(read, id)
                .await
                .map_err(edit_error)?,
        ),
        None => None,
    };
    if let Some(o) = &overlay {
        if input["source_id"] == o["source_id"]
            || (req.algorithm_id != AlgorithmId::SpatialJoin && o["geometry_family"] != "Polygon")
            || (req.algorithm_id == AlgorithmId::Union && input["geometry_family"] != "Polygon")
        {
            return Err(invalid());
        }
    }
    Ok(
        json!({"request":req,"input":input,"overlay":overlay,"effect":"create_new_layer_via_core_job","execution":"queued_only"}),
    )
}
fn expected(value: &Value) -> Result<ExpectedSource, AppError> {
    serde_json::from_value(json!({"source_id":value["source_id"],"content_revision":value["content_revision"],"schema_revision":value["schema_revision"]})).map_err(|_|invalid())
}
pub(super) async fn queue(
    read: &mut ProjectRead<'_>,
    id: Uuid,
    plan: &Value,
) -> Result<Value, AppError> {
    let req: Request = serde_json::from_value(plan["request"].clone()).map_err(|_| invalid())?;
    let req = req.normalize()?;
    let input = expected(&plan["input"])?;
    let overlay = if plan["overlay"].is_null() {
        None
    } else {
        Some(expected(&plan["overlay"])?)
    };
    // Recheck the exact data pins and preview resource cap under the manager lock.
    let fresh = preview(read, &serde_json::to_string(&req).map_err(|_| invalid())?).await?;
    if fresh != *plan {
        return Err(AppError::conflict(
            "STALE_PLUGIN_INPUT",
            "source changed after preview",
        ));
    }
    processing_jobs::plugin_queue_capacity(read)
        .await
        .map_err(edit_error)?;
    let op = format!("plugin-v2:{id}");
    let result = processing_jobs::submit_plugin_job(
        read,
        crate::processing::algorithm_str(req.algorithm_id),
        req.input_layer_id,
        req.overlay_layer_id,
        Value::Object(req.parameters),
        &req.output_name,
        &op,
        &input,
        overlay.as_ref(),
    )
    .await
    .map_err(edit_error)?;
    if result.already_existed {
        return Err(AppError::conflict(
            "PLUGIN_PROCESSING_CONFLICT",
            "reserved operation already exists",
        ));
    }
    Ok(result.wire)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_all_eight_algorithms_and_rejects_unknowns() {
        for (algo, params, overlay) in [
            ("buffer", json!({"distance_meters":10}), false),
            ("clip", json!({}), true),
            ("dissolve", json!({}), false),
            ("difference", json!({}), true),
            ("intersection", json!({}), true),
            ("union", json!({}), true),
            ("reproject", json!({"target_srid":3857}), false),
            ("spatial_join", json!({}), true),
        ] {
            let raw = json!({"algorithm_id":algo,"input_layer_id":Uuid::new_v4(),"overlay_layer_id":overlay.then(Uuid::new_v4),"parameters":params,"output_name":"Result"});
            let req: Request = serde_json::from_value(raw).unwrap();
            assert!(req.normalize().is_ok(), "{algo}");
        }
        let req: Request = serde_json::from_value(
            json!({"algorithm_id":"clip","input_layer_id":Uuid::new_v4(),"output_name":"Result"}),
        )
        .unwrap();
        assert!(req.normalize().is_err());
        assert!(serde_json::from_value::<Request>(
            json!({"algorithm_id":"sql","input_layer_id":Uuid::new_v4(),"output_name":"Result"})
        )
        .is_err());
    }
}
