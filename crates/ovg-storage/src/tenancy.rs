//! Organization, membership, workspace and project tenancy (E01) for verified
//! human browser sessions, mirroring the Python tenancy service and RLS binding.

use std::collections::BTreeSet;

use chrono::{DateTime, SecondsFormat, Utc};
use ovg_core::AppError;
use serde::Serialize;
use serde_json::{json, Value};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::DbPool;

pub const ORGANIZATION_READ: &str = "organization.read";
pub const ORGANIZATION_MANAGE_MEMBERS: &str = "organization.manage_members";
pub const ORGANIZATION_MANAGE_SETTINGS: &str = "organization.manage_settings";
pub const ORGANIZATION_LIFECYCLE: &str = "organization.lifecycle";
pub const ORGANIZATION_OWNER_TRANSFER: &str = "organization.owner.transfer";
pub const ORGANIZATION_DELETE: &str = "organization.delete";
pub const AUDIT_READ: &str = "audit.read";
pub const AUTHORIZATION_MANAGE_GRANTS: &str = "authorization.manage_grants";
pub const WORKSPACE_READ: &str = "workspace.read";
pub const WORKSPACE_MANAGE: &str = "workspace.manage";
pub const PROJECT_READ: &str = "project.read";
pub const PROJECT_EDIT: &str = "project.edit";
pub const PROJECT_MANAGE: &str = "project.manage";
pub const PROJECT_MOVE_WORKSPACE: &str = "project.move_workspace";
pub const PROJECT_ARCHIVE: &str = "project.archive";
pub const PROJECT_DELETE: &str = "project.delete";
pub const PROJECT_SHARE: &str = "project.share";

const ALL_PERMISSIONS: [&str; 19] = [
    ORGANIZATION_READ,
    ORGANIZATION_MANAGE_MEMBERS,
    ORGANIZATION_MANAGE_SETTINGS,
    ORGANIZATION_LIFECYCLE,
    ORGANIZATION_OWNER_TRANSFER,
    ORGANIZATION_DELETE,
    AUTHORIZATION_MANAGE_GRANTS,
    "invitation.manage",
    "service_principal.manage",
    AUDIT_READ,
    WORKSPACE_READ,
    WORKSPACE_MANAGE,
    PROJECT_READ,
    PROJECT_EDIT,
    PROJECT_MANAGE,
    PROJECT_MOVE_WORKSPACE,
    PROJECT_ARCHIVE,
    PROJECT_DELETE,
    PROJECT_SHARE,
];
const OWNER_ONLY: [&str; 3] = [
    ORGANIZATION_LIFECYCLE,
    ORGANIZATION_OWNER_TRANSFER,
    ORGANIZATION_DELETE,
];

/// Organization role permissions (`ORGANIZATION_ROLE_PERMISSIONS`).
#[must_use]
pub fn organization_role_permissions(role: &str) -> BTreeSet<&'static str> {
    match role {
        "owner" => ALL_PERMISSIONS.into_iter().collect(),
        "admin" => ALL_PERMISSIONS
            .into_iter()
            .filter(|permission| !OWNER_ONLY.contains(permission))
            .collect(),
        "member" => BTreeSet::from([ORGANIZATION_READ]),
        _ => BTreeSet::new(),
    }
}

