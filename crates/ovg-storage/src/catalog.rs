//! Read-only catalog and project scope over the canonical PostgreSQL schema.

use std::collections::{HashMap, HashSet};
use std::fmt::Write;

use chrono::{DateTime, SecondsFormat, Utc};
use ovg_core::crs::{reference_coordinate_wire, reference_profile_wire};
use ovg_core::{ActorContext, ActorType, AppError, TenantScope};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::postgres::PgRow;
use sqlx::Row;
use uuid::Uuid;

use crate::{DbPool, TenantTransaction};

mod features;
mod fixtures;
mod map_write;
mod write;
pub use features::FeatureRequest;
pub use map_write::MapSaveIntent;
pub use write::{GeometryPreviewInput, ManualFieldInput, DETAIL_CODE};

#[derive(Clone, Copy)]
pub enum ProjectReadPolicy {
    LayerAcl,
    Unrestricted,
    UnrestrictedEdit,
    UnrestrictedManage,
}

/// Proof of project.read on exactly one organization and project. The
/// authorization checks and reads share the same transaction and row locks.
pub struct ProjectRead<'a> {
    pub(crate) tx: TenantTransaction<'a>,
    pub(crate) project_id: Uuid,
    pub(crate) gis_scoped: bool,
    edit_authorized: bool,
    service_display_name: Option<String>,
}

impl<'a> ProjectRead<'a> {
    pub async fn begin(
        pool: &'a DbPool,
        organization_id: Uuid,
        project_id: Uuid,
        actor_id: Uuid,
        session_id: Uuid,
        policy: ProjectReadPolicy,
    ) -> Result<Self, AppError> {
        Self::open(
            pool,
            organization_id,
            project_id,
            actor_id,
            session_id,
            policy,
        )
        .await
    }

    /// Proof of project.edit on a writable (active) project. Organization and
    /// project rows are locked FOR UPDATE like the reference mutation guard.
    pub async fn begin_edit(
        pool: &'a DbPool,
        organization_id: Uuid,
        project_id: Uuid,
        actor_id: Uuid,
        session_id: Uuid,
    ) -> Result<Self, AppError> {
        Self::begin(
            pool,
            organization_id,
            project_id,
            actor_id,
            session_id,
            ProjectReadPolicy::UnrestrictedEdit,
        )
        .await
    }

    #[must_use]
    pub fn organization_id(&self) -> Uuid {
        self.tx.organization_id()
    }

    #[must_use]
    pub fn project_id(&self) -> Uuid {
        self.project_id
    }

    #[must_use]
    pub fn actor_id(&self) -> Uuid {
        self.tx.actor_id()
    }

