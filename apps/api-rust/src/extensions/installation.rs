//! Reviewed graph installation and project switch. Organization registration
//! and project activation are separate authority boundaries/transactions.
use super::{
    authenticate_json, decode_body,
    marketplace_v2::{load_verified, Profile, VerifiedRelease},
    open_org_scope, open_project_scope, parse_uuid, project_actor_display, require_actor_csrf,
    ExtensionError,
};
use crate::AppState;
use axum::{
    extract::{Path, Request, State},
    http::header,
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use ovg_core::AppError;
use ovg_module_sdk::{
    declarative::Document,
    host_profile::{read_only_policies, validate_read_only},
    release_v2::{canonical, resolve_lock, Pin},
};
use ovg_storage::{
    catalog::{ProjectRead, ProjectReadPolicy},
    extensions::{
        self,
        installation::{project_pins, switch_graph, switch_history, ActivationPin, GraphRelease},
    },
};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;
const ORG: &str = "/organizations/{organization_id}/marketplace/v2/graph";
const PROJECT: &str = "/organizations/{organization_id}/projects/{project_id}/extensions/v2-graph";
pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route(&format!("{ORG}/preview"), post(org_preview))
        .route(&format!("{ORG}/install"), post(install))
        .route(&format!("{PROJECT}/preview"), post(project_preview))
        .route(&format!("{PROJECT}/apply"), post(apply))
}
fn invalid() -> AppError {
    AppError::validation("INVALID_PLUGIN_GRAPH", "invalid or incompatible graph")
}
fn domain(e: AppError) -> ExtensionError {
    ExtensionError::Domain(e)
}
fn digest(v: &Value) -> Result<String, AppError> {
    Ok(hex::encode(Sha256::digest(canonical(v)?)))
}
fn enabled(state: &AppState) -> Result<(), ExtensionError> {
    if !state.config.marketplace_v2_runtime_enabled {
        return Err(domain(AppError::not_found("graph preview unavailable")));
    }
    Ok(())
}
fn reply(v: Value) -> Response {
    ([(header::CACHE_CONTROL, "private, no-store")], Json(v)).into_response()
}
async fn auth(
    state: &AppState,
    req: Request,
    org: &str,
) -> Result<(super::Actor, Option<Value>), ExtensionError> {
    enabled(state)?;
    let (parts, body) = req.into_parts();
    let bytes = axum::body::to_bytes(body, 65536)
        .await
        .map_err(|_| ExtensionError::InvalidRequest)?;
    let (actor, headers, body) = authenticate_json(
        state,
        Request::from_parts(parts, axum::body::Body::from(bytes)),
        org,
    )
    .await?;
    require_actor_csrf(state, &actor, &headers)?;
    if !actor.is_human() {
        return Err(domain(AppError::forbidden("human session required")));
    }
    Ok((actor, body))
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Selection {
    root: String,
    version: String,
    #[serde(default)]
    rollback_of: Option<Uuid>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Install {
    selection: Selection,
    request_id: Uuid,
    plan_digest: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Apply {
    selection: Selection,
    request_id: Uuid,
    plan_digest: String,
    expected: Vec<ActivationPin>,
}
struct Plan {
    graph: Vec<GraphRelease>,
    pins: Value,
    epoch: i64,
}
fn plan(
    selection: &Selection,
    releases: &[VerifiedRelease],
    exact: Option<&BTreeMap<String, String>>,
) -> Result<Plan, AppError> {
    let mut pins = Vec::new();
    for r in releases {
        let e = &r.metadata["envelope"];
        let field = |k: &str| e[k].as_str().map(str::to_owned).ok_or_else(invalid);
        let key = field("extension_key")?;
        let version = field("version")?;
        if exact.is_some_and(|m| m.get(&key) != Some(&version)) {
            continue;
        }
        pins.push(Pin {
            key,
            version,
            digest: r.metadata["release_digest"]
                .as_str()
                .ok_or_else(invalid)?
                .to_owned(),
            dependencies: if e.get("dependencies").is_some() {
                serde_json::from_value(e["dependencies"].clone()).map_err(|_| invalid())?
            } else {
                vec![]
            },
        });
    }
    let selected = resolve_lock(&selection.root, &selection.version, &pins)?;
    if exact.is_some_and(|m| m.len() != selected.len()) {
        return Err(invalid());
    }
    let mut graph = Vec::new();
    let mut public = Vec::new();
    let mut epoch = None;
    for p in selected {
        let r = releases
            .iter()
            .find(|r| r.metadata["release_digest"].as_str() == Some(p.digest.as_str()))
            .ok_or_else(invalid)?;
        let e = &r.metadata["envelope"];
        let current = r.metadata["catalog_epoch"]
            .as_i64()
            .filter(|e| *e > 0)
            .ok_or_else(invalid)?;
        if epoch.is_some_and(|v| v != current) {
            return Err(invalid());
        }
        epoch = Some(current);
        if !p.key.starts_with("partner_")
            || r.manifest["entity_schemas"]
                .as_object()
                .is_none_or(|m| !m.is_empty())
            || e["assets"].as_array().is_none_or(|a| !a.is_empty())
        {
            return Err(invalid());
        }
        let caps: BTreeSet<String> =
            serde_json::from_value(r.manifest["capabilities"].clone()).map_err(|_| invalid())?;
        let perms: BTreeSet<String> =
            serde_json::from_value(e["permissions"].clone()).map_err(|_| invalid())?;
        validate_read_only(
            &Document::parse(&r.document)?,
            &perms,
            &read_only_policies(&p.key, &caps),
        )?;
        public.push(
            json!({"key":p.key,"version":p.version,"release_digest":p.digest,"permissions":perms}),
        );
        graph.push(GraphRelease {
            release_digest: p.digest,
            manifest: r.manifest.clone(),
        });
    }
    Ok(Plan {
        graph,
        pins: json!(public),
        epoch: epoch.ok_or_else(invalid)?,
    })
}
async fn catalog(state: &AppState) -> Result<Vec<VerifiedRelease>, ExtensionError> {
    load_verified(
        state,
        Profile {
            host: "web".into(),
            api: "1.0.0".into(),
        },
    )
    .await
}
fn preview(
    plan: &Plan,
    expected: Option<&[ActivationPin]>,
    rollback: Option<Uuid>,
) -> Result<Value, AppError> {
    let payload = json!({"catalog_epoch":plan.epoch,"pins":plan.pins,"expected":expected,"rollback_of":rollback});
    Ok(json!({"plan":payload,"plan_digest":digest(&payload)?,"grants_created":false}))
}
fn match_digest(preview: &Value, provided: &str) -> Result<(), ExtensionError> {
    if preview["plan_digest"].as_str() != Some(provided) {
        return Err(domain(AppError::conflict(
            "STALE_PLUGIN_PLAN",
            "review current graph before applying",
        )));
    }
    Ok(())
}
async fn org_preview(
    State(state): State<AppState>,
    Path(org): Path<String>,
    req: Request,
) -> Result<Response, ExtensionError> {
    let (actor, body) = auth(&state, req, &org).await?;
    let s: Selection = decode_body(body)?;
    if s.rollback_of.is_some() {
        return Err(ExtensionError::InvalidRequest);
    }
    let p = plan(&s, &catalog(&state).await?, None).map_err(domain)?;
    let tx = open_org_scope(&state, &actor, parse_uuid(&org)?).await?;
    tx.commit().await.map_err(domain)?;
    Ok(reply(preview(&p, None, None).map_err(domain)?))
}
async fn install(
    State(state): State<AppState>,
    Path(org): Path<String>,
    req: Request,
) -> Result<Response, ExtensionError> {
    let (actor, body) = auth(&state, req, &org).await?;
    let b: Install = decode_body(body)?;
    if b.selection.rollback_of.is_some() {
        return Err(ExtensionError::InvalidRequest);
    }
    let p = plan(&b.selection, &catalog(&state).await?, None).map_err(domain)?;
    match_digest(&preview(&p, None, None).map_err(domain)?, &b.plan_digest)?;
    let mut tx = open_org_scope(&state, &actor, parse_uuid(&org)?).await?;
    let result = tx
        .graph_install(b.request_id, &b.plan_digest, p.epoch, &p.graph)
        .await
        .map_err(domain)?;
    tx.commit().await.map_err(domain)?;
    Ok(reply(result))
}
async fn project_plan(
    read: &mut ProjectRead<'_>,
    s: &Selection,
    rows: &[VerifiedRelease],
) -> Result<Plan, AppError> {
    let exact = if let Some(id) = s.rollback_of {
        let h = switch_history(read, id).await?;
        let before: Vec<ActivationPin> =
            serde_json::from_value(h["before"].clone()).map_err(|_| invalid())?;
        if before.is_empty() || before.iter().any(|p| p.status != "active") {
            return Err(AppError::conflict(
                "PLUGIN_ROLLBACK_UNAVAILABLE",
                "rollback of fresh installation is not supported",
            ));
        }
        let mut pins = BTreeMap::new();
        for p in before {
            let m = extensions::manifest(read, p.manifest_id)
                .await?
                .ok_or_else(invalid)?;
            pins.insert(p.key, m.version);
        }
        Some(pins)
    } else {
        None
    };
    plan(s, rows, exact.as_ref())
}
fn keys(p: &Plan) -> Vec<String> {
    p.graph
        .iter()
        .filter_map(|v| v.manifest["extension_key"].as_str().map(str::to_owned))
        .collect()
}
async fn project_preview(
    State(state): State<AppState>,
    Path((org, project)): Path<(String, String)>,
    req: Request,
) -> Result<Response, ExtensionError> {
    let (actor, body) = auth(&state, req, &org).await?;
    let s: Selection = decode_body(body)?;
    let rows = catalog(&state).await?;
    let mut read = open_project_scope(
        &state,
        &actor,
        parse_uuid(&org)?,
        parse_uuid(&project)?,
        ProjectReadPolicy::UnrestrictedManage,
    )
    .await?;
    let p = project_plan(&mut read, &s, &rows).await.map_err(domain)?;
    let current = project_pins(&mut read, &keys(&p)).await.map_err(domain)?;
    let value = preview(&p, Some(&current), s.rollback_of).map_err(domain)?;
    read.commit().await.map_err(domain)?;
    Ok(reply(value))
}
async fn apply(
    State(state): State<AppState>,
    Path((org, project)): Path<(String, String)>,
    req: Request,
) -> Result<Response, ExtensionError> {
    let (actor, body) = auth(&state, req, &org).await?;
    let b: Apply = decode_body(body)?;
    let rows = catalog(&state).await?;
    let mut edit = open_project_scope(
        &state,
        &actor,
        parse_uuid(&org)?,
        parse_uuid(&project)?,
        ProjectReadPolicy::UnrestrictedManage,
    )
    .await?;
    let p = project_plan(&mut edit, &b.selection, &rows)
        .await
        .map_err(domain)?;
    match_digest(
        &preview(&p, Some(&b.expected), b.selection.rollback_of).map_err(domain)?,
        &b.plan_digest,
    )?;
    let display = project_actor_display(&edit, &actor);
    let result = switch_graph(
        &mut edit,
        b.request_id,
        &b.plan_digest,
        p.epoch,
        &p.graph,
        &b.expected,
        b.selection.rollback_of,
        &display,
    )
    .await
    .map_err(domain)?;
    edit.commit().await.map_err(domain)?;
    Ok(reply(result))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn graph_plan_pins_dependencies_and_preview() {
        let mk = |key: &str, deps: Value| VerifiedRelease {
            metadata: json!({"release_digest":if key.ends_with("_a"){"a".repeat(64)}else{"b".repeat(64)},"catalog_epoch":4,
            "envelope":{"extension_key":key,"version":"1.0.0","dependencies":deps,"permissions":[],"assets":[]}}),
            document: br#"{"slot":"workspace","widgets":[],"actions":[]}"#.to_vec(),
            manifest: json!({"extension_key":key,"version":"1.0.0","entity_schemas":{},"capabilities":[]}),
        };
        let rows = vec![
            mk(
                "partner_pub_a",
                json!([{"key":"partner_pub_b","range":"^1"}]),
            ),
            mk("partner_pub_b", json!([])),
        ];
        let s = Selection {
            root: "partner_pub_a".into(),
            version: "1.0.0".into(),
            rollback_of: None,
        };
        let p = plan(&s, &rows, None).unwrap();
        assert_eq!(p.pins[0]["key"], "partner_pub_b");
        assert_eq!(p.pins[1]["key"], "partner_pub_a");
        let v = preview(&p, None, None).unwrap();
        assert!(match_digest(&v, v["plan_digest"].as_str().unwrap()).is_ok());
        assert!(match_digest(&v, "bad").is_err());
        assert!(plan(&s, &rows[..1], None).is_err());
        assert_ne!(
            v["plan_digest"],
            preview(&p, Some(&[]), None).unwrap()["plan_digest"]
        );
    }
}
