//! Stable internal SDK surface for isolated official vertical modules.
//!
//! Constitutional rule (I): `ovg-module-sdk` and all core crates MUST have zero
//! compile-time dependencies on any vertical module (`house_numbering`, `land_asset`, etc.).

use std::collections::HashMap;
use std::sync::Arc;

use ovg_core::{AppError, TenantScope};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ModuleCapability(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleCommandSpec {
    pub id: String,
    pub label: String,
    pub required_permission: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleManifest {
    pub extension_key: String,
    pub version: String,
    pub api_compatibility: String,
    pub display_name: String,
    pub description: String,
    pub permissions: Vec<String>,
    pub commands: Vec<ModuleCommandSpec>,
    pub capabilities: Vec<ModuleCapability>,
}

impl ModuleManifest {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.extension_key.is_empty()
            || !self
                .extension_key
                .bytes()
                .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-'))
        {
            return Err(AppError::validation(
                "INVALID_MODULE_MANIFEST",
                "extension_key must be non-empty lowercase ASCII [a-z0-9_-]",
            ));
        }
        if self.version.trim().is_empty() || self.display_name.trim().is_empty() {
            return Err(AppError::validation(
                "INVALID_MODULE_MANIFEST",
                "module version and display_name are required",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct ModuleExecutionContext {
    pub tenant: TenantScope,
    pub granted_extension_permissions: Vec<String>,
}

impl ModuleExecutionContext {
    #[must_use]
    pub fn has_extension_permission(&self, perm: &str) -> bool {
        self.granted_extension_permissions.iter().any(|p| p == perm)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleCommandRequest {
    pub command_id: String,
    pub payload: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleCommandResult {
    pub command_id: String,
    pub output: Value,
}

/// Contract implemented by every isolated vertical module.
pub trait OvgModule: Send + Sync {
    fn manifest(&self) -> ModuleManifest;

    fn execute_command(
        &self,
        ctx: &ModuleExecutionContext,
        request: &ModuleCommandRequest,
    ) -> Result<ModuleCommandResult, AppError>;
}

/// Thread-safe registry of official vertical modules loaded at application startup.
#[derive(Default, Clone)]
pub struct ModuleRegistry {
    modules: HashMap<String, Arc<dyn OvgModule>>,
}

impl ModuleRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, module: Arc<dyn OvgModule>) -> Result<(), AppError> {
        let manifest = module.manifest();
        manifest.validate()?;
        if self.modules.contains_key(&manifest.extension_key) {
            return Err(AppError::conflict(
                "MODULE_ALREADY_REGISTERED",
                format!("module '{}' is already registered", manifest.extension_key),
            ));
        }
        self.modules.insert(manifest.extension_key, module);
        Ok(())
    }

    #[must_use]
    pub fn get(&self, extension_key: &str) -> Option<Arc<dyn OvgModule>> {
        self.modules.get(extension_key).cloned()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.modules.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.modules.is_empty()
    }

    #[must_use]
    pub fn list_manifests(&self) -> Vec<ModuleManifest> {
        let mut manifests: Vec<ModuleManifest> =
            self.modules.values().map(|m| m.manifest()).collect();
        manifests.sort_by(|a, b| a.extension_key.cmp(&b.extension_key));
        manifests
    }

    /// Dispatch a command after validating tenant scope and command-level permission.
    pub fn dispatch(
        &self,
        extension_key: &str,
        ctx: &ModuleExecutionContext,
        request: &ModuleCommandRequest,
    ) -> Result<ModuleCommandResult, AppError> {
        ctx.tenant.validate()?;
        let module = self
            .get(extension_key)
            .ok_or_else(|| AppError::not_found("module not found"))?;
        let manifest = module.manifest();
        let spec = manifest
            .commands
            .iter()
            .find(|c| c.id == request.command_id)
            .ok_or_else(|| AppError::not_found("module command not found"))?;

        if !ctx.has_extension_permission(&spec.required_permission) {
            return Err(AppError::forbidden(format!(
                "missing extension permission: {}",
                spec.required_permission
            )));
        }

        module.execute_command(ctx, request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ovg_core::{ActorContext, ActorType, Permission};
    use serde_json::json;
    use uuid::Uuid;

    struct DummyModule;

    impl OvgModule for DummyModule {
        fn manifest(&self) -> ModuleManifest {
            ModuleManifest {
                extension_key: "dummy_mod".to_string(),
                version: "1.0.0".to_string(),
                api_compatibility: "1.x".to_string(),
                display_name: "Dummy".to_string(),
                description: "Test module".to_string(),
                permissions: vec!["ext.dummy.read".to_string()],
                commands: vec![ModuleCommandSpec {
                    id: "ping".to_string(),
                    label: "Ping".to_string(),
                    required_permission: "ext.dummy.read".to_string(),
                }],
                capabilities: vec![ModuleCapability("ping".to_string())],
            }
        }

        fn execute_command(
            &self,
            _ctx: &ModuleExecutionContext,
            request: &ModuleCommandRequest,
        ) -> Result<ModuleCommandResult, AppError> {
            Ok(ModuleCommandResult {
                command_id: request.command_id.clone(),
                output: json!({ "pong": true }),
            })
        }
    }

    #[test]
    fn enforces_manifest_uniqueness_and_command_permissions() {
        let mut reg = ModuleRegistry::new();
        reg.register(Arc::new(DummyModule)).unwrap();
        assert_eq!(
            reg.register(Arc::new(DummyModule))
                .unwrap_err()
                .status_code(),
            409
        );

        let scope = TenantScope {
            organization_id: Uuid::new_v4(),
            workspace_id: None,
            project_id: Some(Uuid::new_v4()),
            actor: ActorContext {
                actor_type: ActorType::Human,
                actor_id: Uuid::new_v4(),
                credential_id: None,
                email: None,
            },
            permissions: vec![Permission::ProjectRead],
        };

        let req = ModuleCommandRequest {
            command_id: "ping".to_string(),
            payload: json!({}),
        };

        // Missing extension permission -> 403
        let denied_ctx = ModuleExecutionContext {
            tenant: scope.clone(),
            granted_extension_permissions: vec![],
        };
        assert_eq!(
            reg.dispatch("dummy_mod", &denied_ctx, &req)
                .unwrap_err()
                .status_code(),
            403
        );

        // With extension permission -> succeeds
        let ok_ctx = ModuleExecutionContext {
            tenant: scope,
            granted_extension_permissions: vec!["ext.dummy.read".to_string()],
        };
        let res = reg.dispatch("dummy_mod", &ok_ctx, &req).unwrap();
        assert_eq!(res.output, json!({ "pong": true }));
    }
}