    #[must_use]
    pub fn actor_type(&self) -> &'static str {
        if self.service_display_name.is_some() {
            "service_principal"
        } else {
            "human"
        }
    }

    #[must_use]
    pub fn service_display_name(&self) -> Option<&str> {
        self.service_display_name.as_deref()
    }

    #[must_use]
    pub fn is_gis_scoped(&self) -> bool {
        self.gis_scoped
    }

    /// The authorized, RLS-bound transaction for engine repositories.
    pub fn transaction(&mut self) -> &mut sqlx::Transaction<'a, sqlx::Postgres> {
        self.tx.tx_mut()
    }

    pub async fn begin_service(
        pool: &'a DbPool,
        organization_id: Uuid,
        project_id: Uuid,
        auth: &crate::editing::ServiceAuth,
        policy: ProjectReadPolicy,
    ) -> Result<Self, AppError> {
        if auth.organization_id() != organization_id {
            return Err(AppError::unauthorized("invalid service credential"));
        }
        let edit = matches!(
            policy,
            ProjectReadPolicy::UnrestrictedEdit | ProjectReadPolicy::UnrestrictedManage
        );
        let manage = matches!(policy, ProjectReadPolicy::UnrestrictedManage);
        let lock = if edit { "FOR UPDATE" } else { "FOR SHARE" };
        let mut tx = TenantTransaction::begin(
            pool,
            &TenantScope {
                organization_id,
                workspace_id: None,
                project_id: None,
                actor: ActorContext {
                    actor_type: ActorType::ServicePrincipal,
                    actor_id: auth.principal_id(),
                    credential_id: Some(auth.credential_id()),
                    email: None,
                },
                permissions: Vec::new(),
            },
        )
        .await?;

        let safe_role: bool = sqlx::query_scalar(
            "SELECT current_user = 'openvietgis_runtime'
                 AND row_security_active('spatial.layers'::regclass)
                 AND row_security_active('spatial.data_sources'::regclass)
                 AND row_security_active('spatial.source_fields'::regclass)
                 AND row_security_active('spatial.reference_features'::regclass)",
        )
        .fetch_one(&mut **tx.tx_mut())
        .await
        .map_err(db_error)?;
        if !safe_role {
            return Err(AppError::internal(
                "catalog requires the PostGIS runtime RLS role",
            ));
        }
        tx.clear_project_context().await?;

        let org_sql = format!("SELECT status FROM org.organizations WHERE id = $1 {lock}");
        let org_status: String = sqlx::query_scalar(&org_sql)
            .bind(organization_id)
            .fetch_optional(&mut **tx.tx_mut())
            .await
            .map_err(db_error)?
            .ok_or_else(|| AppError::unauthorized("invalid service credential"))?;
        if org_status != "active" {
            return Err(AppError::unauthorized("invalid service credential"));
        }

        let display_name: String = sqlx::query_scalar(
            "UPDATE org.service_credentials c
             SET last_used_at = GREATEST(c.last_used_at, clock_timestamp())
             FROM org.service_principals sp
             WHERE c.organization_id = $1 AND c.service_principal_id = $2 AND c.id = $3
               AND c.revoked_at IS NULL
               AND (c.expires_at IS NULL OR c.expires_at > clock_timestamp())
               AND sp.organization_id = c.organization_id AND sp.id = c.service_principal_id
               AND sp.status = 'active' AND sp.revoked_at IS NULL
             RETURNING sp.name",
        )
        .bind(organization_id)
        .bind(auth.principal_id())
        .bind(auth.credential_id())
        .fetch_optional(&mut **tx.tx_mut())
        .await
        .map_err(db_error)?
        .ok_or_else(|| AppError::unauthorized("invalid service credential"))?;

        let project_sql = format!(
            "SELECT status, workspace_id FROM workspace.projects
             WHERE organization_id = $1 AND id = $2 {lock}"
        );
        let project = sqlx::query_as::<_, (String, Option<Uuid>)>(&project_sql)
            .bind(organization_id)
            .bind(project_id)
            .fetch_optional(&mut **tx.tx_mut())
            .await
            .map_err(db_error)?;
        let (project_status, workspace_id) = project
            .filter(|(status, _)| matches!(status.as_str(), "active" | "archived"))
            .ok_or_else(|| AppError::not_found("project not found"))?;

        let (roles, permission) = if manage {
            ("'manager'", "project.manage")
        } else if edit {
            ("'manager', 'editor', 'contributor'", "project.edit")
        } else {
            (
                "'manager', 'editor', 'contributor', 'viewer'",
                "project.read",
            )
        };
        let permission_sql = format!(
            "SELECT EXISTS (
                 SELECT 1 FROM org.service_credentials c
                 JOIN org.service_principals sp ON sp.organization_id = c.organization_id
                      AND sp.id = c.service_principal_id
                 JOIN org.resource_grants g ON g.organization_id = sp.organization_id
                      AND g.subject_kind = 'service_principal' AND g.service_principal_id = sp.id
                 WHERE c.organization_id = $1 AND sp.id = $2 AND c.id = $5
                   AND c.revoked_at IS NULL
                   AND (c.expires_at IS NULL OR c.expires_at > clock_timestamp())
                   AND sp.status = 'active' AND sp.revoked_at IS NULL
                   AND g.revoked_at IS NULL AND g.role IN ({roles})
                   AND (c.permission_ceiling IS NULL OR $6 = ANY(c.permission_ceiling))
                   AND (g.scope_kind = 'organization'
                        OR (g.scope_kind = 'workspace' AND g.workspace_id = $3)
                        OR (g.scope_kind = 'project' AND g.project_id = $4))
             )"
        );
        let allowed = sqlx::query_scalar::<_, bool>(&permission_sql)
            .bind(organization_id)
            .bind(auth.principal_id())
            .bind(workspace_id)
            .bind(project_id)
            .bind(auth.credential_id())
            .bind(permission)
            .fetch_one(&mut **tx.tx_mut())
            .await
            .map_err(db_error)?;
        if !allowed {
            return Err(AppError::forbidden(format!(
                "missing permission: {permission}"
            )));
        }
        if edit && project_status != "active" {
            return Err(AppError::conflict(
                "PROJECT_NOT_WRITABLE",
                "The project is read-only.",
            ));
        }

        sqlx::query("SELECT set_config('openvietgis.project_id', $1, true)")
            .bind(project_id.to_string())
            .execute(&mut **tx.tx_mut())
            .await
            .map_err(db_error)?;
        Ok(Self {
            tx,
            project_id,
            gis_scoped: false,
            edit_authorized: edit,
            service_display_name: Some(display_name),
        })
    }

    async fn open(
        pool: &'a DbPool,
        organization_id: Uuid,
        project_id: Uuid,
        actor_id: Uuid,
        session_id: Uuid,
        policy: ProjectReadPolicy,
    ) -> Result<Self, AppError> {
        let edit = matches!(
            policy,
            ProjectReadPolicy::UnrestrictedEdit | ProjectReadPolicy::UnrestrictedManage
        );
        let manage = matches!(policy, ProjectReadPolicy::UnrestrictedManage);
        let lock = if edit { "FOR UPDATE" } else { "FOR SHARE" };
        if organization_id.is_nil() {
            return Err(AppError::not_found("organization not found"));
        }
        let mut tx = TenantTransaction::begin(
            pool,
            &TenantScope {
                organization_id,
                workspace_id: None,
                project_id: None,
                actor: ActorContext {
                    actor_type: ActorType::Human,
                    actor_id,
                    credential_id: None,
                    email: None,
                },
                permissions: Vec::new(),
            },
        )
        .await?;

        // The GIS restrictors are installed for the dedicated runtime role.
        // Refuse to serve data on an administrator or a role that bypasses RLS.
        let safe_role: bool = sqlx::query_scalar(
            "SELECT current_user = 'openvietgis_runtime'
                 AND row_security_active('spatial.layers'::regclass)
                 AND row_security_active('spatial.data_sources'::regclass)
                 AND row_security_active('spatial.source_fields'::regclass)
                 AND row_security_active('spatial.reference_features'::regclass)",
        )
        .fetch_one(&mut **tx.tx_mut())
        .await
        .map_err(db_error)?;
        if !safe_role {
            return Err(AppError::internal(
                "catalog requires the PostGIS runtime RLS role",
            ));
        }
        tx.clear_project_context().await?;

        let current_session: bool = sqlx::query_scalar(
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
        .fetch_one(&mut **tx.tx_mut())
        .await
        .map_err(db_error)?;
        if !current_session {
            return Err(AppError::unauthorized("authentication required"));
        }

        // Verify membership before touching the organization row. The check is
        // repeated under the org -> project locks to serialize grant revocation.
        let member: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                 SELECT 1 FROM org.organization_memberships
                 WHERE organization_id = $1 AND user_id = $2 AND status = 'active'
             )",
        )
        .bind(organization_id)
        .bind(actor_id)
        .fetch_one(&mut **tx.tx_mut())
        .await
        .map_err(db_error)?;
        if !member {
            return Err(AppError::not_found("organization not found"));
        }

        let org_sql = format!("SELECT status FROM org.organizations WHERE id = $1 {lock}");
        let org_status: String = sqlx::query_scalar(&org_sql)
            .bind(organization_id)
            .fetch_optional(&mut **tx.tx_mut())
            .await
            .map_err(db_error)?
            .ok_or_else(|| AppError::not_found("organization not found"))?;

        let project_sql = format!(
            "SELECT status, workspace_id FROM workspace.projects
             WHERE organization_id = $1 AND id = $2 {lock}"
        );
        let project = sqlx::query_as::<_, (String, Option<Uuid>)>(&project_sql)
            .bind(organization_id)
            .bind(project_id)
            .fetch_optional(&mut **tx.tx_mut())
            .await
            .map_err(db_error)?;

        let gis_scoped = sqlx::query_scalar::<_, bool>("SELECT gis_access.scoped($1, $2)")
            .bind(organization_id)
            .bind(project_id)
            .fetch_one(&mut **tx.tx_mut())
            .await
            .map_err(db_error)?;
        if gis_scoped && !matches!(policy, ProjectReadPolicy::LayerAcl) {
            return Err(AppError::forbidden(
                "Thao tác chưa được cấp quyền trong phạm vi GIS",
            ));
        }
        if org_status != "active" {
            return Err(AppError::forbidden("organization is not active"));
        }
        let (project_status, workspace_id) = project
            .filter(|(status, _)| matches!(status.as_str(), "active" | "archived"))
            .ok_or_else(|| AppError::not_found("project not found"))?;

        let (roles, permission) = if manage {
            ("'manager'", "project.manage")
        } else if edit {
            ("'manager', 'editor', 'contributor'", "project.edit")
        } else {
            (
                "'manager', 'editor', 'contributor', 'viewer'",
                "project.read",
            )
        };
        let permission_sql = format!(
            "SELECT m.role IN ('owner', 'admin') OR EXISTS (
                 SELECT 1 FROM org.resource_grants AS g
                 WHERE g.organization_id = m.organization_id
                   AND g.membership_id = m.id AND g.subject_kind = 'membership'
                   AND g.revoked_at IS NULL
                   AND g.role IN ({roles})
                   AND (
                       g.scope_kind = 'organization'
                       OR (g.scope_kind = 'workspace' AND g.workspace_id = $3)
                       OR (g.scope_kind = 'project' AND g.project_id = $4)
                   )
             )
             FROM org.organization_memberships AS m
             WHERE m.organization_id = $1 AND m.user_id = $2 AND m.status = 'active'"
        );
        let allowed = sqlx::query_scalar::<_, bool>(&permission_sql)
            .bind(organization_id)
            .bind(actor_id)
            .bind(workspace_id)
            .bind(project_id)
            .fetch_optional(&mut **tx.tx_mut())
            .await
            .map_err(db_error)?
            .ok_or_else(|| AppError::not_found("organization not found"))?;
        if !allowed {
            return Err(AppError::forbidden(format!(
                "missing permission: {permission}"
            )));
        }
        if edit && project_status != "active" {
            return Err(AppError::conflict(
                "PROJECT_NOT_WRITABLE",
                "The project is read-only.",
            ));
        }

        sqlx::query("SELECT set_config('openvietgis.project_id', $1, true)")
            .bind(project_id.to_string())
            .execute(&mut **tx.tx_mut())
            .await
            .map_err(db_error)?;
        Ok(Self {
            tx,
            project_id,
            gis_scoped,
            edit_authorized: edit,
            service_display_name: None,
        })
    }

    pub async fn list_layers(
        &mut self,
        after_id: Option<Uuid>,
        limit: usize,
    ) -> Result<Value, AppError> {
        let mut rows = self.layer_rows(None, after_id, page_size(limit)?).await?;
        let next = next_page(&mut rows, limit)?;
        let mut items = Vec::with_capacity(rows.len());
        let mut sources = HashMap::new();
        for layer in &rows {
            let source_id = layer.try_get("source_id").map_err(db_error)?;
            let source = match sources.entry(source_id) {
                std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
                std::collections::hash_map::Entry::Vacant(entry) => {
                    entry.insert(self.get_source_record(source_id).await?)
                }
            };
            items.push(self.layer_wire(layer, source).await?);
        }
        Ok(self.page(items, next))
    }

    pub async fn get_layer(&mut self, layer_id: Uuid) -> Result<Value, AppError> {
        let row = self
            .layer_rows(Some(layer_id), None, 1)
            .await?
            .pop()
            .ok_or_else(resource_not_found)?;
        let source = self
            .get_source_record(row.try_get("source_id").map_err(db_error)?)
            .await?;
        self.layer_wire(&row, &source).await
    }

    pub async fn list_sources(
        &mut self,
        after_id: Option<Uuid>,
        limit: usize,
    ) -> Result<Value, AppError> {
        self.require_unrestricted()?;
        let mut rows = self.source_rows(None, after_id, page_size(limit)?).await?;
        let next = next_page(&mut rows, limit)?;
        let mut items = Vec::with_capacity(rows.len());
        for source in &rows {
            items.push(self.source_record(source).await?.wire);
        }
        Ok(self.page(items, next))
    }

    pub async fn get_source(&mut self, source_id: Uuid) -> Result<Value, AppError> {
        self.require_unrestricted()?;
        Ok(self.get_source_record(source_id).await?.wire)
    }

    pub async fn list_changes(
        &mut self,
        after_id: Option<Uuid>,
        limit: usize,
    ) -> Result<Value, AppError> {
        self.require_unrestricted()?;
        let mut rows = sqlx::query(
            "SELECT id, resource_type, resource_id, actor_type, actor_id,
                    actor_display_snapshot, operation, created_at, audit_event_id,
                    prior_revision, new_revision, changes
             FROM spatial.catalog_changes
             WHERE organization_id = $1 AND project_id = $2
               AND ($3::uuid IS NULL OR id > $3)
             ORDER BY id LIMIT $4",
        )
        .bind(self.tx.organization_id())
        .bind(self.project_id)
        .bind(after_id)
        .bind(page_size(limit)?)
        .fetch_all(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;
        let next = next_page(&mut rows, limit)?;
        let organization_id = self.tx.organization_id();
        let mut items = Vec::with_capacity(rows.len());
        for row in &rows {
            // The canonical catalog CHECK constraints restrict revisions and
            // changed values to safe, bounded public metadata.
            items.push(json!({
                "id": row.try_get::<Uuid, _>("id").map_err(db_error)?,
                "organization_id": organization_id,
                "project_id": self.project_id,
                "resource_type": row.try_get::<String, _>("resource_type").map_err(db_error)?,
                "resource_id": row.try_get::<Uuid, _>("resource_id").map_err(db_error)?,
                "actor_type": row.try_get::<String, _>("actor_type").map_err(db_error)?,
                "actor_id": row.try_get::<Uuid, _>("actor_id").map_err(db_error)?,
                "actor_display_snapshot": row
                    .try_get::<String, _>("actor_display_snapshot")
                    .map_err(db_error)?,
                "operation": row.try_get::<String, _>("operation").map_err(db_error)?,
                "created_at": timestamp(row, "created_at")?,
                "audit_event_id": row.try_get::<Uuid, _>("audit_event_id").map_err(db_error)?,
                "prior_revision": row.try_get::<Option<Value>, _>("prior_revision").map_err(db_error)?,
                "new_revision": row.try_get::<Value, _>("new_revision").map_err(db_error)?,
                "changes": row.try_get::<Value, _>("changes").map_err(db_error)?,
            }));
        }
        Ok(self.page(items, next))
    }

    /// Candidate replay of project-wide spatial invalidations. The caller must
    /// obtain a fresh ProjectRead for every page (including reconnect).
    pub async fn replay_spatial_invalidations(
        &mut self,
        cursor: Option<crate::e11_outbox::SpatialCursor>,
        limit: usize,
    ) -> Result<crate::e11_outbox::SpatialReplay, AppError> {
        self.require_unrestricted()?;
        let organization = self.tx.organization_id();
        crate::e11_outbox::replay(
            self.tx.tx_mut(),
            organization,
            self.project_id,
            cursor,
            limit,
        )
        .await
    }

    /// Map configuration is filtered through the same layer RLS as catalog reads.
    pub async fn get_map(&mut self) -> Result<Value, AppError> {
        let org = self.tx.organization_id();
        let row: Option<(Uuid,i64,Value)>=sqlx::query_as(
            "SELECT id, configuration_revision, configuration FROM maps.project_maps WHERE organization_id=$1 AND project_id=$2"
        ).bind(org).bind(self.project_id).fetch_optional(&mut **self.tx.tx_mut()).await.map_err(db_error)?;
        let (id, revision, mut config) = row.unwrap_or_else(|| {
            (
                map_identity(self.project_id),
                0,
                json!({"layers":[],"basemap":"grid","default_viewpoint":null}),
            )
        });
        if id != map_identity(self.project_id) {
            return Err(AppError::internal("invalid persisted map identity"));
        }
        normalize_map_configuration(&mut config)?;
        if self.gis_scoped {
            let allowed: HashSet<Uuid> = sqlx::query_scalar::<_, Uuid>(
                "SELECT id FROM spatial.layers WHERE organization_id=$1 AND project_id=$2",
            )
            .bind(org)
            .bind(self.project_id)
            .fetch_all(&mut **self.tx.tx_mut())
            .await
            .map_err(db_error)?
            .into_iter()
            .collect();
            let layers = config["layers"]
                .as_array_mut()
                .ok_or_else(|| AppError::internal("invalid map layers"))?;
            layers.retain(|layer| {
                layer["layer_id"]
                    .as_str()
                    .and_then(|v| Uuid::parse_str(v).ok())
                    .is_some_and(|id| allowed.contains(&id))
            });
            config["default_viewpoint"] = Value::Null;
        }
        Ok(
            json!({"map_id":id,"organization_id":org,"project_id":self.project_id,"configuration_revision":revision,"configuration":config}),
        )
    }

    pub async fn map_history(
        &mut self,
        after: Option<i64>,
        limit: usize,
    ) -> Result<Value, AppError> {
        self.require_unrestricted()?;
        if !(1..=100).contains(&limit)
            || after.is_some_and(|value| !(1..=9_007_199_254_740_991).contains(&value))
        {
            return Err(AppError::validation(
                "INVALID_MAP_HISTORY",
                "invalid map history",
            ));
        }
        let rows=sqlx::query(
            "SELECT map_id,operation_id,configuration_revision,prior_revision,actor_type,actor_id,actor_display_snapshot,created_at,before_configuration,after_configuration
             FROM maps.map_revisions WHERE organization_id=$1 AND project_id=$2
             AND ($3::bigint IS NULL OR configuration_revision<$3) ORDER BY configuration_revision DESC LIMIT $4"
        ).bind(self.tx.organization_id()).bind(self.project_id).bind(after).bind((limit+1) as i64)
        .fetch_all(&mut **self.tx.tx_mut()).await.map_err(db_error)?;
        let mut items = Vec::with_capacity(limit);
        for row in rows.iter().take(limit) {
            let mut before: Option<Value> =
                row.try_get("before_configuration").map_err(db_error)?;
            let mut after: Value = row.try_get("after_configuration").map_err(db_error)?;
            if let Some(value) = &mut before {
                normalize_map_configuration(value)?;
            }
            normalize_map_configuration(&mut after)?;
            items.push(json!({"map_id":row.try_get::<Uuid,_>("map_id").map_err(db_error)?,
                "operation_id":row.try_get::<Uuid,_>("operation_id").map_err(db_error)?,
                "configuration_revision":row.try_get::<i64,_>("configuration_revision").map_err(db_error)?,
                "prior_revision":row.try_get::<i64,_>("prior_revision").map_err(db_error)?,
                "actor_type":row.try_get::<String,_>("actor_type").map_err(db_error)?,
                "actor_id":row.try_get::<Uuid,_>("actor_id").map_err(db_error)?,
                "actor_display_snapshot":row.try_get::<String,_>("actor_display_snapshot").map_err(db_error)?,
                "created_at":timestamp(row,"created_at")?,"before_configuration":before,"after_configuration":after}));
        }
        let next = if rows.len() > limit {
            items
                .last()
                .and_then(|item| item["configuration_revision"].as_i64())
        } else {
            None
        };
        Ok(json!({"items":items,"next_after_revision":next}))
    }

    pub async fn map_operation(&mut self, operation_id: Uuid) -> Result<Value, AppError> {
        self.require_unrestricted()?;
        if !self.edit_authorized {
            return Err(AppError::forbidden("missing permission: project.edit"));
        }
        let (map_id, revision): (Uuid,i64)=sqlx::query_as(
            "SELECT map_id,committed_revision FROM maps.map_operations WHERE organization_id=$1 AND project_id=$2 AND operation_id=$3 AND actor_type='human' AND actor_id=$4"
        ).bind(self.tx.organization_id()).bind(self.project_id).bind(operation_id).bind(self.tx.actor_id())
        .fetch_optional(&mut **self.tx.tx_mut()).await.map_err(db_error)?.ok_or_else(resource_not_found)?;
        Ok(json!({"map_id":map_id,"operation_id":operation_id,"configuration_revision":revision}))
    }

    pub async fn commit(self) -> Result<(), AppError> {
        self.tx.commit().await
    }

    fn require_unrestricted(&self) -> Result<(), AppError> {
        if self.gis_scoped {
            return Err(AppError::forbidden(
                "Thao tác chưa được cấp quyền trong phạm vi GIS",
            ));
        }
        Ok(())
    }

    fn page(&self, items: Vec<Value>, next_after_id: Option<Uuid>) -> Value {
        json!({
            "organization_id": self.tx.organization_id(),
            "project_id": self.project_id,
            "items": items,
            "next_after_id": next_after_id,
        })
    }

    async fn layer_rows(
        &mut self,
        id: Option<Uuid>,
        after_id: Option<Uuid>,
        limit: i64,
    ) -> Result<Vec<PgRow>, AppError> {
        sqlx::query(
            "SELECT id, source_id, name, lifecycle, layer_revision, created_at, updated_at, retired_at
             FROM spatial.layers
             WHERE organization_id = $1 AND project_id = $2 AND lifecycle = 'active'
               AND ($3::uuid IS NULL OR id = $3)
               AND ($4::uuid IS NULL OR id > $4)
             ORDER BY id LIMIT $5",
        )
        .bind(self.tx.organization_id())
        .bind(self.project_id)
        .bind(id)
        .bind(after_id)
        .bind(limit)
        .fetch_all(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)
    }

    async fn source_rows(
        &mut self,
        id: Option<Uuid>,
        after_id: Option<Uuid>,
        limit: i64,
    ) -> Result<Vec<PgRow>, AppError> {
        sqlx::query(
            "SELECT id, name, kind, reference_key, fixture_version, manifest_digest, source_asset_id,
                    lifecycle, validation_state, validation_reason, source_crs, stored_crs,
                    schema_state, geometry_family, dimensions, feature_count_state,
                    feature_count::bigint AS feature_count,
                    extent_state, extent, catalog_revision, content_revision, schema_revision,
                    created_at, updated_at, retired_at
             FROM spatial.data_sources
             WHERE organization_id = $1 AND project_id = $2 AND lifecycle = 'active'
               AND ($3::uuid IS NULL OR id = $3)
               AND ($4::uuid IS NULL OR id > $4)
             ORDER BY id LIMIT $5 FOR SHARE",
        )
        .bind(self.tx.organization_id())
        .bind(self.project_id)
        .bind(id)
        .bind(after_id)
        .bind(limit)
        .fetch_all(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)
    }

    pub(crate) async fn get_source_record(
        &mut self,
        source_id: Uuid,
    ) -> Result<SourceRecord, AppError> {
        let row = self
            .source_rows(Some(source_id), None, 1)
            .await?
            .pop()
            .ok_or_else(resource_not_found)?;
        self.source_record(&row).await
    }

    async fn source_record(&mut self, row: &PgRow) -> Result<SourceRecord, AppError> {
        let id: Uuid = row.try_get("id").map_err(db_error)?;
        let schema_revision: String = row.try_get("schema_revision").map_err(db_error)?;
        let geometry_family: String = row.try_get("geometry_family").map_err(db_error)?;
        let dimensions: String = row.try_get("dimensions").map_err(db_error)?;
        let schema_state: String = row.try_get("schema_state").map_err(db_error)?;
        let rows = sqlx::query(
            "SELECT field_id, name, ordinal, type, nullable, schema_revision
             FROM spatial.source_fields
             WHERE organization_id = $1 AND project_id = $2 AND source_id = $3
             ORDER BY ordinal LIMIT 101",
        )
        .bind(self.tx.organization_id())
        .bind(self.project_id)
        .bind(id)
        .fetch_all(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;
        let mut fields = Vec::with_capacity(rows.len());
        for field in rows {
            let revision: &str = field.try_get("schema_revision").map_err(db_error)?;
            if revision != schema_revision {
                return Err(invalid_schema());
            }
            fields.push(Field {
                field_id: field.try_get("field_id").map_err(db_error)?,
                name: field.try_get("name").map_err(db_error)?,
                nullable: field.try_get("nullable").map_err(db_error)?,
                ordinal: field.try_get::<i32, _>("ordinal").map_err(db_error)?,
                field_type: field.try_get("type").map_err(db_error)?,
            });
        }
        if schema_digest(&fields, &dimensions, &geometry_family, &schema_state)? != schema_revision
        {
            return Err(invalid_schema());
        }

        let kind: &str = row.try_get("kind").map_err(db_error)?;
        let reference_key: Option<&str> = row.try_get("reference_key").map_err(db_error)?;
        let stored_crs: Option<i32> = row.try_get("stored_crs").map_err(db_error)?;
        let source_crs: Option<i32> = row.try_get("source_crs").map_err(db_error)?;
        let extent: Option<Value> = row.try_get("extent").map_err(db_error)?;
        let count: Option<i64> = row.try_get("feature_count").map_err(db_error)?;
        let validation_state: &str = row.try_get("validation_state").map_err(db_error)?;

        let render_supported = render_supported(
            kind,
            reference_key,
            stored_crs,
            &geometry_family,
            &dimensions,
            count,
            extent.as_ref(),
        );
        let wire = json!({
            "id": id,
            "organization_id": self.tx.organization_id(),
            "project_id": self.project_id,
            "name": row.try_get::<&str, _>("name").map_err(db_error)?,
            "kind": kind,
            "provenance": {
                "reference_key": reference_key,
                "fixture_version": row.try_get::<Option<&str>, _>("fixture_version").map_err(db_error)?,
                "manifest_digest": row.try_get::<Option<&str>, _>("manifest_digest").map_err(db_error)?,
                "source_asset_id": row.try_get::<Option<Uuid>, _>("source_asset_id").map_err(db_error)?,
            },
            "lifecycle": row.try_get::<&str, _>("lifecycle").map_err(db_error)?,
            "validation_state": validation_state,
            "validation_reason": row.try_get::<Option<Value>, _>("validation_reason").map_err(db_error)?,
            "source_crs": profile_wire(source_crs)?,
            "stored_crs": profile_wire(stored_crs)?,
            "schema": {
                "schema_revision": schema_revision,
                "fields": fields,
                "geometry_family": geometry_family,
                "dimensions": dimensions,
                "state": schema_state,
            },
            "feature_count_state": row.try_get::<&str, _>("feature_count_state").map_err(db_error)?,
            "feature_count": count,
            "extent_state": row.try_get::<&str, _>("extent_state").map_err(db_error)?,
            "extent": extent_wire(extent)?,
            "catalog_revision": row.try_get::<i64, _>("catalog_revision").map_err(db_error)?,
            "content_revision": row.try_get::<&str, _>("content_revision").map_err(db_error)?,
            "schema_revision": schema_revision,
            "created_at": timestamp(row, "created_at")?,
            "updated_at": timestamp(row, "updated_at")?,
            "retired_at": timestamp(row, "retired_at")?,
        });
        Ok(SourceRecord {
            wire,
            ready: validation_state == "ready",
            render_supported,
            edit_supported: render_supported && kind != "reference_fixture",
        })
    }

    async fn layer_wire(&mut self, row: &PgRow, record: &SourceRecord) -> Result<Value, AppError> {
        let source_id: Uuid = row.try_get("source_id").map_err(db_error)?;
        let mut source = record.wire.clone();
        let mut actions = None;
        if self.gis_scoped {
            // Feature counts must obey the same object RLS as feature reads.
            let count: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM spatial.reference_features
                 WHERE organization_id = $1 AND project_id = $2 AND source_id = $3",
            )
            .bind(self.tx.organization_id())
            .bind(self.project_id)
            .bind(source_id)
            .fetch_one(&mut **self.tx.tx_mut())
            .await
            .map_err(db_error)?;
            source["feature_count"] = json!(count);
            source["extent"] = Value::Null;
            source["extent_state"] = json!("unavailable");
            source["provenance"] = json!({
                "reference_key": null,
                "fixture_version": null,
                "manifest_digest": null,
                "source_asset_id": null,
            });
            actions = Some(self.allowed_actions(source_id).await?);
        }

        let mut wire = json!({
            "id": row.try_get::<Uuid, _>("id").map_err(db_error)?,
            "organization_id": self.tx.organization_id(),
            "project_id": self.project_id,
            "name": row.try_get::<&str, _>("name").map_err(db_error)?,
            "source_id": source_id,
            "lifecycle": row.try_get::<&str, _>("lifecycle").map_err(db_error)?,
            "layer_revision": row.try_get::<i64, _>("layer_revision").map_err(db_error)?,
            "source": source,
            "capabilities": record.capabilities(actions.as_deref()),
            "created_at": timestamp(row, "created_at")?,
            "updated_at": timestamp(row, "updated_at")?,
            "retired_at": timestamp(row, "retired_at")?,
        });
        if let Some(actions) = actions {
            wire["access_actions"] = json!(actions);
        }
        Ok(wire)
    }

    async fn allowed_actions(&mut self, source_id: Uuid) -> Result<Vec<String>, AppError> {
        sqlx::query_scalar(
            "SELECT a.action
             FROM unnest(ARRAY['read', 'create', 'geometry', 'attributes', 'delete', 'export',
                               'approve']) WITH ORDINALITY AS a(action, ordinal)
             WHERE gis_access.source_allowed($1, $2, $3, a.action)
             ORDER BY a.ordinal",
        )
        .bind(self.tx.organization_id())
        .bind(self.project_id)
        .bind(source_id)
        .fetch_all(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)
    }
}

