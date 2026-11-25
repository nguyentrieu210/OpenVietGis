//! E12 integration-hub authorization context over canonical grants + GIS scope.

use ovg_core::AppError;
use sqlx::Row;
use uuid::Uuid;

use crate::catalog::ProjectRead;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectiveProjectLevel {
    Read,
    Edit,
    Manage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IntegrationCatalogContext {
    pub level: EffectiveProjectLevel,
    pub scoped_access: bool,
}

fn db_error(error: sqlx::Error) -> AppError {
    tracing::error!(%error, "integration-hub authorization query failed");
    AppError::internal("integration-hub authorization query failed")
}

/// Compute only the permission dimensions consumed by the integration catalog.
/// ProjectRead has already proven project.read and current browser-session
/// membership; this query derives the stronger edit/manage levels and the GIS
/// scoped flag inside the same transaction.
pub async fn catalog_context(
    read: &mut ProjectRead<'_>,
    actor_id: Uuid,
) -> Result<IntegrationCatalogContext, AppError> {
    let row = sqlx::query(
        "SELECT
            gis_access.scoped($1, $3) AS scoped_access,
            (
              m.role IN ('owner','admin')
              OR EXISTS (
                SELECT 1
                FROM org.resource_grants g
                JOIN workspace.projects p
                  ON p.organization_id = m.organization_id
                 AND p.id = $3
                WHERE g.organization_id = m.organization_id
                  AND g.membership_id = m.id
                  AND g.subject_kind = 'membership'
                  AND g.revoked_at IS NULL
                  AND g.role = 'manager'
                  AND (
                    g.scope_kind = 'organization'
                    OR (g.scope_kind = 'workspace' AND g.workspace_id = p.workspace_id)
                    OR (g.scope_kind = 'project' AND g.project_id = p.id)
                  )
              )
            ) AS can_manage,
            (
              m.role IN ('owner','admin')
              OR EXISTS (
                SELECT 1
                FROM org.resource_grants g
                JOIN workspace.projects p
                  ON p.organization_id = m.organization_id
                 AND p.id = $3
                WHERE g.organization_id = m.organization_id
                  AND g.membership_id = m.id
                  AND g.subject_kind = 'membership'
                  AND g.revoked_at IS NULL
                  AND g.role IN ('manager','editor','contributor')
                  AND (
                    g.scope_kind = 'organization'
                    OR (g.scope_kind = 'workspace' AND g.workspace_id = p.workspace_id)
                    OR (g.scope_kind = 'project' AND g.project_id = p.id)
                  )
              )
            ) AS can_edit
         FROM org.organization_memberships m
         WHERE m.organization_id = $1
           AND m.user_id = $2
           AND m.status = 'active'",
    )
    .bind(read.organization_id())
    .bind(actor_id)
    .bind(read.project_id())
    .fetch_optional(&mut **read.transaction())
    .await
    .map_err(db_error)?
    .ok_or_else(|| AppError::not_found("organization not found"))?;

    let scoped_access: bool = row.try_get("scoped_access").map_err(db_error)?;
    let can_manage: bool = row.try_get("can_manage").map_err(db_error)?;
    let can_edit: bool = row.try_get("can_edit").map_err(db_error)?;
    let level = if can_manage {
        EffectiveProjectLevel::Manage
    } else if can_edit {
        EffectiveProjectLevel::Edit
    } else {
        EffectiveProjectLevel::Read
    };
    Ok(IntegrationCatalogContext {
        level,
        scoped_access,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_level_order_is_monotonic() {
        assert_ne!(EffectiveProjectLevel::Read, EffectiveProjectLevel::Edit);
        assert_ne!(EffectiveProjectLevel::Edit, EffectiveProjectLevel::Manage);
    }
}
