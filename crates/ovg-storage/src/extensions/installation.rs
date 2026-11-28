//! Transactional registration of reviewed manifests and CAS project graph switching.
//! Caller supplies only a server-verified plan; no downloaded migrations or code.
use super::{
    db_error, manifest_from_row, semantic_eq, ManifestRecord, OrgExtensionScope,
    RegisterManifestInput,
};
use crate::catalog::ProjectRead;
use ovg_core::AppError;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::Row;
use std::collections::BTreeSet;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphRelease {
    pub release_digest: String,
    pub manifest: Value,
}
fn conflict(msg: &str) -> AppError {
    AppError::conflict("PLUGIN_GRAPH_CONFLICT", msg)
}
fn invalid() -> AppError {
    AppError::validation("INVALID_PLUGIN_GRAPH", "invalid plugin graph")
}
fn check_graph(graph: &[GraphRelease]) -> Result<(), AppError> {
    let mut keys = BTreeSet::new();
    if graph.is_empty() || graph.len() > 100 {
        return Err(invalid());
    }
    for p in graph {
        let key = p.manifest["extension_key"].as_str().ok_or_else(invalid)?;
        if !key.starts_with("partner_")
            || !keys.insert(key)
            || p.release_digest.len() != 64
            || !p
                .release_digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(invalid());
        }
    }
    Ok(())
}
fn input(m: &Value) -> RegisterManifestInput<'_> {
    RegisterManifestInput {
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
    }
}
impl OrgExtensionScope<'_> {
    pub async fn graph_install(
        &mut self,
        request: Uuid,
        plan_digest: &str,
        epoch: i64,
        graph: &[GraphRelease],
    ) -> Result<Value, AppError> {
        self.require_graph_admin().await?;
        check_graph(graph)?;
        if request.is_nil() || epoch <= 0 {
            return Err(invalid());
        }
        if let Some(row)=sqlx::query("SELECT plan_digest,receipt FROM extensions.plugin_graph_installs WHERE organization_id=$1 AND request_id=$2")
            .bind(self.organization_id).bind(request).fetch_optional(&mut *self.tx).await.map_err(db_error)?{
            if row.try_get::<String,_>("plan_digest").map_err(db_error)?!=plan_digest{return Err(conflict("request ID reused"));}
            return row.try_get("receipt").map_err(db_error);
        }
        let floor:Option<i64>=sqlx::query_scalar("SELECT max(catalog_epoch) FROM extensions.plugin_graph_installs WHERE organization_id=$1")
            .bind(self.organization_id).fetch_one(&mut *self.tx).await.map_err(db_error)?;
        if epoch < floor.unwrap_or(0) {
            return Err(conflict("catalog epoch rollback"));
        }
        let mut items = Vec::new();
        for p in graph {
            let m = &p.manifest;
            let row=sqlx::query("SELECT * FROM extensions.manifests WHERE organization_id=$1 AND extension_key=$2 AND version=$3")
                .bind(self.organization_id).bind(m["extension_key"].as_str()).bind(m["version"].as_str())
                .fetch_optional(&mut *self.tx).await.map_err(db_error)?;
            let existing = row.is_some();
            let rec: ManifestRecord = match row {
                Some(row) => manifest_from_row(&row)?,
                None => self.register_manifest(input(m)).await?,
            };
            if !rec.contract.enabled || !semantic_eq(&rec.contract.contract(), m) {
                return Err(conflict(
                    "registered manifest differs from reviewed release",
                ));
            }
            items.push(json!({"manifest_id":rec.contract.id,"key":rec.contract.extension_key,"version":rec.contract.version,"release_digest":p.release_digest,"reused":existing}));
        }
        let receipt = json!({"request_id":request,"plan_digest":plan_digest,"catalog_epoch":epoch,"items":items,"activated":false});
        sqlx::query("INSERT INTO extensions.plugin_graph_installs(organization_id,request_id,actor_id,plan_digest,catalog_epoch,receipt) VALUES($1,$2,$3,$4,$5,$6)")
            .bind(self.organization_id).bind(request).bind(self.actor_id).bind(plan_digest).bind(epoch).bind(&receipt)
            .execute(&mut *self.tx).await.map_err(db_error)?;
        Ok(receipt)
    }
    async fn require_graph_admin(&mut self) -> Result<(), AppError> {
        if self.is_service {
            return Err(AppError::forbidden("human administrator required"));
        }
        let status: Option<String> =
            sqlx::query_scalar("SELECT status FROM org.organizations WHERE id=$1 FOR UPDATE")
                .bind(self.organization_id)
                .fetch_optional(&mut *self.tx)
                .await
                .map_err(db_error)?;
        let role:Option<(String,String)>=sqlx::query_as("SELECT role,status FROM org.organization_memberships WHERE organization_id=$1 AND user_id=$2 FOR UPDATE")
            .bind(self.organization_id).bind(self.actor_id).fetch_optional(&mut *self.tx).await.map_err(db_error)?;
        if status.as_deref() != Some("active")
            || !role.is_some_and(|(r, s)| s == "active" && matches!(r.as_str(), "owner" | "admin"))
        {
            return Err(AppError::forbidden("organization administrator required"));
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivationPin {
    pub key: String,
    pub manifest_id: Uuid,
    pub activation_id: Uuid,
    pub revision: i64,
    pub status: String,
}
pub async fn project_pins(
    read: &mut ProjectRead<'_>,
    keys: &[String],
) -> Result<Vec<ActivationPin>, AppError> {
    if read.is_gis_scoped() || read.actor_type() != "human" || keys.len() > 100 {
        return Err(AppError::forbidden("invalid graph scope"));
    }
    let rows=sqlx::query("SELECT extension_key,manifest_id,id,revision,status FROM extensions.project_activations
        WHERE organization_id=$1 AND project_id=$2 AND extension_key=ANY($3) ORDER BY extension_key FOR SHARE")
        .bind(read.organization_id()).bind(read.project_id()).bind(keys).fetch_all(&mut **read.transaction()).await.map_err(db_error)?;
    rows.into_iter()
        .map(|r| {
            Ok(ActivationPin {
                key: r.try_get("extension_key").map_err(db_error)?,
                manifest_id: r.try_get("manifest_id").map_err(db_error)?,
                activation_id: r.try_get("id").map_err(db_error)?,
                revision: r.try_get("revision").map_err(db_error)?,
                status: r.try_get("status").map_err(db_error)?,
            })
        })
        .collect()
}
pub async fn switch_history(read: &mut ProjectRead<'_>, request: Uuid) -> Result<Value, AppError> {
    if read.is_gis_scoped() || read.actor_type() != "human" {
        return Err(AppError::forbidden("invalid graph scope"));
    }
    sqlx::query_scalar("SELECT receipt FROM extensions.plugin_graph_switches WHERE organization_id=$1 AND project_id=$2 AND request_id=$3")
        .bind(read.organization_id()).bind(read.project_id()).bind(request).fetch_optional(&mut **read.transaction()).await.map_err(db_error)?
        .ok_or_else(||AppError::not_found("graph operation not found"))
}
/// Only reviewed stateless partners and same-major, data-contract-preserving
/// transitions. A rollback is an explicit previous graph, never an arbitrary downgrade.
pub async fn switch_graph(
    edit: &mut ProjectRead<'_>,
    request: Uuid,
    plan_digest: &str,
    epoch: i64,
    graph: &[GraphRelease],
    expected: &[ActivationPin],
    rollback_of: Option<Uuid>,
    display: &str,
) -> Result<Value, AppError> {
    if !edit.is_project_manager() || edit.is_gis_scoped() || edit.actor_type() != "human" {
        return Err(AppError::forbidden("human project manager required"));
    }
    check_graph(graph)?;
    if request.is_nil() || epoch <= 0 || expected.len() > 100 {
        return Err(invalid());
    }
    let org = edit.organization_id();
    let project = edit.project_id();
    let actor = edit.actor_id();
    if let Some(row)=sqlx::query("SELECT plan_digest,receipt FROM extensions.plugin_graph_switches WHERE organization_id=$1 AND project_id=$2 AND request_id=$3")
        .bind(org).bind(project).bind(request).fetch_optional(&mut **edit.transaction()).await.map_err(db_error)?{
        if row.try_get::<String,_>("plan_digest").map_err(db_error)?!=plan_digest{return Err(conflict("request ID reused"));}
        return row.try_get("receipt").map_err(db_error);
    }
    let floor:Option<i64>=sqlx::query_scalar("SELECT max(catalog_epoch) FROM extensions.plugin_graph_switches WHERE organization_id=$1 AND project_id=$2")
        .bind(org).bind(project).fetch_one(&mut **edit.transaction()).await.map_err(db_error)?;
    if epoch < floor.unwrap_or(0) || epoch < super::runtime_v2::minimum_approved_epoch(edit).await?
    {
        return Err(conflict("catalog epoch rollback"));
    }
    let keys: Vec<String> = graph
        .iter()
        .map(|p| {
            p.manifest["extension_key"]
                .as_str()
                .unwrap_or_default()
                .to_owned()
        })
        .collect();
    let before = project_pins(edit, &keys).await?;
    if before != expected {
        return Err(conflict("activation changed; refresh preview"));
    }
    let prior = if let Some(id) = rollback_of {
        let h = switch_history(edit, id).await?;
        let old: Vec<ActivationPin> =
            serde_json::from_value(h["before"].clone()).map_err(|_| invalid())?;
        let after: Vec<ActivationPin> =
            serde_json::from_value(h["after"].clone()).map_err(|_| invalid())?;
        if after != before
            || old.len() != keys.len()
            || old.iter().any(|p| p.status != "active")
            || h["rollback_of"].is_string()
        {
            return Err(conflict(
                "rollback requires unchanged prior graph with all previous releases active",
            ));
        }
        Some(old)
    } else {
        None
    };
    let mut targets = Vec::new();
    for p in graph {
        let key = p.manifest["extension_key"].as_str().ok_or_else(invalid)?;
        let version = p.manifest["version"].as_str().ok_or_else(invalid)?;
        let id:Uuid=sqlx::query_scalar("SELECT id FROM extensions.manifests WHERE organization_id=$1 AND extension_key=$2 AND version=$3 AND enabled")
            .bind(org).bind(key).bind(version).fetch_optional(&mut **edit.transaction()).await.map_err(db_error)?
            .ok_or_else(||AppError::not_found("install reviewed graph in organization first"))?;
        let target = super::manifest(edit, id).await?.ok_or_else(invalid)?;
        if target
            .entity_schemas
            .as_object()
            .is_none_or(|m| !m.is_empty())
            || !semantic_eq(&target.contract(), &p.manifest)
            || !super::validation::validate_generic(&target)
        {
            return Err(conflict("reviewed manifest mismatch"));
        }
        if let Some(old) = before.iter().find(|v| v.key == key) {
            let current = super::manifest(edit, old.manifest_id)
                .await?
                .ok_or_else(invalid)?;
            if old.manifest_id != id {
                let a = super::validation::semver(&current.version).ok_or_else(invalid)?;
                let b = super::validation::semver(&target.version).ok_or_else(invalid)?;
                if a.0 != b.0
                    || current.api_compatibility != target.api_compatibility
                    || current.permissions != target.permissions
                    || current.commands != target.commands
                    || current.capabilities != target.capabilities
                    || current.entity_schemas != target.entity_schemas
                    || current
                        .entity_schemas
                        .as_object()
                        .is_none_or(|x| !x.is_empty())
                {
                    return Err(conflict("transition requires verified data migration"));
                }
                if prior.is_none() && b <= a {
                    return Err(conflict("downgrade requires recorded rollback"));
                }
            }
        }
        if prior
            .as_ref()
            .is_some_and(|pins| !pins.iter().any(|v| v.key == key && v.manifest_id == id))
        {
            return Err(conflict("rollback target differs from previous graph"));
        }
        targets.push(target);
    }
    for target in targets {
        match before.iter().find(|p| p.key == target.extension_key) {
            Some(old) if old.manifest_id == target.id && old.status == "active" => {}
            Some(old) if old.manifest_id != target.id => {
                super::transition(
                    edit,
                    actor,
                    display,
                    old.activation_id,
                    old.manifest_id,
                    target.id,
                )
                .await?;
                if old.status != "active" {
                    super::activate(edit, actor, display, &target).await?;
                }
            }
            _ => {
                super::activate(edit, actor, display, &target).await?;
            }
        }
    }
    let after = project_pins(edit, &keys).await?;
    let receipt = json!({"request_id":request,"plan_digest":plan_digest,"catalog_epoch":epoch,"before":before,"after":after,"rollback_of":rollback_of,"grants_created":false});
    sqlx::query("INSERT INTO extensions.plugin_graph_switches(organization_id,project_id,request_id,actor_id,plan_digest,catalog_epoch,receipt) VALUES($1,$2,$3,$4,$5,$6,$7)")
        .bind(org).bind(project).bind(request).bind(actor).bind(plan_digest).bind(epoch).bind(&receipt)
        .execute(&mut **edit.transaction()).await.map_err(db_error)?;
    Ok(receipt)
}