pub(crate) struct SourceRecord {
    pub(crate) wire: Value,
    ready: bool,
    render_supported: bool,
    edit_supported: bool,
}

impl SourceRecord {
    fn capabilities(&self, actions: Option<&[String]>) -> Vec<Value> {
        let can_edit = actions.is_none_or(|actions| {
            actions.iter().any(|action| {
                matches!(
                    action.as_str(),
                    "create" | "geometry" | "attributes" | "delete"
                )
            })
        });
        [
            ("metadata_read", true, true, "project.read"),
            ("feature_read", true, self.ready, "project.read"),
            ("render", self.render_supported, self.ready, "project.read"),
            ("style", self.render_supported, self.ready, "project.read"),
            ("export", self.render_supported, self.ready, "project.read"),
            ("query", self.render_supported, self.ready, "project.read"),
            ("edit", self.edit_supported, self.ready, "project.edit"),
            (
                "analysis",
                self.render_supported,
                self.ready,
                "project.edit",
            ),
            ("offline", self.render_supported, self.ready, "project.read"),
        ]
        .into_iter()
        .map(|(name, supported, ready, permission)| {
            let authorized = match name {
                "feature_read" => true,
                "edit" => can_edit,
                _ => actions.is_none(),
            };
            json!({
                "name": name,
                "technical_supported": supported,
                "available": supported && ready,
                "required_permission": supported.then_some(permission),
                "actor_authorized": supported && authorized,
            })
        })
        .collect()
    }
}

