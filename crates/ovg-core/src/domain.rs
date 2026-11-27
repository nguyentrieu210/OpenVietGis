//! Typed domain models mirroring OpenVietGIS multi-tenant GIS hierarchy.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::crs::CrsPolicy;
use crate::error::AppError;

/// Monotonic revision identifier for optimistic concurrency control.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct RevisionId(pub i64);

/// Stable feature identifier within a layer.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FeatureId(pub String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrganizationStatus {
    Active,
    Suspended,
    Deleted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Organization {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub status: OrganizationStatus,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceStatus {
    Active,
    Archived,
    Deleted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workspace {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub name: String,
    pub status: WorkspaceStatus,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectStatus {
    Active,
    Archived,
    Deleted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub workspace_id: Uuid,
    pub name: String,
    pub status: ProjectStatus,
    pub default_srid: u32,
    pub revision: RevisionId,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LayerKind {
    Vector,
    Raster,
    Cad,
    ExternalTile,
    Terrain,
}

/// Explicit per-layer capability flags (Constitution VI).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayerCapabilities {
    pub readable: bool,
    pub editable: bool,
    pub queryable: bool,
    pub tileable: bool,
    pub exportable: bool,
    pub supports_revisions: bool,
}

impl LayerCapabilities {
    #[must_use]
    pub const fn read_only_vector() -> Self {
        Self {
            readable: true,
            editable: false,
            queryable: true,
            tileable: true,
            exportable: true,
            supports_revisions: true,
        }
    }

    #[must_use]
    pub const fn editable_vector() -> Self {
        Self {
            readable: true,
            editable: true,
            queryable: true,
            tileable: true,
            exportable: true,
            supports_revisions: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Layer {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub kind: LayerKind,
    pub crs_policy: CrsPolicy,
    pub capabilities: LayerCapabilities,
    pub revision: RevisionId,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Feature {
    pub id: FeatureId,
    pub layer_id: Uuid,
    pub srid: u32,
    pub geometry_geojson: Value,
    pub properties: Value,
    pub revision: RevisionId,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Revision {
    pub id: RevisionId,
    pub organization_id: Uuid,
    pub project_id: Uuid,
    pub layer_id: Option<Uuid>,
    pub actor_id: Uuid,
    pub summary: String,
    pub committed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActorType {
    Human,
    ServicePrincipal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    ProjectRead,
    ProjectEdit,
    ProjectManage,
    OrgManage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActorContext {
    pub actor_type: ActorType,
    pub actor_id: Uuid,
    pub credential_id: Option<Uuid>,
    pub email: Option<String>,
}

/// Fail-closed multi-tenant scope bound to every database transaction and service operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TenantScope {
    pub organization_id: Uuid,
    pub workspace_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub actor: ActorContext,
    pub permissions: Vec<Permission>,
}

impl TenantScope {
    /// Validate that the scope contains non-nil tenant and actor identities.
    pub fn validate(&self) -> Result<(), AppError> {
        if self.organization_id.is_nil() {
            return Err(AppError::forbidden("tenant organization scope is required"));
        }
        if self.actor.actor_id.is_nil() {
            return Err(AppError::unauthorized(
                "authenticated actor identity is required",
            ));
        }
        if self.actor.actor_type == ActorType::ServicePrincipal
            && self.actor.credential_id.is_none_or(|id| id.is_nil())
        {
            return Err(AppError::unauthorized(
                "service principal requires a verified credential id",
            ));
        }
        if let Some(pid) = self.project_id {
            if pid.is_nil() {
                return Err(AppError::bad_request("project_id cannot be nil UUID"));
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn has_permission(&self, required: Permission) -> bool {
        self.permissions.contains(&required)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tenant_scope_rejects_nil_identities() {
        let nil_org = TenantScope {
            organization_id: Uuid::nil(),
            workspace_id: None,
            project_id: None,
            actor: ActorContext {
                actor_type: ActorType::Human,
                actor_id: Uuid::new_v4(),
                credential_id: None,
                email: None,
            },
            permissions: vec![Permission::ProjectRead],
        };
        assert_eq!(nil_org.validate().unwrap_err().status_code(), 403);

        let nil_actor = TenantScope {
            organization_id: Uuid::new_v4(),
            workspace_id: None,
            project_id: None,
            actor: ActorContext {
                actor_type: ActorType::Human,
                actor_id: Uuid::nil(),
                credential_id: None,
                email: None,
            },
            permissions: vec![Permission::ProjectRead],
        };
        assert_eq!(nil_actor.validate().unwrap_err().status_code(), 401);

        let sp_no_cred = TenantScope {
            organization_id: Uuid::new_v4(),
            workspace_id: None,
            project_id: None,
            actor: ActorContext {
                actor_type: ActorType::ServicePrincipal,
                actor_id: Uuid::new_v4(),
                credential_id: None,
                email: None,
            },
            permissions: vec![Permission::ProjectRead],
        };
        assert_eq!(sp_no_cred.validate().unwrap_err().status_code(), 401);
    }

    #[test]
    fn valid_tenant_scope_passes() {
        let scope = TenantScope {
            organization_id: Uuid::new_v4(),
            workspace_id: Some(Uuid::new_v4()),
            project_id: Some(Uuid::new_v4()),
            actor: ActorContext {
                actor_type: ActorType::Human,
                actor_id: Uuid::new_v4(),
                credential_id: None,
                email: Some("gis@openvietgis.vn".to_string()),
            },
            permissions: vec![Permission::ProjectRead, Permission::ProjectEdit],
        };
        assert!(scope.validate().is_ok());
        assert!(scope.has_permission(Permission::ProjectEdit));
        assert!(!scope.has_permission(Permission::ProjectManage));
    }
}