/// Resource grant role permissions (`RESOURCE_ROLE_PERMISSIONS`).
fn resource_role_permissions(role: &str) -> &'static [&'static str] {
    match role {
        "manager" => &[
            WORKSPACE_READ,
            WORKSPACE_MANAGE,
            PROJECT_READ,
            PROJECT_EDIT,
            PROJECT_MANAGE,
            PROJECT_MOVE_WORKSPACE,
            PROJECT_ARCHIVE,
            PROJECT_DELETE,
            PROJECT_SHARE,
        ],
        "editor" => &[WORKSPACE_READ, PROJECT_READ, PROJECT_EDIT, PROJECT_SHARE],
        "contributor" => &[WORKSPACE_READ, PROJECT_READ, PROJECT_EDIT],
        "viewer" => &[WORKSPACE_READ, PROJECT_READ],
        _ => &[],
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Organization {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Membership {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub user_id: Uuid,
    pub role: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Workspace {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub name: String,
    pub slug: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Project {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub workspace_id: Option<Uuid>,
    pub name: String,
    pub slug: String,
    pub status: String,
}

const MEMBERSHIP_COLUMNS: &str = "id, organization_id, user_id, role, status";
const WORKSPACE_COLUMNS: &str = "id, organization_id, name, slug, status";
const PROJECT_COLUMNS: &str = "id, organization_id, workspace_id, name, slug, status";

/// One request transaction for a verified human actor. The organization GUC is
/// bound only after the caller's active membership has been resolved.
pub struct TenantTx {
    tx: Transaction<'static, Postgres>,
    actor_id: Uuid,
    display_name: String,
    organization_id: Option<Uuid>,
}

impl TenantTx {
    pub async fn begin(
        pool: &DbPool,
        actor_id: Uuid,
        session_id: Uuid,
        display_name: &str,
    ) -> Result<Self, AppError> {
        let tx = pool.inner().begin().await.map_err(db_error)?;
        let mut this = Self {
            tx,
            actor_id,
            display_name: display_name.to_owned(),
            organization_id: None,
        };
        this.bind(None).await?;
        // Revocation between authentication and this transaction must deny.
        let current: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                 SELECT 1
                 FROM identity.web_sessions AS s
                 JOIN identity.users AS u ON u.id = s.user_id
                 JOIN identity.identity_links AS l ON l.id = s.identity_link_id AND l.user_id = u.id
                 WHERE s.id = $1 AND u.id = $2 AND u.status = 'active'
                   AND s.revoked_at IS NULL AND s.expires_at > clock_timestamp()
             )",
        )
        .bind(session_id)
        .bind(actor_id)
        .fetch_one(&mut *this.tx)
        .await
        .map_err(db_error)?;
        if !current {
            return Err(AppError::unauthorized("authentication required"));
        }
        Ok(this)
    }

    pub async fn commit(self) -> Result<(), AppError> {
        self.tx.commit().await.map_err(db_error)
    }

    async fn bind(&mut self, organization_id: Option<Uuid>) -> Result<(), AppError> {
        sqlx::query(crate::rls::BIND_TENANT_GUCS_SQL)
            .bind(organization_id.map(|id| id.to_string()).unwrap_or_default())
            .bind(self.actor_id.to_string())
            .bind("")
            .execute(&mut *self.tx)
            .await
            .map_err(db_error)?;
        self.organization_id = organization_id;
        Ok(())
    }

    async fn active_membership(
        &mut self,
        organization_id: Uuid,
    ) -> Result<Option<Membership>, AppError> {
        let sql = format!(
            "SELECT {MEMBERSHIP_COLUMNS} FROM org.organization_memberships
             WHERE organization_id = $1 AND user_id = $2 AND status = 'active'"
        );
        let row: Option<MembershipRow> = sqlx::query_as(&sql)
            .bind(organization_id)
            .bind(self.actor_id)
            .fetch_optional(&mut *self.tx)
            .await
            .map_err(db_error)?;
        Ok(row.map(membership))
    }

    /// Port of `human_tenant`: membership first, then the organization row.
    pub async fn member(
        &mut self,
        organization_id: Uuid,
        lock: bool,
        allow_inactive_owner: bool,
    ) -> Result<(Organization, Membership), AppError> {
        let not_found = || AppError::not_found("organization not found");
        self.bind(None).await?;
        self.active_membership(organization_id)
            .await?
            .ok_or_else(not_found)?;
        self.bind(Some(organization_id)).await?;
        let organization = self
            .organization(organization_id, lock)
            .await?
            .ok_or_else(not_found)?;
        // Revalidate after the organization lock to avoid racing member removal.
        let membership = self
            .active_membership(organization_id)
            .await?
            .ok_or_else(not_found)?;
        if organization.status != "active" && !(allow_inactive_owner && membership.role == "owner")
        {
            return Err(AppError::forbidden("organization is not active"));
        }
        Ok((organization, membership))
    }

    /// Port of the project access guard: membership, row locks, GIS scoping.
    pub async fn project_guard(
        &mut self,
        organization_id: Uuid,
        project_id: Uuid,
        read: bool,
    ) -> Result<bool, AppError> {
        self.bind(None).await?;
        self.active_membership(organization_id)
            .await?
            .ok_or_else(|| AppError::not_found("organization not found"))?;
        self.bind(Some(organization_id)).await?;
        let lock = if read { "FOR SHARE" } else { "FOR UPDATE" };
        sqlx::query(&format!(
            "SELECT id FROM org.organizations WHERE id = $1 {lock}"
        ))
        .bind(organization_id)
        .fetch_optional(&mut *self.tx)
        .await
        .map_err(db_error)?;
        sqlx::query(&format!(
            "SELECT id FROM workspace.projects WHERE organization_id = $1 AND id = $2 {lock}"
        ))
        .bind(organization_id)
        .bind(project_id)
        .fetch_optional(&mut *self.tx)
        .await
        .map_err(db_error)?;
        sqlx::query_scalar("SELECT gis_access.scoped($1, $2)")
            .bind(organization_id)
            .bind(project_id)
            .fetch_one(&mut *self.tx)
            .await
            .map_err(db_error)
    }

    pub async fn organization(
        &mut self,
        organization_id: Uuid,
        lock: bool,
    ) -> Result<Option<Organization>, AppError> {
        let suffix = if lock { " FOR UPDATE" } else { "" };
        let row: Option<(Uuid, String, String, String)> = sqlx::query_as(&format!(
            "SELECT id, name, slug, status FROM org.organizations WHERE id = $1{suffix}"
        ))
        .bind(organization_id)
        .fetch_optional(&mut *self.tx)
        .await
        .map_err(db_error)?;
        Ok(row.map(|(id, name, slug, status)| Organization {
            id,
            name,
            slug,
            status,
        }))
    }

    /// Port of `SqlEffectivePermissionProvider.resolve` for a human actor.
    pub async fn permissions(
        &mut self,
        organization_id: Uuid,
        workspace_id: Option<Uuid>,
        project_id: Option<Uuid>,
    ) -> Result<BTreeSet<&'static str>, AppError> {
        if self.organization_id != Some(organization_id) {
            return Ok(BTreeSet::new());
        }
        let rows: Vec<(String, Option<String>)> = sqlx::query_as(
            "WITH requested AS (
                 SELECT $1::uuid AS organization_id, $2::uuid AS user_id,
                        $3::uuid AS workspace_id, $4::uuid AS project_id
             )
             SELECT m.role, g.role
             FROM requested AS r
             JOIN org.organization_memberships AS m
               ON m.organization_id = r.organization_id
              AND m.user_id = r.user_id
              AND m.status = 'active'
             JOIN org.organizations AS o
               ON o.id = m.organization_id
              AND o.status = 'active'
             LEFT JOIN workspace.projects AS p
               ON p.organization_id = r.organization_id
              AND p.id = r.project_id
             LEFT JOIN workspace.workspaces AS w
               ON r.project_id IS NULL
              AND w.organization_id = r.organization_id
              AND w.id = r.workspace_id
             LEFT JOIN org.resource_grants AS g
               ON g.organization_id = r.organization_id
              AND g.subject_kind = 'membership'
              AND g.membership_id = m.id
              AND g.revoked_at IS NULL
              AND (
                   g.scope_kind = 'organization'
                   OR (
                       g.scope_kind = 'workspace'
                       AND g.workspace_id = CASE
                           WHEN r.project_id IS NOT NULL THEN p.workspace_id
                           ELSE r.workspace_id
                       END
                   )
                   OR (g.scope_kind = 'project' AND g.project_id = r.project_id)
              )
             WHERE (
                 r.project_id IS NOT NULL
                 AND p.id IS NOT NULL
                 AND (r.workspace_id IS NULL OR r.workspace_id = p.workspace_id)
             ) OR (
                 r.project_id IS NULL
                 AND (r.workspace_id IS NULL OR w.id IS NOT NULL)
             )",
        )
        .bind(organization_id)
        .bind(self.actor_id)
        .bind(workspace_id)
        .bind(project_id)
        .fetch_all(&mut *self.tx)
        .await
        .map_err(db_error)?;
        let mut permissions = BTreeSet::new();
        for (membership_role, grant_role) in rows {
            permissions.extend(organization_role_permissions(&membership_role));
            if let Some(grant_role) = grant_role {
                permissions.extend(resource_role_permissions(&grant_role).iter().copied());
            }
        }
        Ok(permissions)
    }

    /// `AuthorizationService.require`: 403 `missing permission: <name>`.
    pub async fn require(
        &mut self,
        organization_id: Uuid,
        workspace_id: Option<Uuid>,
        project_id: Option<Uuid>,
        permission: &str,
    ) -> Result<(), AppError> {
        if self
            .permissions(organization_id, workspace_id, project_id)
            .await?
            .contains(permission)
        {
            Ok(())
        } else {
            Err(AppError::forbidden(format!(
                "missing permission: {permission}"
            )))
        }
    }

    pub async fn installation_initialized(&mut self) -> Result<bool, AppError> {
        let initialized: Option<bool> = sqlx::query_scalar(
            "SELECT initialized_at IS NOT NULL FROM platform.installation_state
             WHERE singleton_key = 'installation'",
        )
        .fetch_optional(&mut *self.tx)
        .await
        .map_err(db_error)?;
        Ok(initialized == Some(true))
    }

    pub async fn create_organization(
        &mut self,
        name: &str,
        slug: &str,
    ) -> Result<(Organization, Membership), AppError> {
        let now = Utc::now();
        let organization = Organization {
            id: Uuid::new_v4(),
            name: name.to_owned(),
            slug: slug.to_owned(),
            status: "active".to_owned(),
        };
        let owner = Membership {
            id: Uuid::new_v4(),
            organization_id: organization.id,
            user_id: self.actor_id,
            role: "owner".to_owned(),
            status: "active".to_owned(),
        };
        self.bind(Some(organization.id)).await?;
        sqlx::query(
            "INSERT INTO org.organizations (
                 id, name, slug, status, created_by_actor_type, created_by_actor_id,
                 created_at, updated_at, deleted_at
             ) VALUES ($1, $2, $3, 'active', 'human', $4, $5, $5, NULL)",
        )
        .bind(organization.id)
        .bind(&organization.name)
        .bind(&organization.slug)
        .bind(self.actor_id)
        .bind(now)
        .execute(&mut *self.tx)
        .await
        .map_err(|error| {
            let duplicate = matches!(&error, sqlx::Error::Database(database)
                if database.code().as_deref() == Some("23505"));
            if duplicate {
                AppError::conflict(
                    "ORGANIZATION_SLUG_EXISTS",
                    "organization slug already exists",
                )
            } else {
                db_error(error)
            }
        })?;
        sqlx::query(
            "INSERT INTO org.organization_memberships (
                 id, organization_id, user_id, role, status,
                 created_at, activated_at, suspended_at, revoked_at
             ) VALUES ($1, $2, $3, 'owner', 'active', $4, $4, NULL, NULL)",
        )
        .bind(owner.id)
        .bind(organization.id)
        .bind(self.actor_id)
        .bind(now)
        .execute(&mut *self.tx)
        .await
        .map_err(db_error)?;
        self.audit(
            organization.id,
            "organization.create",
            "organization",
            Some(organization.id),
            "success",
            json!({ "slug": organization.slug }),
        )
        .await?;
        Ok((organization, owner))
    }

    pub async fn update_organization(
        &mut self,
        organization_id: Uuid,
        name: &str,
        status: &str,
    ) -> Result<(), AppError> {
        sqlx::query(
            "UPDATE org.organizations
             SET name = $2, status = $3::varchar, updated_at = $4,
                 deleted_at = CASE WHEN $3::varchar = 'deleted' THEN $4
                                   WHEN $3::varchar = 'active' THEN NULL
                                   ELSE deleted_at END
             WHERE id = $1",
        )
        .bind(organization_id)
        .bind(name)
        .bind(status)
        .bind(Utc::now())
        .execute(&mut *self.tx)
        .await
        .map_err(db_error)?;
        self.audit(
            organization_id,
            "organization.update",
            "organization",
            Some(organization_id),
            "success",
            json!({ "status": status }),
        )
        .await
    }

    pub async fn membership(
        &mut self,
        organization_id: Uuid,
        membership_id: Uuid,
        lock: bool,
    ) -> Result<Option<Membership>, AppError> {
        let suffix = if lock { " FOR UPDATE" } else { "" };
        let sql = format!(
            "SELECT {MEMBERSHIP_COLUMNS} FROM org.organization_memberships
             WHERE organization_id = $1 AND id = $2{suffix}"
        );
        let row: Option<MembershipRow> = sqlx::query_as(&sql)
            .bind(organization_id)
            .bind(membership_id)
            .fetch_optional(&mut *self.tx)
            .await
            .map_err(db_error)?;
        Ok(row.map(membership))
    }

    /// The caller's own membership in any status, locked.
    pub async fn own_membership_for_update(
        &mut self,
        organization_id: Uuid,
    ) -> Result<Option<Membership>, AppError> {
        let sql = format!(
            "SELECT {MEMBERSHIP_COLUMNS} FROM org.organization_memberships
             WHERE organization_id = $1 AND user_id = $2 FOR UPDATE"
        );
        let row: Option<MembershipRow> = sqlx::query_as(&sql)
            .bind(organization_id)
            .bind(self.actor_id)
            .fetch_optional(&mut *self.tx)
            .await
            .map_err(db_error)?;
        Ok(row.map(membership))
    }

    pub async fn list_members(&mut self, organization_id: Uuid) -> Result<Vec<Value>, AppError> {
        let rows = sqlx::query(
            "SELECT m.id, m.organization_id, m.user_id, m.role, m.status,
                    u.display_name, u.email_snapshot
             FROM org.organization_memberships AS m
             LEFT JOIN identity.users AS u ON u.id = m.user_id
             WHERE m.organization_id = $1
             ORDER BY m.created_at, m.id",
        )
        .bind(organization_id)
        .fetch_all(&mut *self.tx)
        .await
        .map_err(db_error)?;
        rows.iter()
            .map(|row| -> Result<Value, sqlx::Error> {
                Ok(json!({
                    "id": row.try_get::<Uuid, _>("id")?,
                    "organization_id": row.try_get::<Uuid, _>("organization_id")?,
                    "user_id": row.try_get::<Uuid, _>("user_id")?,
                    "role": row.try_get::<String, _>("role")?,
                    "status": row.try_get::<String, _>("status")?,
                    "display_name": row.try_get::<Option<String>, _>("display_name")?,
                    "email_snapshot": row.try_get::<Option<String>, _>("email_snapshot")?,
                }))
            })
            .collect::<Result<_, _>>()
            .map_err(db_error)
    }

    pub async fn count_other_active_owners(
        &mut self,
        organization_id: Uuid,
        membership_id: Uuid,
    ) -> Result<i64, AppError> {
        sqlx::query_scalar(
            "SELECT count(*) FROM org.organization_memberships
             WHERE organization_id = $1 AND id <> $2 AND role = 'owner' AND status = 'active'",
        )
        .bind(organization_id)
        .bind(membership_id)
        .fetch_one(&mut *self.tx)
        .await
        .map_err(db_error)
    }

    /// Set role/status with the Python lifecycle timestamps; no audit.
    pub async fn set_membership(
        &mut self,
        organization_id: Uuid,
        membership_id: Uuid,
        role: &str,
        status: &str,
    ) -> Result<(), AppError> {
        sqlx::query(
            "UPDATE org.organization_memberships
             SET role = $3, status = $4::varchar,
                 activated_at = CASE WHEN $4::varchar = 'active' AND status <> 'active'
                                     THEN $5 ELSE activated_at END,
                 suspended_at = CASE WHEN $4::varchar = 'suspended' THEN $5
                                     WHEN $4::varchar = 'active' THEN NULL
                                     ELSE suspended_at END,
                 revoked_at = CASE WHEN $4::varchar = 'revoked' THEN $5
                                   WHEN $4::varchar = 'active' THEN NULL
                                   ELSE revoked_at END
             WHERE organization_id = $1 AND id = $2",
        )
        .bind(organization_id)
        .bind(membership_id)
        .bind(role)
        .bind(status)
        .bind(Utc::now())
        .execute(&mut *self.tx)
        .await
        .map_err(db_error)?;
        Ok(())
    }

    pub async fn list_workspaces(
        &mut self,
        organization_id: Uuid,
    ) -> Result<Vec<Workspace>, AppError> {
        let sql = format!(
            "SELECT {WORKSPACE_COLUMNS} FROM workspace.workspaces
             WHERE organization_id = $1 AND status <> 'deleted' ORDER BY name, id"
        );
        let rows: Vec<WorkspaceRow> = sqlx::query_as(&sql)
            .bind(organization_id)
            .fetch_all(&mut *self.tx)
            .await
            .map_err(db_error)?;
        Ok(rows.into_iter().map(workspace).collect())
    }

    pub async fn workspace(
        &mut self,
        organization_id: Uuid,
        workspace_id: Uuid,
    ) -> Result<Option<Workspace>, AppError> {
        let sql = format!(
            "SELECT {WORKSPACE_COLUMNS} FROM workspace.workspaces
             WHERE organization_id = $1 AND id = $2"
        );
        let row: Option<WorkspaceRow> = sqlx::query_as(&sql)
            .bind(organization_id)
            .bind(workspace_id)
            .fetch_optional(&mut *self.tx)
            .await
            .map_err(db_error)?;
        Ok(row.map(workspace))
    }

    pub async fn create_workspace(
        &mut self,
        organization_id: Uuid,
        name: &str,
        slug: &str,
    ) -> Result<Workspace, AppError> {
        let created = Workspace {
            id: Uuid::new_v4(),
            organization_id,
            name: name.to_owned(),
            slug: slug.to_owned(),
            status: "active".to_owned(),
        };
        sqlx::query(
            "INSERT INTO workspace.workspaces (
                 id, organization_id, name, slug, status, created_by_actor_type,
                 created_by_actor_id, created_at, updated_at, archived_at, deleted_at
             ) VALUES ($1, $2, $3, $4, 'active', 'human', $5, $6, $6, NULL, NULL)",
        )
        .bind(created.id)
        .bind(organization_id)
        .bind(name)
        .bind(slug)
        .bind(self.actor_id)
        .bind(Utc::now())
        .execute(&mut *self.tx)
        .await
        .map_err(db_error)?;
        self.audit(
            organization_id,
            "workspace.create",
            "workspace",
            Some(created.id),
            "success",
            json!({ "slug": slug }),
        )
        .await?;
        Ok(created)
    }

    pub async fn update_workspace(
        &mut self,
        previous: &Workspace,
        name: &str,
        status: &str,
    ) -> Result<(), AppError> {
        sqlx::query(
            "UPDATE workspace.workspaces
             SET name = $3, status = $4::varchar, updated_at = $5,
                 archived_at = CASE WHEN $4::varchar = 'archived' THEN $5
                                    WHEN $4::varchar = 'active' THEN NULL
                                    ELSE archived_at END,
                 deleted_at = CASE WHEN $4::varchar = 'deleted' THEN $5 ELSE NULL END
             WHERE organization_id = $1 AND id = $2",
        )
        .bind(previous.organization_id)
        .bind(previous.id)
        .bind(name)
        .bind(status)
        .bind(Utc::now())
        .execute(&mut *self.tx)
        .await
        .map_err(db_error)?;
        self.audit(
            previous.organization_id,
            "workspace.update",
            "workspace",
            Some(previous.id),
            "success",
            json!({ "status": status, "previous_status": previous.status }),
        )
        .await
    }

    pub async fn count_active_projects(
        &mut self,
        organization_id: Uuid,
        workspace_id: Uuid,
    ) -> Result<i64, AppError> {
        sqlx::query_scalar(
            "SELECT count(*) FROM workspace.projects
             WHERE organization_id = $1 AND workspace_id = $2 AND status = 'active'",
        )
        .bind(organization_id)
        .bind(workspace_id)
        .fetch_one(&mut *self.tx)
        .await
        .map_err(db_error)
    }

    pub async fn list_projects(
        &mut self,
        organization_id: Uuid,
        workspace_id: Option<Uuid>,
    ) -> Result<Vec<Project>, AppError> {
        let sql = format!(
            "SELECT {PROJECT_COLUMNS} FROM workspace.projects
             WHERE organization_id = $1 AND status <> 'deleted'
               AND ($2::uuid IS NULL OR workspace_id = $2)
             ORDER BY name, id"
        );
        let rows: Vec<ProjectRow> = sqlx::query_as(&sql)
            .bind(organization_id)
            .bind(workspace_id)
            .fetch_all(&mut *self.tx)
            .await
            .map_err(db_error)?;
        Ok(rows.into_iter().map(project).collect())
    }

    pub async fn project(
        &mut self,
        organization_id: Uuid,
        project_id: Uuid,
    ) -> Result<Option<Project>, AppError> {
        let sql = format!(
            "SELECT {PROJECT_COLUMNS} FROM workspace.projects
             WHERE organization_id = $1 AND id = $2"
        );
        let row: Option<ProjectRow> = sqlx::query_as(&sql)
            .bind(organization_id)
            .bind(project_id)
            .fetch_optional(&mut *self.tx)
            .await
            .map_err(db_error)?;
        Ok(row.map(project))
    }

    pub async fn create_project(
        &mut self,
        organization_id: Uuid,
        workspace_id: Option<Uuid>,
        name: &str,
        slug: &str,
    ) -> Result<Project, AppError> {
        let created = Project {
            id: Uuid::new_v4(),
            organization_id,
            workspace_id,
            name: name.to_owned(),
            slug: slug.to_owned(),
            status: "active".to_owned(),
        };
        sqlx::query(
            "INSERT INTO workspace.projects (
                 id, organization_id, workspace_id, name, slug, status, created_by_actor_type,
                 created_by_actor_id, created_at, updated_at, archived_at, deleted_at
             ) VALUES ($1, $2, $3, $4, $5, 'active', 'human', $6, $7, $7, NULL, NULL)",
        )
        .bind(created.id)
        .bind(organization_id)
        .bind(workspace_id)
        .bind(name)
        .bind(slug)
        .bind(self.actor_id)
        .bind(Utc::now())
        .execute(&mut *self.tx)
        .await
        .map_err(db_error)?;
        self.audit(
            organization_id,
            "project.create",
            "project",
            Some(created.id),
            "success",
            json!({ "slug": slug }),
        )
        .await?;
        Ok(created)
    }

    pub async fn update_project(
        &mut self,
        previous: &Project,
        updated: &Project,
    ) -> Result<(), AppError> {
        sqlx::query(
            "UPDATE workspace.projects
             SET workspace_id = $3, name = $4, status = $5::varchar, updated_at = $6,
                 archived_at = CASE WHEN $5::varchar = 'archived' THEN $6
                                    WHEN $5::varchar = 'active' THEN NULL
                                    ELSE archived_at END,
                 deleted_at = CASE WHEN $5::varchar = 'deleted' THEN $6 ELSE NULL END
             WHERE organization_id = $1 AND id = $2",
        )
        .bind(previous.organization_id)
        .bind(previous.id)
        .bind(updated.workspace_id)
        .bind(&updated.name)
        .bind(&updated.status)
        .bind(Utc::now())
        .execute(&mut *self.tx)
        .await
        .map_err(db_error)?;
        self.audit(
            previous.organization_id,
            "project.update",
            "project",
            Some(previous.id),
            "success",
            json!({
                "status": updated.status,
                "previous_status": previous.status,
                "workspace_id": updated.workspace_id,
            }),
        )
        .await
    }

    /// Append an audit event for the bound organization and verified actor.
    pub async fn audit(
        &mut self,
        organization_id: Uuid,
        action: &str,
        resource_type: &str,
        resource_id: Option<Uuid>,
        outcome: &str,
        metadata: Value,
    ) -> Result<(), AppError> {
        if self.organization_id != Some(organization_id) {
            return Err(AppError::internal("audit requires the verified tenant"));
        }
        sqlx::query(
            "INSERT INTO audit.audit_events (
                 id, organization_id, actor_type, actor_id, actor_display_snapshot,
                 action, resource_type, resource_id, outcome, metadata, created_at
             ) VALUES ($1, $2, 'human', $3, $4, $5, $6, $7, $8, $9, $10)",
        )
        .bind(Uuid::new_v4())
        .bind(organization_id)
        .bind(self.actor_id)
        .bind(&self.display_name)
        .bind(action)
        .bind(resource_type)
        .bind(resource_id)
        .bind(outcome)
        .bind(metadata)
        .bind(Utc::now())
        .execute(&mut *self.tx)
        .await
        .map_err(db_error)?;
        Ok(())
    }

    /// List all grants, including revoked ones, for the authorized tenant.
    pub async fn list_grants(&mut self, organization_id: Uuid) -> Result<Vec<Value>, AppError> {
        self.require(organization_id, None, None, AUTHORIZATION_MANAGE_GRANTS)
            .await?;
        let rows = sqlx::query(
            "SELECT id, organization_id, subject_kind, membership_id, service_principal_id,
                    scope_kind, workspace_id, project_id, role, created_at, revoked_at
             FROM org.resource_grants
             WHERE organization_id = $1
             ORDER BY created_at, id",
        )
        .bind(organization_id)
        .fetch_all(&mut *self.tx)
        .await
        .map_err(db_error)?;
        rows.iter()
            .map(|row| -> Result<Value, sqlx::Error> {
                let subject_kind: String = row.try_get("subject_kind")?;
                let subject_id: Option<Uuid> = if subject_kind == "membership" {
                    row.try_get("membership_id")?
                } else {
                    row.try_get("service_principal_id")?
                };
                let created_at: DateTime<Utc> = row.try_get("created_at")?;
                let revoked_at: Option<DateTime<Utc>> = row.try_get("revoked_at")?;
                Ok(json!({
                    "id": row.try_get::<Uuid, _>("id")?,
                    "organization_id": row.try_get::<Uuid, _>("organization_id")?,
                    "subject_kind": subject_kind,
                    "subject_id": subject_id,
                    "scope_kind": row.try_get::<String, _>("scope_kind")?,
                    "workspace_id": row.try_get::<Option<Uuid>, _>("workspace_id")?,
                    "project_id": row.try_get::<Option<Uuid>, _>("project_id")?,
                    "role": row.try_get::<String, _>("role")?,
                    "created_at": python_timestamp(created_at),
                    "revoked_at": revoked_at.map(python_timestamp),
                }))
            })
            .collect::<Result<_, _>>()
            .map_err(db_error)
    }

    pub async fn list_audit(
        &mut self,
        organization_id: Uuid,
        limit: i64,
    ) -> Result<Vec<Value>, AppError> {
        let rows = sqlx::query(
            "SELECT id, organization_id, actor_type, actor_id, actor_display_snapshot, action,
                    resource_type, resource_id, outcome, metadata, created_at
             FROM audit.audit_events
             WHERE organization_id = $1
             ORDER BY created_at DESC, id DESC
             LIMIT $2",
        )
        .bind(organization_id)
        .bind(limit)
        .fetch_all(&mut *self.tx)
        .await
        .map_err(db_error)?;
        rows.iter()
            .map(|row| -> Result<Value, sqlx::Error> {
                let created_at: DateTime<Utc> = row.try_get("created_at")?;
                Ok(json!({
                    "id": row.try_get::<Uuid, _>("id")?,
                    "organization_id": row.try_get::<Option<Uuid>, _>("organization_id")?,
                    "actor_type": row.try_get::<String, _>("actor_type")?,
                    "actor_id": row.try_get::<Option<Uuid>, _>("actor_id")?,
                    "actor_display_snapshot": row.try_get::<String, _>("actor_display_snapshot")?,
                    "action": row.try_get::<String, _>("action")?,
                    "resource_type": row.try_get::<Option<String>, _>("resource_type")?,
                    "resource_id": row.try_get::<Option<Uuid>, _>("resource_id")?,
                    "outcome": row.try_get::<String, _>("outcome")?,
                    "metadata": row.try_get::<Value, _>("metadata")?,
                    "created_at": python_timestamp(created_at),
                }))
            })
            .collect::<Result<_, _>>()
            .map_err(db_error)
    }

    /// Logout: revoke the caller's current browser session.
    pub async fn revoke_session(&mut self, session_id: Uuid) -> Result<(), AppError> {
        sqlx::query("UPDATE identity.web_sessions SET revoked_at = now() WHERE id = $1")
            .bind(session_id)
            .execute(&mut *self.tx)
            .await
            .map_err(db_error)?;
        Ok(())
    }
}

