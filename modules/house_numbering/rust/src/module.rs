//! `HouseNumberingModule` implementing `ovg_module_sdk::OvgModule`.

use ovg_core::AppError;
use ovg_module_sdk::{
    ModuleCommandRequest, ModuleCommandResult, ModuleExecutionContext, ModuleLayerRole, ModuleManifest, OvgModule,
};
use serde_json::json;

use crate::numbering::{generate_street_numbers, NumberingRule};

#[derive(Debug, Default, Clone, Copy)]
pub struct HouseNumberingModule;

impl OvgModule for HouseNumberingModule {
    fn manifest(&self) -> ModuleManifest {
        serde_json::from_str(include_str!("../../manifest.json"))
            .expect("shipped house-numbering manifest must satisfy Rust SDK schema")
    }

    fn layer_roles(&self) -> &'static [ModuleLayerRole] {
        &[
            ModuleLayerRole { id: "survey_points", geometry_families: &["Point"], required_fields: &["ap", "ten_duong"] },
            ModuleLayerRole { id: "commune_boundary", geometry_families: &["Polygon"], required_fields: &[] },
            ModuleLayerRole { id: "hamlet_boundary", geometry_families: &["Polygon"], required_fields: &[] },
            ModuleLayerRole { id: "boundary", geometry_families: &["Polygon"], required_fields: &[] },
            ModuleLayerRole { id: "parcel", geometry_families: &["Polygon"], required_fields: &[] },
            ModuleLayerRole { id: "building", geometry_families: &["Point", "Polygon"], required_fields: &[] },
            ModuleLayerRole { id: "road", geometry_families: &["LineString"], required_fields: &[] },
        ]
    }

    fn execute_command(
        &self,
        _ctx: &ModuleExecutionContext,
        request: &ModuleCommandRequest,
    ) -> Result<ModuleCommandResult, AppError> {
        match request.command_id.as_str() {
            "preview_numbering" => {
                let rule: NumberingRule =
                    serde_json::from_value(request.payload.clone()).map_err(|_| {
                        AppError::validation(
                            "INVALID_NUMBERING_PAYLOAD",
                            "Invalid preview_numbering request payload.",
                        )
                    })?;
                let proposals = generate_street_numbers(&rule)?;
                Ok(ModuleCommandResult {
                    command_id: request.command_id.clone(),
                    output: json!({
                        "count": proposals.len(),
                        "proposals": proposals,
                    }),
                })
            }
            other => Err(AppError::not_found(format!(
                "command '{other}' is not implemented in this slice"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use ovg_core::{ActorContext, ActorType, Permission, TenantScope};
    use ovg_module_sdk::ModuleRegistry;
    use uuid::Uuid;

    #[test]
    fn rust_manifest_is_exactly_the_shipped_official_manifest() {
        let rust_manifest = serde_json::to_value(HouseNumberingModule.manifest()).unwrap();
        let shipped: serde_json::Value =
            serde_json::from_str(include_str!("../../manifest.json")).unwrap();
        assert_eq!(rust_manifest, shipped);
        HouseNumberingModule.manifest().validate().unwrap();
    }

    #[test]
    fn registers_with_sdk_and_executes_preview_numbering() {
        let mut registry = ModuleRegistry::new();
        registry.register(Arc::new(HouseNumberingModule)).unwrap();

        let manifests = registry.list_manifests();
        assert_eq!(manifests.len(), 1);
        assert_eq!(manifests[0].extension_key, "house_numbering");

        let ctx = ModuleExecutionContext {
            tenant: TenantScope {
                organization_id: Uuid::new_v4(),
                workspace_id: None,
                project_id: Some(Uuid::new_v4()),
                actor: ActorContext {
                    actor_type: ActorType::Human,
                    actor_id: Uuid::new_v4(),
                    credential_id: None,
                    email: None,
                },
                permissions: vec![Permission::ProjectEdit],
            },
            granted_extension_permissions: vec!["ext.house_numbering.assign".to_string()],
        };

        let res = registry
            .dispatch(
                "house_numbering",
                &ctx,
                &ModuleCommandRequest {
                    command_id: "preview_numbering".to_string(),
                    payload: json!({
                        "street_name": "Lê Lợi",
                        "alley_prefix": "8B",
                        "side": "right_even",
                        "start_number": 2,
                        "step": 2,
                        "count": 2
                    }),
                },
            )
            .unwrap();
        assert_eq!(res.output["count"], 2);
        assert_eq!(res.output["proposals"][0]["house_number"], "8B/2");
        assert_eq!(res.output["proposals"][1]["house_number"], "8B/4");
    }
}
