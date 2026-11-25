//! E07 transfer persistence over the canonical `transfers.*` schema.
//!
//! Mirrors `apps/api/openvietgis_api/adapters/persistence/transfer_repository.py`
//! and the project guard in `adapters/http/gis_access_guard.py`. Every method
//! runs inside one tenant transaction whose project authority was verified by
//! [`TransferScope::guard`] and [`TransferScope::authorize`].

use chrono::{DateTime, Utc};
use ovg_core::{ActorContext, ActorType, AppError, TenantScope};
use serde_json::{json, Value};
use sqlx::postgres::{PgRow, Postgres};
use sqlx::Row;
use uuid::Uuid;

use crate::{DbPool, TenantTransaction};

/// Customer-safe transfer failure, rendered exactly like the FastAPI handlers.
#[derive(Debug)]
pub enum TransferError {
    /// `TransferError` family: `{"code", "message"}`.
    Coded {
        status: u16,
        code: &'static str,
        message: String,
    },
    /// Authorization / tenancy family: `{"detail"}`.
    Detail { status: u16, detail: String },
    /// `SpatialProjectReadOnlyError`: `{"code": "PROJECT_NOT_WRITABLE", "detail"}`.
    ReadOnly,
    /// Request body or parameter validation (FastAPI 422 list body).
    InvalidRequest,
    /// Logged server-side only; rendered as a plain 500.
    Internal(String),
}

impl TransferError {
    pub fn coded(status: u16, code: &'static str, message: impl Into<String>) -> Self {
        Self::Coded {
            status,
            code,
            message: message.into(),
        }
    }

    pub fn validation(code: &'static str, message: impl Into<String>) -> Self {
        Self::coded(422, code, message)
    }

    pub fn conflict(code: &'static str, message: impl Into<String>) -> Self {
        Self::coded(409, code, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::coded(404, "TRANSFER_NOT_FOUND", message)
    }

    pub fn detail(status: u16, detail: impl Into<String>) -> Self {
        Self::Detail {
            status,
            detail: detail.into(),
        }
    }

    pub fn internal(detail: impl Into<String>) -> Self {
        Self::Internal(detail.into())
    }

    /// True for the Python `TransferError` family (job marked `failed`).
    #[must_use]
    pub fn is_coded(&self) -> bool {
        matches!(self, Self::Coded { .. })
    }
}

impl From<sqlx::Error> for TransferError {
    fn from(error: sqlx::Error) -> Self {
        Self::Internal(format!("transfer database operation failed: {error}"))
    }
}

impl From<AppError> for TransferError {
    fn from(error: AppError) -> Self {
        match error {
            AppError::Internal { internal_detail } => Self::Internal(internal_detail),
            other => Self::detail(other.status_code(), other.to_string()),
        }
    }
}

type Result<T> = std::result::Result<T, TransferError>;

fn col<'r, T>(row: &'r PgRow, name: &str) -> Result<T>
where
    T: sqlx::Decode<'r, Postgres> + sqlx::Type<Postgres>,
{
    row.try_get(name).map_err(Into::into)
}

/// Python `datetime.isoformat()` of a UTC timestamp.
#[must_use]
pub fn iso(ts: DateTime<Utc>) -> String {
    if ts.timestamp_subsec_micros() == 0 {
        ts.format("%Y-%m-%dT%H:%M:%S+00:00").to_string()
    } else {
        ts.format("%Y-%m-%dT%H:%M:%S%.6f+00:00").to_string()
    }
}

fn iso_opt(ts: Option<DateTime<Utc>>) -> Value {
    ts.map_or(Value::Null, |ts| Value::String(iso(ts)))
}

/// PostgreSQL stores microseconds; keep in-memory values identical to persisted ones.
#[must_use]
pub fn now_micros() -> DateTime<Utc> {
    let now = Utc::now();
    DateTime::from_timestamp_micros(now.timestamp_micros()).unwrap_or(now)
}

const GIS_SCOPED: &str = "Thao tác chưa được cấp quyền trong phạm vi GIS";

/// One fixed PostGIS pipeline step (never caller supplied).
#[derive(Debug, Clone, Copy)]
pub struct PipelineStep {
    pub inverse: bool,
    pub pipeline: &'static str,
    pub target: i32,
}

/// Raw geometry of one feature after each pipeline step (index 0 = native).
pub struct GeometryRow {
    pub valid: bool,
    pub wkb: Vec<Option<Vec<u8>>>,
    pub step_valid: Vec<bool>,
}

pub struct ExportRow {
    pub feature_id: String,
    pub attributes: Value,
    pub present: bool,
    pub native_srid: Option<i32>,
    pub geometry: GeometryRow,
}

pub struct ExportField {
    pub field_id: String,
    pub name: String,
    pub field_type: String,
}

pub struct ExportSource {
    pub source_id: Uuid,
    pub stored_crs: Option<i32>,
    pub geometry_family: String,
    pub layer_name: String,
    pub fields: Vec<ExportField>,
    pub feature_count: i64,
    pub has_long_text: bool,
}

pub struct Asset {
    pub id: Uuid,
    pub filename: String,
    pub detected_format: String,
    pub byte_size: i64,
    pub sha256: String,
    pub storage_key: String,
    pub uploaded_by_display: String,
    pub created_at: DateTime<Utc>,
}

pub struct Job {
    pub id: Uuid,
    pub kind: String,
    pub status: String,
    pub request_fingerprint: String,
    pub attempt_count: i32,
    pub source_asset_id: Option<Uuid>,
    pub options: Value,
    pub wire: Value,
}

pub struct Artifact {
    pub state: String,
    pub expires_at: DateTime<Utc>,
    pub storage_key: String,
    pub sha256: String,
    pub filename: String,
    pub content_type: String,
    pub wire: Value,
}

pub struct NewJob {
    pub id: Uuid,
    pub operation_id: Uuid,
    pub kind: &'static str,
    pub request_fingerprint: String,
    pub source_asset_id: Option<Uuid>,
    pub target_source_id: Option<Uuid>,
    pub target_layer_id: Option<Uuid>,
    pub source_content_revision: Option<String>,
    pub source_schema_revision: Option<String>,
    pub options: Value,
    pub inspection_report: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Default)]
