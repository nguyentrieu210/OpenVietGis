//! Staging operations readiness, matching the Python dependency probe contract.

use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::{Read, Seek, Write};
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use chrono::{SecondsFormat, Utc};
use ovg_core::AppError;
use ovg_storage::tenancy::{ORGANIZATION_MANAGE_SETTINGS, ORGANIZATION_READ};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::identity::{require_csrf, HumanSession, IdentityError};
use crate::tenancy::{begin, uuid, TenancyError};
use crate::AppState;

async fn dependency_versions(state: &AppState) -> (Option<String>, Option<String>) {
    let Some(pool) = state.db_pool.as_ref() else {
        return (None, None);
    };
    tokio::time::timeout(Duration::from_secs(2), pool.dependency_versions())
        .await
        .unwrap_or((None, None))
}

pub async fn readiness(State(state): State<AppState>) -> (StatusCode, Json<Value>) {
    let (postgres, postgis) = dependency_versions(&state).await;
    let artifact_dir = state.config.artifact_store_dir.clone();
    let artifacts_ok = tokio::task::spawn_blocking(move || probe_artifacts(&artifact_dir))
        .await
        .unwrap_or(false);
    state.metrics.set_db_ready(postgres.is_some());
    let ready = postgres.is_some() && postgis.is_some() && artifacts_ok;

    (
        if ready {
            StatusCode::OK
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        },
        Json(json!({
            "status": if ready { "ready" } else { "not_ready" },
            "database": availability(postgres.is_some()),
            "postgis": availability(postgis.is_some()),
            "postgresql_version": postgres,
            "postgis_version": postgis,
            "artifact_store": availability(artifacts_ok),
            "scope": "dependency_probe_only",
            "production_ready": false,
            "checked_at": Utc::now().to_rfc3339_opts(SecondsFormat::Micros, false),
        })),
    )
}

fn availability(available: bool) -> &'static str {
    if available {
        "ok"
    } else {
        "unavailable"
    }
}

fn probe_artifacts(directory: &std::path::Path) -> bool {
    if !fs::symlink_metadata(directory).is_ok_and(|metadata| metadata.is_dir()) {
        return false;
    }
    let path = directory.join(format!(".readiness-{}", Uuid::new_v4()));
    let Ok(mut probe) = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&path)
    else {
        return false;
    };

    const CONTENT: [u8; 21] = *b"openvietgis-readiness";
    let content = CONTENT;
    let mut observed = [0; CONTENT.len()];
    let verified = probe
        .write_all(&content)
        .and_then(|()| probe.flush())
        .and_then(|()| probe.rewind())
        .and_then(|()| probe.read_exact(&mut observed))
        .is_ok()
        && observed == content;
    drop(probe);
    fs::remove_file(path).is_ok() && verified
}

/// Python `release_inventory()` order: (id, name, milestone).
const RELEASE_CAPABILITIES: [(&str, &str, &str); 15] = [
    ("R001", "Platform Foundation", "M0"),
    ("R002", "Identity, Organization & Tenancy", "M1"),
    ("R003", "Spatial Catalog & CRS (VN-2000)", "M1"),
    ("R004", "Map Workspace", "M1"),
    ("R005", "Import & Export + Durable Jobs", "M1"),
    ("R006", "Editing & Change History", "M2"),
    ("R007", "Attribute & Spatial Query", "M2"),
    ("R008", "Processing & Jobs (Buffer/Clip/Dissolve)", "M2"),
    ("R009", "Cartography & Print Output (PNG/PDF)", "M2"),
    ("R010", "External Sources (SSRF-safe Allowlist)", "M3"),
    ("R011", "Team Collaboration & Reviews", "M3"),
    ("R012", "CAD 2D DXF & Precision COGO", "M3"),
    ("R013", "Field & Offline GeoPackage + Sync", "M3"),
    ("R014", "Internal Extension Platform", "M3"),
    ("R015", "Production Operations & Self-Host Profile", "M3"),
];
const DEFAULT_REQUESTED_FEATURES: [&str; 7] =
    ["R001", "R002", "R003", "R004", "R005", "R006", "R007"];
const DEPLOYMENT_PROFILES: [&str; 3] = ["self_host_team", "cloud_saas", "enterprise_private"];
const PLAN_TIERS: [&str; 3] = ["self_host_community", "team_standard", "enterprise"];