type MembershipRow = (Uuid, Uuid, Uuid, String, String);
type WorkspaceRow = (Uuid, Uuid, String, String, String);
type ProjectRow = (Uuid, Uuid, Option<Uuid>, String, String, String);

fn membership((id, organization_id, user_id, role, status): MembershipRow) -> Membership {
    Membership {
        id,
        organization_id,
        user_id,
        role,
        status,
    }
}

fn workspace((id, organization_id, name, slug, status): WorkspaceRow) -> Workspace {
    Workspace {
        id,
        organization_id,
        name,
        slug,
        status,
    }
}

fn project((id, organization_id, workspace_id, name, slug, status): ProjectRow) -> Project {
    Project {
        id,
        organization_id,
        workspace_id,
        name,
        slug,
        status,
    }
}

/// Pydantic v2 JSON datetime: `Z` suffix, microseconds only when non-zero.
fn python_timestamp(value: DateTime<Utc>) -> String {
    let format = if value.timestamp_subsec_micros() == 0 {
        SecondsFormat::Secs
    } else {
        SecondsFormat::Micros
    };
    value.to_rfc3339_opts(format, true)
}

fn db_error(error: sqlx::Error) -> AppError {
    AppError::internal(format!("tenancy storage query failed: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_permissions_match_the_python_tables() {
        assert_eq!(organization_role_permissions("owner").len(), 19);
        let admin = organization_role_permissions("admin");
        assert_eq!(admin.len(), 16);
        assert!(!admin.contains(ORGANIZATION_DELETE));
        assert!(admin.contains(PROJECT_DELETE));
        assert_eq!(
            organization_role_permissions("member"),
            BTreeSet::from([ORGANIZATION_READ])
        );
        assert!(organization_role_permissions("guest").is_empty());
        assert_eq!(resource_role_permissions("manager").len(), 9);
        assert_eq!(
            resource_role_permissions("viewer"),
            [WORKSPACE_READ, PROJECT_READ]
        );
        assert!(resource_role_permissions("unknown").is_empty());
    }
}