pub struct JobUpdate {
    pub status: &'static str,
    pub attempt_count: Option<i32>,
    pub target_source_id: Option<Uuid>,
    pub target_layer_id: Option<Uuid>,
    pub source_content_revision: Option<String>,
    pub source_schema_revision: Option<String>,
    pub artifact_id: Option<Uuid>,
    pub result_report: Option<Value>,
    pub error_code: Option<&'static str>,
    pub error_message: Option<String>,
    pub completed_at: Option<DateTime<Utc>>,
}

pub struct NewArtifact {
    pub id: Uuid,
    pub job_id: Uuid,
    pub source_id: Uuid,
    pub layer_id: Uuid,
    pub content_revision: String,
    pub schema_revision: String,
    pub output_format: &'static str,
    pub output_crs: i32,
    pub filename: String,
    pub content_type: &'static str,
    pub byte_size: i64,
    pub sha256: String,
    pub storage_key: String,
    pub comparison_report: Value,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

pub struct NewImportedSource {
    pub id: Uuid,
    pub name: String,
    pub reference_key: String,
    pub fixture_version: String,
    pub manifest_digest: String,
    pub source_crs: i32,
    pub stored_crs: i32,
    pub geometry_family: &'static str,
    pub feature_count: i64,
    pub extent: Option<Value>,
    pub content_revision: String,
    pub schema_revision: String,
    pub created_at: DateTime<Utc>,
}

/// Verified project scope for transfer requests.
pub struct TransferScope<'a> {
    tx: TenantTransaction<'a>,
    organization_id: Uuid,
    project_id: Uuid,
    actor_id: Uuid,
    display: String,
    authority: Option<Authority>,
}

struct Authority {
    can_read: bool,
    can_edit: bool,
    project_active: bool,
}

impl<'a> TransferScope<'a> {
    /// `project_access_guard`: membership, org -> project lock, GIS-scoped denial.
    pub async fn guard(
        pool: &'a DbPool,
        organization_id: Uuid,
        project_id: Uuid,
        actor_id: Uuid,
        session_id: Uuid,
        lock_update: bool,
    ) -> Result<Self> {
        if organization_id.is_nil() {
            return Err(TransferError::detail(404, "organization not found"));
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
        let safe_role: bool = sqlx::query_scalar(
            "SELECT current_user = 'openvietgis_runtime'
                 AND row_security_active('transfers.jobs'::regclass)
                 AND row_security_active('spatial.reference_features'::regclass)",
        )
        .fetch_one(&mut **tx.tx_mut())
        .await?;
        if !safe_role {
            return Err(TransferError::internal(
                "transfers require the PostGIS runtime RLS role",
            ));
        }
        tx.clear_project_context().await?;

        let display: Option<String> = sqlx::query_scalar(
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
        .await?;
        let display =
            display.ok_or_else(|| TransferError::detail(401, "authentication required"))?;

        let member: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                 SELECT 1 FROM org.organization_memberships
                 WHERE organization_id = $1 AND user_id = $2 AND status = 'active'
             )",
        )
        .bind(organization_id)
        .bind(actor_id)
        .fetch_one(&mut **tx.tx_mut())
        .await?;
        if !member {
            return Err(TransferError::detail(404, "organization not found"));
        }
        // Serialize ACL changes with project operations: organization -> project lock order.
        let lock = if lock_update {
            "FOR UPDATE"
        } else {
            "FOR SHARE"
        };
        sqlx::query(&format!(
            "SELECT id FROM org.organizations WHERE id = $1 {lock}"
        ))
        .bind(organization_id)
        .execute(&mut **tx.tx_mut())
        .await?;
        sqlx::query(&format!(
            "SELECT id FROM workspace.projects WHERE organization_id = $1 AND id = $2 {lock}"
        ))
        .bind(organization_id)
        .bind(project_id)
        .execute(&mut **tx.tx_mut())
        .await?;
        let scoped: bool = sqlx::query_scalar("SELECT gis_access.scoped($1, $2)")
            .bind(organization_id)
            .bind(project_id)
            .fetch_one(&mut **tx.tx_mut())
            .await?;
        if scoped {
            return Err(TransferError::detail(403, GIS_SCOPED));
        }
        Ok(Self {
            tx,
            organization_id,
            project_id,
            actor_id,
            display,
            authority: None,
        })
    }

