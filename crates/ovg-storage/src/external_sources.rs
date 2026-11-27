//! E12 external-source metadata and lifecycle on tenant-scoped transactions.
//! No outbound provider request or credential access is performed here.

use chrono::{DateTime, Utc};
use ovg_core::AppError;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::postgres::PgRow;
use sqlx::Row;
use uuid::Uuid;

use crate::catalog::ProjectRead;
use crate::collaboration::isoformat;
use crate::tenancy::{TenantTx, ORGANIZATION_MANAGE_SETTINGS};

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

fn invalid(code: &'static str, message: &str) -> AppError {
    AppError::validation(code, message)
}

fn blocked(code: &'static str, message: &str) -> AppError {
    AppError::Forbidden {
        code,
        message: message.to_owned(),
    }
}

/// Registration is metadata-only. Actual outbound transports must also enforce
/// public DNS answers, pinned addresses, redirect policy and response limits.
fn safe_hostname(input: &str) -> Result<String, AppError> {
    let host = input.trim().to_ascii_lowercase();
    let host = host.trim_end_matches('.');
    if host.is_empty()
        || host.len() > 253
        || !host.is_ascii()
        || host.bytes().any(|b| !(b.is_ascii_alphanumeric() || b == b'.' || b == b'-'))
        || host.split('.').any(|part| {
            part.is_empty() || part.len() > 63 || part.starts_with('-') || part.ends_with('-')
        })
    {
        return Err(invalid("INVALID_HOSTNAME", "invalid external hostname"));
    }
    if !host.contains('.')
        || ["localhost", ".localhost", ".local", ".internal", ".home.arpa"]
            .iter()
            .any(|suffix| host.ends_with(suffix))
    {
        return Err(blocked("SSRF_BLOCKED", "SSRF blocked internal hostname"));
    }
    match url::Host::parse(host) {
        Ok(url::Host::Domain(_)) => {}
        Ok(url::Host::Ipv4(ip)) => {
            let [a, b, c, _] = ip.octets();
            // Reject alternate numeric representations as well as non-public
            // ranges. Never depend on URL-parser normalization for authorization.
            if ip.to_string() != host
                || matches!(a, 0 | 10 | 127 | 224..=255)
                || (a == 100 && (64..=127).contains(&b))
                || (a == 169 && b == 254)
                || (a == 172 && (16..=31).contains(&b))
                || (a == 192 && (b == 168 || (b == 0 && matches!(c, 0 | 2))))
                || (a == 198 && (matches!(b, 18 | 19) || (b == 51 && c == 100)))
                || (a == 203 && b == 0 && c == 113)
            {
                return Err(blocked("SSRF_BLOCKED", "SSRF blocked non-public or ambiguous IP address"));
            }
        }
        _ => return Err(invalid("INVALID_HOSTNAME", "invalid external hostname")),
    }
    Ok(host.to_owned())
}

fn default_schemes() -> Vec<String> {
    vec!["https".to_owned()]
}

