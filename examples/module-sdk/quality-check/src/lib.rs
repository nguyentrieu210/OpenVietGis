//! Independent, opt-in native SDK reference module. No HN, HTTP app or SQL dependencies.
use ovg_core::{AppError,Permission};
use ovg_module_sdk::{ModuleManifest,ModuleLayerRole,ModuleExecutionContext,ModuleCommandRequest,ModuleCommandResult,OvgModule,PureModuleCommand};
use serde::Deserialize;
use serde_json::json;

pub struct QualityCheck;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input { total:u32, complete:u32 }

impl OvgModule for QualityCheck {
    fn manifest(&self)->ModuleManifest{
        serde_json::from_str(include_str!("../manifest.json")).expect("checked example manifest")
    }
    fn pure_commands(&self)->&'static [PureModuleCommand]{
        &[PureModuleCommand{id:"completion_rate",core_permission:Permission::ProjectRead}]
    }
    fn layer_roles(&self)->&'static [ModuleLayerRole]{
        &[ModuleLayerRole{id:"survey",geometry_families:&["Point"],required_fields:&["code"]}]
    }
    fn execute_command(&self,ctx:&ModuleExecutionContext,request:&ModuleCommandRequest)->Result<ModuleCommandResult,AppError>{
        if request.command_id!="completion_rate"{return Err(AppError::not_found("unknown quality command"));}
        if !ctx.has_extension_permission("ext.quality_check.read")||!ctx.tenant.has_permission(Permission::ProjectRead){
            return Err(AppError::forbidden("quality read permission required"));
        }
        let input:Input=serde_json::from_value(request.payload.clone()).map_err(|_|AppError::validation("INVALID_COUNTS","integer total and complete required"))?;
        if input.complete>input.total{return Err(AppError::validation("INVALID_COUNTS","complete exceeds total"));}
        let percent=if input.total==0{None}else{Some(100.0*f64::from(input.complete)/f64::from(input.total))};
        Ok(ModuleCommandResult{command_id:request.command_id.clone(),output:json!({"total":input.total,"complete":input.complete,"percent":percent})})
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use ovg_module_sdk::ModuleRegistry;
    use std::sync::Arc;
    #[test]
    fn independent_manifest_registration(){
        let mut registry=ModuleRegistry::new();
        registry.register(Arc::new(QualityCheck)).unwrap();
        assert!(registry.pure_command("quality_check","completion_rate").is_some());
        assert_eq!(registry.layer_roles("quality_check").unwrap()[0].id,"survey");
    }
    #[test]
    fn calculation_uses_verified_permissions_and_handles_zero(){
        use ovg_core::{ActorContext,ActorType,TenantScope};
        let mut registry=ModuleRegistry::new();
        registry.register(Arc::new(QualityCheck)).unwrap();
        let mut ctx=ModuleExecutionContext{tenant:TenantScope{organization_id:uuid::Uuid::new_v4(),workspace_id:None,project_id:Some(uuid::Uuid::new_v4()),actor:ActorContext{actor_type:ActorType::Human,actor_id:uuid::Uuid::new_v4(),credential_id:None,email:None},permissions:vec![Permission::ProjectRead]},granted_extension_permissions:vec!["ext.quality_check.read".into()]};
        let request=|total,complete|ModuleCommandRequest{command_id:"completion_rate".into(),payload:json!({"total":total,"complete":complete})};
        assert_eq!(registry.dispatch_pure("quality_check",&ctx,&request(100,75)).unwrap().output["percent"],75.0);
        assert!(registry.dispatch_pure("quality_check",&ctx,&request(0,0)).unwrap().output["percent"].is_null());
        assert_eq!(registry.dispatch_pure("quality_check",&ctx,&request(3,4)).unwrap_err().status_code(),422);
        ctx.granted_extension_permissions.clear();
        assert_eq!(registry.dispatch_pure("quality_check",&ctx,&request(100,75)).unwrap_err().status_code(),403);
    }

}
