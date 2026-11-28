//! E14 extension platform storage repository over canonical RLS tables.

pub mod marketplace;
pub mod partner;
pub mod runtime_v2;
pub mod records;
pub mod validation;

use std::collections::BTreeMap;
use std::fmt::Write;

use chrono::{DateTime, SecondsFormat, Utc};
use ovg_core::AppError;
use serde_json::{json, Map, Number, Value};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::catalog::ProjectRead;
use crate::editing::ServiceAuth;
use crate::DbPool;

pub const MAX_RECORDS: usize = 200;
pub const MAX_REVISIONS: i64 = 1_000;
pub const MAX_BUNDLE_BYTES: usize = 2 * 1024 * 1024;
pub const PORTABLE_FORMAT: &str = "openvietgis.extension.portable.v1";

/// FastAPI raises tenant-access failures as `TenantResourceNotFoundError` (`{detail}`),
/// not as a `PlatformDomainError` (`{code, message}`); the HTTP layer keys on this code.
pub const TENANT_NOT_FOUND: &str = "TENANT_NOT_FOUND";

fn tenant_not_found() -> AppError {
    AppError::NotFound {
        code: TENANT_NOT_FOUND,
        message: "organization not found".to_owned(),
    }
}

pub(crate) fn db_error(error: sqlx::Error) -> AppError {
    tracing::error!(%error, "extension database operation failed");
    AppError::internal("extension database operation failed")
}

/// FastAPI (Pydantic) serializes some timestamps as UTC `Z`, e.g. `2026-11-26T10:24:09.331155Z`.
#[must_use]
pub fn iso_z(value: DateTime<Utc>) -> String {
    iso(value).replacen("+00:00", "Z", 1)
}

#[must_use]
pub fn iso(value: DateTime<Utc>) -> String {
    let format = if value.timestamp_subsec_micros() == 0 {
        SecondsFormat::Secs
    } else {
        SecondsFormat::Micros
    };
    value.to_rfc3339_opts(format, false)
}

fn canonicalize_portable(value: &Value) -> Result<Value, AppError> {
    const LIMIT: i128 = 9_007_199_254_740_991;
    let invalid = || AppError::validation("VALIDATION_ERROR", "invalid portable JSON");
    match value {
        Value::Null | Value::Bool(_) | Value::String(_) => Ok(value.clone()),
        Value::Number(number) => {
            if let Some(v) = number.as_i64() {
                if i128::from(v).abs() > LIMIT {
                    return Err(invalid());
                }
                Ok(Value::Number(Number::from(v)))
            } else if let Some(v) = number.as_u64() {
                if i128::from(v) > LIMIT {
                    return Err(invalid());
                }
                Ok(Value::Number(Number::from(v)))
            } else if let Some(f) = number.as_f64() {
                if !f.is_finite() || f.abs() > LIMIT as f64 {
                    return Err(invalid());
                }
                if f.fract() == 0.0 {
                    Ok(Value::Number(Number::from(f as i64)))
                } else {
                    Number::from_f64(f).map(Value::Number).ok_or_else(invalid)
                }
            } else {
                Err(invalid())
            }
        }
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(canonicalize_portable(item)?);
            }
            Ok(Value::Array(out))
        }
        Value::Object(map) => {
            let mut sorted = BTreeMap::new();
            for (k, v) in map {
                sorted.insert(k.clone(), canonicalize_portable(v)?);
            }
            let mut out = Map::new();
            for (k, v) in sorted {
                out.insert(k, v);
            }
            Ok(Value::Object(out))
        }
    }
}

/// Deterministic portable JSON bytes matching Python `domain.canonical_bytes`.
pub fn canonical_bytes(value: &Value, limit: usize) -> Result<Vec<u8>, AppError> {
    let normalized = canonicalize_portable(value)?;
    let encoded = serde_json::to_vec(&normalized)
        .map_err(|_| AppError::validation("VALIDATION_ERROR", "invalid portable JSON"))?;
    if encoded.len() > limit {
        return Err(AppError::validation(
            "EXTENSION_TRANSFER_LIMIT",
            "portable data exceeds bounded transfer limit",
        ));
    }
    Ok(encoded)
}

pub fn bundle_digest(bundle: &Value) -> Result<String, AppError> {
    let bytes = canonical_bytes(bundle, MAX_BUNDLE_BYTES)?;
    let digest = Sha256::digest(&bytes);
    let mut out = String::with_capacity(64);
    for b in digest {
        let _ = write!(out, "{b:02x}");
    }
    Ok(out)
}

