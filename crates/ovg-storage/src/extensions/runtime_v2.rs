//! Canonical project activation pin and append-only command receipt.
use super::db_error;
use crate::catalog::ProjectRead;
use ovg_core::AppError;
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

pub struct ActivePin {
    pub activation_id: Uuid,
    pub manifest_id: Uuid,
    pub revision: i64,
    pub manifest: Value,
}
pub async fn active_pin(
    read: &mut ProjectRead<'_>,
    manifest_id: Uuid,
) -> Result<ActivePin, AppError> {
    if read.is_gis_scoped() || read.actor_type() != "human" {
        return Err(AppError::forbidden(
            "unrestricted human project read required",
        ));
    }
    let row=sqlx::query("SELECT a.id, a.revision, m.extension_key, m.version, m.api_compatibility, m.display_name,
      m.description, m.permissions, m.commands, m.capabilities, m.entity_schemas, m.ui_contributions
      FROM extensions.project_activations a JOIN extensions.manifests m
        ON m.organization_id=a.organization_id AND m.id=a.manifest_id
      WHERE a.organization_id=$1 AND a.project_id=$2 AND a.manifest_id=$3 AND a.status='active' AND m.enabled
      FOR SHARE OF a")
        .bind(read.organization_id()).bind(read.project_id()).bind(manifest_id)
        .fetch_optional(&mut **read.transaction()).await.map_err(db_error)?
        .ok_or_else(||AppError::not_found("active plugin not found"))?;
    let manifest = json!({
        "extension_key":row.try_get::<String,_>("extension_key").map_err(db_error)?,
        "version":row.try_get::<String,_>("version").map_err(db_error)?,
        "api_compatibility":row.try_get::<String,_>("api_compatibility").map_err(db_error)?,
        "display_name":row.try_get::<String,_>("display_name").map_err(db_error)?,
        "description":row.try_get::<String,_>("description").map_err(db_error)?,
        "permissions":row.try_get::<Value,_>("permissions").map_err(db_error)?,
        "commands":row.try_get::<Value,_>("commands").map_err(db_error)?,
        "capabilities":row.try_get::<Value,_>("capabilities").map_err(db_error)?,
        "entity_schemas":row.try_get::<Value,_>("entity_schemas").map_err(db_error)?,
        "ui_contributions":row.try_get::<Value,_>("ui_contributions").map_err(db_error)?,
    });
    Ok(ActivePin {
        activation_id: row.try_get("id").map_err(db_error)?,
        manifest_id,
        revision: row.try_get("revision").map_err(db_error)?,
        manifest,
    })
}

pub struct ReceiptInput<'a> {
    pub request_id: Uuid,
    pub release_digest: &'a str,
    pub request_digest: &'a str,
    pub command_id: &'a str,
}
/// Serialized per actor/project. Read-only commands have no mutation effects;
/// store only fingerprints, not user input/notice text.
pub async fn receipt(
    read: &mut ProjectRead<'_>,
    pin: &ActivePin,
    input: ReceiptInput<'_>,
) -> Result<Uuid, AppError> {
    let org = read.organization_id();
    let project = read.project_id();
    let actor = read.actor_id();
    if read.is_gis_scoped() || read.actor_type() != "human" || input.request_id.is_nil() {
        return Err(AppError::forbidden("invalid invocation scope"));
    }
    let lock_key = format!("plugin-v2:{org}:{project}:{actor}");
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(lock_key)
        .execute(&mut **read.transaction())
        .await
        .map_err(db_error)?;
    let previous=sqlx::query("SELECT id,activation_id,activation_revision,manifest_id,release_digest,request_digest,command_id
        FROM extensions.plugin_v2_receipts WHERE organization_id=$1 AND project_id=$2 AND actor_id=$3 AND request_id=$4")
        .bind(org).bind(project).bind(actor).bind(input.request_id).fetch_optional(&mut **read.transaction()).await.map_err(db_error)?;
    if let Some(row) = previous {
        if row.try_get::<Uuid, _>("activation_id").map_err(db_error)? != pin.activation_id
            || row.try_get::<Uuid, _>("manifest_id").map_err(db_error)? != pin.manifest_id
            || row
                .try_get::<i64, _>("activation_revision")
                .map_err(db_error)?
                != pin.revision
            || row
                .try_get::<String, _>("release_digest")
                .map_err(db_error)?
                != input.release_digest
            || row
                .try_get::<String, _>("request_digest")
                .map_err(db_error)?
                != input.request_digest
            || row.try_get::<String, _>("command_id").map_err(db_error)? != input.command_id
        {
            return Err(AppError::conflict(
                "PLUGIN_REQUEST_CONFLICT",
                "request ID already used for different invocation",
            ));
        }
        return row.try_get("id").map_err(db_error);
    }
    let n:i64=sqlx::query_scalar("SELECT count(*) FROM extensions.plugin_v2_receipts WHERE organization_id=$1 AND project_id=$2 AND actor_id=$3 AND created_at>clock_timestamp()-interval '1 minute'")
        .bind(org).bind(project).bind(actor).fetch_one(&mut **read.transaction()).await.map_err(db_error)?;
    if n >= 60 {
        return Err(AppError::rate_limited(Some(60)));
    }
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO extensions.plugin_v2_receipts (id,organization_id,project_id,actor_id,request_id,activation_id,activation_revision,manifest_id,release_digest,request_digest,command_id)
      VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)")
      .bind(id).bind(org).bind(project).bind(actor).bind(input.request_id).bind(pin.activation_id).bind(pin.revision).bind(pin.manifest_id)
      .bind(input.release_digest).bind(input.request_digest).bind(input.command_id).execute(&mut **read.transaction()).await.map_err(db_error)?;
    Ok(id)
}

