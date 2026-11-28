//! E14 public reviewed marketplace catalog and tenant-admin release installation.

use axum::extract::{Path, Query, Request, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use ovg_core::AppError;
use ovg_storage::extensions::RegisterManifestInput;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::AppState;

use super::validation::{load_catalog, CatalogRelease};
use super::{authenticate_actor, open_org_scope, parse_uuid, require_actor_csrf, ExtensionError};

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/marketplace/catalog", get(marketplace_catalog))
        .route("/marketplace/search", get(search_marketplace))
        .route(
            "/organizations/{organization_id}/marketplace/install/{publisher_id}/{extension_key}/{version}",
            post(install_marketplace_release),
        )
}

async fn marketplace_catalog(State(state): State<AppState>) -> Result<Response, ExtensionError> {
    let releases = load_catalog(&state.config).map_err(ExtensionError::Domain)?;
    let items: Vec<Value> = releases.iter().map(|r| r.wire()).collect();
    Ok((
        [(header::CACHE_CONTROL, "no-store")],
        Json(json!({ "items": items })),
    )
        .into_response())
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchQuery {
    #[serde(default)]
    q: String,
    #[serde(default)]
    offset: usize,
    #[serde(default = "default_limit")]
    limit: usize,
}
fn default_limit() -> usize {
    50
}

fn search_releases(
    mut releases: Vec<CatalogRelease>,
    query: &SearchQuery,
) -> Result<Value, ExtensionError> {
    if query.q.chars().count() > 200
        || query.limit == 0
        || query.limit > 100
        || query.offset > 100_000
    {
        return Err(ExtensionError::InvalidRequest);
    }
    let terms: Vec<String> = query.q.split_whitespace().map(str::to_lowercase).collect();
    releases.retain(|r| {
        let mut fields = vec![
            r.publisher_id.to_lowercase(),
            r.publisher_name.to_lowercase(),
        ];
        for field in ["extension_key", "display_name", "description", "version"] {
            if let Some(s) = r.manifest.get(field).and_then(Value::as_str) {
                fields.push(s.to_lowercase());
            }
        }
        terms
            .iter()
            .all(|term| fields.iter().any(|field| field.contains(term)))
    });
    releases.sort_by(|a, b| {
        a.publisher_id
            .cmp(&b.publisher_id)
            .then_with(|| {
                a.manifest["extension_key"]
                    .as_str()
                    .cmp(&b.manifest["extension_key"].as_str())
            })
            .then_with(|| {
                a.manifest["version"]
                    .as_str()
                    .cmp(&b.manifest["version"].as_str())
            })
    });
    let total = releases.len();
    let end = query.offset.saturating_add(query.limit);
    let items: Vec<Value> = releases
        .iter()
        .skip(query.offset)
        .take(query.limit)
        .map(CatalogRelease::wire)
        .collect();
    Ok(json!({"items":items,"total":total,"next_offset":(end < total).then_some(end)}))
}

async fn search_marketplace(
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> Result<Response, ExtensionError> {
    let releases = load_catalog(&state.config).map_err(ExtensionError::Domain)?;
    Ok((
        [(header::CACHE_CONTROL, "no-store")],
        Json(search_releases(releases, &query)?),
    )
        .into_response())
}

async fn install_marketplace_release(
    State(state): State<AppState>,
    Path((org, publisher_id, extension_key, version)): Path<(String, String, String, String)>,
    request: Request<axum::body::Body>,
) -> Result<(StatusCode, Json<Value>), ExtensionError> {
    let (mut parts, _) = request.into_parts();
    let actor = authenticate_actor(&state, &mut parts, &org).await?;
    require_actor_csrf(&state, &actor, &parts.headers)?;
    let org_id = parse_uuid(&org)?;

    let releases = load_catalog(&state.config).map_err(ExtensionError::Domain)?;
    let release = releases
        .into_iter()
        .find(|r| {
            r.publisher_id == publisher_id
                && r.manifest.get("extension_key").and_then(Value::as_str)
                    == Some(extension_key.as_str())
                && r.manifest.get("version").and_then(Value::as_str) == Some(version.as_str())
        })
        .ok_or_else(|| {
            ExtensionError::Domain(AppError::not_found(
                "reviewed marketplace release not found",
            ))
        })?;

    let m = release
        .manifest
        .as_object()
        .ok_or(ExtensionError::InvalidRequest)?;
    let mut scope = open_org_scope(&state, &actor, org_id).await?;
    let rec = scope
        .register_manifest(RegisterManifestInput {
            extension_key: m["extension_key"].as_str().unwrap_or_default(),
            version: m["version"].as_str().unwrap_or_default(),
            api_compatibility: m["api_compatibility"].as_str().unwrap_or_default(),
            display_name: m["display_name"].as_str().unwrap_or_default(),
            description: m["description"].as_str().unwrap_or_default(),
            permissions: &m["permissions"],
            commands: &m["commands"],
            capabilities: &m["capabilities"],
            entity_schemas: &m["entity_schemas"],
            ui_contributions: &m["ui_contributions"],
        })
        .await
        .map_err(ExtensionError::from_catalog_app)?;
    scope.commit().await.map_err(ExtensionError::Domain)?;
    Ok((StatusCode::CREATED, Json(rec.wire())))
}

#[cfg(test)]
mod search_tests {
    use super::*;
    fn release(key: &str, name: &str) -> CatalogRelease {
        CatalogRelease {
            publisher_id: "pub".into(),
            publisher_name: "Publisher".into(),
            publisher_public_key: String::new(),
            manifest: json!({"extension_key":key,"display_name":name,"description":"GIS","version":"1.0.0"}),
            signature: "fixture".into(),
        }
    }
    #[test]
    fn search_filter_and_page() {
        let rows = vec![
            release("partner_pub_b", "Water GIS"),
            release("partner_pub_a", "Land GIS"),
            release("partner_pub_c", "Water Survey"),
        ];
        let result = search_releases(
            rows,
            &SearchQuery {
                q: "WATER pub".into(),
                offset: 0,
                limit: 1,
            },
        )
        .ok()
        .unwrap();
        assert_eq!(result["total"], 2);
        assert_eq!(
            result["items"][0]["manifest"]["extension_key"],
            "partner_pub_b"
        );
        assert_eq!(result["next_offset"], 1);
        assert!(search_releases(
            vec![],
            &SearchQuery {
                q: String::new(),
                offset: 0,
                limit: 101
            }
        )
        .is_err());
    }
}