#[must_use]
pub fn semantic_eq(left: &Value, right: &Value) -> bool {
    match (
        canonical_bytes(left, usize::MAX),
        canonical_bytes(right, usize::MAX),
    ) {
        (Ok(a), Ok(b)) => a == b,
        _ => left == right,
    }
}

#[derive(Debug)]
pub struct ActivationManifestRow {
    pub wire: Value,
    pub extension_key: String,
    pub manifest_version: String,
    pub manifest_enabled: bool,
    pub manifest_contract: Value,
}

#[derive(Debug, Clone)]
pub struct ManifestContractRow {
    pub id: Uuid,
    pub extension_key: String,
    pub version: String,
    pub api_compatibility: String,
    pub display_name: String,
    pub description: String,
    pub permissions: Value,
    pub commands: Value,
    pub capabilities: Value,
    pub entity_schemas: Value,
    pub ui_contributions: Value,
    pub enabled: bool,
}

impl ManifestContractRow {
    #[must_use]
    pub fn contract(&self) -> Value {
        json!({
            "extension_key": self.extension_key,
            "version": self.version,
            "api_compatibility": self.api_compatibility,
            "display_name": self.display_name,
            "description": self.description,
            "permissions": self.permissions,
            "commands": self.commands,
            "capabilities": self.capabilities,
            "entity_schemas": self.entity_schemas,
            "ui_contributions": self.ui_contributions,
        })
    }

    #[must_use]
    pub fn portable_contract(&self) -> Value {
        json!({
            "extension_key": self.extension_key,
            "version": self.version,
            "api_compatibility": self.api_compatibility,
            "permissions": self.permissions,
            "commands": self.commands,
            "capabilities": self.capabilities,
            "entity_schemas": self.entity_schemas,
        })
    }
}

#[derive(Debug, Clone)]
pub struct ManifestRecord {
    pub contract: ManifestContractRow,
    pub organization_id: Uuid,
    pub created_at: DateTime<Utc>,
}

impl ManifestRecord {
    #[must_use]
    pub fn wire(&self) -> Value {
        json!({
            "id": self.contract.id,
            "organization_id": self.organization_id,
            "extension_key": self.contract.extension_key,
            "version": self.contract.version,
            "api_compatibility": self.contract.api_compatibility,
            "display_name": self.contract.display_name,
            "description": self.contract.description,
            "permissions": self.contract.permissions,
            "commands": self.contract.commands,
            "capabilities": self.contract.capabilities,
            "entity_schemas": self.contract.entity_schemas,
            "ui_contributions": self.contract.ui_contributions,
            "enabled": self.contract.enabled,
            "created_at": iso(self.created_at),
        })
    }
}

pub(crate) fn manifest_from_row(row: &sqlx::postgres::PgRow) -> Result<ManifestRecord, AppError> {
    Ok(ManifestRecord {
        contract: ManifestContractRow {
            id: row.try_get("id").map_err(db_error)?,
            extension_key: row.try_get("extension_key").map_err(db_error)?,
            version: row.try_get("version").map_err(db_error)?,
            api_compatibility: row.try_get("api_compatibility").map_err(db_error)?,
            display_name: row.try_get("display_name").map_err(db_error)?,
            description: row.try_get("description").map_err(db_error)?,
            permissions: row.try_get("permissions").map_err(db_error)?,
            commands: row.try_get("commands").map_err(db_error)?,
            capabilities: row.try_get("capabilities").map_err(db_error)?,
            entity_schemas: row.try_get("entity_schemas").map_err(db_error)?,
            ui_contributions: row.try_get("ui_contributions").map_err(db_error)?,
            enabled: row.try_get("enabled").map_err(db_error)?,
        },
        organization_id: row.try_get("organization_id").map_err(db_error)?,
        created_at: row.try_get("created_at").map_err(db_error)?,
    })
}

pub struct RegisterManifestInput<'a> {
    pub extension_key: &'a str,
    pub version: &'a str,
    pub api_compatibility: &'a str,
    pub display_name: &'a str,
    pub description: &'a str,
    pub permissions: &'a Value,
    pub commands: &'a Value,
    pub capabilities: &'a Value,
    pub entity_schemas: &'a Value,
    pub ui_contributions: &'a Value,
}

/// Organization-scoped RLS transaction for extension manifest discovery & registration.
pub struct OrgExtensionScope<'a> {
    tx: Transaction<'a, Postgres>,
    organization_id: Uuid,
    actor_id: Uuid,
    is_service: bool,
}