fn default_ports() -> Vec<i32> {
    vec![443]
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AllowlistInput {
    hostname: String,
    #[serde(default = "default_schemes")]
    allowed_schemes: Vec<String>,
    #[serde(default = "default_ports")]
    allowed_ports: Vec<i32>,
    #[serde(default)]
    secret_ref_name: Option<String>,
}

fn allowlist_wire(row: &PgRow) -> Result<Value, AppError> {
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
}

pub async fn register_allowlist(
    tx: &mut TenantTx,
    organization_id: Uuid,
    actor_id: Uuid,
    input: AllowlistInput,
) -> Result<Value, AppError> {
    tx.member(organization_id, true, false).await?;
    tx.require(organization_id, None, None, ORGANIZATION_MANAGE_SETTINGS)
        .await?;
    let host = safe_hostname(&input.hostname)?;
    let schemes = if input.allowed_schemes.is_empty() {
        default_schemes()
    } else {
        input.allowed_schemes.iter().map(|s| s.to_ascii_lowercase()).collect()
    };
    if schemes.iter().any(|s| !matches!(s.as_str(), "http" | "https")) {
        return Err(invalid("VALIDATION_ERROR", "allowed_schemes must be http or https"));
    }
    let ports = if input.allowed_ports.is_empty() {
        default_ports()
    } else {
        input.allowed_ports
    };
    if ports.iter().any(|p| !(1..=65535).contains(p)) {
        return Err(invalid("VALIDATION_ERROR", "allowed_ports must be 1..65535"));
    }
    let secret = input.secret_ref_name.filter(|s| !s.is_empty()).map(|s| {
        s.trim().chars().take(128).collect::<String>()
    });
    let row = sqlx::query(
        "INSERT INTO external_sources.allowlisted_hosts
         (id, organization_id, hostname, allowed_schemes, allowed_ports,
          secret_ref_name, enabled, created_by_actor_id, created_at)
         VALUES ($1,$2,$3,$4,$5,$6,true,$7,$8)
         ON CONFLICT (organization_id, hostname) DO NOTHING
         RETURNING id, organization_id, hostname, allowed_schemes, allowed_ports,
                   secret_ref_name IS NOT NULL AS has_secret_ref, enabled, created_at",
    )
    .bind(Uuid::new_v4())
    .bind(organization_id)
    .bind(&host)
    .bind(json!(schemes))
    .bind(json!(ports))
    .bind(secret)
    .bind(actor_id)
    .bind(Utc::now())
    .fetch_optional(&mut **tx.tx_mut())
    .await
    .map_err(db_error)?
    .ok_or_else(|| AppError::conflict("HOST_EXISTS", format!("host '{host}' already allowlisted")))?;
    allowlist_wire(&row)
}

fn default_layer() -> String {
    "default".to_owned()
}

fn default_attribution() -> String {
    "External Provider".to_owned()
}

fn default_license() -> String {
    "Provider License Terms".to_owned()
}

fn default_crs() -> Vec<i32> {
    vec![4326, 3857]
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectionInput {
    allowlist_id: Uuid,
    name: String,
    protocol: String,
    endpoint_url: String,
    #[serde(default = "default_layer")]
    layer_identifier: String,
    #[serde(default = "default_attribution")]
    attribution: String,
    #[serde(default = "default_license")]
    license_text: String,
    #[serde(default = "default_crs")]
    crs_codes: Vec<i32>,
}

fn endpoint(
    value: &str,
    host: &str,
    schemes: &[String],
    ports: &[i32],
    enabled: bool,
) -> Result<String, AppError> {
    let clean = value.trim();
    if clean.is_empty() || clean.len() > 1000 || !clean.is_ascii()
        || clean.bytes().any(|b| b <= 32 || b == 127 || b == b'\\')
    {
        return Err(invalid("INVALID_URL", "invalid endpoint URL"));
    }
    let url = url::Url::parse(clean).map_err(|_| invalid("INVALID_URL", "invalid endpoint URL"))?;
    // Inspect raw authority too: even an empty userinfo marker is prohibited.
    let authority = clean.split_once("://").map(|(_, tail)| {
        tail.split(['/', '?', '#']).next().unwrap_or_default()
    }).unwrap_or_default();
    if authority.contains('@') || !url.username().is_empty() || url.password().is_some() {
        return Err(blocked("URL_CREDENTIALS_FORBIDDEN", "credentials must not be embedded in external URLs; use secret_ref_name"));
    }
    if clean.contains(['?', '#']) {
        return Err(blocked("URL_PARAMETERS_FORBIDDEN", "endpoint query strings and fragments are not supported"));
    }
    if !matches!(url.scheme(), "http" | "https") {
        return Err(invalid("INVALID_SCHEME", "only http/https external endpoints are permitted"));
    }
    if authority.is_empty() || authority.contains('%') {
        return Err(invalid("INVALID_URL", "invalid endpoint authority"));
    }
    let parsed_host = url.host_str().ok_or_else(|| invalid("INVALID_URL", "endpoint URL requires a hostname"))?;
    let raw_host = authority.split(':').next().unwrap_or_default();
    let actual_host = safe_hostname(raw_host)?;
    if actual_host != parsed_host.trim_end_matches('.') {
        return Err(blocked("SSRF_BLOCKED", "SSRF blocked ambiguous hostname"));
    }
    let port = url.port_or_known_default().ok_or_else(|| invalid("INVALID_URL", "invalid endpoint port"))?;
    if !enabled {
        return Err(blocked("HOST_DISABLED", "allowlisted host is disabled"));
    }
    if actual_host != host {
        return Err(blocked("HOST_NOT_ALLOWLISTED", "endpoint host does not match allowlisted host"));
    }
    if !schemes.iter().any(|scheme| scheme == url.scheme()) {
        return Err(blocked("SCHEME_NOT_ALLOWLISTED", "endpoint scheme is not allowlisted"));
    }
    if !ports.contains(&i32::from(port)) {
        return Err(blocked("PORT_NOT_ALLOWLISTED", "endpoint port is not allowlisted"));
    }
    Ok(clean.to_owned())
}

fn bounded_text(value: &str, max: usize, fallback: &str) -> String {
    let value: String = value.trim().chars().take(max).collect();
    if value.is_empty() { fallback.to_owned() } else { value }
}

fn metadata_capabilities() -> Value {
    json!({
        "metadata_read": true, "render": false, "feature_read": false,
        "query": false, "style": false, "edit": false, "export": false,
        "analysis": false, "offline": false
    })
}

pub async fn create_connection(
    edit: &mut ProjectRead<'_>,
    actor_id: Uuid,
    input: ConnectionInput,
) -> Result<Value, AppError> {
    let organization_id = edit.organization_id();
    let project_id = edit.project_id();
    let allowlist = sqlx::query(
        "SELECT hostname, allowed_schemes, allowed_ports, enabled
         FROM external_sources.allowlisted_hosts
         WHERE organization_id=$1 AND id=$2 FOR SHARE",
    )
    .bind(organization_id)
    .bind(input.allowlist_id)
    .fetch_optional(&mut **edit.transaction())
    .await
    .map_err(db_error)?
    .ok_or_else(|| AppError::not_found("allowlisted host not found"))?;
    let schemes: Value = allowlist.try_get("allowed_schemes").map_err(db_error)?;
    let ports: Value = allowlist.try_get("allowed_ports").map_err(db_error)?;
    let schemes: Vec<String> = serde_json::from_value(schemes)
        .map_err(|_| AppError::internal("invalid stored scheme allowlist"))?;
    let ports: Vec<i32> = serde_json::from_value(ports)
        .map_err(|_| AppError::internal("invalid stored port allowlist"))?;
    let host: String = allowlist.try_get("hostname").map_err(db_error)?;
    let enabled: bool = allowlist.try_get("enabled").map_err(db_error)?;
    let clean = endpoint(&input.endpoint_url, &host, &schemes, &ports, enabled)?;
    if input.crs_codes.is_empty() || input.crs_codes.iter().any(|c| !matches!(c, 4326 | 3857 | 4756 | 5899)) {
        return Err(invalid("UNSUPPORTED_CRS", "crs_codes must contain supported EPSG codes (4326, 3857, 4756, 5899)"));
    }
    if !matches!(input.protocol.as_str(), "wms" | "wmts_xyz" | "cog_geotiff" | "wfs_ogc_features") {
        return Err(invalid("VALIDATION_ERROR", "invalid external protocol"));
    }
    let row = sqlx::query(
        "INSERT INTO external_sources.connections
         (id,organization_id,project_id,allowlist_id,name,protocol,endpoint_url,
          layer_identifier,attribution,license_text,crs_codes,capabilities,
          read_only,cache_generation,lifecycle,created_by_actor_id,created_at,updated_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,true,1,'active',$13,$14,$14)
         RETURNING *",
    )
    .bind(Uuid::new_v4())
    .bind(organization_id)
    .bind(project_id)
    .bind(input.allowlist_id)
    .bind(bounded_text(&input.name, 200, "External Layer"))
    .bind(input.protocol)
    .bind(clean)
    .bind(bounded_text(&input.layer_identifier, 200, "default"))
    .bind(bounded_text(&input.attribution, 300, "External Provider"))
    .bind(bounded_text(&input.license_text, 300, "Provider License Terms Apply"))
    .bind(json!(input.crs_codes))
    .bind(metadata_capabilities())
    .bind(actor_id)
    .bind(Utc::now())
    .fetch_one(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    connection_wire(&row)
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
    let items = rows.iter().map(allowlist_wire).collect::<Result<Vec<_>, _>>()?;
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
        "capabilities": metadata_capabilities(),
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
    fn registration_blocks_internal_and_ambiguous_addresses() {
        for host in ["localhost", "a.internal", "127.0.0.1", "127.1", "0x7f000001",
            "169.254.169.254", "10.0.0.1", "192.168.0.1", "100.64.0.1"] {
            assert!(safe_hostname(host).is_err(), "{host}");
        }
        assert_eq!(safe_hostname(" Maps.Example.org. ").unwrap(), "maps.example.org");
    }

    #[test]
    fn endpoint_requires_exact_approved_authority() {
        let schemes = vec!["https".to_owned()];
        let ports = vec![443];
        let check = |url| endpoint(url, "maps.example.org", &schemes, &ports, true);
        assert!(check("https://maps.example.org/maps").is_ok());
        for url in ["https://maps.example.org@evil.example/maps",
            "https://maps.example.org.evil.example/maps",
            "https://maps.example.org:8443/maps",
            "https://maps.example.org/maps?token=secret",
            "http://maps.example.org/maps",
            "https://127.0.0.1/maps"] {
            assert!(check(url).is_err(), "{url}");
        }
    }

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
