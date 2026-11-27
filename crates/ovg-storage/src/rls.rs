//! Fail-closed transaction-local PostgreSQL RLS scope binding.
//!
//! Mirrors `apps/api/openvietgis_api/adapters/persistence/rls.py` by binding
//! transaction-local (`is_local = true`) PostgreSQL GUC settings immediately
//! after opening a transaction. If tenant or actor scope is missing or nil,
//! transaction creation fails closed before executing any query.

use ovg_core::{ActorType, AppError, TenantScope};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

/// Exact transaction-local GUC bindings prepared from a validated `TenantScope`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TenantGucBindings {
    pub organization_id: String,
    pub actor_user_id: String,
    pub project_id: String,
}

impl TenantGucBindings {
    /// Validate a `TenantScope` (fail-closed) and derive GUC string bindings.
    pub fn from_scope(scope: &TenantScope) -> Result<Self, AppError> {
        scope.validate()?;
        let actor_user_id = match scope.actor.actor_type {
            ActorType::Human => scope.actor.actor_id.to_string(),
            ActorType::ServicePrincipal => String::new(),
        };
        let project_id = scope
            .project_id
            .map(|id| id.to_string())
            .unwrap_or_default();
        Ok(Self {
            organization_id: scope.organization_id.to_string(),
            actor_user_id,
            project_id,
        })
    }
}

/// Validate an official extension private-access namespace key matching Python `module_sdk.py`.
pub fn validate_extension_key(extension_key: &str) -> Result<(), AppError> {
    if extension_key.is_empty()
        || extension_key.len() > 64
        || !extension_key
            .bytes()
            .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-'))
    {
        return Err(AppError::bad_request("invalid extension key"));
    }
    Ok(())
}

/// SQL statement that binds both canonical OpenVietGIS RLS GUCs (`openvietgis.*`)
/// and standard tenant session GUCs (`app.current_*`) transaction-locally.
pub const BIND_TENANT_GUCS_SQL: &str = "SELECT \
    set_config('openvietgis.organization_id', $1, true), \
    set_config('openvietgis.actor_user_id', $2, true), \
    set_config('openvietgis.project_id', $3, true), \
    set_config('app.current_organization_id', $1, true), \
    set_config('app.current_user_id', $2, true)";

/// A PostgreSQL transaction with verified, transaction-local tenant RLS GUCs bound.
pub struct TenantTransaction<'a> {
    tx: Transaction<'a, Postgres>,
    organization_id: Uuid,
    project_id: Option<Uuid>,
    actor_id: Uuid,
}

impl<'a> TenantTransaction<'a> {
    /// Open a database transaction and bind tenant RLS GUCs fail-closed.
    pub async fn begin(pool: &'a crate::DbPool, scope: &TenantScope) -> Result<Self, AppError> {
        let bindings = TenantGucBindings::from_scope(scope)?;
        let mut tx = pool
            .inner()
            .begin()
            .await
            .map_err(|err| AppError::internal(format!("begin transaction failed: {err}")))?;

        sqlx::query(BIND_TENANT_GUCS_SQL)
            .bind(&bindings.organization_id)
            .bind(&bindings.actor_user_id)
            .bind(&bindings.project_id)
            .execute(&mut *tx)
            .await
            .map_err(|err| AppError::internal(format!("bind RLS context failed: {err}")))?;

        Ok(Self {
            tx,
            organization_id: scope.organization_id,
            project_id: scope.project_id,
            actor_id: scope.actor.actor_id,
        })
    }

    #[must_use]
    pub fn organization_id(&self) -> Uuid {
        self.organization_id
    }

    #[must_use]
    pub fn project_id(&self) -> Option<Uuid> {
        self.project_id
    }

    #[must_use]
    pub fn actor_id(&self) -> Uuid {
        self.actor_id
    }

    /// Mutable access to the underlying SQLx transaction for scoped repository queries.
    pub fn tx_mut(&mut self) -> &mut Transaction<'a, Postgres> {
        &mut self.tx
    }

    /// Bind or clear `openvietgis.extension_private_access` for one trusted official module.
    pub async fn set_extension_private_access(
        &mut self,
        extension_key: &str,
        enabled: bool,
    ) -> Result<(), AppError> {
        validate_extension_key(extension_key)?;
        let value = if enabled { extension_key } else { "" };
        sqlx::query("SELECT set_config('openvietgis.extension_private_access', $1, true)")
            .bind(value)
            .execute(&mut *self.tx)
            .await
            .map_err(|err| AppError::internal(format!("set extension access failed: {err}")))?;
        Ok(())
    }

    /// Clear transaction-local project context without releasing the organization pin.
    pub async fn clear_project_context(&mut self) -> Result<(), AppError> {
        self.project_id = None;
        sqlx::query(
            "SELECT set_config('openvietgis.project_id', '', true), \
             set_config('openvietgis.acl_internal', '', true)",
        )
        .execute(&mut *self.tx)
        .await
        .map_err(|err| AppError::internal(format!("clear project context failed: {err}")))?;
        Ok(())
    }

    /// Commit the tenant-scoped transaction.
    pub async fn commit(self) -> Result<(), AppError> {
        self.tx
            .commit()
            .await
            .map_err(|err| AppError::internal(format!("transaction commit failed: {err}")))
    }

    /// Explicitly roll back the tenant-scoped transaction.
    pub async fn rollback(self) -> Result<(), AppError> {
        self.tx
            .rollback()
            .await
            .map_err(|err| AppError::internal(format!("transaction rollback failed: {err}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ovg_core::{ActorContext, Permission};

    #[test]
    fn fail_closed_on_missing_or_nil_tenant_scope() {
        let bad = TenantScope {
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
        assert!(TenantGucBindings::from_scope(&bad).is_err());
    }

    #[test]
    fn binds_human_and_service_principal_gucs_matching_rls_py() {
        let org = Uuid::new_v4();
        let proj = Uuid::new_v4();
        let user = Uuid::new_v4();

        let human = TenantScope {
            organization_id: org,
            workspace_id: None,
            project_id: Some(proj),
            actor: ActorContext {
                actor_type: ActorType::Human,
                actor_id: user,
                credential_id: None,
                email: None,
            },
            permissions: vec![Permission::ProjectRead],
        };
        let hb = TenantGucBindings::from_scope(&human).unwrap();
        assert_eq!(hb.organization_id, org.to_string());
        assert_eq!(hb.actor_user_id, user.to_string());
        assert_eq!(hb.project_id, proj.to_string());

        let sp = TenantScope {
            organization_id: org,
            workspace_id: None,
            project_id: None,
            actor: ActorContext {
                actor_type: ActorType::ServicePrincipal,
                actor_id: Uuid::new_v4(),
                credential_id: Some(Uuid::new_v4()),
                email: None,
            },
            permissions: vec![Permission::ProjectRead],
        };
        let sb = TenantGucBindings::from_scope(&sp).unwrap();
        assert_eq!(sb.organization_id, org.to_string());
        assert_eq!(sb.actor_user_id, "");
        assert_eq!(sb.project_id, "");
    }

    #[test]
    fn validates_extension_keys_strictly() {
        assert!(validate_extension_key("house_numbering").is_ok());
        assert!(validate_extension_key("land-asset-2").is_ok());
        assert!(validate_extension_key("").is_err());
        assert!(validate_extension_key("House_Numbering").is_err());
        assert!(validate_extension_key("bad;drop table").is_err());
    }
}