impl<'a> OrgExtensionScope<'a> {
    pub async fn begin_human(
        pool: &'a DbPool,
        organization_id: Uuid,
        actor_id: Uuid,
        session_id: Uuid,
    ) -> Result<Self, AppError> {
        if organization_id.is_nil() {
            return Err(tenant_not_found());
        }
        let mut tx = pool.inner().begin().await.map_err(db_error)?;
        sqlx::query(crate::rls::BIND_TENANT_GUCS_SQL)
            .bind("")
            .bind(actor_id.to_string())
            .bind("")
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;

        let safe_role: bool = sqlx::query_scalar(
            "SELECT current_user = 'openvietgis_runtime'
                 AND row_security_active('extensions.manifests'::regclass)",
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(db_error)?;
        if !safe_role {
            return Err(AppError::internal(
                "extensions require the PostGIS runtime RLS role",
            ));
        }

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
        .fetch_one(&mut *tx)
        .await
        .map_err(db_error)?;
        if !current_session {
            return Err(AppError::unauthorized("authentication required"));
        }

        let member: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                 SELECT 1 FROM org.organization_memberships
                 WHERE organization_id = $1 AND user_id = $2 AND status = 'active'
             )",
        )
        .bind(organization_id)
        .bind(actor_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(db_error)?;
        if !member {
            return Err(tenant_not_found());
        }

        sqlx::query(crate::rls::BIND_TENANT_GUCS_SQL)
            .bind(organization_id.to_string())
            .bind(actor_id.to_string())
            .bind("")
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;

        Ok(Self {
            tx,
            organization_id,
            actor_id,
            is_service: false,
        })
    }

    pub async fn begin_service(
        pool: &'a DbPool,
        organization_id: Uuid,
        auth: &ServiceAuth,
    ) -> Result<Self, AppError> {
        if auth.organization_id() != organization_id || organization_id.is_nil() {
            return Err(AppError::unauthorized("invalid service credential"));
        }
        let mut tx = pool.inner().begin().await.map_err(db_error)?;
        sqlx::query(crate::rls::BIND_TENANT_GUCS_SQL)
            .bind(organization_id.to_string())
            .bind("")
            .bind("")
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;

        let safe_role: bool = sqlx::query_scalar(
            "SELECT current_user = 'openvietgis_runtime'
                 AND row_security_active('extensions.manifests'::regclass)",
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(db_error)?;
        if !safe_role {
            return Err(AppError::internal(
                "extensions require the PostGIS runtime RLS role",
            ));
        }

        let valid: Option<Uuid> = sqlx::query_scalar(
            "UPDATE org.service_credentials c
             SET last_used_at = GREATEST(c.last_used_at, clock_timestamp())
             FROM org.service_principals sp
             JOIN org.organizations o ON o.id = sp.organization_id
             WHERE c.organization_id = $1 AND c.service_principal_id = $2 AND c.id = $3
               AND c.revoked_at IS NULL
               AND (c.expires_at IS NULL OR c.expires_at > clock_timestamp())
               AND sp.organization_id = c.organization_id AND sp.id = c.service_principal_id
               AND sp.status = 'active' AND sp.revoked_at IS NULL
               AND o.status = 'active'
             RETURNING sp.id",
        )
        .bind(organization_id)
        .bind(auth.principal_id())
        .bind(auth.credential_id())
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_error)?;
        if valid.is_none() {
            return Err(AppError::unauthorized("invalid service credential"));
        }

        Ok(Self {
            tx,
            organization_id,
            actor_id: auth.principal_id(),
            is_service: true,
        })
    }

    pub async fn list_manifests(&mut self) -> Result<Vec<ManifestRecord>, AppError> {
        let rows = sqlx::query(
            "SELECT * FROM extensions.manifests
             WHERE organization_id = $1
             ORDER BY extension_key ASC, created_at DESC",
        )
        .bind(self.organization_id)
        .fetch_all(&mut *self.tx)
        .await
        .map_err(db_error)?;
        rows.iter().map(manifest_from_row).collect()
    }