/// The newest event for an activation is authoritative. Any activation revision,
/// manifest or release change invalidates prior approval, including rollback.
pub struct GrantState {
    pub granted: bool,
    pub sequence: i64,
    pub lock_digest: Option<String>,
    pub catalog_epoch: Option<i64>,
}
pub async fn grant_status(
    read: &mut ProjectRead<'_>,
    pin: &ActivePin,
    digest: &str,
) -> Result<GrantState, AppError> {
    if read.is_gis_scoped() || read.actor_type() != "human" {
        return Err(AppError::forbidden("unrestricted human project required"));
    }
    let row=sqlx::query("SELECT sequence,activation_revision,manifest_id,release_digest,enabled,lock_digest,catalog_epoch FROM extensions.plugin_v2_grant_events
      WHERE organization_id=$1 AND project_id=$2 AND activation_id=$3 ORDER BY sequence DESC LIMIT 1")
      .bind(read.organization_id()).bind(read.project_id()).bind(pin.activation_id)
      .fetch_optional(&mut **read.transaction()).await.map_err(db_error)?;
    match row {
        None => Ok(GrantState {
            granted: false,
            sequence: 0,
            lock_digest: None,
            catalog_epoch: None,
        }),
        Some(row) => Ok(GrantState {
            sequence: row.try_get("sequence").map_err(db_error)?,
            lock_digest: row.try_get("lock_digest").map_err(db_error)?,
            catalog_epoch: row.try_get("catalog_epoch").map_err(db_error)?,
            granted: row.try_get::<bool, _>("enabled").map_err(db_error)?
                && row
                    .try_get::<i64, _>("activation_revision")
                    .map_err(db_error)?
                    == pin.revision
                && row.try_get::<Uuid, _>("manifest_id").map_err(db_error)? == pin.manifest_id
                && row
                    .try_get::<String, _>("release_digest")
                    .map_err(db_error)?
                    == digest,
        }),
    }
}
/// The manager proof is derived by ProjectRead::begin, never from a client flag.
/// The canonical project FOR UPDATE lock serializes with invocation FOR SHARE.
pub async fn set_grant(
    read: &mut ProjectRead<'_>,
    pin: &ActivePin,
    digest: &str,
    enabled: bool,
    expected_sequence: i64,
    approval: Option<DependencyApproval<'_>>,
) -> Result<GrantState, AppError> {
    if !read.is_project_manager() || read.is_gis_scoped() || read.actor_type() != "human" {
        return Err(AppError::forbidden("human project manager required"));
    }
    if digest.len() != 64
        || !digest
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(AppError::validation(
            "INVALID_PLUGIN_RELEASE",
            "invalid digest",
        ));
    }
    let current = grant_status(read, pin, digest).await?;
    if current.sequence != expected_sequence {
        return Err(AppError::conflict(
            "STALE_PLUGIN_GRANT",
            "approval changed; reload plugin",
        ));
    }
    if enabled != approval.is_some() {
        return Err(AppError::validation(
            "INVALID_PLUGIN_LOCK",
            "approval requires a dependency lock",
        ));
    }
    if let Some(a) = &approval {
        if a.epoch < minimum_approved_epoch(read).await? {
            return Err(AppError::conflict(
                "STALE_PLUGIN_CATALOG",
                "catalog epoch rollback",
            ));
        }
        if current.granted
            && current.lock_digest.as_deref() == Some(a.digest)
            && current.catalog_epoch == Some(a.epoch)
        {
            return Ok(current);
        }
    }
    let lock = approval.as_ref().map(|a| a.pins);
    let lock_digest = approval.as_ref().map(|a| a.digest);
    let epoch = approval.as_ref().map(|a| a.epoch);
    let sequence:i64=sqlx::query_scalar("INSERT INTO extensions.plugin_v2_grant_events
      (organization_id,project_id,activation_id,activation_revision,manifest_id,actor_id,release_digest,enabled,dependency_lock,lock_digest,catalog_epoch)
      VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) RETURNING sequence")
      .bind(read.organization_id()).bind(read.project_id()).bind(pin.activation_id).bind(pin.revision)
      .bind(pin.manifest_id).bind(read.actor_id()).bind(digest).bind(enabled).bind(lock).bind(lock_digest).bind(epoch)
      .fetch_one(&mut **read.transaction()).await.map_err(db_error)?;
    Ok(GrantState {
        granted: enabled,
        sequence,
        lock_digest: lock_digest.map(str::to_owned),
        catalog_epoch: epoch,
    })
}