    /// `verify_project_authority`: lifecycle, project.read / project.edit, writability.
    pub async fn authorize(&mut self, write: bool) -> Result<()> {
        if self.authority.is_none() {
            let org_status: Option<String> =
                sqlx::query_scalar("SELECT status FROM org.organizations WHERE id = $1")
                    .bind(self.organization_id)
                    .fetch_optional(&mut **self.tx.tx_mut())
                    .await?;
            let org_status =
                org_status.ok_or_else(|| TransferError::detail(404, "organization not found"))?;
            if org_status != "active" {
                return Err(TransferError::detail(403, "organization is not active"));
            }
            let project = sqlx::query_as::<_, (String, Option<Uuid>)>(
                "SELECT status, workspace_id FROM workspace.projects
                 WHERE organization_id = $1 AND id = $2",
            )
            .bind(self.organization_id)
            .bind(self.project_id)
            .fetch_optional(&mut **self.tx.tx_mut())
            .await?;
            let (status, workspace_id) = project
                .filter(|(status, _)| status != "deleted")
                .ok_or_else(|| TransferError::detail(404, "project not found"))?;
            if !matches!(status.as_str(), "active" | "archived") {
                return Err(TransferError::detail(404, "resource not found"));
            }
            let grants = "SELECT 1 FROM org.resource_grants AS g
                 WHERE g.organization_id = m.organization_id AND g.membership_id = m.id
                   AND g.subject_kind = 'membership' AND g.revoked_at IS NULL
                   AND (g.scope_kind = 'organization'
                        OR (g.scope_kind = 'workspace' AND g.workspace_id = $3)
                        OR (g.scope_kind = 'project' AND g.project_id = $4))";
            let (can_read, can_edit) = sqlx::query_as::<_, (bool, bool)>(&format!(
                "SELECT m.role IN ('owner', 'admin') OR EXISTS ({grants}
                        AND g.role IN ('manager', 'editor', 'contributor', 'viewer')),
                        m.role IN ('owner', 'admin') OR EXISTS ({grants}
                        AND g.role IN ('manager', 'editor', 'contributor'))
                 FROM org.organization_memberships AS m
                 WHERE m.organization_id = $1 AND m.user_id = $2 AND m.status = 'active'"
            ))
            .bind(self.organization_id)
            .bind(self.actor_id)
            .bind(workspace_id)
            .bind(self.project_id)
            .fetch_optional(&mut **self.tx.tx_mut())
            .await?
            .ok_or_else(|| TransferError::detail(404, "organization not found"))?;
            sqlx::query("SELECT set_config('openvietgis.project_id', $1, true)")
                .bind(self.project_id.to_string())
                .execute(&mut **self.tx.tx_mut())
                .await?;
            self.authority = Some(Authority {
                can_read,
                can_edit,
                project_active: status == "active",
            });
        }
        let authority = self
            .authority
            .as_ref()
            .ok_or_else(|| TransferError::internal("authority missing"))?;
        if write && !authority.can_edit {
            return Err(TransferError::detail(
                403,
                "missing permission: project.edit",
            ));
        }
        if !write && !authority.can_read {
            return Err(TransferError::detail(
                403,
                "missing permission: project.read",
            ));
        }
        if write && !authority.project_active {
            return Err(TransferError::ReadOnly);
        }
        Ok(())
    }

    /// Guard and authorize in one step (internal follow-up transactions).
    pub async fn open(
        pool: &'a DbPool,
        organization_id: Uuid,
        project_id: Uuid,
        actor_id: Uuid,
        session_id: Uuid,
        write: bool,
    ) -> Result<Self> {
        let mut scope = Self::guard(
            pool,
            organization_id,
            project_id,
            actor_id,
            session_id,
            write,
        )
        .await?;
        scope.authorize(write).await?;
        Ok(scope)
    }

    #[must_use]
    pub fn organization_id(&self) -> Uuid {
        self.organization_id
    }

    #[must_use]
    pub fn project_id(&self) -> Uuid {
        self.project_id
    }

    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.display
    }

    pub async fn commit(self) -> Result<()> {
        self.tx.commit().await.map_err(Into::into)
    }

    fn checked(&self) -> Result<()> {
        if self.authority.is_none() {
            return Err(TransferError::internal(
                "transfer scope used before authority",
            ));
        }
        Ok(())
    }

    // ----- source assets -----

    pub async fn create_source_asset(&mut self, asset: &Asset) -> Result<()> {
        self.checked()?;
        sqlx::query(
            "INSERT INTO transfers.source_assets (
                 id, organization_id, project_id, filename, detected_format, byte_size, sha256,
                 storage_key, uploaded_by_type, uploaded_by_id, uploaded_by_display, created_at
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'human', $9, $10, $11)",
        )
        .bind(asset.id)
        .bind(self.organization_id)
        .bind(self.project_id)
        .bind(&asset.filename)
        .bind(&asset.detected_format)
        .bind(asset.byte_size)
        .bind(&asset.sha256)
        .bind(&asset.storage_key)
        .bind(self.actor_id)
        .bind(&asset.uploaded_by_display)
        .bind(asset.created_at)
        .execute(&mut **self.tx.tx_mut())
        .await?;
        Ok(())
    }

    pub async fn get_source_asset(&mut self, id: Uuid) -> Result<Option<Asset>> {
        self.checked()?;
        let row = sqlx::query(
            "SELECT * FROM transfers.source_assets
             WHERE organization_id = $1 AND project_id = $2 AND id = $3",
        )
        .bind(self.organization_id)
        .bind(self.project_id)
        .bind(id)
        .fetch_optional(&mut **self.tx.tx_mut())
        .await?;
        row.map(|row| {
            Ok(Asset {
                id: col(&row, "id")?,
                filename: col(&row, "filename")?,
                detected_format: col(&row, "detected_format")?,
                byte_size: col(&row, "byte_size")?,
                sha256: col(&row, "sha256")?,
                storage_key: col(&row, "storage_key")?,
                uploaded_by_display: col(&row, "uploaded_by_display")?,
                created_at: col(&row, "created_at")?,
            })
        })
        .transpose()
    }

    #[must_use]
    pub fn asset_wire(&self, asset: &Asset) -> Value {
        json!({
            "id": asset.id,
            "organization_id": self.organization_id,
            "project_id": self.project_id,
            "filename": asset.filename,
            "detected_format": asset.detected_format,
            "byte_size": asset.byte_size,
            "sha256": asset.sha256,
            "uploaded_by_display": asset.uploaded_by_display,
            "created_at": iso(asset.created_at),
        })
    }

    // ----- jobs -----

    async fn hydrate(&mut self, row: PgRow) -> Result<Job> {
        let id: Uuid = col(&row, "id")?;
        let attempt_rows = sqlx::query(
            "SELECT * FROM transfers.job_attempts
             WHERE organization_id = $1 AND project_id = $2 AND job_id = $3
             ORDER BY attempt_number ASC",
        )
        .bind(self.organization_id)
        .bind(self.project_id)
        .bind(id)
        .fetch_all(&mut **self.tx.tx_mut())
        .await?;
        let mut attempts = Vec::with_capacity(attempt_rows.len());
        for a in &attempt_rows {
            attempts.push(json!({
                "id": col::<Uuid>(a, "id")?,
                "attempt_number": col::<i32>(a, "attempt_number")?,
                "status": col::<String>(a, "status")?,
                "actor_type": col::<String>(a, "actor_type")?,
                "actor_id": col::<Uuid>(a, "actor_id")?,
                "started_at": iso(col(a, "started_at")?),
                "finished_at": iso_opt(col(a, "finished_at")?),
                "error_code": col::<Option<String>>(a, "error_code")?,
                "error_message": col::<Option<String>>(a, "error_message")?,
            }));
        }
        let artifact_id: Option<Uuid> = col(&row, "artifact_id")?;
        let artifact = match artifact_id {
            Some(aid) => self.get_artifact(aid).await?.map(|a| a.wire),
            None => None,
        };
        let non_empty = |value: Option<Value>| {
            value.filter(|v| !(v.is_null() || v.as_object().is_some_and(|o| o.is_empty())))
        };
        let options: Value = col(&row, "options_json")?;
        let wire = json!({
            "id": id,
            "organization_id": col::<Uuid>(&row, "organization_id")?,
            "project_id": col::<Uuid>(&row, "project_id")?,
            "operation_id": col::<Uuid>(&row, "operation_id")?,
            "kind": col::<String>(&row, "kind")?,
            "request_fingerprint": col::<String>(&row, "request_fingerprint")?,
            "status": col::<String>(&row, "status")?,
            "actor_type": col::<String>(&row, "actor_type")?,
            "actor_id": col::<Uuid>(&row, "actor_id")?,
            "actor_display_snapshot": col::<String>(&row, "actor_display_snapshot")?,
            "source_asset_id": col::<Option<Uuid>>(&row, "source_asset_id")?,
            "target_source_id": col::<Option<Uuid>>(&row, "target_source_id")?,
            "target_layer_id": col::<Option<Uuid>>(&row, "target_layer_id")?,
            "source_content_revision": col::<Option<String>>(&row, "source_content_revision")?,
            "source_schema_revision": col::<Option<String>>(&row, "source_schema_revision")?,
            "artifact_id": artifact_id,
            "attempt_count": col::<i32>(&row, "attempt_count")?,
            "inspection_report": non_empty(col(&row, "inspection_report")?),
            "result_report": non_empty(col(&row, "result_report")?),
            "error_code": col::<Option<String>>(&row, "error_code")?,
            "error_message": col::<Option<String>>(&row, "error_message")?,
            "created_at": iso(col(&row, "created_at")?),
            "updated_at": iso(col(&row, "updated_at")?),
            "completed_at": iso_opt(col(&row, "completed_at")?),
            "attempts": attempts,
            "artifact": artifact,
        });
        Ok(Job {
            id,
            kind: col(&row, "kind")?,
            status: col(&row, "status")?,
            request_fingerprint: col(&row, "request_fingerprint")?,
            attempt_count: col(&row, "attempt_count")?,
            source_asset_id: col(&row, "source_asset_id")?,
            options,
            wire,
        })
    }

    async fn job_where(&mut self, clause: &str, key: Uuid, lock: bool) -> Result<Option<Job>> {
        self.checked()?;
        let suffix = if lock { " FOR UPDATE" } else { "" };
        let row = sqlx::query(&format!(
            "SELECT * FROM transfers.jobs
             WHERE organization_id = $1 AND project_id = $2 AND {clause} = $3{suffix}"
        ))
        .bind(self.organization_id)
        .bind(self.project_id)
        .bind(key)
        .fetch_optional(&mut **self.tx.tx_mut())
        .await?;
        match row {
            Some(row) => Ok(Some(self.hydrate(row).await?)),
            None => Ok(None),
        }
    }

    pub async fn get_job(&mut self, id: Uuid, lock: bool) -> Result<Option<Job>> {
        self.job_where("id", id, lock).await
    }

    pub async fn get_job_by_operation(&mut self, operation: Uuid) -> Result<Option<Job>> {
        self.job_where("operation_id", operation, true).await
    }

    pub async fn list_jobs(&mut self, limit: i64) -> Result<Vec<Value>> {
        self.checked()?;
        let rows = sqlx::query(
            "SELECT * FROM transfers.jobs
             WHERE organization_id = $1 AND project_id = $2
             ORDER BY created_at DESC LIMIT $3",
        )
        .bind(self.organization_id)
        .bind(self.project_id)
        .bind(limit.clamp(1, 100))
        .fetch_all(&mut **self.tx.tx_mut())
        .await?;
        let mut items = Vec::with_capacity(rows.len());
        for row in rows {
            items.push(self.hydrate(row).await?.wire);
        }
        Ok(items)
    }

    pub async fn create_job(&mut self, job: &NewJob) -> Result<()> {
        self.checked()?;
        let inspection = (!job.inspection_report.is_null()).then_some(&job.inspection_report);
        sqlx::query(
            "INSERT INTO transfers.jobs (
                 id, organization_id, project_id, operation_id, kind, request_fingerprint, status,
                 actor_type, actor_id, actor_display_snapshot, credential_id, source_asset_id,
                 target_source_id, target_layer_id, source_content_revision,
                 source_schema_revision, artifact_id, attempt_count, options_json,
                 inspection_report, result_report, error_code, error_message, created_at,
                 updated_at, completed_at
             ) VALUES (
                 $1, $2, $3, $4, $5, $6, 'active', 'human', $7, $8, NULL, $9, $10, $11, $12, $13,
                 NULL, 1, $14, $15, NULL, NULL, NULL, $16, $16, NULL
             )",
        )
        .bind(job.id)
        .bind(self.organization_id)
        .bind(self.project_id)
        .bind(job.operation_id)
        .bind(job.kind)
        .bind(&job.request_fingerprint)
        .bind(self.actor_id)
        .bind(&self.display)
        .bind(job.source_asset_id)
        .bind(job.target_source_id)
        .bind(job.target_layer_id)
        .bind(&job.source_content_revision)
        .bind(&job.source_schema_revision)
        .bind(&job.options)
        .bind(inspection)
        .bind(job.created_at)
        .execute(&mut **self.tx.tx_mut())
        .await?;
        Ok(())
    }

    pub async fn record_attempt(
        &mut self,
        job_id: Uuid,
        attempt_number: i32,
        started_at: DateTime<Utc>,
    ) -> Result<()> {
        self.checked()?;
        sqlx::query(
            "INSERT INTO transfers.job_attempts (
                 id, organization_id, project_id, job_id, attempt_number, status, actor_type,
                 actor_id, credential_id, started_at, finished_at, error_code, error_message
             ) VALUES ($1, $2, $3, $4, $5, 'active', 'human', $6, NULL, $7, NULL, NULL, NULL)",
        )
        .bind(Uuid::new_v4())
        .bind(self.organization_id)
        .bind(self.project_id)
        .bind(job_id)
        .bind(attempt_number)
        .bind(self.actor_id)
        .bind(started_at)
        .execute(&mut **self.tx.tx_mut())
        .await?;
        Ok(())
    }

    pub async fn update_job(&mut self, job_id: Uuid, update: JobUpdate) -> Result<Job> {
        self.checked()?;
        let result = update
            .result_report
            .filter(|v| !(v.is_null() || v.as_object().is_some_and(|o| o.is_empty())));
        sqlx::query(
            "UPDATE transfers.jobs
             SET status = $4,
                 attempt_count = COALESCE($5, attempt_count),
                 target_source_id = COALESCE($6, target_source_id),
                 target_layer_id = COALESCE($7, target_layer_id),
                 source_content_revision = COALESCE($8, source_content_revision),
                 source_schema_revision = COALESCE($9, source_schema_revision),
                 artifact_id = COALESCE($10, artifact_id),
                 result_report = COALESCE($11, result_report),
                 error_code = $12,
                 error_message = $13,
                 updated_at = $14,
                 completed_at = COALESCE($15, completed_at)
             WHERE organization_id = $1 AND project_id = $2 AND id = $3",
        )
        .bind(self.organization_id)
        .bind(self.project_id)
        .bind(job_id)
        .bind(update.status)
        .bind(update.attempt_count)
        .bind(update.target_source_id)
        .bind(update.target_layer_id)
        .bind(update.source_content_revision)
        .bind(update.source_schema_revision)
        .bind(update.artifact_id)
        .bind(result)
        .bind(update.error_code)
        .bind(update.error_message)
        .bind(now_micros())
        .bind(update.completed_at)
        .execute(&mut **self.tx.tx_mut())
        .await?;
        self.get_job(job_id, false)
            .await?
            .ok_or_else(|| TransferError::internal("updated transfer job disappeared"))
    }

    /// Lock the job row on the publication boundary and return its status.
    pub async fn lock_job_status(&mut self, job_id: Uuid) -> Result<Option<String>> {
        self.checked()?;
        sqlx::query_scalar(
            "SELECT status FROM transfers.jobs
             WHERE organization_id = $1 AND project_id = $2 AND id = $3 FOR UPDATE",
        )
        .bind(self.organization_id)
        .bind(self.project_id)
        .bind(job_id)
        .fetch_optional(&mut **self.tx.tx_mut())
        .await
        .map_err(Into::into)
    }

    // ----- artifacts -----

    pub async fn get_artifact(&mut self, id: Uuid) -> Result<Option<Artifact>> {
        self.checked()?;
        let row = sqlx::query(
            "SELECT * FROM transfers.derived_artifacts
             WHERE organization_id = $1 AND project_id = $2 AND id = $3",
        )
        .bind(self.organization_id)
        .bind(self.project_id)
        .bind(id)
        .fetch_optional(&mut **self.tx.tx_mut())
        .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let created_at: DateTime<Utc> = col(&row, "created_at")?;
        let expires_at: DateTime<Utc> = col(&row, "expires_at")?;
        let wire = json!({
            "id": col::<Uuid>(&row, "id")?,
            "organization_id": col::<Uuid>(&row, "organization_id")?,
            "project_id": col::<Uuid>(&row, "project_id")?,
            "job_id": col::<Uuid>(&row, "job_id")?,
            "source_id": col::<Uuid>(&row, "source_id")?,
            "layer_id": col::<Uuid>(&row, "layer_id")?,
            "source_content_revision": col::<String>(&row, "source_content_revision")?,
            "source_schema_revision": col::<String>(&row, "source_schema_revision")?,
            "output_format": col::<String>(&row, "output_format")?,
            "output_crs": col::<i32>(&row, "output_crs")?,
            "filename": col::<String>(&row, "filename")?,
            "content_type": col::<String>(&row, "content_type")?,
            "byte_size": col::<i64>(&row, "byte_size")?,
            "sha256": col::<String>(&row, "sha256")?,
            "state": col::<String>(&row, "state")?,
            "created_by_display": col::<String>(&row, "created_by_display")?,
            "created_at": iso(created_at),
            "expires_at": iso(expires_at),
            "comparison_report": col::<Value>(&row, "comparison_report")?,
        });
        Ok(Some(Artifact {
            state: col(&row, "state")?,
            expires_at,
            storage_key: col(&row, "storage_key")?,
            sha256: col(&row, "sha256")?,
            filename: col(&row, "filename")?,
            content_type: col(&row, "content_type")?,
            wire,
        }))
    }

    pub async fn mark_artifact_state(&mut self, id: Uuid, state: &'static str) -> Result<()> {
        self.checked()?;
        sqlx::query(
            "UPDATE transfers.derived_artifacts SET state = $4
             WHERE organization_id = $1 AND project_id = $2 AND id = $3",
        )
        .bind(self.organization_id)
        .bind(self.project_id)
        .bind(id)
        .bind(state)
        .execute(&mut **self.tx.tx_mut())
        .await?;
        Ok(())
    }

    // ----- export -----

    /// `load_pinned_export_source` without materializing features. With
    /// `lock_source` the source row is held FOR SHARE so keyset pages stay pinned.
    pub async fn load_export_source(
        &mut self,
        layer_id: Uuid,
        content_revision: &str,
        schema_revision: &str,
        lock_source: bool,
    ) -> Result<ExportSource> {
        self.checked()?;
        let layer = sqlx::query_as::<_, (Uuid, String, String)>(
            "SELECT source_id, name, lifecycle FROM spatial.layers
             WHERE organization_id = $1 AND project_id = $2 AND id = $3",
        )
        .bind(self.organization_id)
        .bind(self.project_id)
        .bind(layer_id)
        .fetch_optional(&mut **self.tx.tx_mut())
        .await?;
        let (source_id, layer_name) = match layer {
            Some((source_id, name, lifecycle)) if lifecycle == "active" => (source_id, name),
            _ => return Err(TransferError::not_found("Layer not found.")),
        };
        let lock = if lock_source { " FOR SHARE" } else { "" };
        let source = sqlx::query(&format!(
            "SELECT lifecycle, validation_state, stored_crs, geometry_family, content_revision,
                    schema_revision
             FROM spatial.data_sources
             WHERE organization_id = $1 AND project_id = $2 AND id = $3{lock}"
        ))
        .bind(self.organization_id)
        .bind(self.project_id)
        .bind(source_id)
        .fetch_optional(&mut **self.tx.tx_mut())
        .await?;
        let source = match source {
            Some(row) if col::<String>(&row, "lifecycle")? == "active" => row,
            _ => return Err(TransferError::not_found("Source not found.")),
        };
        if col::<String>(&source, "validation_state")? != "ready" {
            return Err(TransferError::validation(
                "SOURCE_NOT_READY",
                "Source is not ready for export.",
            ));
        }
        if col::<String>(&source, "content_revision")? != content_revision
            || col::<String>(&source, "schema_revision")? != schema_revision
        {
            return Err(TransferError::conflict(
                "STALE_READ_PIN",
                "Requested source revision is stale; reload Layer metadata before exporting.",
            ));
        }
        let fields = sqlx::query_as::<_, (String, String, String)>(
            "SELECT field_id, name, type FROM spatial.source_fields
             WHERE organization_id = $1 AND project_id = $2 AND source_id = $3
               AND schema_revision = $4
             ORDER BY ordinal ASC",
        )
        .bind(self.organization_id)
        .bind(self.project_id)
        .bind(source_id)
        .bind(schema_revision)
        .fetch_all(&mut **self.tx.tx_mut())
        .await?
        .into_iter()
        .map(|(field_id, name, field_type)| ExportField {
            field_id,
            name,
            field_type,
        })
        .collect();
        let (feature_count, has_long_text) = sqlx::query_as::<_, (i64, bool)>(
            "SELECT count(*), COALESCE(bool_or(EXISTS (
                        SELECT 1 FROM jsonb_each(f.attributes) AS e
                        WHERE jsonb_typeof(e.value) = 'string'
                          AND octet_length(e.value #>> '{}') > 254
                    )), false)
             FROM spatial.reference_features AS f
             WHERE f.organization_id = $1 AND f.project_id = $2 AND f.source_id = $3
               AND f.content_revision = $4 AND f.schema_revision = $5",
        )
        .bind(self.organization_id)
        .bind(self.project_id)
        .bind(source_id)
        .bind(content_revision)
        .bind(schema_revision)
        .fetch_one(&mut **self.tx.tx_mut())
        .await?;
        Ok(ExportSource {
            source_id,
            stored_crs: col(&source, "stored_crs")?,
            geometry_family: col(&source, "geometry_family")?,
            layer_name,
            fields,
            feature_count,
            has_long_text,
        })
    }

    /// One keyset page of pinned features, each with its native geometry and
    /// the output of every pipeline step (bounded memory streaming).
    pub async fn fetch_export_rows(
        &mut self,
        source_id: Uuid,
        content_revision: &str,
        schema_revision: &str,
        steps: &[PipelineStep],
        after_feature_id: Option<&str>,
        limit: i64,
    ) -> Result<Vec<ExportRow>> {
        self.checked()?;
        let ok = "ST_Zmflag(f.geometry) = 0 AND ST_NPoints(f.geometry) BETWEEN 1 AND 4096
                  AND NOT ST_IsEmpty(f.geometry)";
        let (columns, laterals) = step_sql(steps);
        let sql = format!(
            "SELECT f.feature_id, f.attributes, f.geometry_state = 'present' AS present,
                    ST_SRID(f.geometry) AS native_srid,
                    CASE WHEN {ok} THEN ST_IsValid(f.geometry) ELSE false END AS valid0,
                    ST_AsBinary(s0.g, 'NDR') AS w0{columns}
             FROM spatial.reference_features AS f
             CROSS JOIN LATERAL (SELECT CASE WHEN {ok} THEN f.geometry END AS g) AS s0{laterals}
             WHERE f.organization_id = $1 AND f.project_id = $2 AND f.source_id = $3
               AND f.content_revision = $4 AND f.schema_revision = $5
               AND ($6::text IS NULL OR f.feature_id > $6)
             ORDER BY f.feature_id ASC
             LIMIT $7"
        );
        let rows = sqlx::query(&sql)
            .bind(self.organization_id)
            .bind(self.project_id)
            .bind(source_id)
            .bind(content_revision)
            .bind(schema_revision)
            .bind(after_feature_id)
            .bind(limit)
            .fetch_all(&mut **self.tx.tx_mut())
            .await
            .map_err(|error| step_error(steps, error))?;
        rows.iter()
            .map(|row| {
                Ok(ExportRow {
                    feature_id: col(row, "feature_id")?,
                    attributes: col(row, "attributes")?,
                    present: col(row, "present")?,
                    native_srid: col(row, "native_srid")?,
                    geometry: geometry_row(row, steps.len(), "valid0")?,
                })
            })
            .collect()
    }

    /// `validate_and_transform_geometries` batch: PostGIS validity of WKT input and
    /// the output of every approved pipeline step, in input order.
    pub async fn check_and_transform(
        &mut self,
        wkts: &[String],
        source_srid: i32,
        steps: &[PipelineStep],
    ) -> Result<Vec<(bool, GeometryRow)>> {
        self.checked()?;
        let (columns, laterals) = step_sql(steps);
        let sql = format!(
            "SELECT ST_IsValid(s0.g) AS valid0, ST_IsEmpty(s0.g) AS empty0,
                    ST_AsBinary(s0.g, 'NDR') AS w0{columns}
             FROM unnest($1::text[]) WITH ORDINALITY AS r(wkt, i)
             CROSS JOIN LATERAL (SELECT ST_GeomFromText(r.wkt, $2) AS g) AS s0{laterals}
             ORDER BY r.i ASC"
        );
        let rows = sqlx::query(&sql)
            .bind(wkts)
            .bind(source_srid)
            .fetch_all(&mut **self.tx.tx_mut())
            .await
            .map_err(|error| step_error(steps, error))?;
        rows.iter()
            .map(|row| {
                Ok((
                    col::<Option<bool>>(row, "empty0")?.unwrap_or(true),
                    geometry_row(row, steps.len(), "valid0")?,
                ))
            })
            .collect()
    }

    pub async fn publish_export_artifact(
        &mut self,
        artifact: &NewArtifact,
        result_report: Value,
    ) -> Result<Job> {
        self.checked()?;
        let status = self
            .lock_job_status(artifact.job_id)
            .await?
            .ok_or_else(|| TransferError::not_found("Transfer job not found."))?;
        if matches!(status.as_str(), "cancelled" | "cancel_requested") {
            return Err(TransferError::conflict(
                "JOB_CANCELLED",
                "Export cancelled before publication.",
            ));
        }
        sqlx::query(
            "INSERT INTO transfers.derived_artifacts (
                 id, organization_id, project_id, job_id, source_id, layer_id,
                 source_content_revision, source_schema_revision, output_format, output_crs,
                 filename, content_type, byte_size, sha256, storage_key, comparison_report,
                 created_by_type, created_by_id, created_by_display, created_at, expires_at, state
             ) VALUES (
                 $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16,
                 'human', $17, $18, $19, $20, 'ready'
             )",
        )
        .bind(artifact.id)
        .bind(self.organization_id)
        .bind(self.project_id)
        .bind(artifact.job_id)
        .bind(artifact.source_id)
        .bind(artifact.layer_id)
        .bind(&artifact.content_revision)
        .bind(&artifact.schema_revision)
        .bind(artifact.output_format)
        .bind(artifact.output_crs)
        .bind(&artifact.filename)
        .bind(artifact.content_type)
        .bind(artifact.byte_size)
        .bind(&artifact.sha256)
        .bind(&artifact.storage_key)
        .bind(&artifact.comparison_report)
        .bind(self.actor_id)
        .bind(&self.display)
        .bind(artifact.created_at)
        .bind(artifact.expires_at)
        .execute(&mut **self.tx.tx_mut())
        .await?;
        self.audit(
            "transfers.export.completed",
            "derived_artifact",
            artifact.id,
            artifact.created_at,
            json!({
                "project_id": self.project_id.to_string(),
                "job_id": artifact.job_id.to_string(),
                "source_id": artifact.source_id.to_string(),
                "content_revision": artifact.content_revision,
                "output_format": artifact.output_format,
                "output_crs": artifact.output_crs,
            }),
        )
        .await?;
        self.update_job(
            artifact.job_id,
            JobUpdate {
                status: "succeeded",
                artifact_id: Some(artifact.id),
                result_report: Some(result_report),
                completed_at: Some(artifact.created_at),
                ..JobUpdate::default()
            },
        )
        .await
    }

    // ----- import publication -----

    /// Atomically publish the imported DataSource, fields, features, Layer,
    /// catalog changes and audit events, then mark the job succeeded.
    #[allow(clippy::too_many_arguments)]
    pub async fn publish_imported_source(
        &mut self,
        job_id: Uuid,
        asset_id: Uuid,
        source: &NewImportedSource,
        fields: &Value,
        features: &[Value],
        layer_id: Uuid,
        layer_name: &str,
        result_report: Value,
    ) -> Result<Job> {
        self.authorize(true).await?;
        let status = self
            .lock_job_status(job_id)
            .await?
            .ok_or_else(|| TransferError::not_found("Transfer job not found."))?;
        match status.as_str() {
            "cancelled" | "cancel_requested" => {
                return Err(TransferError::conflict(
                    "JOB_CANCELLED",
                    "Import cancelled before publication.",
                ))
            }
            "denied" => {
                return Err(TransferError::conflict(
                    "JOB_DENIED",
                    "Import authority was denied.",
                ))
            }
            _ => {}
        }
        let (org, project) = (self.organization_id, self.project_id);
        sqlx::query(
            "INSERT INTO spatial.data_sources (
                 id, organization_id, project_id, name, kind, reference_key, fixture_version,
                 manifest_digest, source_asset_id, lifecycle, validation_state, validation_reason,
                 source_crs, stored_crs, schema_state, geometry_family, dimensions,
                 feature_count_state, feature_count, extent_state, extent, catalog_revision,
                 content_revision, schema_revision, created_at, updated_at, retired_at
             ) VALUES (
                 $1, $2, $3, $4, 'imported_file', $5, $6, $7, $8, 'active', 'ready', NULL,
                 $9, $10, 'known', $11, 'XY', 'known', $12, $13, $14, 1, $15, $16, $17, $17, NULL
             )",
        )
        .bind(source.id)
        .bind(org)
        .bind(project)
        .bind(&source.name)
        .bind(&source.reference_key)
        .bind(&source.fixture_version)
        .bind(&source.manifest_digest)
        .bind(asset_id)
        .bind(source.source_crs)
        .bind(source.stored_crs)
        .bind(source.geometry_family)
        .bind(source.feature_count)
        .bind(if source.extent.is_some() {
            "available"
        } else {
            "empty"
        })
        .bind(&source.extent)
        .bind(&source.content_revision)
        .bind(&source.schema_revision)
        .bind(source.created_at)
        .execute(&mut **self.tx.tx_mut())
        .await?;
        if fields.as_array().is_some_and(|f| !f.is_empty()) {
            sqlx::query(
                "INSERT INTO spatial.source_fields (
                     organization_id, project_id, source_id, field_id, name, ordinal, type,
                     nullable, schema_revision
                 )
                 SELECT $1, $2, $3, r.field_id, r.name, r.ordinal, r.type, r.nullable, $4
                 FROM jsonb_to_recordset($5::jsonb)
                     AS r(field_id text, name text, ordinal integer, type text, nullable boolean)",
            )
            .bind(org)
            .bind(project)
            .bind(source.id)
            .bind(&source.schema_revision)
            .bind(fields)
            .execute(&mut **self.tx.tx_mut())
            .await?;
        }
        for chunk in features.chunks(2500) {
            sqlx::query(
                "INSERT INTO spatial.reference_features (
                     organization_id, project_id, source_id, feature_id, attributes, geometry,
                     geometry_state, content_revision, schema_revision
                 )
                 SELECT $1, $2, $3, r.feature_id, r.attributes, ST_GeomFromText(r.wkt, $4),
                        r.geometry_state, $5, $6
                 FROM jsonb_to_recordset($7::jsonb)
                     AS r(feature_id text, attributes jsonb, wkt text, geometry_state text)",
            )
            .bind(org)
            .bind(project)
            .bind(source.id)
            .bind(source.stored_crs)
            .bind(&source.content_revision)
            .bind(&source.schema_revision)
            .bind(Value::Array(chunk.to_vec()))
            .execute(&mut **self.tx.tx_mut())
            .await?;
        }
        let now = source.created_at;
        sqlx::query(
            "INSERT INTO spatial.layers (
                 id, organization_id, project_id, source_id, name, lifecycle, layer_revision,
                 created_at, updated_at, retired_at
             ) VALUES ($1, $2, $3, $4, $5, 'active', 1, $6, $6, NULL)",
        )
        .bind(layer_id)
        .bind(org)
        .bind(project)
        .bind(source.id)
        .bind(layer_name)
        .bind(now)
        .execute(&mut **self.tx.tx_mut())
        .await?;

        let change = |field: &str, value: Value| json!({"field": field, "old_value": null, "new_value": value});
        let source_changes = json!([
            change("name", json!(source.name)),
            change("kind", json!("imported_file")),
            change("manifest_digest", json!(source.manifest_digest)),
            change("source_asset_id", json!(asset_id.to_string())),
            change("source_crs", json!(source.source_crs)),
            change("stored_crs", json!(source.stored_crs)),
            change("feature_count", json!(source.feature_count)),
            change("content_revision", json!(source.content_revision)),
            change("schema_revision", json!(source.schema_revision)),
        ]);
        self.catalog_change(
            ("data_source", source.id, Some(source.id), None),
            "import_source",
            "spatial.catalog.import_source",
            now,
            json!({
                "catalog_revision": 1,
                "content_revision": source.content_revision,
                "schema_revision": source.schema_revision,
            }),
            source_changes,
        )
        .await?;
        let layer_changes = json!([
            change("name", json!(layer_name)),
            change("source_id", json!(source.id.to_string())),
            change("lifecycle", json!("active")),
        ]);
        self.catalog_change(
            ("layer", layer_id, None, Some(layer_id)),
            "create_layer",
            "spatial.catalog.create_layer",
            now,
            json!({"layer_revision": 1}),
            layer_changes,
        )
        .await?;

        self.update_job(
            job_id,
            JobUpdate {
                status: "succeeded",
                target_source_id: Some(source.id),
                target_layer_id: Some(layer_id),
                source_content_revision: Some(source.content_revision.clone()),
                source_schema_revision: Some(source.schema_revision.clone()),
                result_report: Some(result_report),
                completed_at: Some(now_micros()),
                ..JobUpdate::default()
            },
        )
        .await
    }

    #[allow(clippy::type_complexity)]
    async fn catalog_change(
        &mut self,
        (resource_type, resource_id, source_id, layer_id): (
            &'static str,
            Uuid,
            Option<Uuid>,
            Option<Uuid>,
        ),
        operation: &'static str,
        action: &'static str,
        now: DateTime<Utc>,
        new_revision: Value,
        changes: Value,
    ) -> Result<()> {
        let change_id = Uuid::new_v4();
        let audit_id = Uuid::new_v4();
        self.audit_with_id(
            audit_id,
            action,
            resource_type,
            resource_id,
            now,
            json!({
                "project_id": self.project_id.to_string(),
                "catalog_change_id": change_id.to_string(),
            }),
        )
        .await?;
        sqlx::query(
            "INSERT INTO spatial.catalog_changes (
                 id, organization_id, project_id, resource_type, resource_id, source_id,
                 layer_id, actor_type, actor_id, actor_display_snapshot, operation, created_at,
                 audit_event_id, prior_revision, new_revision, changes
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, 'human', $8, $9, $10, $11, $12, NULL, $13, $14)",
        )
        .bind(change_id)
        .bind(self.organization_id)
        .bind(self.project_id)
        .bind(resource_type)
        .bind(resource_id)
        .bind(source_id)
        .bind(layer_id)
        .bind(self.actor_id)
        .bind(&self.display)
        .bind(operation)
        .bind(now)
        .bind(audit_id)
        .bind(new_revision)
        .bind(changes)
        .execute(&mut **self.tx.tx_mut())
        .await?;
        Ok(())
    }

    async fn audit(
        &mut self,
        action: &'static str,
        resource_type: &'static str,
        resource_id: Uuid,
        created_at: DateTime<Utc>,
        metadata: Value,
    ) -> Result<()> {
        self.audit_with_id(
            Uuid::new_v4(),
            action,
            resource_type,
            resource_id,
            created_at,
            metadata,
        )
        .await
    }

    async fn audit_with_id(
        &mut self,
        id: Uuid,
        action: &'static str,
        resource_type: &'static str,
        resource_id: Uuid,
        created_at: DateTime<Utc>,
        metadata: Value,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO audit.audit_events (
                 id, organization_id, actor_type, actor_id, actor_display_snapshot, action,
                 resource_type, resource_id, outcome, metadata, created_at
             ) VALUES ($1, $2, 'human', $3, $4, $5, $6, $7, 'success', $8, $9)",
        )
        .bind(id)
        .bind(self.organization_id)
        .bind(self.actor_id)
        .bind(&self.display)
        .bind(action)
        .bind(resource_type)
        .bind(resource_id)
        .bind(metadata)
        .bind(created_at)
        .execute(&mut **self.tx.tx_mut())
        .await?;
        Ok(())
    }
}

