//! R006 editing persistence over the canonical schema, mirroring
//! `adapters/persistence/editing_repository.py` statement for statement:
//! optimistic content revisions, operation-ID idempotency bound to the audit
//! request digest, immutable changesets and inverse-changeset revert.

// Borrowed binds keep the long statement sequences readable.
#![allow(clippy::needless_borrows_for_generic_args)]

pub mod domain;

use std::collections::HashSet;

use chrono::{DateTime, SubsecRound, Utc};
use ovg_core::{ActorContext, ActorType, TenantScope};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use sqlx::postgres::PgRow;
use sqlx::Row;
use uuid::Uuid;

use domain::{
    geometry_wkt, py_eq, py_strip, redact_audit_value, request_digest, typed_attributes,
    validate_feature_id, validate_operation_id, FieldDef,
};
pub use domain::{snap, EditError, EditOperation, OpType};

use crate::{DbPool, TenantTransaction};

/// Route behavior of FastAPI `project_access_guard` for a scoped (GIS ACL) account.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Guard {
    /// GET: `FOR SHARE` locks; scoped accounts denied.
    Read,
    /// POST in `SAFE_POST`: `FOR UPDATE` locks; scoped accounts continue.
    ScopedAllowed,
    /// Other POST: `FOR UPDATE` locks; scoped accounts denied.
    ScopedDenied,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    Read,
    Edit,
}

/// Public `CommitChangesetRequest` body (extra fields forbidden).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommitRequest {
    pub operation_id: String,
    pub base_content_revision: String,
    #[serde(default)]
    pub base_schema_revision: Option<String>,
    #[serde(default = "default_summary")]
    pub summary: String,
    pub operations: Vec<EditOperation>,
}

fn default_summary() -> String {
    "Feature edit".to_owned()
}

const GIS_SCOPE_DENIED: &str = "Thao tác chưa được cấp quyền trong phạm vi GIS";

const CHANGESET_COLUMNS: &str = "cs.id, cs.organization_id, cs.project_id, cs.source_id, \
    cs.actor_type, cs.actor_id, cs.actor_display_snapshot, cs.operation_id, \
    cs.base_content_revision, cs.resulting_content_revision, cs.base_catalog_revision, \
    cs.resulting_catalog_revision, cs.summary, cs.status, cs.operation_count, \
    cs.audit_event_id, cs.reverted_from_changeset_id, cs.created_at";

/// One request transaction holding the project guard locks and RLS scope.
pub struct EditScope<'a> {
    tx: TenantTransaction<'a>,
    project_id: Uuid,
    actor_id: Uuid,
    display_name: String,
    scoped: bool,
    organization_status: Option<String>,
    project: Option<(String, Option<Uuid>)>,
}

struct Prepared {
    op_type: OpType,
    feature_id: String,
    before_wkb: Option<Vec<u8>>,
    after_wkb: Option<Vec<u8>>,
    before_props: Option<Value>,
    after_props: Option<Value>,
}

