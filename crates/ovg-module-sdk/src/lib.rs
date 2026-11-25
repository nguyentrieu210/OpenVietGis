//! Stable internal SDK surface for isolated official vertical modules.
//!
//! Constitutional rule (I): `ovg-module-sdk` and all core crates MUST have zero
//! compile-time dependencies on any vertical module (`house_numbering`, `land_asset`, etc.).

use std::collections::{HashMap, HashSet};
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

// The internal native SDK supports only trusted, compiled-in official modules.
// Partner manifests and all persistent activation checks remain in the legacy API.
fn identifier(s: &str) -> bool {
    let mut bytes = s.bytes();
    s.len() <= 64
        && bytes.next().is_some_and(|b| b.is_ascii_alphabetic())
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

fn unique_bounded<'a>(values: impl Iterator<Item = &'a str>) -> bool {
    let mut seen = HashSet::new();
    values
        .into_iter()
        .all(|s| seen.len() < 64 && seen.insert(s))
}

impl ModuleManifest {
    pub fn validate(&self) -> Result<(), AppError> {
        let invalid = |message| AppError::validation("INVALID_MODULE_MANIFEST", message);
        let key = &self.extension_key;
        if !(2..=100).contains(&key.len())
            || !key.as_bytes()[0].is_ascii_lowercase()
            || !key
                .bytes()
                .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-'))
            || key.starts_with("partner_")
        {
            return Err(invalid("invalid official module extension_key"));
        }
        let parts: Vec<_> = self.version.split('.').collect();
        if self.version.len() > 32
            || parts.len() != 3
            || parts.iter().any(|p| {
                p.is_empty()
                    || !p.bytes().all(|b| b.is_ascii_digit())
                    || (p.len() > 1 && p.starts_with('0'))
            })
        {
            return Err(AppError::validation(
                "INVALID_VERSION",
                "version must be semver X.Y.Z",
            ));
        }
        if self.api_compatibility != "1.x" {
            return Err(AppError::validation(
                "INCOMPATIBLE_EXTENSION_API",
                "expected API compatibility 1.x",
            ));
        }
        if self.display_name.trim().is_empty() {
            return Err(invalid("display_name is required"));
        }
        let prefix = format!("ext.{key}.");
        if !unique_bounded(self.permissions.iter().map(String::as_str))
            || self
                .permissions
                .iter()
                .any(|p| !p.strip_prefix(&prefix).is_some_and(identifier))
        {
            return Err(AppError::validation(
                "INVALID_PERMISSION_NAMESPACE",
                "permissions must be unique and module-namespaced",
            ));
        }
        if !unique_bounded(self.commands.iter().map(|c| c.id.as_str()))
            || self.commands.iter().any(|c| {
                !identifier(&c.id)
                    || c.label.trim().is_empty()
                    || c.label.chars().count() > 200
                    || !self.permissions.contains(&c.required_permission)
            })
        {
            return Err(invalid(
                "invalid, duplicate, or undeclared command permission",
            ));
        }
        if !unique_bounded(self.capabilities.iter().map(|c| c.0.as_str()))
            || self.capabilities.iter().any(|c| !identifier(&c.0))
        {
            return Err(invalid("invalid or duplicate capability"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivationStatus {
    Active,
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActivationPin {
    pub manifest_id: uuid::Uuid,
    pub revision: i64,
    pub status: ActivationStatus,
}

impl ActivationPin {
    /// Validate a version transition using the same compare-and-set boundary as
    /// the retained Python repository. Persistence must still lock and update
    /// the canonical activation row atomically.
    pub fn transition(
        self,
        expected_manifest_id: uuid::Uuid,
        target_manifest_id: uuid::Uuid,
    ) -> Result<Self, AppError> {
        if self.manifest_id != expected_manifest_id {
            return Err(AppError::conflict(
                "STALE_EXTENSION_VERSION",
                "activation version changed",
            ));
        }
        if target_manifest_id.is_nil() {
            return Err(AppError::validation(
                "INVALID_MODULE_MANIFEST",
                "target manifest id must not be nil",
            ));
        }
        Ok(Self {
            manifest_id: target_manifest_id,
            revision: self.revision,
            status: self.status,
        })
    }

    #[must_use]
    pub fn disable(self) -> Self {
        if self.status == ActivationStatus::Disabled {
            self
        } else {
            Self {
                status: ActivationStatus::Disabled,
                revision: self.revision.saturating_add(1),
                ..self
            }
        }
    }

    #[must_use]
    pub fn reactivate(self) -> Self {
        if self.status == ActivationStatus::Active {
            self
        } else {
            Self {
                status: ActivationStatus::Active,
                revision: self.revision.saturating_add(1),
                ..self
            }
        }
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

#[derive(Clone)]
struct RegisteredModule {
    manifest: ModuleManifest,
    runtime: Arc<dyn OvgModule>,
}

/// Thread-safe registry of official vertical modules loaded at application startup.
#[derive(Default, Clone)]
pub struct ModuleRegistry {
    modules: HashMap<String, RegisteredModule>,
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
        self.modules.insert(
            manifest.extension_key.clone(),
            RegisteredModule {
                manifest,
                runtime: module,
            },
        );
        Ok(())
    }

    #[must_use]
    pub fn get(&self, extension_key: &str) -> Option<Arc<dyn OvgModule>> {
        self.modules
            .get(extension_key)
            .map(|entry| Arc::clone(&entry.runtime))
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
            self.modules.values().map(|m| m.manifest.clone()).collect();
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
        // Never expose dispatch as an HTTP endpoint without verified activation and
        // grants from the E01/project persistence boundary. This is internal only.
        if ctx.tenant.project_id.is_none() {
            return Err(AppError::forbidden("module commands require project scope"));
        }
        let module = self
            .modules
            .get(extension_key)
            .ok_or_else(|| AppError::not_found("module not found"))?;
        let manifest = &module.manifest;
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

        let result = module.runtime.execute_command(ctx, request)?;
        if result.command_id != request.command_id {
            return Err(AppError::Internal {
                internal_detail: "module returned mismatched command ID".into(),
            });
        }
        Ok(result)
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
                permissions: vec!["ext.dummy_mod.read".to_string()],
                commands: vec![ModuleCommandSpec {
                    id: "ping".to_string(),
                    label: "Ping".to_string(),
                    required_permission: "ext.dummy_mod.read".to_string(),
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
    fn shared_python_manifest_contract_fixtures() {
        let cases: Value = serde_json::from_str(include_str!(
            "../../../fixtures/rust-platform/e14/manifest-contract.json"
        ))
        .unwrap();
        for case in cases.as_array().unwrap() {
            let manifest: ModuleManifest =
                serde_json::from_value(case["manifest"].clone()).unwrap();
            assert_eq!(
                manifest.validate().is_ok(),
                case["valid"].as_bool().unwrap(),
                "{}",
                case["id"]
            );
        }
    }

    #[test]
    fn rejects_invalid_native_manifest_contracts() {
        let good = DummyModule.manifest();
        assert!(good.validate().is_ok());
        for key in ["a", "1test", "partner_demo", "Foo", "bad key"] {
            let mut m = good.clone();
            m.extension_key = key.into();
            assert!(m.validate().is_err(), "{key}");
        }
        for version in ["1", "01.2.3", "1.2.3-beta", "1.2.3.4"] {
            let mut m = good.clone();
            m.version = version.into();
            assert!(m.validate().is_err(), "{version}");
        }
        let mut m = good.clone();
        m.api_compatibility = "2.x".into();
        assert!(m.validate().is_err());
        let mut m = good.clone();
        m.permissions.push("ext.other.read".into());
        assert!(m.validate().is_err());
        let mut m = good.clone();
        m.commands[0].required_permission = "ext.dummy_mod.write".into();
        assert!(m.validate().is_err());
        let mut m = good.clone();
        m.commands.push(m.commands[0].clone());
        assert!(m.validate().is_err());
        let mut m = good;
        m.capabilities.push(m.capabilities[0].clone());
        assert!(m.validate().is_err());
    }

    #[test]
    fn activation_pin_enforces_version_cas_and_idempotent_status() {
        let current = uuid::Uuid::new_v4();
        let target = uuid::Uuid::new_v4();
        let pin = ActivationPin {
            manifest_id: current,
            revision: 7,
            status: ActivationStatus::Active,
        };
        let moved = pin.transition(current, target).unwrap();
        assert_eq!(moved.manifest_id, target);
        assert_eq!(moved.revision, 7);
        assert_eq!(
            pin.transition(uuid::Uuid::new_v4(), target)
                .unwrap_err()
                .code(),
            "STALE_EXTENSION_VERSION"
        );
        assert_eq!(moved.disable().revision, 8);
        assert_eq!(moved.disable().disable().revision, 8);
        assert_eq!(moved.disable().reactivate().revision, 9);
    }

    struct ChangingManifest(std::sync::atomic::AtomicUsize);
    impl OvgModule for ChangingManifest {
        fn manifest(&self) -> ModuleManifest {
            let mut m = DummyModule.manifest();
            if self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst) > 0 {
                m.commands[0].required_permission = "ext.other.read".into();
            }
            m
        }
        fn execute_command(
            &self,
            ctx: &ModuleExecutionContext,
            request: &ModuleCommandRequest,
        ) -> Result<ModuleCommandResult, AppError> {
            DummyModule.execute_command(ctx, request)
        }
    }

    #[test]
    fn pins_manifest_at_registration() {
        let mut registry = ModuleRegistry::new();
        registry
            .register(Arc::new(ChangingManifest(0.into())))
            .unwrap();
        assert_eq!(registry.list_manifests(), vec![DummyModule.manifest()]);
        assert_eq!(
            registry.clone().list_manifests(),
            vec![DummyModule.manifest()]
        );
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
            granted_extension_permissions: vec!["ext.dummy_mod.read".to_string()],
        };
        let mut no_project = ok_ctx.clone();
        no_project.tenant.project_id = None;
        assert_eq!(
            reg.dispatch("dummy_mod", &no_project, &req)
                .unwrap_err()
                .status_code(),
            403
        );
        let res = reg.dispatch("dummy_mod", &ok_ctx, &req).unwrap();
        assert_eq!(res.output, json!({ "pong": true }));
    }
}