/// Columns `w{k}`/`v{k}` and LATERAL joins `s{k}` for the fixed pipeline steps.
fn step_sql(steps: &[PipelineStep]) -> (String, String) {
    let mut columns = String::new();
    let mut laterals = String::new();
    for (index, step) in steps.iter().enumerate() {
        let k = index + 1;
        let function = if step.inverse {
            "ST_InverseTransformPipeline"
        } else {
            "ST_TransformPipeline"
        };
        // Pipelines and SRIDs are compile-time registry constants, never request data.
        columns.push_str(&format!(
            ", ST_AsBinary(s{k}.g, 'NDR') AS w{k}, ST_IsValid(s{k}.g) AS v{k}"
        ));
        laterals.push_str(&format!(
            " CROSS JOIN LATERAL (SELECT {function}(s{prev}.g, '{pipeline}', {target}) AS g) AS s{k}",
            prev = index,
            pipeline = step.pipeline,
            target = step.target,
        ));
    }
    (columns, laterals)
}

fn step_error(steps: &[PipelineStep], error: sqlx::Error) -> TransferError {
    if steps.is_empty() || !matches!(error, sqlx::Error::Database(_)) {
        return error.into();
    }
    tracing::warn!(%error, "approved coordinate pipeline failed");
    TransferError::validation(
        "TRANSFORMATION_FAILED",
        "Approved coordinate transformation pipeline failed.",
    )
}