    pub async fn register_manifest(
        &mut self,
        input: RegisterManifestInput<'_>,
    ) -> Result<ManifestRecord, AppError> {
        let org_status: Option<String> =
            sqlx::query_scalar("SELECT status FROM org.organizations WHERE id = $1 FOR UPDATE")
                .bind(self.organization_id)
                .fetch_optional(&mut *self.tx)
                .await
                .map_err(db_error)?;
        if org_status.as_deref() != Some("active") {
            return Err(tenant_not_found());
        }
        if self.is_service {
            return Err(AppError::forbidden(
                "manifest registration requires human administrator",
            ));
        }
        let membership = sqlx::query_as::<_, (String, String)>(
            "SELECT role, status FROM org.organization_memberships
             WHERE organization_id = $1 AND user_id = $2 FOR UPDATE",
        )
        .bind(self.organization_id)
        .bind(self.actor_id)
        .fetch_optional(&mut *self.tx)
        .await
        .map_err(db_error)?;
        let allowed = membership.is_some_and(|(role, status)| {
            status == "active" && matches!(role.as_str(), "owner" | "admin")
        });
        if !allowed {
            return Err(AppError::forbidden(
                "manifest registration requires organization settings authority",
            ));
        }

        validation::validate_manifest_fields(
            input.extension_key,
            input.version,
            input.api_compatibility,
            input.permissions,
            input.commands,
            input.capabilities,
            input.entity_schemas,
            input.ui_contributions,
        )?;
        let trimmed_name = input.display_name.trim();
        if trimmed_name.is_empty()
            || input.display_name.chars().count() > 200
            || input.description.chars().count() > 500
        {
            return Err(AppError::validation(
                "VALIDATION_ERROR",
                "invalid manifest display name or description",
            ));
        }

        let exists: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM extensions.manifests
             WHERE organization_id = $1 AND extension_key = $2 AND version = $3",
        )
        .bind(self.organization_id)
        .bind(input.extension_key)
        .bind(input.version)
        .fetch_optional(&mut *self.tx)
        .await
        .map_err(db_error)?;
        if exists.is_some() {
            return Err(AppError::conflict(
                "MANIFEST_EXISTS",
                format!(
                    "manifest {}@{} already registered",
                    input.extension_key, input.version
                ),
            ));
        }

        let dname: String = trimmed_name.chars().take(200).collect();
        let desc: String = input.description.trim().chars().take(500).collect();
        let now = Utc::now();
        let row = sqlx::query(
            "INSERT INTO extensions.manifests (
                id, organization_id, extension_key, version, api_compatibility,
                display_name, description, permissions, commands, capabilities,
                entity_schemas, ui_contributions, enabled, registered_by_actor_id, created_at
             ) VALUES (
                $1, $2, $3, $4, $5,
                $6, $7, $8, $9, $10, $11, $12,
                true, $13, $14
             ) RETURNING *",
        )
        .bind(Uuid::new_v4())
        .bind(self.organization_id)
        .bind(input.extension_key)
        .bind(input.version)
        .bind(input.api_compatibility)
        .bind(&dname)
        .bind(&desc)
        .bind(input.permissions)
        .bind(input.commands)
        .bind(input.capabilities)
        .bind(input.entity_schemas)
        .bind(input.ui_contributions)
        .bind(self.actor_id)
        .bind(now)
        .fetch_one(&mut *self.tx)
        .await
        .map_err(db_error)?;
        manifest_from_row(&row)
    }

    pub async fn commit(self) -> Result<(), AppError> {
        self.tx.commit().await.map_err(db_error)
    }
}

