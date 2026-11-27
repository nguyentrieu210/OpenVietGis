//! E12 external-source metadata and lifecycle on tenant-scoped transactions.
//! No outbound provider request or credential access is performed here.

use chrono::{DateTime, Utc};
use ovg_core::AppError;
use serde_json::{json, Value};
use sqlx::postgres::PgRow;
use sqlx::Row;
use uuid::Uuid;

use crate::catalog::ProjectRead;
use crate::collaboration::isoformat;
use crate::tenancy::TenantTx;

fn db_error(error: sqlx::Error) -> AppError {
    tracing::error!(%error, "external source database operation failed");
    AppError::internal("external source database operation failed")
}

fn safe_display_endpoint(endpoint: &str) -> String {
    let Ok(url) = url::Url::parse(endpoint) else {
        return String::new();
    };
    if !matches!(url.scheme(), "http" | "https") {
        return String::new();
    }
    let Some(host) = url.host_str() else {
        return String::new();
    };
    let port = url.port().map(|p| format!(":{p}")).unwrap_or_default();
    format!("{}://{}{}{}", url.scheme(), host, port, url.path())
}

pub async fn list_allowlisted_hosts(
    tx: &mut TenantTx,
    organization_id: Uuid,
) -> Result<Value, AppError> {
    tx.member_scope(organization_id).await?;
    let rows = sqlx::query(
        "SELECT id, organization_id, hostname, allowed_schemes, allowed_ports,
                secret_ref_name IS NOT NULL AS has_secret_ref, enabled, created_at
         FROM external_sources.allowlisted_hosts
         WHERE organization_id = $1 ORDER BY hostname ASC",
    )
    .bind(organization_id)
    .fetch_all(&mut **tx.tx_mut())
    .await
    .map_err(db_error)?;
    let items = rows
        .iter()
        .map(|row| {
            Ok(json!({
                "id": row.try_get::<Uuid, _>("id").map_err(db_error)?,
                "organization_id": row.try_get::<Uuid, _>("organization_id").map_err(db_error)?,
                "hostname": row.try_get::<String, _>("hostname").map_err(db_error)?,
                "allowed_schemes": row.try_get::<Value, _>("allowed_schemes").map_err(db_error)?,
                "allowed_ports": row.try_get::<Value, _>("allowed_ports").map_err(db_error)?,
                "has_secret_ref": row.try_get::<bool, _>("has_secret_ref").map_err(db_error)?,
                "secret_ref_name": null,
                "enabled": row.try_get::<bool, _>("enabled").map_err(db_error)?,
                "created_at": isoformat(row.try_get::<DateTime<Utc>, _>("created_at").map_err(db_error)?),
            }))
        })
        .collect::<Result<Vec<Value>, AppError>>()?;
    Ok(json!({"items": items}))
}

fn connection_wire(row: &PgRow) -> Result<Value, AppError> {
    let endpoint: String = row.try_get("endpoint_url").map_err(db_error)?;
    let lifecycle: String = row.try_get("lifecycle").map_err(db_error)?;
    let availability = if lifecycle == "revoked" {
        "revoked"
    } else {
        "unverified"
    };
    Ok(json!({
        "id": row.try_get::<Uuid, _>("id").map_err(db_error)?,
        "organization_id": row.try_get::<Uuid, _>("organization_id").map_err(db_error)?,
        "project_id": row.try_get::<Uuid, _>("project_id").map_err(db_error)?,
        "allowlist_id": row.try_get::<Uuid, _>("allowlist_id").map_err(db_error)?,
        "name": row.try_get::<String, _>("name").map_err(db_error)?,
        "protocol": row.try_get::<String, _>("protocol").map_err(db_error)?,
        "endpoint_url": safe_display_endpoint(&endpoint),
        "layer_identifier": row.try_get::<String, _>("layer_identifier").map_err(db_error)?,
        "attribution": row.try_get::<String, _>("attribution").map_err(db_error)?,
        "license_text": row.try_get::<String, _>("license_text").map_err(db_error)?,
        "crs_codes": row.try_get::<Value, _>("crs_codes").map_err(db_error)?,
        "capabilities": {
            "metadata_read": true, "render": false, "feature_read": false,
            "query": false, "style": false, "edit": false, "export": false,
            "analysis": false, "offline": false
        },
        "read_only": row.try_get::<bool, _>("read_only").map_err(db_error)?,
        "cache_generation": row.try_get::<i32, _>("cache_generation").map_err(db_error)?,
        "lifecycle": lifecycle,
        "availability": availability,
        "created_at": isoformat(row.try_get::<DateTime<Utc>, _>("created_at").map_err(db_error)?),
    }))
}

pub async fn list_connections(read: &mut ProjectRead<'_>) -> Result<Value, AppError> {
    let rows = sqlx::query(
        "SELECT * FROM external_sources.connections
         WHERE organization_id = $1 AND project_id = $2 ORDER BY created_at DESC",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(db_error)?;
    let items = rows
        .iter()
        .map(connection_wire)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({"items": items}))
}

pub async fn update_lifecycle(
    edit: &mut ProjectRead<'_>,
    connection_id: Uuid,
    revoke: bool,
) -> Result<Value, AppError> {
    let row = sqlx::query(
        "UPDATE external_sources.connections
         SET lifecycle = CASE WHEN $4 THEN 'revoked' ELSE lifecycle END,
             cache_generation = cache_generation + 1, updated_at = $5
         WHERE organization_id = $1 AND project_id = $2 AND id = $3
         RETURNING *",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(connection_id)
    .bind(revoke)
    .bind(Utc::now())
    .fetch_optional(&mut **edit.transaction())
    .await
    .map_err(db_error)?
    .ok_or_else(|| AppError::not_found("external connection not found"))?;
    connection_wire(&row)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_display_never_returns_credentials_or_query() {
        assert_eq!(
            safe_display_endpoint("https://user:password@example.org:8443/maps?token=secret#frag"),
            "https://example.org:8443/maps"
        );
        assert_eq!(safe_display_endpoint("file:///etc/passwd"), "");
        assert_eq!(safe_display_endpoint("invalid"), "");
    }
}