/// No release is certified: there is no accepted exact-candidate/profile record.
fn release_inventory() -> Value {
    let items: Vec<Value> = RELEASE_CAPABILITIES
        .iter()
        .map(|(id, name, milestone)| {
            json!({
                "id": id,
                "name": name,
                "milestone": milestone,
                "enabled": false,
                "status": "unverified",
                "reason": "exact_candidate_profile_acceptance_missing",
                "required_for_team": DEFAULT_REQUESTED_FEATURES.contains(id)
                    || matches!(*id, "R011" | "R015"),
            })
        })
        .collect();
    json!({
        "profile": "self_host_team",
        "profile_status": "unverified",
        "verified_candidate": null,
        "verification_record": null,
        "production_ready": false,
        "enabled_semantics": "release_certification",
        "total_releases": items.len(),
        "accepted_release_count": 0,
        "items": items,
    })
}

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/ops/release-capabilities",
            get(|| async { Json(release_inventory()) }),
        )
        .route(
            "/organizations/{organization_id}/ops/entitlement",
            get(get_entitlement).put(put_entitlement),
        )
}

/// Python `SqlOpsRepository.render_prometheus_metrics` lines, in order.
pub(crate) async fn prometheus_metrics(state: &AppState) -> String {
    let (postgres, postgis) = dependency_versions(state).await;
    let total = RELEASE_CAPABILITIES.len();
    format!(
        "# HELP openvietgis_up Application liveness indicator.\n\
         # TYPE openvietgis_up gauge\n\
         openvietgis_up 1\n\
         # HELP openvietgis_database_ready PostgreSQL connectivity indicator.\n\
         # TYPE openvietgis_database_ready gauge\n\
         openvietgis_database_ready {}\n\
         # HELP openvietgis_postgis_ready PostGIS extension availability.\n\
         # TYPE openvietgis_postgis_ready gauge\n\
         openvietgis_postgis_ready {}\n\
         # HELP openvietgis_release_capabilities_enabled Total certified release slices.\n\
         # TYPE openvietgis_release_capabilities_enabled gauge\n\
         openvietgis_release_capabilities_enabled 0\n\
         # HELP openvietgis_release_inventory_count Registered roadmap groups.\n\
         # TYPE openvietgis_release_inventory_count gauge\n\
         openvietgis_release_inventory_count {total}\n\
         # HELP openvietgis_release_gate_ready Accepted deployment profile.\n\
         # TYPE openvietgis_release_gate_ready gauge\n\
         openvietgis_release_gate_ready 0\n",
        u8::from(postgres.is_some()),
        u8::from(postgres.is_some() && postgis.is_some()),
    )
}

pub(crate) enum OpsError {
    Tenancy(TenancyError),
    /// `PlatformValidationError` envelope.
    Validation(&'static str),
}

impl From<TenancyError> for OpsError {
    fn from(error: TenancyError) -> Self {
        Self::Tenancy(error)
    }
}

impl From<AppError> for OpsError {
    fn from(error: AppError) -> Self {
        Self::Tenancy(error.into())
    }
}

impl From<IdentityError> for OpsError {
    fn from(error: IdentityError) -> Self {
        Self::Tenancy(error.into())
    }
}

impl IntoResponse for OpsError {
    fn into_response(self) -> Response {
        match self {
            Self::Tenancy(error) => error.into_response(),
            Self::Validation(message) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "code": "VALIDATION_ERROR", "message": message })),
            )
                .into_response(),
        }
    }
}

async fn get_entitlement(
    State(state): State<AppState>,
    session: HumanSession,
    Path(organization_id): Path<String>,
) -> Result<Json<Value>, OpsError> {
    let organization_id = uuid(&organization_id)?;
    let mut tx = begin(&state, &session).await?;
    tx.member_scope(organization_id).await?;
    tx.require(organization_id, None, None, ORGANIZATION_READ)
        .await?;
    let found = tx.entitlement(organization_id).await?;
    tx.commit().await?;
    Ok(Json(found.unwrap_or_else(|| {
        json!({
            "organization_id": organization_id,
            "deployment_profile": "self_host_team",
            "plan_tier": "self_host_community",
            "max_projects": 100,
            "max_storage_mb": 10240,
            "features_enabled": DEFAULT_REQUESTED_FEATURES,
            "authorization_distinct": true,
            "data_deletion_on_plan_change": false,
            "enforcement": "metadata_only",
        })
    })))
}