impl<'a> EditScope<'a> {
    /// `project_access_guard`: membership, organization -> project row locks and
    /// GIS scope, before CSRF and body validation run.
    pub async fn begin(
        pool: &'a DbPool,
        organization_id: Uuid,
        project_id: Uuid,
        actor_id: Uuid,
        session_id: Uuid,
        guard: Guard,
    ) -> Result<Self, EditError> {
        if organization_id.is_nil() {
            return Err(EditError::detail(404, "organization not found"));
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
        .await
        .map_err(|e| EditError::Internal(e.to_string()))?;

        let safe_role: bool = sqlx::query_scalar(
            "SELECT current_user = 'openvietgis_runtime'
                 AND row_security_active('spatial.data_sources'::regclass)
                 AND row_security_active('spatial.reference_features'::regclass)
                 AND row_security_active('editing.changesets'::regclass)
                 AND row_security_active('editing.changeset_operations'::regclass)",
        )
        .fetch_one(&mut **tx.tx_mut())
        .await
        .map_err(db_error)?;
        if !safe_role {
            return Err(EditError::Internal(
                "editing requires the PostGIS runtime RLS role".into(),
            ));
        }
        tx.clear_project_context()
            .await
            .map_err(|e| EditError::Internal(e.to_string()))?;

        let display_name: String = sqlx::query_scalar(
            "SELECT u.display_name
             FROM identity.web_sessions AS s
             JOIN identity.users AS u ON u.id = s.user_id
             JOIN identity.identity_links AS l ON l.id = s.identity_link_id AND l.user_id = u.id
             WHERE s.id = $1 AND u.id = $2 AND u.status = 'active'
               AND s.revoked_at IS NULL AND s.expires_at > clock_timestamp()",
        )
        .bind(session_id)
        .bind(actor_id)
        .fetch_optional(&mut **tx.tx_mut())
        .await
        .map_err(db_error)?
        .ok_or_else(|| EditError::detail(401, "authentication required"))?;

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
            return Err(EditError::detail(404, "organization not found"));
        }

        let lock = if guard == Guard::Read {
            "FOR SHARE"
        } else {
            "FOR UPDATE"
        };
        let organization_status: Option<String> = sqlx::query_scalar(&format!(
            "SELECT status FROM org.organizations WHERE id = $1 {lock}"
        ))
        .bind(organization_id)
        .fetch_optional(&mut **tx.tx_mut())
        .await
        .map_err(db_error)?;
        let project = sqlx::query_as::<_, (String, Option<Uuid>)>(&format!(
            "SELECT status, workspace_id FROM workspace.projects
             WHERE organization_id = $1 AND id = $2 {lock}"
        ))
        .bind(organization_id)
        .bind(project_id)
        .fetch_optional(&mut **tx.tx_mut())
        .await
        .map_err(db_error)?;
        let scoped: bool = sqlx::query_scalar("SELECT gis_access.scoped($1, $2)")
            .bind(organization_id)
            .bind(project_id)
            .fetch_one(&mut **tx.tx_mut())
            .await
            .map_err(db_error)?;
        if scoped && guard != Guard::ScopedAllowed {
            return Err(EditError::detail(403, GIS_SCOPE_DENIED));
        }
        Ok(Self {
            tx,
            project_id,
            actor_id,
            display_name,
            scoped,
            organization_status,
            project,
        })
    }

    /// `prepare_project_scope`: lifecycle, live grants and project writability.
    pub async fn authorize(&mut self, permission: Permission) -> Result<(), EditError> {
        let organization_id = self.tx.organization_id();
        match self.organization_status.as_deref() {
            None => return Err(EditError::detail(404, "organization not found")),
            Some("active") => {}
            Some(_) => return Err(EditError::detail(403, "organization is not active")),
        }
        let (status, workspace_id) = self
            .project
            .clone()
            .filter(|(status, _)| matches!(status.as_str(), "active" | "archived"))
            .ok_or_else(|| EditError::detail(404, "project not found"))?;
        let roles: &[&str] = match permission {
            Permission::Read => &["manager", "editor", "contributor", "viewer"],
            Permission::Edit => &["manager", "editor", "contributor"],
        };
        let granted = sqlx::query_scalar::<_, bool>(
            "SELECT m.role IN ('owner', 'admin') OR EXISTS (
                 SELECT 1 FROM org.resource_grants AS g
                 WHERE g.organization_id = m.organization_id
                   AND g.membership_id = m.id AND g.subject_kind = 'membership'
                   AND g.revoked_at IS NULL
                   AND g.role = ANY($5)
                   AND (
                       g.scope_kind = 'organization'
                       OR (g.scope_kind = 'workspace' AND g.workspace_id = $3)
                       OR (g.scope_kind = 'project' AND g.project_id = $4)
                   )
             )
             FROM org.organization_memberships AS m
             WHERE m.organization_id = $1 AND m.user_id = $2 AND m.status = 'active'",
        )
        .bind(organization_id)
        .bind(self.actor_id)
        .bind(workspace_id)
        .bind(self.project_id)
        .bind(roles)
        .fetch_optional(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?
        .ok_or_else(|| EditError::detail(404, "organization not found"))?;
        if !granted {
            let name = match permission {
                Permission::Read => "project.read",
                Permission::Edit => "project.edit",
            };
            return Err(EditError::detail(403, format!("missing permission: {name}")));
        }
        if permission == Permission::Edit && status != "active" {
            return Err(EditError::Spatial(
                409,
                "PROJECT_NOT_WRITABLE",
                "The project is read-only.",
            ));
        }
        sqlx::query("SELECT set_config('openvietgis.project_id', $1, true)")
            .bind(self.project_id.to_string())
            .execute(&mut **self.tx.tx_mut())
            .await
            .map_err(db_error)?;
        Ok(())
    }

    pub async fn commit(self) -> Result<(), EditError> {
        self.tx
            .commit()
            .await
            .map_err(|e| EditError::Internal(e.to_string()))
    }