pub async fn list_activations(
    read: &mut ProjectRead<'_>,
) -> Result<Vec<ActivationManifestRow>, AppError> {
    let rows = sqlx::query(
        "SELECT
            a.id, a.manifest_id, a.extension_key, a.status, a.revision,
            a.activated_at, a.disabled_at, a.updated_at,
            m.version, m.api_compatibility, m.display_name, m.description,
            m.permissions, m.commands, m.capabilities, m.entity_schemas,
            m.ui_contributions, m.enabled
         FROM extensions.project_activations a
         JOIN extensions.manifests m
           ON m.organization_id = a.organization_id AND m.id = a.manifest_id
         WHERE a.organization_id = $1 AND a.project_id = $2
         ORDER BY a.extension_key ASC",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(db_error)?;

    rows.into_iter()
        .map(|row| {
            let extension_key: String = row.try_get("extension_key").map_err(db_error)?;
            let version: String = row.try_get("version").map_err(db_error)?;
            let disabled_at = row
                .try_get::<Option<DateTime<Utc>>, _>("disabled_at")
                .map_err(db_error)?
                .map(iso);
            let wire = json!({
                "id": row.try_get::<Uuid, _>("id").map_err(db_error)?,
                "manifest_id": row.try_get::<Uuid, _>("manifest_id").map_err(db_error)?,
                "extension_key": extension_key,
                "status": row.try_get::<String, _>("status").map_err(db_error)?,
                "revision": row.try_get::<i64, _>("revision").map_err(db_error)?,
                "activated_at": iso(row.try_get::<DateTime<Utc>, _>("activated_at").map_err(db_error)?),
                "disabled_at": disabled_at,
                "updated_at": iso(row.try_get::<DateTime<Utc>, _>("updated_at").map_err(db_error)?),
            });
            let manifest_contract = json!({
                "extension_key": extension_key,
                "version": version,
                "api_compatibility": row.try_get::<String, _>("api_compatibility").map_err(db_error)?,
                "display_name": row.try_get::<String, _>("display_name").map_err(db_error)?,
                "description": row.try_get::<String, _>("description").map_err(db_error)?,
                "permissions": row.try_get::<Value, _>("permissions").map_err(db_error)?,
                "commands": row.try_get::<Value, _>("commands").map_err(db_error)?,
                "capabilities": row.try_get::<Value, _>("capabilities").map_err(db_error)?,
                "entity_schemas": row.try_get::<Value, _>("entity_schemas").map_err(db_error)?,
                "ui_contributions": row.try_get::<Value, _>("ui_contributions").map_err(db_error)?,
            });
            Ok(ActivationManifestRow {
                wire,
                extension_key,
                manifest_version: version,
                manifest_enabled: row.try_get("enabled").map_err(db_error)?,
                manifest_contract,
            })
        })
        .collect()
}

pub(crate) async fn activation_history_items(
    read: &mut ProjectRead<'_>,
    activation_id: Uuid,
) -> Result<Vec<Value>, AppError> {
    let rows = sqlx::query(
        "SELECT id, event, previous_manifest_id, manifest_id, status, actor_id, occurred_at
         FROM extensions.activation_events
         WHERE organization_id = $1 AND project_id = $2 AND activation_id = $3
         ORDER BY occurred_at, id
         LIMIT $4",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(activation_id)
    .bind(MAX_REVISIONS + 1)
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(db_error)?;

    if rows.len() > MAX_REVISIONS as usize {
        return Err(AppError::validation(
            "EXTENSION_TRANSFER_LIMIT",
            "activation history exceeds portable limit",
        ));
    }

    rows.into_iter()
        .map(|row| {
            let previous_manifest_id = row
                .try_get::<Option<Uuid>, _>("previous_manifest_id")
                .map_err(db_error)?;
            Ok(json!({
                "id": row.try_get::<Uuid, _>("id").map_err(db_error)?,
                "event": row.try_get::<String, _>("event").map_err(db_error)?,
                "previous_manifest_id": previous_manifest_id,
                "manifest_id": row.try_get::<Uuid, _>("manifest_id").map_err(db_error)?,
                "status": row.try_get::<String, _>("status").map_err(db_error)?,
                "actor_id": row.try_get::<Uuid, _>("actor_id").map_err(db_error)?,
                "occurred_at": iso(
                    row.try_get::<DateTime<Utc>, _>("occurred_at")
                        .map_err(db_error)?
                ),
            }))
        })
        .collect()
}

pub async fn activation_history(
    read: &mut ProjectRead<'_>,
    activation_id: Uuid,
) -> Result<Value, AppError> {
    let visible = sqlx::query_scalar::<_, Uuid>(
        "SELECT id
         FROM extensions.project_activations
         WHERE organization_id = $1 AND project_id = $2 AND id = $3",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(activation_id)
    .fetch_optional(&mut **read.transaction())
    .await
    .map_err(db_error)?;
    if visible.is_none() {
        return Err(AppError::not_found("extension activation not found"));
    }
    let items = activation_history_items(read, activation_id).await?;
    Ok(json!({ "items": items }))
}

pub async fn manifest(
    read: &mut ProjectRead<'_>,
    manifest_id: Uuid,
) -> Result<Option<ManifestContractRow>, AppError> {
    let row = sqlx::query(
        "SELECT id,extension_key,version,api_compatibility,display_name,description,
                permissions,commands,capabilities,entity_schemas,ui_contributions,enabled
         FROM extensions.manifests
         WHERE organization_id=$1 AND id=$2",
    )
    .bind(read.organization_id())
    .bind(manifest_id)
    .fetch_optional(&mut **read.transaction())
    .await
    .map_err(db_error)?;
    row.map(|row| {
        Ok(ManifestContractRow {
            id: row.try_get("id").map_err(db_error)?,
            extension_key: row.try_get("extension_key").map_err(db_error)?,
            version: row.try_get("version").map_err(db_error)?,
            api_compatibility: row.try_get("api_compatibility").map_err(db_error)?,
            display_name: row.try_get("display_name").map_err(db_error)?,
            description: row.try_get("description").map_err(db_error)?,
            permissions: row.try_get("permissions").map_err(db_error)?,
            commands: row.try_get("commands").map_err(db_error)?,
            capabilities: row.try_get("capabilities").map_err(db_error)?,
            entity_schemas: row.try_get("entity_schemas").map_err(db_error)?,
            ui_contributions: row.try_get("ui_contributions").map_err(db_error)?,
            enabled: row.try_get("enabled").map_err(db_error)?,
        })
    })
    .transpose()
}

pub(crate) async fn activation_row(
    read: &mut ProjectRead<'_>,
    extension_key: &str,
) -> Result<sqlx::postgres::PgRow, AppError> {
    sqlx::query(
        "SELECT * FROM extensions.project_activations
         WHERE organization_id = $1 AND project_id = $2 AND extension_key = $3",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(extension_key)
    .fetch_optional(&mut **read.transaction())
    .await
    .map_err(db_error)?
    .ok_or_else(|| AppError::not_found("extension activation not found"))
}

pub(crate) async fn active_manifest(
    read: &mut ProjectRead<'_>,
    extension_key: &str,
) -> Result<ManifestContractRow, AppError> {
    let row = activation_row(read, extension_key).await?;
    let status: String = row.try_get("status").map_err(db_error)?;
    let manifest_id: Uuid = row.try_get("manifest_id").map_err(db_error)?;
    let m = manifest(read, manifest_id)
        .await?
        .ok_or_else(|| AppError::not_found("extension manifest not found"))?;
    if status != "active"
        || m.extension_key != extension_key
        || !m.enabled
        || m.api_compatibility != "1.x"
    {
        return Err(AppError::not_found(
            "extension is not active and compatible in this project",
        ));
    }
    if !validation::validate_generic(&m) {
        return Err(AppError::conflict(
            "EXTENSION_MANIFEST_REVALIDATION_REQUIRED",
            "stored manifest no longer satisfies the metadata contract",
        ));
    }
    Ok(m)
}

fn activation_wire(row: &sqlx::postgres::PgRow) -> Result<Value, AppError> {
    Ok(json!({
        "id": row.try_get::<Uuid,_>("id").map_err(db_error)?,
        "manifest_id": row.try_get::<Uuid,_>("manifest_id").map_err(db_error)?,
        "extension_key": row.try_get::<String,_>("extension_key").map_err(db_error)?,
        "status": row.try_get::<String,_>("status").map_err(db_error)?,
        "revision": row.try_get::<i64,_>("revision").map_err(db_error)?,
        "activated_at": iso(row.try_get::<DateTime<Utc>,_>("activated_at").map_err(db_error)?),
        "disabled_at": row.try_get::<Option<DateTime<Utc>>,_>("disabled_at").map_err(db_error)?.map(iso),
        "updated_at": iso(row.try_get::<DateTime<Utc>,_>("updated_at").map_err(db_error)?),
    }))
}

async fn activation_event(
    edit: &mut ProjectRead<'_>,
    row: &sqlx::postgres::PgRow,
    actor_id: Uuid,
    event: &str,
    previous_manifest_id: Option<Uuid>,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO extensions.activation_events (
           id,organization_id,project_id,activation_id,event,previous_manifest_id,
           manifest_id,status,actor_id,occurred_at
         ) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
    )
    .bind(Uuid::new_v4())
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(row.try_get::<Uuid, _>("id").map_err(db_error)?)
    .bind(event)
    .bind(previous_manifest_id)
    .bind(row.try_get::<Uuid, _>("manifest_id").map_err(db_error)?)
    .bind(row.try_get::<String, _>("status").map_err(db_error)?)
    .bind(actor_id)
    .bind(
        row.try_get::<DateTime<Utc>, _>("updated_at")
            .map_err(db_error)?,
    )
    .execute(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn append_project_audit(
    edit: &mut ProjectRead<'_>,
    actor_id: Uuid,
    actor_display: &str,
    action: &str,
    resource_type: &str,
    resource_id: Uuid,
    metadata: Value,
    occurred_at: DateTime<Utc>,
) -> Result<(), AppError> {
    let actor_type = edit.actor_type();
    sqlx::query(
        "INSERT INTO audit.audit_events (
          id,organization_id,actor_type,actor_id,actor_display_snapshot,
          action,resource_type,resource_id,outcome,metadata,created_at
         ) VALUES($1,$2,$3,$4,$5,$6,$7,$8,'success',$9,$10)",
    )
    .bind(Uuid::new_v4())
    .bind(edit.organization_id())
    .bind(actor_type)
    .bind(actor_id)
    .bind(actor_display)
    .bind(action)
    .bind(resource_type)
    .bind(resource_id)
    .bind(metadata)
    .bind(occurred_at)
    .execute(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    Ok(())
}

pub async fn activate(
    edit: &mut ProjectRead<'_>,
    actor_id: Uuid,
    actor_display: &str,
    manifest: &ManifestContractRow,
) -> Result<Value, AppError> {
    let existing = sqlx::query_scalar::<_, Uuid>(
        "SELECT manifest_id FROM extensions.project_activations
         WHERE organization_id=$1 AND project_id=$2 AND extension_key=$3
         FOR UPDATE",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(&manifest.extension_key)
    .fetch_optional(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    if existing.is_some_and(|id| id != manifest.id) {
        return Err(AppError::conflict(
            "EXTENSION_VERSION_TRANSITION_REQUIRED",
            "version transitions require a verified migration; implicit replacement is blocked",
        ));
    }
    let now = Utc::now();
    let row = sqlx::query(
        "INSERT INTO extensions.project_activations (
           id,organization_id,project_id,manifest_id,extension_key,status,revision,
           activated_by_actor_id,activated_at,disabled_by_actor_id,disabled_at,updated_at
         ) VALUES($1,$2,$3,$4,$5,'active',1,$6,$7,NULL,NULL,$7)
         ON CONFLICT (organization_id,project_id,extension_key) DO UPDATE
         SET status='active',
             revision=extensions.project_activations.revision+1,
             activated_by_actor_id=EXCLUDED.activated_by_actor_id,
             activated_at=EXCLUDED.activated_at,
             disabled_by_actor_id=NULL,disabled_at=NULL,updated_at=EXCLUDED.updated_at
         RETURNING *",
    )
    .bind(Uuid::new_v4())
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(manifest.id)
    .bind(&manifest.extension_key)
    .bind(actor_id)
    .bind(now)
    .fetch_one(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    let activation_id = row.try_get::<Uuid, _>("id").map_err(db_error)?;
    append_project_audit(
        edit,
        actor_id,
        actor_display,
        "extension.activate",
        "extension_activation",
        activation_id,
        json!({"manifest_id":manifest.id,"extension_key":manifest.extension_key}),
        now,
    )
    .await?;
    activation_event(edit, &row, actor_id, "activate", existing).await?;
    activation_wire(&row)
}

pub async fn disable(
    edit: &mut ProjectRead<'_>,
    actor_id: Uuid,
    actor_display: &str,
    extension_key: &str,
) -> Result<Value, AppError> {
    let now = Utc::now();
    let changed = sqlx::query(
        "UPDATE extensions.project_activations
         SET status='disabled',revision=revision+1,disabled_by_actor_id=$4,
             disabled_at=$5,updated_at=$5
         WHERE organization_id=$1 AND project_id=$2 AND extension_key=$3 AND status='active'
         RETURNING *",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(extension_key)
    .bind(actor_id)
    .bind(now)
    .fetch_optional(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    let (row, changed) = if let Some(row) = changed {
        (row, true)
    } else {
        let row = sqlx::query(
            "SELECT * FROM extensions.project_activations
         WHERE organization_id=$1 AND project_id=$2 AND extension_key=$3",
        )
        .bind(edit.organization_id())
        .bind(edit.project_id())
        .bind(extension_key)
        .fetch_optional(&mut **edit.transaction())
        .await
        .map_err(db_error)?
        .ok_or_else(|| AppError::not_found("project extension activation not found"))?;
        (row, false)
    };
    if changed {
        let activation_id = row.try_get::<Uuid, _>("id").map_err(db_error)?;
        let manifest_id = row.try_get::<Uuid, _>("manifest_id").map_err(db_error)?;
        let revision = row.try_get::<i64, _>("revision").map_err(db_error)?;
        append_project_audit(
            edit,
            actor_id,
            actor_display,
            "extension.disable",
            "extension_activation",
            activation_id,
            json!({"manifest_id":manifest_id,"extension_key":extension_key,"revision":revision}),
            now,
        )
        .await?;
        activation_event(edit, &row, actor_id, "deactivate", Some(manifest_id)).await?;
    }
    activation_wire(&row)
}

pub async fn deactivate(
    edit: &mut ProjectRead<'_>,
    actor_id: Uuid,
    actor_display: &str,
    activation_id: Uuid,
) -> Result<Value, AppError> {
    let now = Utc::now();
    let row = sqlx::query(
        "UPDATE extensions.project_activations
         SET status='disabled',updated_at=$4
         WHERE id=$3 AND organization_id=$1 AND project_id=$2
         RETURNING *",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(activation_id)
    .bind(now)
    .fetch_optional(&mut **edit.transaction())
    .await
    .map_err(db_error)?
    .ok_or_else(|| AppError::not_found("extension activation not found"))?;
    let manifest_id = row.try_get::<Uuid, _>("manifest_id").map_err(db_error)?;
    let extension_key = row
        .try_get::<String, _>("extension_key")
        .map_err(db_error)?;
    append_project_audit(
        edit,
        actor_id,
        actor_display,
        "extension.deactivate",
        "extension_activation",
        activation_id,
        json!({"manifest_id":manifest_id,"extension_key":extension_key}),
        now,
    )
    .await?;
    activation_event(edit, &row, actor_id, "deactivate", Some(manifest_id)).await?;
    activation_wire(&row)
}

pub async fn transition(
    edit: &mut ProjectRead<'_>,
    actor_id: Uuid,
    actor_display: &str,
    activation_id: Uuid,
    expected_manifest_id: Uuid,
    target_manifest_id: Uuid,
) -> Result<Value, AppError> {
    let current = sqlx::query(
        "SELECT * FROM extensions.project_activations
         WHERE organization_id=$1 AND project_id=$2 AND id=$3 FOR UPDATE",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(activation_id)
    .fetch_optional(&mut **edit.transaction())
    .await
    .map_err(db_error)?
    .ok_or_else(|| AppError::not_found("extension activation not found"))?;
    let current_manifest = current
        .try_get::<Uuid, _>("manifest_id")
        .map_err(db_error)?;
    if current_manifest != expected_manifest_id {
        return Err(AppError::conflict(
            "STALE_EXTENSION_VERSION",
            "activation version changed",
        ));
    }
    let now = Utc::now();
    let row = sqlx::query(
        "UPDATE extensions.project_activations
         SET manifest_id=$4,updated_at=$5
         WHERE organization_id=$1 AND project_id=$2 AND id=$3 AND manifest_id=$6
         RETURNING *",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(activation_id)
    .bind(target_manifest_id)
    .bind(now)
    .bind(expected_manifest_id)
    .fetch_one(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    activation_event(
        edit,
        &row,
        actor_id,
        "transition",
        Some(expected_manifest_id),
    )
    .await?;
    append_project_audit(
        edit,
        actor_id,
        actor_display,
        "extension.transition",
        "extension_activation",
        activation_id,
        json!({"previous_manifest_id":expected_manifest_id,"manifest_id":target_manifest_id,"mode":"metadata_only"}),
        now,
    )
    .await?;
    activation_wire(&row)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activation_history_limit_matches_python() {
        assert_eq!(MAX_REVISIONS, 1_000);
    }

    #[test]
    fn python_isoformat_uses_explicit_utc_offset() {
        let value = DateTime::parse_from_rfc3339("2026-11-26T03:04:05.123456+00:00")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(iso(value), "2026-11-26T03:04:05.123456+00:00");

        let zero = DateTime::parse_from_rfc3339("2026-11-26T03:04:05+00:00")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(iso(zero), "2026-11-26T03:04:05+00:00");

        let millis = DateTime::parse_from_rfc3339("2026-11-26T03:04:05.123000+00:00")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(iso(millis), "2026-11-26T03:04:05.123000+00:00");
    }

    #[test]
    fn canonical_digest_preserves_semantic_numbers_and_unicode() {
        let a = json!({"a": 12.0, "b": false, "text": "Hà Nội"});
        let b = json!({"text": "Hà Nội", "b": false, "a": 12});
        assert_eq!(bundle_digest(&a).unwrap(), bundle_digest(&b).unwrap());
        assert_ne!(
            bundle_digest(&json!({"a": false})).unwrap(),
            bundle_digest(&json!({"a": 0})).unwrap()
        );
    }
}
