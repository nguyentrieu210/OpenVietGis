//! `HouseNumberingModule` implementing `ovg_module_sdk::OvgModule`.

use ovg_core::AppError;
use ovg_module_sdk::{
    ModuleCapability, ModuleCommandRequest, ModuleCommandResult, ModuleCommandSpec,
    ModuleExecutionContext, ModuleManifest, OvgModule,
};
use serde_json::json;

use crate::numbering::{generate_street_numbers, NumberingRule};

#[derive(Debug, Default, Clone, Copy)]
pub struct HouseNumberingModule;

impl OvgModule for HouseNumberingModule {
    fn manifest(&self) -> ModuleManifest {
        ModuleManifest {
            extension_key: "house_numbering".to_string(),
            version: "2.0.0".to_string(),
            api_compatibility: "1.x".to_string(),
            display_name: "Quản lý và đánh số nhà".to_string(),
            description: "Official project-scoped house numbering workflow: registry, inspection, numbering, approval, reconciliation, field work and handover.".to_string(),
            permissions: vec![
                "ext.house_numbering.read".to_string(),
                "ext.house_numbering.edit".to_string(),
                "ext.house_numbering.assign".to_string(),
                "ext.house_numbering.approve".to_string(),
                "ext.house_numbering.survey".to_string(),
                "ext.house_numbering.private_read".to_string(),
                "ext.house_numbering.export_sensitive".to_string(),
                "ext.house_numbering.manage".to_string(),
            ],
            commands: vec![
                ModuleCommandSpec {
                    id: "inspect_location".to_string(),
                    label: "Kiểm tra vị trí".to_string(),
                    required_permission: "ext.house_numbering.read".to_string(),
                },
                ModuleCommandSpec {
                    id: "preview_numbering".to_string(),
                    label: "Xem trước đánh số".to_string(),
                    required_permission: "ext.house_numbering.assign".to_string(),
                },
                ModuleCommandSpec {
                    id: "apply_numbering".to_string(),
                    label: "Áp dụng số đề xuất".to_string(),
                    required_permission: "ext.house_numbering.assign".to_string(),
                },
                ModuleCommandSpec {
                    id: "submit_proposal".to_string(),
                    label: "Trình phương án".to_string(),
                    required_permission: "ext.house_numbering.edit".to_string(),
                },
                ModuleCommandSpec {
                    id: "approve_proposal".to_string(),
                    label: "Duyệt phương án".to_string(),
                    required_permission: "ext.house_numbering.approve".to_string(),
                },
            ],
            capabilities: [
                "area_tree",
                "spatial_house_registry",
                "source_import",
                "location_inspection",
                "numbering_rules",
                "bulk_numbering",
                "source_reconciliation",
                "approval_workflow",
                "field_survey",
                "progress_tracking",
                "inspection_documents",
                "search_and_handover",
                "protected_household_data",
            ]
            .into_iter()
            .map(|c| ModuleCapability(c.to_string()))
            .collect(),
        }
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