/// `UpsertEntitlementRequest` (Pydantic, `extra="forbid"`).
#[derive(Debug, PartialEq)]
struct EntitlementRequest {
    deployment_profile: String,
    plan_tier: String,
    max_projects: i32,
    max_storage_mb: i32,
    features_enabled: Vec<String>,
}

impl EntitlementRequest {
    fn parse(bytes: &[u8]) -> Result<Self, TenancyError> {
        const KEYS: [&str; 5] = [
            "deployment_profile",
            "plan_tier",
            "max_projects",
            "max_storage_mb",
            "features_enabled",
        ];
        let invalid = || TenancyError::InvalidRequest;
        let Ok(Value::Object(body)) = serde_json::from_slice::<Value>(bytes) else {
            return Err(invalid());
        };
        if !body.keys().all(|key| KEYS.contains(&key.as_str())) {
            return Err(invalid());
        }
        let text = |key: &str, default: &str| match body.get(key) {
            None => Ok(default.to_owned()),
            Some(Value::String(value)) => Ok(value.clone()),
            Some(_) => Err(invalid()),
        };
        // Pydantic v2 lax int: integral numbers, booleans and integer strings, then bounds.
        let int = |key: &str, default: i32, min: i32| {
            let value = match body.get(key) {
                None => return Ok(default),
                Some(Value::Bool(flag)) => Some(i64::from(*flag)),
                Some(Value::Number(number)) => number.as_i64().or_else(|| {
                    number
                        .as_f64()
                        .filter(|value| value.fract() == 0.0 && value.abs() < 1e15)
                        .map(|value| value as i64)
                }),
                Some(Value::String(text)) => lax_int(text),
                Some(_) => None,
            };
            value
                .and_then(|value| i32::try_from(value).ok())
                .filter(|value| *value >= min)
                .ok_or_else(invalid)
        };
        let features_enabled = match body.get("features_enabled") {
            None => DEFAULT_REQUESTED_FEATURES.map(str::to_owned).to_vec(),
            Some(Value::Array(items)) if items.len() <= 15 => items
                .iter()
                .map(|item| item.as_str().map(str::to_owned).ok_or_else(invalid))
                .collect::<Result<_, _>>()?,
            Some(_) => return Err(invalid()),
        };
        Ok(Self {
            deployment_profile: text("deployment_profile", "self_host_team")?,
            plan_tier: text("plan_tier", "self_host_community")?,
            max_projects: int("max_projects", 100, 1)?,
            max_storage_mb: int("max_storage_mb", 10240, 64)?,
            features_enabled,
        })
    }

    /// Repository checks after authorization, in the Python order.
    fn validate(&self) -> Result<(), OpsError> {
        if !DEPLOYMENT_PROFILES.contains(&self.deployment_profile.as_str()) {
            return Err(OpsError::Validation("invalid deployment_profile"));
        }
        if !PLAN_TIERS.contains(&self.plan_tier.as_str()) {
            return Err(OpsError::Validation("invalid plan_tier"));
        }
        let unique: BTreeSet<&str> = self.features_enabled.iter().map(String::as_str).collect();
        if unique.len() != self.features_enabled.len() {
            return Err(OpsError::Validation("duplicate requested feature IDs"));
        }
        if !unique
            .iter()
            .all(|id| RELEASE_CAPABILITIES.iter().any(|(known, _, _)| known == id))
        {
            return Err(OpsError::Validation("unknown requested feature IDs"));
        }
        Ok(())
    }
}

/// pydantic-core `str_as_int`: trimmed integer text, optionally with an all-zero fraction.
fn lax_int(text: &str) -> Option<i64> {
    let text = text.trim();
    let whole = match text.split_once('.') {
        Some((whole, fraction)) if fraction.bytes().all(|digit| digit == b'0') => whole,
        Some(_) => return None,
        None => text,
    };
    whole.parse().ok()
}

/// FastAPI decodes the body as JSON for no Content-Type, `application/json` or
/// `application/*+json` (parameters ignored); other bodies fail model validation.
fn json_content_type(headers: &HeaderMap) -> bool {
    let Some(value) = headers.get(header::CONTENT_TYPE) else {
        return true;
    };
    let Ok(value) = value.to_str() else {
        return false;
    };
    let media = value.split(';').next().unwrap_or_default().trim();
    if media.is_empty() {
        return value.is_empty();
    }
    media
        .to_ascii_lowercase()
        .split_once('/')
        .is_some_and(|(main, sub)| {
            main == "application" && (sub == "json" || sub.ends_with("+json"))
        })
}