/// Field order in this struct is also the canonical schema-revision key order.
#[derive(Serialize)]
struct Field {
    field_id: String,
    name: String,
    nullable: bool,
    ordinal: i32,
    #[serde(rename = "type")]
    field_type: String,
}

fn schema_digest(
    fields: &[Field],
    dimensions: &str,
    geometry_family: &str,
    schema_state: &str,
) -> Result<String, AppError> {
    let mut ordered: Vec<_> = fields.iter().collect();
    ordered.sort_by(|a, b| a.field_id.cmp(&b.field_id));
    let ordinals: HashSet<_> = fields.iter().map(|field| field.ordinal).collect();
    if fields.len() > 100
        || (schema_state == "unavailable" && !fields.is_empty())
        || ordered
            .windows(2)
            .any(|pair| pair[0].field_id == pair[1].field_id)
        || ordinals.len() != fields.len()
    {
        return Err(invalid_schema());
    }
    #[derive(Serialize)]
    struct Manifest<'a> {
        dimensions: &'a str,
        fields: Vec<&'a Field>,
        geometry_family: &'a str,
        manifest_type: &'static str,
        schema_state: &'a str,
    }
    let canonical = serde_json::to_string(&Manifest {
        dimensions,
        fields: ordered,
        geometry_family,
        manifest_type: "r003-schema-v1",
        schema_state,
    })
    .map_err(|_| invalid_schema())?;

    // Match Python's ensure_ascii=True including astral-plane surrogate pairs.
    let mut ascii = String::with_capacity(canonical.len());
    for c in canonical.chars() {
        if c < '\u{7f}' {
            ascii.push(c);
        } else {
            for code in c.encode_utf16(&mut [0; 2]) {
                write!(ascii, "\\u{code:04x}").map_err(|_| invalid_schema())?;
            }
        }
    }
    Ok(format!("{:x}", Sha256::digest(ascii.as_bytes())))
}