fn geometry_row(row: &PgRow, steps: usize, valid0: &str) -> Result<GeometryRow> {
    let mut wkb = Vec::with_capacity(steps + 1);
    let mut step_valid = Vec::with_capacity(steps);
    wkb.push(col::<Option<Vec<u8>>>(row, "w0")?);
    for k in 1..=steps {
        wkb.push(col::<Option<Vec<u8>>>(row, &format!("w{k}"))?);
        step_valid.push(col::<Option<bool>>(row, &format!("v{k}"))?.unwrap_or(false));
    }
    Ok(GeometryRow {
        valid: col::<Option<bool>>(row, valid0)?.unwrap_or(false),
        wkb,
        step_valid,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isoformat_matches_python_utc_rendering() {
        let whole = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        assert_eq!(iso(whole), "2023-11-14T22:13:20+00:00");
        let micro = DateTime::from_timestamp(1_700_000_000, 1_000).unwrap();
        assert_eq!(iso(micro), "2023-11-14T22:13:20.000001+00:00");
    }

    #[test]
    fn step_sql_chains_fixed_pipelines() {
        let (columns, laterals) = step_sql(&[
            PipelineStep {
                inverse: true,
                pipeline: "+proj=a",
                target: 4756,
            },
            PipelineStep {
                inverse: false,
                pipeline: "urn:b",
                target: 4326,
            },
        ]);
        assert!(columns.contains("AS w2") && columns.contains("AS v1"));
        assert!(laterals.contains("ST_InverseTransformPipeline(s0.g, '+proj=a', 4756) AS g) AS s1"));
        assert!(laterals.contains("ST_TransformPipeline(s1.g, 'urn:b', 4326) AS g) AS s2"));
    }
}