/// Only currently active manifest identities for the requested signed catalog keys.
/// The canonical project lock remains held by the caller.
pub async fn dependency_candidates(
    read: &mut ProjectRead<'_>,
    keys: &[String],
) -> Result<Vec<(Uuid, String, String)>, AppError> {
    if read.is_gis_scoped() || read.actor_type() != "human" || keys.len() > 100 {
        return Err(AppError::forbidden("invalid dependency scope"));
    }
    sqlx::query_as(
        "SELECT m.id,m.extension_key,m.version
        FROM extensions.project_activations a JOIN extensions.manifests m
          ON m.organization_id=a.organization_id AND m.id=a.manifest_id
        WHERE a.organization_id=$1 AND a.project_id=$2 AND a.status='active' AND m.enabled
          AND m.extension_key=ANY($3) ORDER BY m.extension_key FOR SHARE OF a",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(keys)
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(db_error)
}

pub struct DependencyApproval<'a> {
    pub pins: &'a Value,
    pub digest: &'a str,
    pub epoch: i64,
}
/// Read the maximum previously approved or switched epoch for this project,
/// including revoked events. A stale operator catalog cannot re-enable an older approval.
pub async fn minimum_approved_epoch(read: &mut ProjectRead<'_>) -> Result<i64, AppError> {
    if read.is_gis_scoped() || read.actor_type() != "human" {
        return Err(AppError::forbidden("unrestricted human project required"));
    }
    let v: Option<i64> = sqlx::query_scalar(
        "SELECT max(catalog_epoch) FROM (
           SELECT catalog_epoch FROM extensions.plugin_v2_grant_events WHERE organization_id=$1 AND project_id=$2
           UNION ALL
           SELECT catalog_epoch FROM extensions.plugin_graph_switches WHERE organization_id=$1 AND project_id=$2
        ) accepted",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .fetch_one(&mut **read.transaction())
    .await
    .map_err(db_error)?;
    Ok(v.unwrap_or(0))
}

/// Actor-owned immutable fingerprints. No document, input, output or
/// cross-actor query. Available even when the plugin is disabled.
pub async fn list_own_receipts(
    read: &mut ProjectRead<'_>,
    after_id: Option<Uuid>,
    limit: u32,
) -> Result<Value, AppError> {
    if read.is_gis_scoped() || read.actor_type() != "human" || !(1..=100).contains(&limit) {
        return Err(AppError::forbidden("invalid receipt export scope"));
    }
    let org = read.organization_id();
    let project = read.project_id();
    let actor = read.actor_id();
    let mut rows=sqlx::query_scalar::<_,Value>("SELECT jsonb_build_object('id',id,'request_id',request_id,'activation_id',activation_id,
        'activation_revision',activation_revision::text,'manifest_id',manifest_id,'release_digest',release_digest,'request_digest',request_digest,'command_id',command_id,'created_at',created_at)
        FROM extensions.plugin_v2_receipts WHERE organization_id=$1 AND project_id=$2 AND actor_id=$3 AND ($4::uuid IS NULL OR id>$4)
        ORDER BY id LIMIT $5")
        .bind(org).bind(project).bind(actor).bind(after_id).bind(i64::from(limit)+1)
        .fetch_all(&mut **read.transaction()).await.map_err(db_error)?;
    let more = rows.len() > limit as usize;
    rows.truncate(limit as usize);
    let cursor = if more {
        rows.last()
            .and_then(|r| r["id"].as_str())
            .map(str::to_owned)
    } else {
        None
    };
    Ok(json!({"items":rows,"next_after_id":cursor,"limit":limit,"scope":"own_project_receipts"}))
}