fn render_supported(
    kind: &str,
    reference_key: Option<&str>,
    crs: Option<i32>,
    geometry_family: &str,
    dimensions: &str,
    feature_count: Option<i64>,
    extent: Option<&Value>,
) -> bool {
    let regional_provenance = kind == "imported_file"
        || !matches!(crs, Some(4756 | 5899))
        || matches!(
            reference_key,
            Some("reference_points_4756" | "reference_points_4756_hvs" | "reference_points_5899")
        );
    let display_extent = crs != Some(4326)
        || feature_count == Some(0)
        || extent
            .and_then(|e| Some((e.get("min_y")?.as_f64()?, e.get("max_y")?.as_f64()?)))
            .is_some_and(|(min, max)| {
                use ovg_core::crs::WEB_MERCATOR_MAX_LATITUDE;
                min >= -WEB_MERCATOR_MAX_LATITUDE && min <= max && max <= WEB_MERCATOR_MAX_LATITUDE
            });
    matches!(
        kind,
        "reference_fixture" | "imported_file" | "derived_processing" | "manual"
    ) && matches!(geometry_family, "Point" | "LineString" | "Polygon")
        && dimensions == "XY"
        && feature_count.is_some_and(|count| count <= 50_000)
        && matches!(crs, Some(4326 | 3857 | 4756 | 5899))
        && regional_provenance
        && display_extent
}

