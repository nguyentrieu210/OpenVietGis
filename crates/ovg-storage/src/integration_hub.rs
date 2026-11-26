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

#[derive(Debug, Clone)]
pub struct LayerAccessRow {
    pub layer_id: Uuid,
    pub source_id: Uuid,
    pub name: String,
    pub geometry_family: String,
    pub dimensions: String,
    pub validation_state: String,
    pub source_crs_code: Option<i32>,
    pub stored_crs_code: Option<i32>,
    pub layer_revision: i64,
    pub content_revision: String,
    pub schema_revision: String,
    pub export_acl: bool,
}

pub async fn accessible_layers(
    read: &mut ProjectRead<'_>,
    scoped_access: bool,
) -> Result<Vec<LayerAccessRow>, AppError> {
    let rows = sqlx::query(
        "SELECT
            l.id AS layer_id,
            l.source_id AS source_id,
            l.name AS name,
            s.geometry_family AS geometry_family,
            s.dimensions AS dimensions,
            s.validation_state AS validation_state,
            s.source_crs AS source_crs_code,
            s.stored_crs AS stored_crs_code,
            l.layer_revision AS layer_revision,
            s.content_revision AS content_revision,
            s.schema_revision AS schema_revision,
            CASE WHEN $3
              THEN gis_access.source_allowed($1,$2,s.id,'export')
              ELSE true
            END AS export_acl
         FROM spatial.layers l
         JOIN spatial.data_sources s
           ON s.organization_id=l.organization_id
          AND s.project_id=l.project_id
          AND s.id=l.source_id
         WHERE l.organization_id=$1
           AND l.project_id=$2
           AND l.lifecycle='active'
           AND s.lifecycle='active'
         ORDER BY l.name ASC,l.id ASC
         LIMIT 1000",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(scoped_access)
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(db_error)?;

    rows.into_iter()
        .map(|row| {
            Ok(LayerAccessRow {
                layer_id: row.try_get("layer_id").map_err(db_error)?,
                source_id: row.try_get("source_id").map_err(db_error)?,
                name: row.try_get("name").map_err(db_error)?,
                geometry_family: row.try_get("geometry_family").map_err(db_error)?,
                dimensions: row.try_get("dimensions").map_err(db_error)?,
                validation_state: row.try_get("validation_state").map_err(db_error)?,
                source_crs_code: row.try_get("source_crs_code").map_err(db_error)?,
                stored_crs_code: row.try_get("stored_crs_code").map_err(db_error)?,
                layer_revision: row.try_get("layer_revision").map_err(db_error)?,
                content_revision: row.try_get("content_revision").map_err(db_error)?,
                schema_revision: row.try_get("schema_revision").map_err(db_error)?,
                export_acl: row.try_get("export_acl").map_err(db_error)?,
            })
        })
        .collect()
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