    pub async fn list_changesets(
        &mut self,
        source_id: Uuid,
        limit: i64,
    ) -> Result<Value, EditError> {
        self.authorize(Permission::Read).await?;
        let rows = sqlx::query(&format!(
            "SELECT {CHANGESET_COLUMNS} FROM editing.changesets AS cs
             WHERE cs.organization_id = $1 AND cs.project_id = $2 AND cs.source_id = $3
             ORDER BY cs.created_at DESC, cs.resulting_catalog_revision DESC
             LIMIT $4"
        ))
        .bind(self.tx.organization_id())
        .bind(self.project_id)
        .bind(source_id)
        .bind(limit.clamp(1, 200))
        .fetch_all(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;
        let items = rows
            .iter()
            .map(|row| changeset_wire(row, Vec::new()))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(json!({ "items": items }))
    }

    pub async fn get_changeset(&mut self, changeset_id: Uuid) -> Result<Value, EditError> {
        self.authorize(Permission::Read).await?;
        let row = sqlx::query(&format!(
            "SELECT {CHANGESET_COLUMNS} FROM editing.changesets AS cs
             WHERE cs.organization_id = $1 AND cs.project_id = $2 AND cs.id = $3"
        ))
        .bind(self.tx.organization_id())
        .bind(self.project_id)
        .bind(changeset_id)
        .fetch_optional(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?
        .ok_or_else(|| EditError::not_found("changeset not found"))?;
        let operations = self.load_operations(changeset_id).await?;
        changeset_wire(&row, operations)
    }

    pub async fn commit_changeset(
        &mut self,
        source_id: Uuid,
        request: CommitRequest,
    ) -> Result<Value, EditError> {
        self.commit_inner(source_id, request, None).await
    }

    /// Commit the inverse of the latest changeset as a new `reverted` changeset.
    pub async fn revert_changeset(
        &mut self,
        changeset_id: Uuid,
        operation_id: String,
        summary: Option<String>,
    ) -> Result<Value, EditError> {
        let target = self.get_changeset(changeset_id).await?;
        let object = |v: &Value| v.as_object().cloned();
        let mut operations = Vec::new();
        for op in target["operations"].as_array().into_iter().flatten().rev() {
            let feature_id = op["feature_id"].as_str().unwrap_or_default().to_owned();
            let (op_type, geometry, properties) = match op["op_type"].as_str() {
                Some("create") => (OpType::Delete, None, None),
                Some("delete") => (
                    OpType::Create,
                    object(&op["before_geometry"]),
                    object(&op["before_properties"]),
                ),
                _ => (
                    OpType::Update,
                    object(&op["before_geometry"]),
                    object(&op["before_properties"]),
                ),
            };
            operations.push(EditOperation {
                op_type,
                feature_id,
                geometry,
                properties,
            });
        }
        let source_id = target["source_id"]
            .as_str()
            .and_then(|s| Uuid::parse_str(s).ok())
            .ok_or_else(|| EditError::Internal("changeset without source".into()))?;
        let request = CommitRequest {
            operation_id,
            base_content_revision: target["resulting_content_revision"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            base_schema_revision: None,
            summary: summary
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| format!("Revert changeset {changeset_id}")),
            operations,
        };
        self.commit_inner(source_id, request, Some(changeset_id))
            .await
    }

    async fn commit_inner(
        &mut self,
        source_id: Uuid,
        request: CommitRequest,
        reverted_from: Option<Uuid>,
    ) -> Result<Value, EditError> {
        self.authorize(Permission::Edit).await?;
        let org = self.tx.organization_id();
        let proj = self.project_id;
        if self.scoped {
            // shortcut: scoped (GIS ACL) edits need `openvietgis.acl_internal`, which request
            // paths must never set; they fail closed until a reviewed ACL edit adapter exists.
            return Err(EditError::forbidden(
                "GIS_ACCESS_DENIED",
                "Đối tượng hoặc thao tác nằm ngoài quyền được cấp",
            ));
        }
        let op_id = validate_operation_id(&request.operation_id)?;
        let summary: String = py_strip(&request.summary).chars().take(500).collect();
        let summary = if summary.is_empty() {
            "Feature changeset".to_owned()
        } else {
            summary
        };
        if request.operations.len() > 2000 {
            return Err(EditError::validation(
                "CHANGESET_TOO_LARGE",
                "changeset exceeds maximum 2000 operations",
            ));
        }
        let digest = request_digest(
            &request.base_content_revision,
            request.base_schema_revision.as_deref(),
            &summary,
            &request.operations,
            reverted_from,
        )?;

        // Serialize replay lookup with source mutation (row lock) like the oracle.
        let source = sqlx::query(
            "SELECT kind, lifecycle, validation_state, stored_crs, geometry_family,
                    catalog_revision, content_revision, schema_revision
             FROM spatial.data_sources
             WHERE organization_id = $1 AND project_id = $2 AND id = $3
             FOR UPDATE",
        )
        .bind(org)
        .bind(proj)
        .bind(source_id)
        .fetch_optional(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?
        .ok_or_else(|| EditError::not_found("data source not found"))?;

        let existing = sqlx::query(&format!(
            "SELECT {CHANGESET_COLUMNS}, ae.metadata->>'request_sha256' AS request_digest
             FROM editing.changesets AS cs JOIN audit.audit_events AS ae
               ON ae.organization_id = cs.organization_id AND ae.id = cs.audit_event_id
             WHERE cs.organization_id = $1 AND cs.project_id = $2
               AND cs.source_id = $3 AND cs.operation_id = $4"
        ))
        .bind(org)
        .bind(proj)
        .bind(source_id)
        .bind(&op_id)
        .fetch_optional(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;
        if let Some(row) = existing {
            let actor: Uuid = get(&row, "actor_id")?;
            let actor_type: String = get(&row, "actor_type")?;
            let stored: Option<String> = get(&row, "request_digest")?;
            if actor != self.actor_id
                || actor_type != "human"
                || stored.as_deref() != Some(digest.as_str())
            {
                return Err(EditError::conflict(
                    "OPERATION_ID_CONFLICT",
                    "operation_id already committed with different or unverified request identity",
                ));
            }
            let operations = self.load_operations(get(&row, "id")?).await?;
            return changeset_wire(&row, operations);
        }

        if request.operations.is_empty() {
            return Err(EditError::validation(
                "EMPTY_CHANGESET",
                "changeset requires at least one operation",
            ));
        }
        let lifecycle: String = get(&source, "lifecycle")?;
        let validation_state: String = get(&source, "validation_state")?;
        if lifecycle != "active" || validation_state != "ready" {
            return Err(EditError::validation(
                "SOURCE_NOT_READY",
                "source is not active and ready",
            ));
        }
        if get::<String>(&source, "kind")? == "reference_fixture" {
            return Err(EditError::forbidden(
                "SOURCE_READ_ONLY",
                "reference fixture sources are read-only; import or derive a source to edit",
            ));
        }
        let stored_crs: i32 = get::<Option<i32>>(&source, "stored_crs")?
            .ok_or_else(|| EditError::Internal("ready source without stored CRS".into()))?;
        let family: String = get(&source, "geometry_family")?;
        let catalog_revision: i64 = get(&source, "catalog_revision")?;
        let content_revision: String = get(&source, "content_revision")?;
        let schema_revision: String = get(&source, "schema_revision")?;

        if content_revision != request.base_content_revision {
            return Err(EditError::conflict(
                "STALE_CONTENT_REVISION",
                "stale content revision; refresh and reapply your local draft",
            )
            .with_details(json!({
                "expected_content_revision": request.base_content_revision,
                "current_content_revision": content_revision,
                "current_catalog_revision": catalog_revision,
                "draft_preserved": true,
            })));
        }
        if let Some(expected) = &request.base_schema_revision {
            if *expected != schema_revision {
                return Err(EditError::conflict(
                    "STALE_SCHEMA_REVISION",
                    "stale schema revision; refresh and review field identities before reapplying",
                )
                .with_details(json!({
                    "expected_schema_revision": expected,
                    "current_schema_revision": schema_revision,
                    "draft_preserved": true,
                })));
            }
        }
        let fields = self.load_fields(source_id).await?;
        if fields.iter().any(|f| f.schema_revision != schema_revision) {
            return Err(EditError::conflict(
                "STALE_SCHEMA_REVISION",
                "source field schema does not match its current revision",
            ));
        }
        let now = Utc::now().trunc_subsecs(6);

        // Snapshot and validate every operation before mutating.
        let mut prepared = Vec::with_capacity(request.operations.len());
        let mut fingerprint = Vec::with_capacity(request.operations.len());
        let mut seen = HashSet::new();
        for op in &request.operations {
            let fid = validate_feature_id(&op.feature_id)?;
            if !seen.insert(fid.clone()) {
                return Err(EditError::validation(
                    "DUPLICATE_FEATURE_OP",
                    format!("duplicate feature_id '{fid}' in same changeset"),
                ));
            }
            let current = sqlx::query(
                "SELECT attributes, ST_AsEWKB(geometry) AS geom_wkb
                 FROM spatial.reference_features
                 WHERE organization_id = $1 AND project_id = $2
                   AND source_id = $3 AND feature_id = $4",
            )
            .bind(org)
            .bind(proj)
            .bind(source_id)
            .bind(&fid)
            .fetch_optional(&mut **self.tx.tx_mut())
            .await
            .map_err(db_error)?;
            let missing = || EditError::not_found(format!("feature '{fid}' not found"));
            match op.op_type {
                OpType::Create => {
                    if current.is_some() {
                        return Err(EditError::conflict(
                            "FEATURE_EXISTS",
                            format!("feature '{fid}' already exists"),
                        ));
                    }
                    let wkt = geometry_wkt(op.geometry.as_ref(), &family)?;
                    let after_wkb = self.validated_wkb(&wkt, stored_crs).await?;
                    let props = Value::Object(typed_attributes(op.properties.as_ref(), &fields)?);
                    fingerprint
                        .push(json!({"op": "create", "fid": fid, "props": props, "wkt": wkt}));
                    prepared.push(Prepared {
                        op_type: op.op_type,
                        feature_id: fid,
                        before_wkb: None,
                        after_wkb: Some(after_wkb),
                        before_props: None,
                        after_props: Some(props),
                    });
                }
                OpType::Update => {
                    let current = current.ok_or_else(missing)?;
                    let before_wkb: Option<Vec<u8>> = get(&current, "geom_wkb")?;
                    let before_props: Value = get(&current, "attributes")?;
                    let (after_wkb, wkt) = if op.geometry.is_some() {
                        let wkt = geometry_wkt(op.geometry.as_ref(), &family)?;
                        (Some(self.validated_wkb(&wkt, stored_crs).await?), wkt)
                    } else {
                        (before_wkb.clone(), "UNCHANGED".to_owned())
                    };
                    let after_props = match &op.properties {
                        Some(props) => Value::Object(typed_attributes(Some(props), &fields)?),
                        None => before_props.clone(),
                    };
                    fingerprint.push(
                        json!({"op": "update", "fid": fid, "props": after_props, "wkt": wkt}),
                    );
                    prepared.push(Prepared {
                        op_type: op.op_type,
                        feature_id: fid,
                        before_wkb,
                        after_wkb,
                        before_props: Some(before_props),
                        after_props: Some(after_props),
                    });
                }
                OpType::Delete => {
                    let current = current.ok_or_else(missing)?;
                    fingerprint.push(json!({"op": "delete", "fid": fid}));
                    prepared.push(Prepared {
                        op_type: op.op_type,
                        feature_id: fid,
                        before_wkb: get(&current, "geom_wkb")?,
                        after_wkb: None,
                        before_props: Some(get(&current, "attributes")?),
                        after_props: None,
                    });
                }
            }
        }
        if prepared.iter().all(|item| {
            item.op_type == OpType::Update
                && item.before_wkb == item.after_wkb
                && match (&item.before_props, &item.after_props) {
                    (Some(before), Some(after)) => py_eq(before, after),
                    _ => false,
                }
        }) {
            return Err(EditError::conflict(
                "NO_MATERIAL_CHANGE",
                "changeset does not contain a material change",
            ));
        }

        // The source row is locked: lease acquisition cannot race this commit.
        let feature_ids: Vec<String> = seen.into_iter().collect();
        let leased: Option<i32> = sqlx::query_scalar(
            "SELECT 1 FROM collaboration.feature_edit_leases
             WHERE organization_id = $1 AND project_id = $2 AND source_id = $3
               AND feature_id = ANY(CAST($4 AS varchar[])) AND expires_at > clock_timestamp()
               AND actor_id <> $5
             LIMIT 1",
        )
        .bind(org)
        .bind(proj)
        .bind(source_id)
        .bind(&feature_ids)
        .bind(self.actor_id)
        .fetch_optional(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;
        if leased.is_some() {
            return Err(EditError::conflict(
                "FEATURE_LOCKED",
                "feature currently being edited by another account",
            )
            .with_details(json!({ "draft_preserved": true })));
        }

        let new_content = domain::content_revision(source_id, &content_revision, fingerprint);
        let new_catalog = catalog_revision + 1;

        // fk_features_pins is DEFERRABLE INITIALLY DEFERRED.
        for item in &prepared {
            let key = sqlx::query(match item.op_type {
                OpType::Create => {
                    "INSERT INTO spatial.reference_features (
                         organization_id, project_id, source_id, feature_id, attributes,
                         geometry, geometry_state, content_revision, schema_revision
                     ) VALUES ($1, $2, $3, $4, $5, ST_GeomFromEWKB($6), 'present', $7, $8)"
                }
                OpType::Update => {
                    "UPDATE spatial.reference_features
                     SET attributes = $5, geometry = ST_GeomFromEWKB($6), content_revision = $7
                     WHERE organization_id = $1 AND project_id = $2
                       AND source_id = $3 AND feature_id = $4"
                }
                OpType::Delete => {
                    "DELETE FROM spatial.reference_features
                     WHERE organization_id = $1 AND project_id = $2
                       AND source_id = $3 AND feature_id = $4"
                }
            })
            .bind(org)
            .bind(proj)
            .bind(source_id)
            .bind(&item.feature_id);
            let statement = match item.op_type {
                OpType::Create => key
                    .bind(&item.after_props)
                    .bind(&item.after_wkb)
                    .bind(&new_content)
                    .bind(&schema_revision),
                OpType::Update => key
                    .bind(&item.after_props)
                    .bind(&item.after_wkb)
                    .bind(&new_content),
                OpType::Delete => key,
            };
            statement
                .execute(&mut **self.tx.tx_mut())
                .await
                .map_err(db_error)?;
        }

        sqlx::query(
            "UPDATE spatial.reference_features SET content_revision = $4
             WHERE organization_id = $1 AND project_id = $2 AND source_id = $3
               AND content_revision != $4",
        )
        .bind(org)
        .bind(proj)
        .bind(source_id)
        .bind(&new_content)
        .execute(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;

        let stats = sqlx::query(
            "SELECT COUNT(*)::int AS cnt,
                    ST_XMin(ST_Extent(geometry)) AS min_x, ST_YMin(ST_Extent(geometry)) AS min_y,
                    ST_XMax(ST_Extent(geometry)) AS max_x, ST_YMax(ST_Extent(geometry)) AS max_y
             FROM spatial.reference_features
             WHERE organization_id = $1 AND project_id = $2 AND source_id = $3",
        )
        .bind(org)
        .bind(proj)
        .bind(source_id)
        .fetch_one(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;
        let count: i32 = get(&stats, "cnt")?;
        let (extent_state, extent) = if count == 0 {
            ("empty", None)
        } else {
            let bound = |key: &str| {
                get::<Option<f64>>(&stats, key)?
                    .ok_or_else(|| EditError::Internal("features without extent".into()))
            };
            (
                "available",
                Some(json!({
                    "min_x": bound("min_x")?,
                    "min_y": bound("min_y")?,
                    "max_x": bound("max_x")?,
                    "max_y": bound("max_y")?,
                    "crs_code": stored_crs,
                })),
            )
        };
        sqlx::query(
            "UPDATE spatial.data_sources
             SET content_revision = $4, catalog_revision = $5, feature_count = $6,
                 extent_state = $7, extent = $8, updated_at = $9
             WHERE organization_id = $1 AND project_id = $2 AND id = $3",
        )
        .bind(org)
        .bind(proj)
        .bind(source_id)
        .bind(&new_content)
        .bind(new_catalog)
        .bind(count)
        .bind(extent_state)
        .bind(&extent)
        .bind(now)
        .execute(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;

        let audit_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO audit.audit_events (
                 id, organization_id, actor_type, actor_id, actor_display_snapshot,
                 action, resource_type, resource_id, outcome, metadata, created_at
             ) VALUES ($1, $2, 'human', $3, $4, 'editing.changeset.commit', 'data_source',
                       $5, 'success', $6, $7)",
        )
        .bind(audit_id)
        .bind(org)
        .bind(self.actor_id)
        .bind(&self.display_name)
        .bind(source_id)
        .bind(json!({
            "operation_id": redact_audit_value(&op_id),
            "request_sha256": digest,
            "operation_count": prepared.len(),
            "base_content_revision": content_revision,
            "base_schema_revision": schema_revision,
            "submitted_schema_revision": request.base_schema_revision,
            "resulting_content_revision": new_content,
        }))
        .bind(now)
        .execute(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;

        let actor_label: String = if self.display_name.is_empty() {
            self.actor_id.to_string()
        } else {
            self.display_name.chars().take(200).collect()
        };
        let revision = |catalog: i64, content: &str| {
            json!({
                "catalog_revision": catalog,
                "content_revision": content,
                "schema_revision": schema_revision,
            })
        };
        sqlx::query(
            "INSERT INTO spatial.catalog_changes (
                 id, organization_id, project_id, resource_type, resource_id,
                 source_id, layer_id, actor_type, actor_id, actor_display_snapshot,
                 operation, created_at, audit_event_id, prior_revision, new_revision, changes
             ) VALUES ($1, $2, $3, 'data_source', $4, $4, NULL, 'human', $5, $6,
                       'edit_features', $7, $8, $9, $10, $11)",
        )
        .bind(Uuid::new_v4())
        .bind(org)
        .bind(proj)
        .bind(source_id)
        .bind(self.actor_id)
        .bind(&actor_label)
        .bind(now)
        .bind(audit_id)
        .bind(revision(catalog_revision, &content_revision))
        .bind(revision(new_catalog, &new_content))
        .bind(json!([{
            "field": "content_revision",
            "old_value": content_revision,
            "new_value": new_content,
        }]))
        .execute(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;

        let changeset_id = Uuid::new_v4();
        let row = sqlx::query(&format!(
            "INSERT INTO editing.changesets AS cs (
                 id, organization_id, project_id, source_id,
                 actor_type, actor_id, actor_display_snapshot, operation_id,
                 base_content_revision, resulting_content_revision,
                 base_catalog_revision, resulting_catalog_revision,
                 summary, status, operation_count, audit_event_id,
                 reverted_from_changeset_id, created_at
             ) VALUES ($1, $2, $3, $4, 'human', $5, $6, $7, $8, $9, $10, $11, $12, $13, $14,
                       $15, $16, $17)
             RETURNING {CHANGESET_COLUMNS}"
        ))
        .bind(changeset_id)
        .bind(org)
        .bind(proj)
        .bind(source_id)
        .bind(self.actor_id)
        .bind(&actor_label)
        .bind(&op_id)
        .bind(&content_revision)
        .bind(&new_content)
        .bind(catalog_revision)
        .bind(new_catalog)
        .bind(&summary)
        .bind(if reverted_from.is_some() {
            "reverted"
        } else {
            "committed"
        })
        .bind(i32::try_from(prepared.len()).unwrap_or(i32::MAX))
        .bind(audit_id)
        .bind(reverted_from)
        .bind(now)
        .fetch_one(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;

        for (ordinal, item) in prepared.iter().enumerate() {
            sqlx::query(
                "INSERT INTO editing.changeset_operations (
                     id, organization_id, project_id, changeset_id, ordinal,
                     op_type, feature_id, before_geometry_wkb, after_geometry_wkb,
                     before_properties, after_properties, created_at
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
            )
            .bind(Uuid::new_v4())
            .bind(org)
            .bind(proj)
            .bind(changeset_id)
            .bind(i32::try_from(ordinal).unwrap_or(i32::MAX))
            .bind(item.op_type.as_str())
            .bind(&item.feature_id)
            .bind(&item.before_wkb)
            .bind(&item.after_wkb)
            .bind(&item.before_props)
            .bind(&item.after_props)
            .bind(now)
            .execute(&mut **self.tx.tx_mut())
            .await
            .map_err(db_error)?;
        }

        let operations = self.load_operations(changeset_id).await?;
        sqlx::query(
            "DELETE FROM collaboration.feature_edit_leases
             WHERE organization_id = $1 AND project_id = $2 AND source_id = $3
               AND feature_id = ANY(CAST($4 AS varchar[])) AND actor_id = $5",
        )
        .bind(org)
        .bind(proj)
        .bind(source_id)
        .bind(&feature_ids)
        .bind(self.actor_id)
        .execute(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;
        changeset_wire(&row, operations)
    }

    /// PostGIS validity (GEOS `ST_IsValid`, no repair) and the 4096-vertex bound,
    /// so inserts always satisfy `ck_feature_geometry_state`.
    async fn validated_wkb(&mut self, wkt: &str, srid: i32) -> Result<Vec<u8>, EditError> {
        let row = sqlx::query(
            "WITH g AS (SELECT ST_SetSRID(ST_GeomFromText($1), $2) AS geom)
             SELECT ST_IsValid(geom) AS valid, ST_IsValidReason(geom) AS reason,
                    ST_NPoints(geom) AS npts, ST_AsEWKB(geom) AS wkb
             FROM g",
        )
        .bind(wkt)
        .bind(srid)
        .fetch_one(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;
        if !get::<Option<bool>>(&row, "valid")?.unwrap_or(false) {
            let reason: Option<String> = get(&row, "reason")?;
            return Err(EditError::validation(
                "INVALID_GEOMETRY",
                format!(
                    "invalid geometry rejected (no silent repair): {}",
                    reason.as_deref().unwrap_or("None")
                ),
            ));
        }
        if get::<i32>(&row, "npts")? > 4096 {
            return Err(EditError::validation(
                "BOUNDS_EXCEEDED",
                "geometry exceeds 4096 vertex limit",
            ));
        }
        get(&row, "wkb")
    }

    async fn load_fields(&mut self, source_id: Uuid) -> Result<Vec<FieldDef>, EditError> {
        let rows = sqlx::query(
            "SELECT field_id, type, nullable, schema_revision
             FROM spatial.source_fields
             WHERE organization_id = $1 AND project_id = $2 AND source_id = $3
             ORDER BY ordinal ASC",
        )
        .bind(self.tx.organization_id())
        .bind(self.project_id)
        .bind(source_id)
        .fetch_all(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;
        rows.iter()
            .map(|row| {
                Ok(FieldDef {
                    field_id: get(row, "field_id")?,
                    field_type: get(row, "type")?,
                    nullable: get(row, "nullable")?,
                    schema_revision: get(row, "schema_revision")?,
                })
            })
            .collect()
    }

    async fn load_operations(&mut self, changeset_id: Uuid) -> Result<Vec<Value>, EditError> {
        let rows = sqlx::query(
            "SELECT id, ordinal, op_type, feature_id,
                    CASE WHEN before_geometry_wkb IS NOT NULL
                         THEN ST_AsGeoJSON(ST_GeomFromEWKB(before_geometry_wkb))::jsonb END
                        AS before_geom,
                    CASE WHEN after_geometry_wkb IS NOT NULL
                         THEN ST_AsGeoJSON(ST_GeomFromEWKB(after_geometry_wkb))::jsonb END
                        AS after_geom,
                    before_properties, after_properties, created_at
             FROM editing.changeset_operations
             WHERE organization_id = $1 AND project_id = $2 AND changeset_id = $3
             ORDER BY ordinal ASC",
        )
        .bind(self.tx.organization_id())
        .bind(self.project_id)
        .bind(changeset_id)
        .fetch_all(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;
        rows.iter()
            .map(|row| {
                Ok(json!({
                    "id": get::<Uuid>(row, "id")?,
                    "ordinal": get::<i32>(row, "ordinal")?,
                    "op_type": get::<String>(row, "op_type")?,
                    "feature_id": get::<String>(row, "feature_id")?,
                    "before_geometry": get::<Option<Value>>(row, "before_geom")?,
                    "after_geometry": get::<Option<Value>>(row, "after_geom")?,
                    "before_properties": get::<Option<Value>>(row, "before_properties")?,
                    "after_properties": get::<Option<Value>>(row, "after_properties")?,
                    "created_at": iso(get(row, "created_at")?),
                }))
            })
            .collect()
    }
}

fn changeset_wire(row: &PgRow, operations: Vec<Value>) -> Result<Value, EditError> {
    let mut wire = Map::new();
    for key in [
        "id",
        "organization_id",
        "project_id",
        "source_id",
        "actor_id",
        "audit_event_id",
    ] {
        wire.insert(key.into(), json!(get::<Uuid>(row, key)?));
    }
    for key in [
        "actor_type",
        "actor_display_snapshot",
        "operation_id",
        "base_content_revision",
        "resulting_content_revision",
        "summary",
        "status",
    ] {
        wire.insert(key.into(), json!(get::<String>(row, key)?));
    }
    for key in ["base_catalog_revision", "resulting_catalog_revision"] {
        wire.insert(key.into(), json!(get::<i64>(row, key)?));
    }
    wire.insert(
        "operation_count".into(),
        json!(get::<i32>(row, "operation_count")?),
    );
    wire.insert(
        "reverted_from_changeset_id".into(),
        json!(get::<Option<Uuid>>(row, "reverted_from_changeset_id")?),
    );
    wire.insert("created_at".into(), json!(iso(get(row, "created_at")?)));
    wire.insert("operations".into(), Value::Array(operations));
    Ok(Value::Object(wire))
}

/// Python `datetime.isoformat()` of a UTC timestamptz (the database runs in UTC).
fn iso(ts: DateTime<Utc>) -> String {
    let format = if ts.timestamp_subsec_micros() == 0 {
        "%Y-%m-%dT%H:%M:%S+00:00"
    } else {
        "%Y-%m-%dT%H:%M:%S%.6f+00:00"
    };
    ts.format(format).to_string()
}

fn get<'r, T>(row: &'r PgRow, key: &str) -> Result<T, EditError>
where
    T: sqlx::Decode<'r, sqlx::Postgres> + sqlx::Type<sqlx::Postgres>,
{
    row.try_get(key).map_err(db_error)
}

/// SQLSTATE OV409 is a database guard (e.g. a signed house-numbering survey point);
/// the oracle renders it as 409 `DATA_LOCKED` with the trigger's primary message.
fn db_error(error: sqlx::Error) -> EditError {
    if let Some(db) = error.as_database_error() {
        if db.code().as_deref() == Some("OV409") {
            let message = match db.message() {
                "" => "Dữ liệu đang bị khóa",
                message => message,
            };
            return EditError::conflict("DATA_LOCKED", message);
        }
    }
    EditError::Internal(format!("editing database operation failed: {error}"))
}