fn profile_wire(crs: Option<i32>) -> Result<Option<Value>, AppError> {
    crs.map(|code| {
        u32::try_from(code)
            .ok()
            .and_then(reference_profile_wire)
            .ok_or_else(|| AppError::internal("unrecognized persisted CRS definition"))
    })
    .transpose()
}

fn extent_wire(extent: Option<Value>) -> Result<Option<Value>, AppError> {
    extent
        .map(|e| {
            let reference = e
                .get("crs_code")
                .and_then(Value::as_u64)
                .and_then(|code| u32::try_from(code).ok())
                .and_then(reference_coordinate_wire)
                .ok_or_else(|| AppError::internal("persisted extent has no readable CRS"))?;
            Ok(json!({
                "min_x": e["min_x"],
                "min_y": e["min_y"],
                "max_x": e["max_x"],
                "max_y": e["max_y"],
                "coordinate_reference": reference,
            }))
        })
        .transpose()
}

/// RFC 4122 UUIDv5, using the Project UUID as namespace, as in Python.
fn map_identity(project_id: Uuid) -> Uuid {
    let mut hasher = sha1::Sha1::new();
    hasher.update(project_id.as_bytes());
    hasher.update(b"openvietgis:map");
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&hasher.finalize()[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

fn normalize_map_configuration(config: &mut Value) -> Result<(), AppError> {
    let invalid = || AppError::internal("invalid persisted map configuration");
    let layers = config
        .get_mut("layers")
        .and_then(Value::as_array_mut)
        .ok_or_else(invalid)?;
    for layer in layers {
        let style = layer
            .get_mut("style")
            .and_then(Value::as_object_mut)
            .ok_or_else(invalid)?;
        for field in ["opacity", "width", "radius"] {
            let n = style
                .get(field)
                .and_then(Value::as_f64)
                .ok_or_else(invalid)?;
            style.insert(field.to_owned(), json!(n));
        }
        if layer.get("group").is_some_and(Value::is_null) {
            layer.as_object_mut().ok_or_else(invalid)?.remove("group");
        }
        if layer.get("display").is_some_and(Value::is_null) {
            layer.as_object_mut().ok_or_else(invalid)?.remove("display");
        }
    }
    if let Some(viewpoint) = config
        .get_mut("default_viewpoint")
        .and_then(Value::as_object_mut)
    {
        for field in ["longitude", "latitude", "zoom", "bearing", "pitch"] {
            let n = viewpoint
                .get(field)
                .and_then(Value::as_f64)
                .ok_or_else(invalid)?;
            viewpoint.insert(field.to_owned(), json!(n));
        }
    }
    Ok(())
}

fn timestamp(row: &PgRow, key: &str) -> Result<Option<String>, AppError> {
    let value: Option<DateTime<Utc>> = row.try_get(key).map_err(db_error)?;
    Ok(value.map(|ts| {
        let format = if ts.timestamp_subsec_micros() == 0 {
            SecondsFormat::Secs
        } else {
            SecondsFormat::Micros
        };
        ts.to_rfc3339_opts(format, true)
    }))
}

fn page_size(limit: usize) -> Result<i64, AppError> {
    if !(1..=100).contains(&limit) {
        return Err(AppError::validation(
            "INVALID_CATALOG_PAGE",
            "invalid catalog limit",
        ));
    }
    Ok(limit as i64 + 1)
}

fn next_page(rows: &mut Vec<PgRow>, limit: usize) -> Result<Option<Uuid>, AppError> {
    if rows.len() <= limit {
        return Ok(None);
    }
    rows.truncate(limit);
    rows.last()
        .map(|row| row.try_get("id"))
        .transpose()
        .map_err(db_error)
}

fn resource_not_found() -> AppError {
    AppError::not_found("resource not found")
}

fn invalid_schema() -> AppError {
    AppError::internal("catalog schema revision integrity check failed")
}

pub(crate) fn db_error(error: sqlx::Error) -> AppError {
    AppError::internal(format!("catalog database operation failed: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_digest_matches_python_sorted_ascii_manifest() {
        let fields = [
            Field {
                field_id: "z_example".into(),
                name: "Mã điểm".into(),
                nullable: false,
                ordinal: 0,
                field_type: "integer".into(),
            },
            Field {
                field_id: "a_example".into(),
                name: "Địa chỉ 😀".into(),
                nullable: true,
                ordinal: 1,
                field_type: "text".into(),
            },
        ];
        assert_eq!(
            schema_digest(&fields, "XY", "Point", "known").unwrap(),
            "d1d7799610ab050af49d6429e328d3e5bfbff4aa2450c5f23a68a61ed2deee6a",
        );
    }
}