async fn put_entitlement(
    State(state): State<AppState>,
    session: Result<HumanSession, IdentityError>,
    headers: HeaderMap,
    Path(organization_id): Path<String>,
    body: Bytes,
) -> Result<Json<Value>, OpsError> {
    // FastAPI decodes a JSON body before it resolves authentication dependencies.
    let json = json_content_type(&headers);
    if json && !body.is_empty() && serde_json::from_slice::<Value>(&body).is_err() {
        return Err(TenancyError::InvalidRequest.into());
    }
    let session = session?;
    require_csrf(&state, &session, &headers)?;
    let organization_id = uuid(&organization_id)?;
    if !json {
        return Err(TenancyError::InvalidRequest.into());
    }
    let request = EntitlementRequest::parse(&body)?;
    let mut tx = begin(&state, &session).await?;
    tx.member_scope(organization_id).await?;
    tx.require(organization_id, None, None, ORGANIZATION_MANAGE_SETTINGS)
        .await?;
    request.validate()?;
    let wire = tx
        .upsert_entitlement(
            organization_id,
            &request.deployment_profile,
            &request.plan_tier,
            request.max_projects,
            request.max_storage_mb,
            &request.features_enabled,
        )
        .await?;
    tx.commit().await?;
    Ok(Json(wire))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_inventory_matches_python_catalog() {
        let inventory = release_inventory();
        assert_eq!(inventory["total_releases"], 15);
        let required: Vec<&str> = inventory["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|item| item["required_for_team"] == true)
            .map(|item| item["id"].as_str().unwrap())
            .collect();
        assert_eq!(
            required,
            ["R001", "R002", "R003", "R004", "R005", "R006", "R007", "R011", "R015"]
        );
    }

    #[test]
    fn lax_integers_follow_pydantic() {
        for (text, expected) in [
            ("5", Some(5)),
            (" 6 ", Some(6)),
            ("+7", Some(7)),
            ("8.00", Some(8)),
        ] {
            assert_eq!(lax_int(text), expected, "{text}");
        }
        for text in ["", "5.5", "five", "1e3"] {
            assert_eq!(lax_int(text), None, "{text}");
        }
        let parsed =
            EntitlementRequest::parse(br#"{"max_projects": true, "max_storage_mb": "128"}"#)
                .ok()
                .unwrap();
        assert_eq!((parsed.max_projects, parsed.max_storage_mb), (1, 128));
        assert!(EntitlementRequest::parse(br#"{"max_storage_mb": true}"#).is_err());
    }

    #[test]
    fn json_content_types_match_fastapi() {
        let with = |value: &str| {
            let mut headers = HeaderMap::new();
            headers.insert(header::CONTENT_TYPE, value.parse().unwrap());
            json_content_type(&headers)
        };
        assert!(json_content_type(&HeaderMap::new()));
        assert!(with("application/json; charset=utf-8"));
        assert!(with("Application/Merge-Patch+JSON"));
        assert!(!with("text/plain"));
        assert!(!with("application/x-www-form-urlencoded"));
    }

    #[test]
    fn entitlement_body_follows_the_pydantic_model() {
        let defaults = EntitlementRequest::parse(b"{}").ok().unwrap();
        assert_eq!(defaults.max_projects, 100);
        assert_eq!(defaults.features_enabled.len(), 7);
        assert!(defaults.validate().is_ok());
        let parsed = EntitlementRequest::parse(br#"{"max_projects": 5.0}"#)
            .ok()
            .unwrap();
        assert_eq!(parsed.max_projects, 5);
        for invalid in [
            &br#"{"extra": 1}"#[..],
            br#"{"max_projects": 0}"#,
            br#"{"max_storage_mb": 63}"#,
            br#"{"max_projects": 2147483648}"#,
            br#"{"max_projects": 1.5}"#,
            br#"{"plan_tier": null}"#,
            br#"{"features_enabled": [1]}"#,
            b"[]",
            b"",
        ] {
            assert!(EntitlementRequest::parse(invalid).is_err());
        }
        let mut request = EntitlementRequest::parse(br#"{"features_enabled": ["R001", "R001"]}"#)
            .ok()
            .unwrap();
        assert!(matches!(
            request.validate(),
            Err(OpsError::Validation("duplicate requested feature IDs"))
        ));
        request.features_enabled = vec!["R099".into()];
        assert!(matches!(
            request.validate(),
            Err(OpsError::Validation("unknown requested feature IDs"))
        ));
    }
}
