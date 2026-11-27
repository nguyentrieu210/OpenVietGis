//! E12 integration-hub authorization context, export scopes, and QGIS bindings.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{DateTime, Duration, Utc};
use ovg_core::{lookup_readable_crs, AppError};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use unicode_normalization::{char::is_combining_mark, UnicodeNormalization};
use uuid::Uuid;

use crate::catalog::ProjectRead;
use crate::collaboration::isoformat;
use crate::editing::{EditError, EditScope};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectiveProjectLevel {
    Read,
    Edit,
    Manage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IntegrationCatalogContext {
    pub level: EffectiveProjectLevel,
    pub scoped_access: bool,
}

fn db_error(error: sqlx::Error) -> AppError {
    tracing::error!(%error, "integration-hub authorization query failed");
    AppError::internal("integration-hub authorization query failed")
}

/// Compute only the permission dimensions consumed by the integration catalog.
/// ProjectRead has already proven project.read and current browser-session
/// membership; this query derives the stronger edit/manage levels and the GIS
/// scoped flag inside the same transaction.
pub async fn catalog_context(
    read: &mut ProjectRead<'_>,
    actor_id: Uuid,
) -> Result<IntegrationCatalogContext, AppError> {
    let row = sqlx::query(
        "SELECT
            gis_access.scoped($1, $3) AS scoped_access,
            (
              m.role IN ('owner','admin')
              OR EXISTS (
                SELECT 1
                FROM org.resource_grants g
                JOIN workspace.projects p
                  ON p.organization_id = m.organization_id
                 AND p.id = $3
                WHERE g.organization_id = m.organization_id
                  AND g.membership_id = m.id
                  AND g.subject_kind = 'membership'
                  AND g.revoked_at IS NULL
                  AND g.role = 'manager'
                  AND (
                    g.scope_kind = 'organization'
                    OR (g.scope_kind = 'workspace' AND g.workspace_id = p.workspace_id)
                    OR (g.scope_kind = 'project' AND g.project_id = p.id)
                  )
              )
            ) AS can_manage,
            (
              m.role IN ('owner','admin')
              OR EXISTS (
                SELECT 1
                FROM org.resource_grants g
                JOIN workspace.projects p
                  ON p.organization_id = m.organization_id
                 AND p.id = $3
                WHERE g.organization_id = m.organization_id
                  AND g.membership_id = m.id
                  AND g.subject_kind = 'membership'
                  AND g.revoked_at IS NULL
                  AND g.role IN ('manager','editor','contributor')
                  AND (
                    g.scope_kind = 'organization'
                    OR (g.scope_kind = 'workspace' AND g.workspace_id = p.workspace_id)
                    OR (g.scope_kind = 'project' AND g.project_id = p.id)
                  )
              )
            ) AS can_edit
         FROM org.organization_memberships m
         WHERE m.organization_id = $1
           AND m.user_id = $2
           AND m.status = 'active'",
    )
    .bind(read.organization_id())
    .bind(actor_id)
    .bind(read.project_id())
    .fetch_optional(&mut **read.transaction())
    .await
    .map_err(db_error)?
    .ok_or_else(|| AppError::not_found("organization not found"))?;

    let scoped_access: bool = row.try_get("scoped_access").map_err(db_error)?;
    let can_manage: bool = row.try_get("can_manage").map_err(db_error)?;
    let can_edit: bool = row.try_get("can_edit").map_err(db_error)?;
    let level = if can_manage {
        EffectiveProjectLevel::Manage
    } else if can_edit {
        EffectiveProjectLevel::Edit
    } else {
        EffectiveProjectLevel::Read
    };
    Ok(IntegrationCatalogContext {
        level,
        scoped_access,
    })
}

#[derive(Debug, Clone)]
pub struct LayerAccessRow {
    pub layer_id: Uuid,
    pub source_id: Uuid,
    pub name: String,
    pub geometry_family: String,
    pub dimensions: String,
    pub validation_state: String,
    pub source_crs_code: Option<i32>,
    pub stored_crs_code: Option<i32>,
    pub layer_revision: i64,
    pub content_revision: String,
    pub schema_revision: String,
    pub export_acl: bool,
}

pub async fn accessible_layers(
    read: &mut ProjectRead<'_>,
    scoped_access: bool,
) -> Result<Vec<LayerAccessRow>, AppError> {
    let rows = sqlx::query(
        "SELECT
            l.id AS layer_id,
            l.source_id AS source_id,
            l.name AS name,
            s.geometry_family AS geometry_family,
            s.dimensions AS dimensions,
            s.validation_state AS validation_state,
            s.source_crs AS source_crs_code,
            s.stored_crs AS stored_crs_code,
            l.layer_revision AS layer_revision,
            s.content_revision AS content_revision,
            s.schema_revision AS schema_revision,
            CASE WHEN $3
              THEN gis_access.source_allowed($1,$2,s.id,'export')
              ELSE true
            END AS export_acl
         FROM spatial.layers l
         JOIN spatial.data_sources s
           ON s.organization_id=l.organization_id
          AND s.project_id=l.project_id
          AND s.id=l.source_id
         WHERE l.organization_id=$1
           AND l.project_id=$2
           AND l.lifecycle='active'
           AND s.lifecycle='active'
         ORDER BY l.name ASC,l.id ASC
         LIMIT 1000",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(scoped_access)
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(db_error)?;

    rows.into_iter()
        .map(|row| {
            Ok(LayerAccessRow {
                layer_id: row.try_get("layer_id").map_err(db_error)?,
                source_id: row.try_get("source_id").map_err(db_error)?,
                name: row.try_get("name").map_err(db_error)?,
                geometry_family: row.try_get("geometry_family").map_err(db_error)?,
                dimensions: row.try_get("dimensions").map_err(db_error)?,
                validation_state: row.try_get("validation_state").map_err(db_error)?,
                source_crs_code: row.try_get("source_crs_code").map_err(db_error)?,
                stored_crs_code: row.try_get("stored_crs_code").map_err(db_error)?,
                layer_revision: row.try_get("layer_revision").map_err(db_error)?,
                content_revision: row.try_get("content_revision").map_err(db_error)?,
                schema_revision: row.try_get("schema_revision").map_err(db_error)?,
                export_acl: row.try_get("export_acl").map_err(db_error)?,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_level_order_is_monotonic() {
        assert_ne!(EffectiveProjectLevel::Read, EffectiveProjectLevel::Edit);
        assert_ne!(EffectiveProjectLevel::Edit, EffectiveProjectLevel::Manage);
    }
}

fn edit_db_err(error: sqlx::Error) -> EditError {
    tracing::error!(%error, "integration-hub storage query failed");
    EditError::Internal(error.to_string())
}

/// Strip Vietnamese diacritics ("Bình Hưng" -> "Binh Hung") before ASCII folding.
fn ascii_fold(text: &str) -> String {
    let mut folded = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            'đ' => folded.push('d'),
            'Đ' => folded.push('D'),
            other => {
                for decomposed in other.nfkd() {
                    if !is_combining_mark(decomposed) {
                        folded.push(decomposed);
                    }
                }
            }
        }
    }
    folded
}

fn ascii_tokens(text: &str) -> Vec<String> {
    let folded = ascii_fold(text).to_ascii_lowercase();
    let mut tokens = Vec::new();
    let mut cur = String::new();
    for ch in folded.chars() {
        if ch.is_ascii_alphanumeric() {
            cur.push(ch);
        } else if !cur.is_empty() {
            tokens.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        tokens.push(cur);
    }
    tokens
}

fn natural_key(name: &str) -> (String, u64, String) {
    if let Some(pos) = name.find(|c: char| c.is_ascii_digit()) {
        let prefix = name[..pos].to_owned();
        let digits: String = name[pos..]
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        let num = digits.parse::<u64>().unwrap_or(u64::MAX);
        (prefix, num, name.to_owned())
    } else {
        (name.to_owned(), 0, name.to_owned())
    }
}

/// Commune -> hamlet -> route choices for scoped export (`Spec 054`).
pub async fn export_scopes(scope: &mut EditScope<'_>) -> Result<Value, EditError> {
    let org_id = scope.organization_id();
    let project_id = scope.project_id();
    let layer_rows = sqlx::query(
        "SELECT l.id AS layer_id, l.name AS name, l.source_id AS source_id,
                s.geometry_family AS geometry_family
         FROM spatial.layers l
         JOIN spatial.data_sources s
           ON s.organization_id = l.organization_id
          AND s.project_id = l.project_id
          AND s.id = l.source_id
         WHERE l.organization_id = $1 AND l.project_id = $2 AND l.lifecycle = 'active'",
    )
    .bind(org_id)
    .bind(project_id)
    .fetch_all(&mut **scope.tx_mut())
    .await
    .map_err(edit_db_err)?;

    let mut commune_sids = Vec::<Uuid>::new();
    let mut hamlet_sids = Vec::<Uuid>::new();
    let mut route_sids = Vec::<Uuid>::new();

    for row in layer_rows {
        let name: String = row.try_get("name").map_err(edit_db_err)?;
        let sid: Uuid = row.try_get("source_id").map_err(edit_db_err)?;
        let family: String = row.try_get("geometry_family").map_err(edit_db_err)?;
        let tokens = ascii_tokens(&name);
        let first = tokens.first().map(String::as_str).unwrap_or("");
        let words: HashSet<&str> = tokens.iter().map(String::as_str).collect();
        let polygon = matches!(family.as_str(), "Polygon" | "MultiPolygon");
        let boundary = words.contains("ranh");

        if polygon
            && (words.contains("xa") || words.contains("phuong"))
            && (boundary || matches!(first, "xa" | "phuong"))
        {
            commune_sids.push(sid);
        } else if polygon
            && words.contains("ap")
            && (boundary || first == "ap")
            && !words.contains("cu")
            && !words.contains("lich")
        {
            hamlet_sids.push(sid);
        } else if matches!(family.as_str(), "LineString" | "MultiLineString")
            && (words.contains("tuyen") || (words.contains("duong") && words.contains("hem")))
        {
            route_sids.push(sid);
        }
    }

    let mut communes = Vec::<(String, String, String)>::new();
    if !commune_sids.is_empty() {
        let rows = sqlx::query(
            "SELECT feature_id, attributes FROM spatial.reference_features
             WHERE organization_id = $1 AND project_id = $2
               AND source_id = ANY($3) AND geometry IS NOT NULL",
        )
        .bind(org_id)
        .bind(project_id)
        .bind(&commune_sids)
        .fetch_all(&mut **scope.tx_mut())
        .await
        .map_err(edit_db_err)?;
        for row in rows {
            let fid: String = row.try_get("feature_id").map_err(edit_db_err)?;
            let attrs: Value = row.try_get("attributes").map_err(edit_db_err)?;
            let name = attr_non_empty_str(&attrs, "name")
                .or_else(|| attr_non_empty_str(&attrs, "ten"))
                .unwrap_or_else(|| "Xã".to_owned());
            let code = attr_non_empty_str(&attrs, "ma_xa").unwrap_or_default();
            communes.push((fid, name, code));
        }
        communes.sort_by(|a, b| natural_key(&a.1).cmp(&natural_key(&b.1)));
    }

    let mut hamlets = Vec::<(String, String, String)>::new();
    if !hamlet_sids.is_empty() {
        let rows = sqlx::query(
            "SELECT feature_id, attributes FROM spatial.reference_features
             WHERE organization_id = $1 AND project_id = $2
               AND source_id = ANY($3) AND geometry IS NOT NULL",
        )
        .bind(org_id)
        .bind(project_id)
        .bind(&hamlet_sids)
        .fetch_all(&mut **scope.tx_mut())
        .await
        .map_err(edit_db_err)?;
        for row in rows {
            let fid: String = row.try_get("feature_id").map_err(edit_db_err)?;
            let attrs: Value = row.try_get("attributes").map_err(edit_db_err)?;
            let name = attr_non_empty_str(&attrs, "name")
                .or_else(|| attr_non_empty_str(&attrs, "ten"))
                .unwrap_or_else(|| "Ấp".to_owned());
            let code = attr_non_empty_str(&attrs, "ma_xa").unwrap_or_default();
            hamlets.push((fid, name, code));
        }
        hamlets.sort_by(|a, b| natural_key(&a.1).cmp(&natural_key(&b.1)));
    }

    #[derive(Default)]
    struct RouteGroup {
        name: String,
        route_ids: Vec<String>,
        hamlet_ids: BTreeSet<String>,
    }
    let mut route_groups = BTreeMap::<String, RouteGroup>::new();
    if !route_sids.is_empty() {
        let rows = sqlx::query(
            "SELECT r.attributes->>'ten' AS name,
                    coalesce(r.attributes->>'id', r.feature_id) AS route_id,
                    array_remove(array_agg(DISTINCT h.feature_id), NULL) AS hamlet_ids
             FROM spatial.reference_features r
             LEFT JOIN spatial.reference_features h
               ON h.organization_id = r.organization_id
              AND h.project_id = r.project_id
              AND h.source_id = ANY($4)
              AND h.geometry IS NOT NULL
              AND ST_Intersects(h.geometry, ST_Transform(r.geometry, ST_SRID(h.geometry)))
             WHERE r.organization_id = $1
               AND r.project_id = $2
               AND r.source_id = ANY($3)
               AND r.geometry IS NOT NULL
               AND coalesce(r.attributes->>'ten', '') <> ''
             GROUP BY r.feature_id, r.attributes",
        )
        .bind(org_id)
        .bind(project_id)
        .bind(&route_sids)
        .bind(&hamlet_sids)
        .fetch_all(&mut **scope.tx_mut())
        .await
        .map_err(edit_db_err)?;
        for row in rows {
            let raw_name: String = row.try_get("name").map_err(edit_db_err)?;
            let normalized_name = raw_name.split_whitespace().collect::<Vec<_>>().join(" ");
            let route_id: String = row.try_get("route_id").map_err(edit_db_err)?;
            let hids: Option<Vec<String>> = row.try_get("hamlet_ids").map_err(edit_db_err)?;
            let key = ascii_fold(&normalized_name).to_ascii_lowercase();
            let entry = route_groups.entry(key).or_insert_with(|| RouteGroup {
                name: normalized_name,
                route_ids: Vec::new(),
                hamlet_ids: BTreeSet::new(),
            });
            entry.route_ids.push(route_id);
            if let Some(list) = hids {
                for hid in list {
                    entry.hamlet_ids.insert(hid);
                }
            }
        }
    }

    let mut routes: Vec<RouteGroup> = route_groups.into_values().collect();
    routes.sort_by(|a, b| natural_key(&a.name).cmp(&natural_key(&b.name)));

    Ok(json!({
        "communes": communes.into_iter().map(|(id, name, code)| json!({
            "id": id,
            "name": name,
            "code": code,
        })).collect::<Vec<_>>(),
        "hamlets": hamlets.into_iter().map(|(id, name, commune_code)| json!({
            "id": id,
            "name": name,
            "commune_code": commune_code,
        })).collect::<Vec<_>>(),
        "routes": routes.into_iter().map(|r| json!({
            "name": r.name,
            "route_ids": r.route_ids,
            "hamlet_ids": r.hamlet_ids.into_iter().collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    }))
}

fn attr_non_empty_str(attrs: &Value, key: &str) -> Option<String> {
    match attrs.get(key) {
        Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
        Some(Value::Number(n)) => Some(n.to_string()),
        Some(Value::Bool(b)) => Some(if *b { "True" } else { "False" }.to_owned()),
        _ => None,
    }
}

pub fn is_valid_binding_field_id(value: &str) -> bool {
    let bytes = value.as_bytes();
    (1..=64).contains(&bytes.len())
        && bytes[0].is_ascii_alphabetic()
        && bytes[1..]
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b'_')
}

pub fn is_valid_dataset_ref(value: &str) -> bool {
    let bytes = value.as_bytes();
    (1..=128).contains(&bytes.len())
        && bytes[0].is_ascii_alphanumeric()
        && bytes[1..]
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

fn is_explicit_readable(stored_crs: Option<i32>, validation_state: &str) -> bool {
    validation_state == "ready"
        && stored_crs
            .and_then(|c| u32::try_from(c).ok())
            .is_some_and(|c| lookup_readable_crs(c).is_some())
}

const BINDING_VIEW_COLS: &str = "
    id, layer_id, adapter_key, external_dataset_ref, direction,
    source_crs_code, target_crs_code, external_primary_key,
    allowed_field_ids, base_content_revision, base_schema_revision,
    state, revision, created_at, updated_at, activated_at, last_paired_at,
    service_principal_id";

fn serialize_binding(row: &sqlx::postgres::PgRow, open_conflicts: i64) -> Result<Value, EditError> {
    let id: Uuid = row.try_get("id").map_err(edit_db_err)?;
    let layer_id: Uuid = row.try_get("layer_id").map_err(edit_db_err)?;
    let adapter_key: String = row.try_get("adapter_key").map_err(edit_db_err)?;
    let external_dataset_ref: String = row.try_get("external_dataset_ref").map_err(edit_db_err)?;
    let direction: String = row.try_get("direction").map_err(edit_db_err)?;
    let source_crs_code: i32 = row.try_get("source_crs_code").map_err(edit_db_err)?;
    let target_crs_code: i32 = row.try_get("target_crs_code").map_err(edit_db_err)?;
    let external_primary_key: String = row.try_get("external_primary_key").map_err(edit_db_err)?;
    let allowed_field_ids: Value = row.try_get("allowed_field_ids").map_err(edit_db_err)?;
    let base_content_revision: String =
        row.try_get("base_content_revision").map_err(edit_db_err)?;
    let base_schema_revision: String = row.try_get("base_schema_revision").map_err(edit_db_err)?;
    let state: String = row.try_get("state").map_err(edit_db_err)?;
    let revision: i32 = row.try_get("revision").map_err(edit_db_err)?;
    let created_at: DateTime<Utc> = row.try_get("created_at").map_err(edit_db_err)?;
    let updated_at: DateTime<Utc> = row.try_get("updated_at").map_err(edit_db_err)?;
    let activated_at: Option<DateTime<Utc>> = row.try_get("activated_at").map_err(edit_db_err)?;
    let last_paired_at: Option<DateTime<Utc>> =
        row.try_get("last_paired_at").map_err(edit_db_err)?;

    let is_active = state == "active";
    let is_paired = is_active && last_paired_at.is_some();
    let reason = if is_paired {
        "ACTIVE_PAIRED_MANUAL_SYNC"
    } else if is_active {
        "ACTIVE_AWAITING_QGIS_PAIRING"
    } else {
        "DRAFT_ONLY_AGENT_NOT_PAIRED"
    };

    Ok(json!({
        "id": id.to_string(),
        "layer_id": layer_id.to_string(),
        "adapter_key": adapter_key,
        "external_dataset_ref": external_dataset_ref,
        "direction": direction,
        "source_crs_code": source_crs_code,
        "target_crs_code": target_crs_code,
        "external_primary_key": external_primary_key,
        "allowed_field_ids": allowed_field_ids,
        "base_content_revision": base_content_revision,
        "base_schema_revision": base_schema_revision,
        "state": state,
        "revision": revision,
        "created_at": isoformat(created_at),
        "updated_at": isoformat(updated_at),
        "activated_at": activated_at.map(isoformat),
        "last_paired_at": last_paired_at.map(isoformat),
        "writes_allowed": is_active,
        "deletes_allowed": false,
        "connected": is_paired,
        "open_conflicts_count": open_conflicts,
        "reason": reason,
    }))
}

fn serialize_token(row: &sqlx::postgres::PgRow) -> Result<Value, EditError> {
    let id: Uuid = row.try_get("id").map_err(edit_db_err)?;
    let binding_id: Uuid = row.try_get("binding_id").map_err(edit_db_err)?;
    let label: String = row.try_get("label").map_err(edit_db_err)?;
    let token_prefix: String = row.try_get("token_prefix").map_err(edit_db_err)?;
    let expires_at: DateTime<Utc> = row.try_get("expires_at").map_err(edit_db_err)?;
    let last_used_at: Option<DateTime<Utc>> = row.try_get("last_used_at").map_err(edit_db_err)?;
    let revoked_at: Option<DateTime<Utc>> = row.try_get("revoked_at").map_err(edit_db_err)?;
    let created_at: DateTime<Utc> = row.try_get("created_at").map_err(edit_db_err)?;
    Ok(json!({
        "id": id.to_string(),
        "binding_id": binding_id.to_string(),
        "label": label,
        "token_prefix": token_prefix,
        "expires_at": isoformat(expires_at),
        "last_used_at": last_used_at.map(isoformat),
        "revoked_at": revoked_at.map(isoformat),
        "created_at": isoformat(created_at),
    }))
}

fn serialize_conflict(row: &sqlx::postgres::PgRow) -> Result<Value, EditError> {
    let id: Uuid = row.try_get("id").map_err(edit_db_err)?;
    let binding_id: Uuid = row.try_get("binding_id").map_err(edit_db_err)?;
    let layer_id: Uuid = row.try_get("layer_id").map_err(edit_db_err)?;
    let feature_id: String = row.try_get("feature_id").map_err(edit_db_err)?;
    let conflict_kind: String = row.try_get("conflict_kind").map_err(edit_db_err)?;
    let field_name: Option<String> = row.try_get("field_name").map_err(edit_db_err)?;
    let baseline_value: Option<Value> = row.try_get("baseline_value").map_err(edit_db_err)?;
    let web_value: Option<Value> = row.try_get("web_value").map_err(edit_db_err)?;
    let qgis_value: Option<Value> = row.try_get("qgis_value").map_err(edit_db_err)?;
    let status: String = row.try_get("status").map_err(edit_db_err)?;
    let resolved_value: Option<Value> = row.try_get("resolved_value").map_err(edit_db_err)?;
    let resolution_note: Option<String> = row.try_get("resolution_note").map_err(edit_db_err)?;
    let created_at: DateTime<Utc> = row.try_get("created_at").map_err(edit_db_err)?;
    let resolved_at: Option<DateTime<Utc>> = row.try_get("resolved_at").map_err(edit_db_err)?;

    Ok(json!({
        "id": id.to_string(),
        "binding_id": binding_id.to_string(),
        "layer_id": layer_id.to_string(),
        "feature_id": feature_id,
        "conflict_kind": conflict_kind,
        "field_name": field_name,
        "baseline_value": baseline_value,
        "web_value": web_value,
        "qgis_value": qgis_value,
        "status": status,
        "resolved_value": resolved_value,
        "resolution_note": resolution_note,
        "created_at": isoformat(created_at),
        "resolved_at": resolved_at.map(isoformat),
    }))
}

async fn append_binding_audit(
    tx: &mut Transaction<'_, Postgres>,
    org_id: Uuid,
    actor_type: &str,
    actor_id: Uuid,
    actor_display: &str,
    action: &str,
    resource_type: &str,
    resource_id: Uuid,
    metadata: Value,
) -> Result<(), EditError> {
    sqlx::query(
        "INSERT INTO audit.audit_events (
            id, organization_id, actor_type, actor_id, actor_display_snapshot,
            action, resource_type, resource_id, outcome, metadata, created_at
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'success', $9, $10)",
    )
    .bind(Uuid::new_v4())
    .bind(org_id)
    .bind(actor_type)
    .bind(actor_id)
    .bind(actor_display)
    .bind(action)
    .bind(resource_type)
    .bind(resource_id)
    .bind(metadata)
    .bind(Utc::now())
    .execute(&mut **tx)
    .await
    .map_err(edit_db_err)?;
    Ok(())
}

pub async fn list_binding_candidates(scope: &mut EditScope<'_>) -> Result<Value, EditError> {
    let org_id = scope.organization_id();
    let project_id = scope.project_id();
    let rows = sqlx::query(
        "SELECT l.id AS layer_id, l.name AS layer_name, l.source_id,
                s.stored_crs, s.geometry_family, s.dimensions, s.validation_state
         FROM spatial.layers l
         JOIN spatial.data_sources s
           ON s.organization_id = l.organization_id
          AND s.project_id = l.project_id
          AND s.id = l.source_id
         WHERE l.organization_id = $1 AND l.project_id = $2
           AND l.lifecycle = 'active' AND s.lifecycle = 'active'
         ORDER BY l.name, l.id
         LIMIT 200",
    )
    .bind(org_id)
    .bind(project_id)
    .fetch_all(&mut **scope.tx_mut())
    .await
    .map_err(edit_db_err)?;

    let mut items = Vec::new();
    for row in rows {
        let validation_state: String = row.try_get("validation_state").map_err(edit_db_err)?;
        let geometry_family: String = row.try_get("geometry_family").map_err(edit_db_err)?;
        let dimensions: String = row.try_get("dimensions").map_err(edit_db_err)?;
        let stored_crs: Option<i32> = row.try_get("stored_crs").map_err(edit_db_err)?;
        if validation_state != "ready"
            || !matches!(geometry_family.as_str(), "Point" | "LineString" | "Polygon")
            || dimensions != "XY"
            || !is_explicit_readable(stored_crs, &validation_state)
        {
            continue;
        }
        let source_id: Uuid = row.try_get("source_id").map_err(edit_db_err)?;
        let frows: Vec<String> = sqlx::query_scalar(
            "SELECT field_id FROM spatial.source_fields
             WHERE organization_id = $1 AND project_id = $2 AND source_id = $3",
        )
        .bind(org_id)
        .bind(project_id)
        .bind(source_id)
        .fetch_all(&mut **scope.tx_mut())
        .await
        .map_err(edit_db_err)?;
        let mut allowed_fields: Vec<String> = frows
            .into_iter()
            .filter(|f| is_valid_binding_field_id(f))
            .collect();
        allowed_fields.sort();
        allowed_fields.truncate(100);
        if allowed_fields.is_empty() {
            continue;
        }
        let layer_id: Uuid = row.try_get("layer_id").map_err(edit_db_err)?;
        let layer_name: String = row.try_get("layer_name").map_err(edit_db_err)?;
        items.push(json!({
            "layer_id": layer_id.to_string(),
            "source_id": source_id.to_string(),
            "layer_name": layer_name,
            "stored_crs_code": stored_crs.unwrap_or_default(),
            "geometry_family": geometry_family,
            "allowed_field_ids": allowed_fields,
            "binding_status": "draft_only",
        }));
    }
    Ok(json!({ "items": items }))
}

pub async fn list_bindings(scope: &mut EditScope<'_>) -> Result<Value, EditError> {
    let org_id = scope.organization_id();
    let project_id = scope.project_id();
    let sql = format!(
        "SELECT {BINDING_VIEW_COLS} FROM integration_hub.connector_bindings
         WHERE organization_id = $1 AND project_id = $2
         ORDER BY created_at DESC, id DESC LIMIT 200"
    );
    let rows = sqlx::query(&sql)
        .bind(org_id)
        .bind(project_id)
        .fetch_all(&mut **scope.tx_mut())
        .await
        .map_err(edit_db_err)?;
    let counts = sqlx::query(
        "SELECT binding_id, COUNT(*)::bigint AS open_count
         FROM integration_hub.sync_conflicts
         WHERE organization_id = $1 AND project_id = $2 AND status = 'open'
         GROUP BY binding_id",
    )
    .bind(org_id)
    .bind(project_id)
    .fetch_all(&mut **scope.tx_mut())
    .await
    .map_err(edit_db_err)?;
    let mut map = BTreeMap::<Uuid, i64>::new();
    for r in counts {
        let bid: Uuid = r.try_get("binding_id").map_err(edit_db_err)?;
        let cnt: i64 = r.try_get("open_count").map_err(edit_db_err)?;
        map.insert(bid, cnt);
    }
    let items: Vec<Value> = rows
        .iter()
        .map(|row| {
            let id: Uuid = row.try_get("id").map_err(edit_db_err)?;
            serialize_binding(row, *map.get(&id).unwrap_or(&0))
        })
        .collect::<Result<_, EditError>>()?;
    Ok(json!({ "items": items }))
}

pub struct CreateBindingInput {
    pub layer_id: Uuid,
    pub external_dataset_ref: String,
    pub direction: String,
    pub source_crs_code: i32,
    pub target_crs_code: i32,
    pub external_primary_key: String,
    pub allowed_field_ids: Vec<String>,
}

pub async fn create_binding(
    scope: &mut EditScope<'_>,
    input: CreateBindingInput,
) -> Result<Value, EditError> {
    let org_id = scope.organization_id();
    let project_id = scope.project_id();
    let actor_id = scope.actor_id();
    let actor_display = scope.display_name().to_owned();

    let layer = sqlx::query(
        "SELECT l.source_id, s.geometry_family, s.dimensions, s.validation_state,
                s.stored_crs, s.schema_revision, s.content_revision
         FROM spatial.layers l
         JOIN spatial.data_sources s
           ON s.organization_id = l.organization_id
          AND s.project_id = l.project_id AND s.id = l.source_id
         WHERE l.organization_id = $1 AND l.project_id = $2
           AND l.id = $3 AND l.lifecycle = 'active' AND s.lifecycle = 'active'
         FOR SHARE OF l, s",
    )
    .bind(org_id)
    .bind(project_id)
    .bind(input.layer_id)
    .fetch_optional(&mut **scope.tx_mut())
    .await
    .map_err(edit_db_err)?;
    let Some(layer) = layer else {
        return Err(EditError::detail(404, "Layer not found."));
    };
    let val_state: String = layer.try_get("validation_state").map_err(edit_db_err)?;
    let family: String = layer.try_get("geometry_family").map_err(edit_db_err)?;
    let dimensions: String = layer.try_get("dimensions").map_err(edit_db_err)?;
    if val_state != "ready"
        || !matches!(family.as_str(), "Point" | "LineString" | "Polygon")
        || dimensions != "XY"
    {
        return Err(EditError::detail(
            422,
            "Only verified single-part 2D layers can be bound.",
        ));
    }
    let stored_crs: Option<i32> = layer.try_get("stored_crs").map_err(edit_db_err)?;
    if !is_explicit_readable(stored_crs, &val_state) {
        return Err(EditError::detail(
            422,
            "Layer CRS must be verified and readable.",
        ));
    }
    let stored_crs = stored_crs.unwrap_or_default();
    if input.source_crs_code != stored_crs || input.target_crs_code != stored_crs {
        return Err(EditError::detail(
            422,
            "MVP requires identical verified EPSG CRS; no silent reprojection.",
        ));
    }
    let source_id: Uuid = layer.try_get("source_id").map_err(edit_db_err)?;
    let present: HashSet<String> = sqlx::query_scalar::<_, String>(
        "SELECT field_id FROM spatial.source_fields
         WHERE organization_id = $1 AND project_id = $2 AND source_id = $3",
    )
    .bind(org_id)
    .bind(project_id)
    .bind(source_id)
    .fetch_all(&mut **scope.tx_mut())
    .await
    .map_err(edit_db_err)?
    .into_iter()
    .collect();
    if input.allowed_field_ids.iter().any(|f| !present.contains(f)) {
        return Err(EditError::detail(
            422,
            "Only known source field IDs may be mapped.",
        ));
    }

    let binding_id = Uuid::new_v4();
    let content_rev: String = layer.try_get("content_revision").map_err(edit_db_err)?;
    let schema_rev: String = layer.try_get("schema_revision").map_err(edit_db_err)?;
    let fields_json = Value::Array(
        input
            .allowed_field_ids
            .iter()
            .cloned()
            .map(Value::String)
            .collect(),
    );

    let insert_sql = format!(
        "INSERT INTO integration_hub.connector_bindings
           (id, organization_id, project_id, layer_id, adapter_key,
            external_dataset_ref, direction, source_crs_code, target_crs_code,
            external_primary_key, allowed_field_ids, base_content_revision,
            base_schema_revision, created_by)
         VALUES ($1, $2, $3, $4, 'qgis', $5, $6, $7, $8, $9, $10, $11, $12, $13)
         ON CONFLICT (organization_id, project_id, layer_id, adapter_key, external_dataset_ref)
           WHERE state <> 'revoked' DO NOTHING
         RETURNING {BINDING_VIEW_COLS}"
    );
    let inserted = sqlx::query(&insert_sql)
        .bind(binding_id)
        .bind(org_id)
        .bind(project_id)
        .bind(input.layer_id)
        .bind(&input.external_dataset_ref)
        .bind(&input.direction)
        .bind(input.source_crs_code)
        .bind(input.target_crs_code)
        .bind(&input.external_primary_key)
        .bind(&fields_json)
        .bind(&content_rev)
        .bind(&schema_rev)
        .bind(actor_id)
        .fetch_optional(&mut **scope.tx_mut())
        .await
        .map_err(edit_db_err)?;
    let Some(row) = inserted else {
        return Err(EditError::detail(
            409,
            "Binding already exists; use or revoke the existing draft.",
        ));
    };

    append_binding_audit(
        scope.tx_mut(),
        org_id,
        "human",
        actor_id,
        &actor_display,
        "integration.binding.draft_created",
        "connector_binding",
        binding_id,
        json!({ "adapter": "qgis", "layer_id": input.layer_id.to_string(), "writes_allowed": false }),
    )
    .await?;

    serialize_binding(&row, 0)
}

pub async fn activate_binding(
    scope: &mut EditScope<'_>,
    binding_id: Uuid,
    expected_revision: i32,
) -> Result<Value, EditError> {
    let org_id = scope.organization_id();
    let project_id = scope.project_id();
    let actor_id = scope.actor_id();
    let actor_display = scope.display_name().to_owned();
    let sql = format!(
        "UPDATE integration_hub.connector_bindings
         SET state = 'active', revision = revision + 1,
             activated_at = COALESCE(activated_at, now()), updated_at = now()
         WHERE organization_id = $1 AND project_id = $2
           AND id = $3 AND revision = $4 AND state IN ('draft', 'paused')
         RETURNING {BINDING_VIEW_COLS}"
    );
    let updated = sqlx::query(&sql)
        .bind(org_id)
        .bind(project_id)
        .bind(binding_id)
        .bind(expected_revision)
        .fetch_optional(&mut **scope.tx_mut())
        .await
        .map_err(edit_db_err)?;
    let Some(row) = updated else {
        return Err(EditError::detail(
            409,
            "Binding unavailable, already active/revoked, or revision changed.",
        ));
    };
    let rev: i32 = row.try_get("revision").map_err(edit_db_err)?;
    append_binding_audit(
        scope.tx_mut(),
        org_id,
        "human",
        actor_id,
        &actor_display,
        "integration.binding.activated",
        "connector_binding",
        binding_id,
        json!({ "state": "active", "revision": rev }),
    )
    .await?;
    serialize_binding(&row, 0)
}

async fn revoke_service_credential_tx(
    tx: &mut Transaction<'_, Postgres>,
    org_id: Uuid,
    actor_id: Uuid,
    actor_display: &str,
    sp_id: Uuid,
    cred_id: Uuid,
) -> Result<(), EditError> {
    let res = sqlx::query(
        "UPDATE org.service_credentials SET revoked_at = now()
         WHERE organization_id = $1 AND service_principal_id = $2
           AND id = $3 AND revoked_at IS NULL",
    )
    .bind(org_id)
    .bind(sp_id)
    .bind(cred_id)
    .execute(&mut **tx)
    .await
    .map_err(edit_db_err)?;
    if res.rows_affected() > 0 {
        append_binding_audit(
            tx,
            org_id,
            "human",
            actor_id,
            actor_display,
            "service_credential.revoke",
            "service_credential",
            cred_id,
            json!({ "service_principal_id": sp_id.to_string() }),
        )
        .await?;
    }
    Ok(())
}

pub async fn change_binding_lifecycle(
    scope: &mut EditScope<'_>,
    binding_id: Uuid,
    action: &str,
    expected_revision: i32,
) -> Result<Value, EditError> {
    let org_id = scope.organization_id();
    let project_id = scope.project_id();
    let actor_id = scope.actor_id();
    let actor_display = scope.display_name().to_owned();
    let new_state = if action == "pause" {
        "paused"
    } else {
        "revoked"
    };
    let sql = format!(
        "UPDATE integration_hub.connector_bindings
         SET state = $5, revision = revision + 1, updated_at = now()
         WHERE organization_id = $1 AND project_id = $2
           AND id = $3 AND revision = $4 AND state <> 'revoked'
         RETURNING {BINDING_VIEW_COLS}"
    );
    let updated = sqlx::query(&sql)
        .bind(org_id)
        .bind(project_id)
        .bind(binding_id)
        .bind(expected_revision)
        .bind(new_state)
        .fetch_optional(&mut **scope.tx_mut())
        .await
        .map_err(edit_db_err)?;
    let Some(row) = updated else {
        return Err(EditError::detail(
            409,
            "Binding unavailable or revision changed; reload binding state.",
        ));
    };

    if action == "revoke" {
        let active_tokens = sqlx::query(
            "SELECT id, service_principal_id, service_credential_id
             FROM integration_hub.binding_pairing_tokens
             WHERE organization_id = $1 AND project_id = $2
               AND binding_id = $3 AND revoked_at IS NULL",
        )
        .bind(org_id)
        .bind(project_id)
        .bind(binding_id)
        .fetch_all(&mut **scope.tx_mut())
        .await
        .map_err(edit_db_err)?;
        for tok in active_tokens {
            let sp_id: Uuid = tok.try_get("service_principal_id").map_err(edit_db_err)?;
            let cred_id: Uuid = tok.try_get("service_credential_id").map_err(edit_db_err)?;
            revoke_service_credential_tx(
                scope.tx_mut(),
                org_id,
                actor_id,
                &actor_display,
                sp_id,
                cred_id,
            )
            .await?;
        }
        sqlx::query(
            "UPDATE integration_hub.binding_pairing_tokens
             SET revoked_at = now()
             WHERE organization_id = $1 AND project_id = $2
               AND binding_id = $3 AND revoked_at IS NULL",
        )
        .bind(org_id)
        .bind(project_id)
        .bind(binding_id)
        .execute(&mut **scope.tx_mut())
        .await
        .map_err(edit_db_err)?;
    }

    let state_str: String = row.try_get("state").map_err(edit_db_err)?;
    let rev: i32 = row.try_get("revision").map_err(edit_db_err)?;
    append_binding_audit(
        scope.tx_mut(),
        org_id,
        "human",
        actor_id,
        &actor_display,
        &format!("integration.binding.{action}"),
        "connector_binding",
        binding_id,
        json!({ "state": state_str, "revision": rev }),
    )
    .await?;

    serialize_binding(&row, 0)
}

pub async fn list_pairing_tokens(
    scope: &mut EditScope<'_>,
    binding_id: Uuid,
) -> Result<Value, EditError> {
    let rows = sqlx::query(
        "SELECT id, binding_id, label, token_prefix, expires_at,
                last_used_at, revoked_at, created_at
         FROM integration_hub.binding_pairing_tokens
         WHERE organization_id = $1 AND project_id = $2 AND binding_id = $3
         ORDER BY created_at DESC LIMIT 50",
    )
    .bind(scope.organization_id())
    .bind(scope.project_id())
    .bind(binding_id)
    .fetch_all(&mut **scope.tx_mut())
    .await
    .map_err(edit_db_err)?;
    let items: Vec<Value> = rows
        .iter()
        .map(serialize_token)
        .collect::<Result<_, EditError>>()?;
    Ok(json!({ "items": items }))
}

pub async fn issue_pairing_token(
    scope: &mut EditScope<'_>,
    binding_id: Uuid,
    label: &str,
    expires_in_days: i64,
    server_url: &str,
) -> Result<Value, EditError> {
    let org_id = scope.organization_id();
    let project_id = scope.project_id();
    let actor_id = scope.actor_id();
    let actor_display = scope.display_name().to_owned();

    let select_sql = format!(
        "SELECT {BINDING_VIEW_COLS} FROM integration_hub.connector_bindings
         WHERE organization_id = $1 AND project_id = $2 AND id = $3"
    );
    let binding = sqlx::query(&select_sql)
        .bind(org_id)
        .bind(project_id)
        .bind(binding_id)
        .fetch_optional(&mut **scope.tx_mut())
        .await
        .map_err(edit_db_err)?;
    let Some(binding) = binding else {
        return Err(EditError::detail(404, "Binding not found."));
    };
    let state: String = binding.try_get("state").map_err(edit_db_err)?;
    if state == "revoked" {
        return Err(EditError::detail(409, "Cannot pair a revoked binding."));
    }
    let layer_id: Uuid = binding.try_get("layer_id").map_err(edit_db_err)?;
    let external_dataset_ref: String = binding
        .try_get("external_dataset_ref")
        .map_err(edit_db_err)?;
    let existing_sp_id: Option<Uuid> = binding
        .try_get("service_principal_id")
        .map_err(edit_db_err)?;

    let sp_id = if let Some(id) = existing_sp_id {
        id
    } else {
        let new_sp_id = Uuid::new_v4();
        let b_str = binding_id.to_string();
        let suffix = &Uuid::new_v4().simple().to_string()[..4];
        let sp_name = format!("qgis-binding-{}-{suffix}", &b_str[..8]);
        let sp_desc = format!("R027 QGIS connector binding for {external_dataset_ref}");
        sqlx::query(
            "INSERT INTO org.service_principals (
                id, organization_id, name, description, status,
                created_at, created_by_actor_type, created_by_actor_id, revoked_at
             ) VALUES ($1, $2, $3, $4, 'active', now(), 'human', $5, NULL)",
        )
        .bind(new_sp_id)
        .bind(org_id)
        .bind(&sp_name)
        .bind(&sp_desc)
        .bind(actor_id)
        .execute(&mut **scope.tx_mut())
        .await
        .map_err(edit_db_err)?;

        let grant_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO org.resource_grants (
                id, organization_id, subject_kind, membership_id, service_principal_id,
                scope_kind, workspace_id, project_id, role, created_at,
                created_by_actor_type, created_by_actor_id, revoked_at
             ) VALUES (
                $1, $2, 'service_principal', NULL, $3,
                'project', NULL, $4, 'editor', now(),
                'human', $5, NULL
             )",
        )
        .bind(grant_id)
        .bind(org_id)
        .bind(new_sp_id)
        .bind(project_id)
        .bind(actor_id)
        .execute(&mut **scope.tx_mut())
        .await
        .map_err(edit_db_err)?;

        new_sp_id
    };

    let cred_id = Uuid::new_v4();
    let mut entropy = [0u8; 32];
    getrandom::getrandom(&mut entropy)
        .map_err(|_| EditError::Internal("OS credential randomness unavailable".to_owned()))?;
    let secret = format!(
        "ovgsvc{}_{}",
        cred_id.simple(),
        URL_SAFE_NO_PAD.encode(entropy)
    );
    let prefix = secret[..20].to_owned();
    let secret_hash = Sha256::digest(secret.as_bytes()).to_vec();
    let cred_name: String = format!("QGIS · {label}").chars().take(200).collect();
    let ceiling = vec!["project.read".to_owned(), "project.edit".to_owned()];
    let now = Utc::now();
    let expires_at = now + Duration::days(expires_in_days);

    sqlx::query(
        "INSERT INTO org.service_credentials (
            id, organization_id, service_principal_id, name, prefix, secret_hash,
            permission_ceiling, created_at, expires_at, last_used_at, revoked_at
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, NULL, NULL)",
    )
    .bind(cred_id)
    .bind(org_id)
    .bind(sp_id)
    .bind(&cred_name)
    .bind(&prefix)
    .bind(&secret_hash)
    .bind(&ceiling)
    .bind(now)
    .bind(expires_at)
    .execute(&mut **scope.tx_mut())
    .await
    .map_err(edit_db_err)?;

    let token_id = Uuid::new_v4();
    let token_hash_hex: String = secret_hash.iter().map(|b| format!("{b:02x}")).collect();
    let token_row = sqlx::query(
        "INSERT INTO integration_hub.binding_pairing_tokens
           (id, organization_id, project_id, binding_id, label,
            token_prefix, token_hash, service_principal_id, service_credential_id,
            expires_at, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
         RETURNING id, binding_id, label, token_prefix, expires_at,
                   last_used_at, revoked_at, created_at",
    )
    .bind(token_id)
    .bind(org_id)
    .bind(project_id)
    .bind(binding_id)
    .bind(label)
    .bind(&prefix)
    .bind(&token_hash_hex)
    .bind(sp_id)
    .bind(cred_id)
    .bind(expires_at)
    .bind(actor_id)
    .fetch_one(&mut **scope.tx_mut())
    .await
    .map_err(edit_db_err)?;

    let mark_paired_sql = format!(
        "UPDATE integration_hub.connector_bindings
         SET state = 'active',
             revision = CASE WHEN state <> 'active' THEN revision + 1 ELSE revision END,
             activated_at = COALESCE(activated_at, now()),
             last_paired_at = now(),
             service_principal_id = COALESCE(service_principal_id, $4),
             updated_at = now()
         WHERE organization_id = $1 AND project_id = $2
           AND id = $3 AND state <> 'revoked'
         RETURNING {BINDING_VIEW_COLS}"
    );
    let updated_binding = sqlx::query(&mark_paired_sql)
        .bind(org_id)
        .bind(project_id)
        .bind(binding_id)
        .bind(sp_id)
        .fetch_one(&mut **scope.tx_mut())
        .await
        .map_err(edit_db_err)?;

    let pairing_payload = serde_json::to_vec(&json!({
        "v": 1,
        "server_url": server_url.trim_end_matches('/'),
        "organization_id": org_id.to_string(),
        "project_id": project_id.to_string(),
        "binding_id": binding_id.to_string(),
        "layer_id": layer_id.to_string(),
        "external_dataset_ref": external_dataset_ref,
        "bearer_token": secret,
    }))
    .map_err(|e| EditError::Internal(e.to_string()))?;
    let pairing_code = format!("ovg-pair-v1:{}", URL_SAFE_NO_PAD.encode(&pairing_payload));

    append_binding_audit(
        scope.tx_mut(),
        org_id,
        "human",
        actor_id,
        &actor_display,
        "integration.binding.pairing_token_issued",
        "connector_binding",
        binding_id,
        json!({
            "token_id": token_id.to_string(),
            "label": label,
            "expires_in_days": expires_in_days,
        }),
    )
    .await?;

    Ok(json!({
        "token": serialize_token(&token_row)?,
        "binding": serialize_binding(&updated_binding, 0)?,
        "pairing_code": pairing_code,
        "bearer_token": secret,
    }))
}

pub async fn revoke_pairing_token(
    scope: &mut EditScope<'_>,
    binding_id: Uuid,
    token_id: Uuid,
) -> Result<Value, EditError> {
    let org_id = scope.organization_id();
    let project_id = scope.project_id();
    let actor_id = scope.actor_id();
    let actor_display = scope.display_name().to_owned();

    let tok = sqlx::query(
        "SELECT id, service_principal_id, service_credential_id, revoked_at
         FROM integration_hub.binding_pairing_tokens
         WHERE organization_id = $1 AND project_id = $2
           AND binding_id = $3 AND id = $4",
    )
    .bind(org_id)
    .bind(project_id)
    .bind(binding_id)
    .bind(token_id)
    .fetch_optional(&mut **scope.tx_mut())
    .await
    .map_err(edit_db_err)?;
    let Some(tok) = tok else {
        return Err(EditError::detail(404, "Pairing token not found."));
    };
    let revoked_at: Option<DateTime<Utc>> = tok.try_get("revoked_at").map_err(edit_db_err)?;
    if revoked_at.is_none() {
        let sp_id: Uuid = tok.try_get("service_principal_id").map_err(edit_db_err)?;
        let cred_id: Uuid = tok.try_get("service_credential_id").map_err(edit_db_err)?;
        revoke_service_credential_tx(
            scope.tx_mut(),
            org_id,
            actor_id,
            &actor_display,
            sp_id,
            cred_id,
        )
        .await?;
    }

    let revoked = sqlx::query(
        "UPDATE integration_hub.binding_pairing_tokens
         SET revoked_at = now()
         WHERE organization_id = $1 AND project_id = $2
           AND binding_id = $3 AND id = $4 AND revoked_at IS NULL
         RETURNING id, binding_id, label, token_prefix, expires_at,
                   last_used_at, revoked_at, created_at",
    )
    .bind(org_id)
    .bind(project_id)
    .bind(binding_id)
    .bind(token_id)
    .fetch_optional(&mut **scope.tx_mut())
    .await
    .map_err(edit_db_err)?;
    let Some(row) = revoked else {
        return Err(EditError::detail(409, "Pairing token already revoked."));
    };

    append_binding_audit(
        scope.tx_mut(),
        org_id,
        "human",
        actor_id,
        &actor_display,
        "integration.binding.pairing_token_revoked",
        "connector_binding",
        binding_id,
        json!({ "token_id": token_id.to_string() }),
    )
    .await?;

    serialize_token(&row)
}

pub async fn list_binding_conflicts(
    scope: &mut EditScope<'_>,
    binding_id: Uuid,
) -> Result<Value, EditError> {
    let rows = sqlx::query(
        "SELECT id, binding_id, layer_id, feature_id, conflict_kind, field_name,
                baseline_value, web_value, qgis_value, status,
                resolved_value, resolution_note, created_at, resolved_at
         FROM integration_hub.sync_conflicts
         WHERE organization_id = $1 AND project_id = $2 AND binding_id = $3
         ORDER BY CASE WHEN status = 'open' THEN 0 ELSE 1 END, created_at DESC
         LIMIT 200",
    )
    .bind(scope.organization_id())
    .bind(scope.project_id())
    .bind(binding_id)
    .fetch_all(&mut **scope.tx_mut())
    .await
    .map_err(edit_db_err)?;
    let items: Vec<Value> = rows
        .iter()
        .map(serialize_conflict)
        .collect::<Result<_, EditError>>()?;
    Ok(json!({ "items": items }))
}

pub struct ConflictItemInput {
    pub feature_id: String,
    pub conflict_kind: String,
    pub field_name: Option<String>,
    pub baseline_value: Value,
    pub web_value: Value,
    pub qgis_value: Value,
}

pub async fn record_binding_conflicts(
    scope: &mut EditScope<'_>,
    binding_id: Uuid,
    items: Vec<ConflictItemInput>,
) -> Result<Value, EditError> {
    let org_id = scope.organization_id();
    let project_id = scope.project_id();
    let actor_kind = scope.actor_kind();
    let actor_id = scope.actor_id();
    let actor_display = scope.display_name().to_owned();
    let created_by = if actor_kind == "human" {
        Some(actor_id)
    } else {
        None
    };

    let layer_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT layer_id FROM integration_hub.connector_bindings
         WHERE organization_id = $1 AND project_id = $2 AND id = $3",
    )
    .bind(org_id)
    .bind(project_id)
    .bind(binding_id)
    .fetch_optional(&mut **scope.tx_mut())
    .await
    .map_err(edit_db_err)?;
    let Some(layer_id) = layer_id else {
        return Err(EditError::detail(404, "Binding not found."));
    };

    let mut created = Vec::with_capacity(items.len());
    for item in items {
        let row = sqlx::query(
            "INSERT INTO integration_hub.sync_conflicts
               (id, organization_id, project_id, binding_id, layer_id, feature_id,
                conflict_kind, field_name, baseline_value, web_value, qgis_value, created_by)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
             RETURNING id, binding_id, layer_id, feature_id, conflict_kind, field_name,
                       baseline_value, web_value, qgis_value, status,
                       resolved_value, resolution_note, created_at, resolved_at",
        )
        .bind(Uuid::new_v4())
        .bind(org_id)
        .bind(project_id)
        .bind(binding_id)
        .bind(layer_id)
        .bind(&item.feature_id)
        .bind(&item.conflict_kind)
        .bind(&item.field_name)
        .bind(&item.baseline_value)
        .bind(&item.web_value)
        .bind(&item.qgis_value)
        .bind(created_by)
        .fetch_one(&mut **scope.tx_mut())
        .await
        .map_err(edit_db_err)?;
        created.push(serialize_conflict(&row)?);
    }

    append_binding_audit(
        scope.tx_mut(),
        org_id,
        actor_kind,
        actor_id,
        &actor_display,
        "integration.binding.conflicts_recorded",
        "connector_binding",
        binding_id,
        json!({ "count": created.len() }),
    )
    .await?;

    Ok(json!({ "items": created }))
}

pub async fn resolve_binding_conflict(
    scope: &mut EditScope<'_>,
    binding_id: Uuid,
    conflict_id: Uuid,
    decision: &str,
    merged_value: Option<Value>,
    note: Option<&str>,
) -> Result<Value, EditError> {
    let org_id = scope.organization_id();
    let project_id = scope.project_id();
    let actor_id = scope.actor_id();
    let actor_display = scope.display_name().to_owned();

    let existing = sqlx::query(
        "SELECT id, feature_id, web_value, qgis_value, status
         FROM integration_hub.sync_conflicts
         WHERE organization_id = $1 AND project_id = $2
           AND binding_id = $3 AND id = $4",
    )
    .bind(org_id)
    .bind(project_id)
    .bind(binding_id)
    .bind(conflict_id)
    .fetch_optional(&mut **scope.tx_mut())
    .await
    .map_err(edit_db_err)?;
    let Some(existing) = existing else {
        return Err(EditError::detail(404, "Conflict not found."));
    };
    let status: String = existing.try_get("status").map_err(edit_db_err)?;
    if status != "open" {
        return Err(EditError::detail(409, "Conflict is already resolved."));
    }
    let feature_id: String = existing.try_get("feature_id").map_err(edit_db_err)?;
    let web_value: Option<Value> = existing.try_get("web_value").map_err(edit_db_err)?;
    let qgis_value: Option<Value> = existing.try_get("qgis_value").map_err(edit_db_err)?;

    let (new_status, chosen_value) = match decision {
        "keep_web" => ("resolved_web", web_value.unwrap_or(Value::Null)),
        "keep_qgis" => ("resolved_qgis", qgis_value.unwrap_or(Value::Null)),
        "merge" => match merged_value {
            Some(v) if !v.is_null() => ("resolved_merged", v),
            _ => {
                return Err(EditError::detail(
                    422,
                    "merged_value is required when decision is 'merge'.",
                ));
            }
        },
        _ => ("dismissed", Value::Null),
    };

    let resolved = sqlx::query(
        "UPDATE integration_hub.sync_conflicts
         SET status = $5, resolved_value = $6, resolution_note = $7,
             resolved_by = $8, resolved_at = now()
         WHERE organization_id = $1 AND project_id = $2
           AND binding_id = $3 AND id = $4 AND status = 'open'
         RETURNING id, binding_id, layer_id, feature_id, conflict_kind, field_name,
                   baseline_value, web_value, qgis_value, status,
                   resolved_value, resolution_note, created_at, resolved_at",
    )
    .bind(org_id)
    .bind(project_id)
    .bind(binding_id)
    .bind(conflict_id)
    .bind(new_status)
    .bind(&chosen_value)
    .bind(note)
    .bind(actor_id)
    .fetch_optional(&mut **scope.tx_mut())
    .await
    .map_err(edit_db_err)?;
    let Some(row) = resolved else {
        return Err(EditError::detail(409, "Conflict could not be updated."));
    };

    append_binding_audit(
        scope.tx_mut(),
        org_id,
        "human",
        actor_id,
        &actor_display,
        "integration.binding.conflict_resolved",
        "connector_binding",
        binding_id,
        json!({
            "conflict_id": conflict_id.to_string(),
            "feature_id": feature_id,
            "decision": decision,
        }),
    )
    .await?;

    serialize_conflict(&row)
}
