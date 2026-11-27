//! E01 GIS Access scope, groups, rules, export, and changeset reviews storage.

use chrono::{DateTime, SecondsFormat, Utc};
use ovg_core::AppError;
use serde_json::{json, Value};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::editing::{self, EditError, EditScope};
use crate::tenancy::TenantTx;

fn db_err(e: sqlx::Error) -> EditError {
    tracing::error!(%e, "gis_access database error");
    EditError::Internal("gis_access database error".to_owned())
}

fn app_db(e: sqlx::Error) -> AppError {
    tracing::error!(%e, "gis_access database error");
    AppError::internal("gis_access database error")
}

fn iso(v: DateTime<Utc>) -> String {
    v.to_rfc3339_opts(SecondsFormat::AutoSi, false)
}

pub fn bump_permission_epoch_sql() -> &'static str {
    "INSERT INTO integration_hub.project_event_counters
        (organization_id, project_id, last_cursor, permission_epoch, retention_floor, updated_at)
     VALUES ($1, $2, 0, 2, 0, now())
     ON CONFLICT (organization_id, project_id) DO UPDATE
     SET permission_epoch = integration_hub.project_event_counters.permission_epoch + 1,
         updated_at = now()"
}

pub async fn bump_project_permission_epoch(
    tx: &mut Transaction<'_, Postgres>,
    org_id: Uuid,
    project_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(bump_permission_epoch_sql())
        .bind(org_id)
        .bind(project_id)
        .execute(&mut **tx)
        .await?;
    sqlx::query("SELECT pg_notify('ovg_team_collaboration', $1)")
        .bind(json!({ "o": org_id.to_string(), "p": project_id.to_string(), "kind": "session_revoked" }).to_string())
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn append_gis_audit(
    tx: &mut Transaction<'_, Postgres>,
    org_id: Uuid,
    actor_id: Uuid,
    actor_display: &str,
    project_id: Uuid,
    action: &str,
    resource_id: Uuid,
    mut details: Value,
) -> Result<(), sqlx::Error> {
    if let Some(map) = details.as_object_mut() {
        map.insert("project_id".to_owned(), json!(project_id.to_string()));
    }
    sqlx::query(
        "INSERT INTO audit.audit_events (
            id, organization_id, actor_type, actor_id, actor_display_snapshot,
            action, resource_type, resource_id, outcome, metadata, created_at
         ) VALUES ($1, $2, 'human', $3, $4, $5, 'gis_access', $6, 'success', $7, $8)",
    )
    .bind(Uuid::new_v4())
    .bind(org_id)
    .bind(actor_id)
    .bind(actor_display)
    .bind(format!("gis_access.{action}"))
    .bind(resource_id)
    .bind(details)
    .bind(Utc::now())
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn restrict_member(
    tx: &mut Transaction<'_, Postgres>,
    org_id: Uuid,
    project_id: Uuid,
    uid: Uuid,
) -> Result<(), EditError> {
    let row = sqlx::query(
        "SELECT role, status FROM org.organization_memberships
         WHERE organization_id = $1 AND user_id = $2",
    )
    .bind(org_id)
    .bind(uid)
    .fetch_optional(&mut **tx)
    .await
    .map_err(db_err)?;

    let ok = row.as_ref().is_some_and(|r| {
        let role: Result<String, _> = r.try_get("role");
        let status: Result<String, _> = r.try_get("status");
        matches!(
            (status.as_deref(), role.as_deref()),
            (Ok("active"), Ok(rl)) if !matches!(rl, "owner" | "admin")
        )
    });
    if !ok {
        return Err(EditError::detail(
            422,
            "Only active non-admin organization members may be scoped",
        ));
    }
    sqlx::query(
        "INSERT INTO gis_access.scoped_members(organization_id, project_id, user_id)
         VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
    )
    .bind(org_id)
    .bind(project_id)
    .bind(uid)
    .execute(&mut **tx)
    .await
    .map_err(db_err)?;
    Ok(())
}

pub async fn get_access_admin(
    t: &mut TenantTx<'_>,
    org_id: Uuid,
    project_id: Uuid,
) -> Result<Value, AppError> {
    let members = sqlx::query(
        "SELECT m.id, m.user_id, m.role, m.status, u.display_name, u.email_snapshot,
                EXISTS(
                    SELECT 1 FROM gis_access.scoped_members sm
                    WHERE sm.organization_id = $1 AND sm.project_id = $2 AND sm.user_id = m.user_id
                ) AS restricted
         FROM org.organization_memberships m
         JOIN identity.users u ON u.id = m.user_id
         WHERE m.organization_id = $1
         ORDER BY u.display_name",
    )
    .bind(org_id)
    .bind(project_id)
    .fetch_all(&mut **t.tx_mut())
    .await
    .map_err(app_db)?;

    let members_json: Vec<Value> = members
        .into_iter()
        .map(|r| {
            Ok(json!({
                "id": r.try_get::<Uuid, _>("id").map_err(app_db)?.to_string(),
                "user_id": r.try_get::<Uuid, _>("user_id").map_err(app_db)?.to_string(),
                "role": r.try_get::<String, _>("role").map_err(app_db)?,
                "status": r.try_get::<String, _>("status").map_err(app_db)?,
                "display_name": r.try_get::<String, _>("display_name").map_err(app_db)?,
                "email_snapshot": r.try_get::<Option<String>, _>("email_snapshot").map_err(app_db)?,
                "restricted": r.try_get::<bool, _>("restricted").map_err(app_db)?,
            }))
        })
        .collect::<Result<_, AppError>>()?;

    let groups = sqlx::query(
        "SELECT g.id, g.name,
                ARRAY(
                    SELECT gm.user_id FROM gis_access.group_members gm
                    WHERE gm.organization_id = $1 AND gm.project_id = $2 AND gm.group_id = g.id
                ) AS user_ids
         FROM gis_access.groups g
         WHERE organization_id = $1 AND project_id = $2
         ORDER BY name",
    )
    .bind(org_id)
    .bind(project_id)
    .fetch_all(&mut **t.tx_mut())
    .await
    .map_err(app_db)?;

    let groups_json: Vec<Value> = groups
        .into_iter()
        .map(|r| {
            let uids: Vec<Uuid> = r.try_get("user_ids").map_err(app_db)?;
            Ok(json!({
                "id": r.try_get::<Uuid, _>("id").map_err(app_db)?.to_string(),
                "name": r.try_get::<String, _>("name").map_err(app_db)?,
                "user_ids": uids.into_iter().map(|u| u.to_string()).collect::<Vec<_>>(),
            }))
        })
        .collect::<Result<_, AppError>>()?;

    let layers = sqlx::query(
        "SELECT l.id, l.name, l.source_id,
                COALESCE(
                    (SELECT jsonb_agg(jsonb_build_object('id', f.field_id, 'name', f.name, 'type', f.type) ORDER BY f.ordinal)
                     FROM spatial.source_fields f
                     WHERE f.organization_id = $1 AND f.project_id = $2 AND f.source_id = l.source_id),
                    '[]'::jsonb
                ) AS fields
         FROM spatial.layers l
         WHERE l.organization_id = $1 AND l.project_id = $2 AND l.lifecycle = 'active'
         ORDER BY l.name",
    )
    .bind(org_id)
    .bind(project_id)
    .fetch_all(&mut **t.tx_mut())
    .await
    .map_err(app_db)?;

    let layers_json: Vec<Value> = layers
        .into_iter()
        .map(|r| {
            Ok(json!({
                "id": r.try_get::<Uuid, _>("id").map_err(app_db)?.to_string(),
                "name": r.try_get::<String, _>("name").map_err(app_db)?,
                "source_id": r.try_get::<Uuid, _>("source_id").map_err(app_db)?.to_string(),
                "fields": r.try_get::<Value, _>("fields").map_err(app_db)?,
            }))
        })
        .collect::<Result<_, AppError>>()?;

    let rules = sqlx::query(
        "SELECT id, user_id, group_id, layer_id, actions, feature_ids, attribute_filter,
                CAST(ST_AsGeoJSON(region) AS jsonb) AS region, created_at
         FROM gis_access.rules
         WHERE organization_id = $1 AND project_id = $2 AND revoked_at IS NULL
         ORDER BY created_at DESC",
    )
    .bind(org_id)
    .bind(project_id)
    .fetch_all(&mut **t.tx_mut())
    .await
    .map_err(app_db)?;

    let rules_json: Vec<Value> = rules
        .into_iter()
        .map(|r| {
            Ok(json!({
                "id": r.try_get::<Uuid, _>("id").map_err(app_db)?.to_string(),
                "user_id": r.try_get::<Option<Uuid>, _>("user_id").map_err(app_db)?.map(|u| u.to_string()),
                "group_id": r.try_get::<Option<Uuid>, _>("group_id").map_err(app_db)?.map(|u| u.to_string()),
                "layer_id": r.try_get::<Uuid, _>("layer_id").map_err(app_db)?.to_string(),
                "actions": r.try_get::<Vec<String>, _>("actions").map_err(app_db)?,
                "feature_ids": r.try_get::<Option<Vec<String>>, _>("feature_ids").map_err(app_db)?,
                "attribute_filter": r.try_get::<Option<Value>, _>("attribute_filter").map_err(app_db)?,
                "region": r.try_get::<Option<Value>, _>("region").map_err(app_db)?,
                "created_at": iso(r.try_get::<DateTime<Utc>, _>("created_at").map_err(app_db)?),
            }))
        })
        .collect::<Result<_, AppError>>()?;

    let audit = sqlx::query(
        "SELECT id, action, actor_display_snapshot, metadata, created_at
         FROM audit.audit_events
         WHERE organization_id = $1 AND action LIKE 'gis_access.%'
           AND metadata->>'project_id' = $2
         ORDER BY created_at DESC LIMIT 100",
    )
    .bind(org_id)
    .bind(project_id.to_string())
    .fetch_all(&mut **t.tx_mut())
    .await
    .map_err(app_db)?;

    let audit_json: Vec<Value> = audit
        .into_iter()
        .map(|r| {
            Ok(json!({
                "id": r.try_get::<Uuid, _>("id").map_err(app_db)?.to_string(),
                "action": r.try_get::<String, _>("action").map_err(app_db)?,
                "actor_display_snapshot": r.try_get::<String, _>("actor_display_snapshot").map_err(app_db)?,
                "metadata": r.try_get::<Value, _>("metadata").map_err(app_db)?,
                "created_at": iso(r.try_get::<DateTime<Utc>, _>("created_at").map_err(app_db)?),
            }))
        })
        .collect::<Result<_, AppError>>()?;

    Ok(json!({
        "members": members_json,
        "groups": groups_json,
        "layers": layers_json,
        "rules": rules_json,
        "audit": audit_json,
    }))
}

pub async fn my_access(scope: &mut EditScope<'_>) -> Result<Value, EditError> {
    let org_id = scope.organization_id();
    let project_id = scope.project_id();
    let limited = scope.is_scoped();
    if !limited {
        return Ok(json!({ "restricted": false, "rules": [] }));
    }
    let rows = sqlx::query(
        "SELECT l.id AS layer_id, l.name, r.actions, r.feature_ids, r.attribute_filter,
                CAST(ST_AsGeoJSON(r.region) AS jsonb) AS region
         FROM gis_access.subject_rules($1, $2) r
         JOIN spatial.layers l
           ON l.organization_id = $1 AND l.project_id = $2 AND l.id = r.layer_id",
    )
    .bind(org_id)
    .bind(project_id)
    .fetch_all(&mut **scope.tx_mut())
    .await
    .map_err(db_err)?;

    let rules: Vec<Value> = rows
        .into_iter()
        .map(|r| {
            Ok(json!({
                "layer_id": r.try_get::<Uuid, _>("layer_id").map_err(db_err)?.to_string(),
                "name": r.try_get::<String, _>("name").map_err(db_err)?,
                "actions": r.try_get::<Vec<String>, _>("actions").map_err(db_err)?,
                "feature_ids": r.try_get::<Option<Vec<String>>, _>("feature_ids").map_err(db_err)?,
                "attribute_filter": r.try_get::<Option<Value>, _>("attribute_filter").map_err(db_err)?,
                "region": r.try_get::<Option<Value>, _>("region").map_err(db_err)?,
            }))
        })
        .collect::<Result<_, EditError>>()?;
    Ok(json!({ "restricted": true, "rules": rules }))
}

pub async fn create_group(
    t: &mut TenantTx<'_>,
    org_id: Uuid,
    project_id: Uuid,
    actor_id: Uuid,
    actor_display: &str,
    raw_name: &str,
    user_ids: &[Uuid],
) -> Result<Uuid, EditError> {
    let trimmed = raw_name.trim();
    let exists: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM gis_access.groups
         WHERE organization_id = $1 AND project_id = $2 AND name = $3",
    )
    .bind(org_id)
    .bind(project_id)
    .bind(trimmed)
    .fetch_optional(&mut **t.tx_mut())
    .await
    .map_err(db_err)?;
    if exists.is_some() {
        return Err(EditError::detail(409, "group already exists"));
    }

    let gid = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO gis_access.groups(organization_id, project_id, id, name)
         VALUES ($1, $2, $3, $4)",
    )
    .bind(org_id)
    .bind(project_id)
    .bind(gid)
    .bind(trimmed)
    .execute(&mut **t.tx_mut())
    .await
    .map_err(db_err)?;

    let mut seen = std::collections::HashSet::with_capacity(user_ids.len());
    for &uid in user_ids {
        if !seen.insert(uid) {
            continue;
        }
        restrict_member(t.tx_mut(), org_id, project_id, uid).await?;
        sqlx::query(
            "INSERT INTO gis_access.group_members(organization_id, project_id, group_id, user_id)
             VALUES ($1, $2, $3, $4)",
        )
        .bind(org_id)
        .bind(project_id)
        .bind(gid)
        .bind(uid)
        .execute(&mut **t.tx_mut())
        .await
        .map_err(db_err)?;
    }

    append_gis_audit(
        t.tx_mut(),
        org_id,
        actor_id,
        actor_display,
        project_id,
        "group_created",
        gid,
        json!({ "name": raw_name, "members": user_ids.len() }),
    )
    .await
    .map_err(db_err)?;
    bump_project_permission_epoch(t.tx_mut(), org_id, project_id)
        .await
        .map_err(db_err)?;
    Ok(gid)
}

pub async fn replace_group(
    t: &mut TenantTx<'_>,
    org_id: Uuid,
    project_id: Uuid,
    actor_id: Uuid,
    actor_display: &str,
    group_id: Uuid,
    raw_name: &str,
    user_ids: &[Uuid],
) -> Result<Uuid, EditError> {
    let trimmed = raw_name.trim();
    let found: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM gis_access.groups
         WHERE organization_id = $1 AND project_id = $2 AND id = $3",
    )
    .bind(org_id)
    .bind(project_id)
    .bind(group_id)
    .fetch_optional(&mut **t.tx_mut())
    .await
    .map_err(db_err)?;
    if found.is_none() {
        return Err(EditError::detail(404, "group not found"));
    }

    let dup: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM gis_access.groups
         WHERE organization_id = $1 AND project_id = $2 AND name = $3 AND id <> $4",
    )
    .bind(org_id)
    .bind(project_id)
    .bind(trimmed)
    .bind(group_id)
    .fetch_optional(&mut **t.tx_mut())
    .await
    .map_err(db_err)?;
    if dup.is_some() {
        return Err(EditError::detail(409, "group name already exists"));
    }

    sqlx::query(
        "UPDATE gis_access.groups SET name = $4
         WHERE organization_id = $1 AND project_id = $2 AND id = $3",
    )
    .bind(org_id)
    .bind(project_id)
    .bind(group_id)
    .bind(trimmed)
    .execute(&mut **t.tx_mut())
    .await
    .map_err(db_err)?;

    sqlx::query(
        "DELETE FROM gis_access.group_members
         WHERE organization_id = $1 AND project_id = $2 AND group_id = $3",
    )
    .bind(org_id)
    .bind(project_id)
    .bind(group_id)
    .execute(&mut **t.tx_mut())
    .await
    .map_err(db_err)?;

    let mut seen = std::collections::HashSet::with_capacity(user_ids.len());
    for &uid in user_ids {
        if !seen.insert(uid) {
            continue;
        }
        restrict_member(t.tx_mut(), org_id, project_id, uid).await?;
        sqlx::query(
            "INSERT INTO gis_access.group_members(organization_id, project_id, group_id, user_id)
             VALUES ($1, $2, $3, $4)",
        )
        .bind(org_id)
        .bind(project_id)
        .bind(group_id)
        .bind(uid)
        .execute(&mut **t.tx_mut())
        .await
        .map_err(db_err)?;
    }

    append_gis_audit(
        t.tx_mut(),
        org_id,
        actor_id,
        actor_display,
        project_id,
        "group_updated",
        group_id,
        json!({ "name": raw_name, "members": user_ids.len() }),
    )
    .await
    .map_err(db_err)?;
    bump_project_permission_epoch(t.tx_mut(), org_id, project_id)
        .await
        .map_err(db_err)?;
    Ok(group_id)
}

pub struct RuleInput {
    pub user_id: Option<Uuid>,
    pub group_id: Option<Uuid>,
    pub layer_id: Uuid,
    pub actions: Vec<String>,
    pub feature_ids: Option<Vec<String>>,
    pub attribute_filter: Option<Value>,
    pub region: Option<Value>,
}

async fn insert_rule_inner(
    tx: &mut Transaction<'_, Postgres>,
    org_id: Uuid,
    project_id: Uuid,
    actor_id: Uuid,
    actor_display: &str,
    rule: &RuleInput,
    skip_identical: bool,
) -> Result<(Uuid, bool), EditError> {
    let sid: Option<Uuid> = sqlx::query_scalar(
        "SELECT source_id FROM spatial.layers
         WHERE organization_id = $1 AND project_id = $2 AND id = $3 AND lifecycle = 'active'",
    )
    .bind(org_id)
    .bind(project_id)
    .bind(rule.layer_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(db_err)?;
    let Some(sid) = sid else {
        return Err(EditError::detail(404, "layer not found"));
    };

    if let Some(uid) = rule.user_id {
        restrict_member(tx, org_id, project_id, uid).await?;
    }
    if let Some(gid) = rule.group_id {
        let g: Option<i32> = sqlx::query_scalar(
            "SELECT 1 FROM gis_access.groups
             WHERE organization_id = $1 AND project_id = $2 AND id = $3",
        )
        .bind(org_id)
        .bind(project_id)
        .bind(gid)
        .fetch_optional(&mut **tx)
        .await
        .map_err(db_err)?;
        if g.is_none() {
            return Err(EditError::detail(404, "group not found"));
        }
    }

    if let Some(ref fids) = rule.feature_ids {
        let unique: std::collections::HashSet<&str> = fids.iter().map(String::as_str).collect();
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*)::bigint FROM spatial.reference_features
             WHERE organization_id = $1 AND project_id = $2 AND source_id = $3
               AND feature_id = ANY($4)",
        )
        .bind(org_id)
        .bind(project_id)
        .bind(sid)
        .bind(fids)
        .fetch_one(&mut **tx)
        .await
        .map_err(db_err)?;
        if count as usize != unique.len() {
            return Err(EditError::detail(422, "feature IDs missing from layer"));
        }
    }

    if let Some(Value::Object(ref map)) = rule.attribute_filter {
        let avail: Vec<String> = sqlx::query_scalar(
            "SELECT field_id FROM spatial.source_fields
             WHERE organization_id = $1 AND project_id = $2 AND source_id = $3",
        )
        .bind(org_id)
        .bind(project_id)
        .bind(sid)
        .fetch_all(&mut **tx)
        .await
        .map_err(db_err)?;
        let avail_set: std::collections::HashSet<&str> = avail.iter().map(String::as_str).collect();
        if map.keys().any(|k| !avail_set.contains(k.as_str())) {
            return Err(EditError::detail(422, "attribute filter field missing"));
        }
    }

    let unique_actions: Vec<String> = rule
        .actions
        .iter()
        .cloned()
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    let region_str = rule
        .region
        .as_ref()
        .map(|r| serde_json::to_string(r).map_err(|_| EditError::detail(422, "invalid region")))
        .transpose()?;

    if let Some(ref reg_json) = region_str {
        let valid: Option<bool> = sqlx::query_scalar(
            "SELECT ST_IsValid(g) AND NOT ST_IsEmpty(g)
                    AND ST_CoveredBy(g, ST_MakeEnvelope(-180, -90, 180, 90, 4326))
             FROM (SELECT ST_SetSRID(ST_GeomFromGeoJSON($1), 4326) AS g) v",
        )
        .bind(reg_json)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|_| EditError::detail(422, "invalid region"))?;
        if valid != Some(true) {
            return Err(EditError::detail(422, "invalid region"));
        }
    }

    if skip_identical {
        let existing: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM gis_access.rules
             WHERE organization_id = $1 AND project_id = $2 AND layer_id = $3
               AND revoked_at IS NULL
               AND user_id IS NOT DISTINCT FROM $4
               AND group_id IS NOT DISTINCT FROM $5
               AND actions @> $6 AND actions <@ $6
               AND ((feature_ids IS NULL AND $7::text[] IS NULL)
                    OR (feature_ids @> $7 AND feature_ids <@ $7))
               AND attribute_filter IS NOT DISTINCT FROM $8
               AND ((region IS NULL AND $9::text IS NULL)
                    OR (region IS NOT NULL AND $9::text IS NOT NULL
                        AND ST_Equals(region, ST_SetSRID(ST_GeomFromGeoJSON($9), 4326))))
             LIMIT 1",
        )
        .bind(org_id)
        .bind(project_id)
        .bind(rule.layer_id)
        .bind(rule.user_id)
        .bind(rule.group_id)
        .bind(&unique_actions)
        .bind(&rule.feature_ids)
        .bind(&rule.attribute_filter)
        .bind(&region_str)
        .fetch_optional(&mut **tx)
        .await
        .map_err(db_err)?;
        if let Some(id) = existing {
            return Ok((id, false));
        }
    }

    let rid = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO gis_access.rules(
            organization_id, project_id, id, user_id, group_id, layer_id, source_id,
            actions, feature_ids, attribute_filter, region
         ) VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
            CASE WHEN $11::text IS NULL THEN NULL
                 ELSE ST_Multi(ST_SetSRID(ST_GeomFromGeoJSON($11), 4326)) END
         )",
    )
    .bind(org_id)
    .bind(project_id)
    .bind(rid)
    .bind(rule.user_id)
    .bind(rule.group_id)
    .bind(rule.layer_id)
    .bind(sid)
    .bind(&unique_actions)
    .bind(&rule.feature_ids)
    .bind(&rule.attribute_filter)
    .bind(&region_str)
    .execute(&mut **tx)
    .await
    .map_err(db_err)?;

    append_gis_audit(
        tx,
        org_id,
        actor_id,
        actor_display,
        project_id,
        "rule_created",
        rid,
        json!({
            "layer_id": rule.layer_id.to_string(),
            "actions": rule.actions,
            "user_id": rule.user_id.map(|u| u.to_string()),
            "group_id": rule.group_id.map(|u| u.to_string()),
        }),
    )
    .await
    .map_err(db_err)?;
    Ok((rid, true))
}

pub async fn create_rule(
    t: &mut TenantTx<'_>,
    org_id: Uuid,
    project_id: Uuid,
    actor_id: Uuid,
    actor_display: &str,
    rule: &RuleInput,
) -> Result<Uuid, EditError> {
    let (rid, _) = insert_rule_inner(
        t.tx_mut(),
        org_id,
        project_id,
        actor_id,
        actor_display,
        rule,
        false,
    )
    .await?;
    bump_project_permission_epoch(t.tx_mut(), org_id, project_id)
        .await
        .map_err(db_err)?;
    Ok(rid)
}

async fn revoke_rule_inner(
    tx: &mut Transaction<'_, Postgres>,
    org_id: Uuid,
    project_id: Uuid,
    actor_id: Uuid,
    actor_display: &str,
    rule_id: Uuid,
) -> Result<(), EditError> {
    let res = sqlx::query(
        "UPDATE gis_access.rules SET revoked_at = now()
         WHERE organization_id = $1 AND project_id = $2 AND id = $3 AND revoked_at IS NULL",
    )
    .bind(org_id)
    .bind(project_id)
    .bind(rule_id)
    .execute(&mut **tx)
    .await
    .map_err(db_err)?;
    if res.rows_affected() != 1 {
        return Err(EditError::detail(404, "rule not found"));
    }
    append_gis_audit(
        tx,
        org_id,
        actor_id,
        actor_display,
        project_id,
        "rule_revoked",
        rule_id,
        json!({}),
    )
    .await
    .map_err(db_err)?;
    Ok(())
}

pub async fn revoke_rule(
    t: &mut TenantTx<'_>,
    org_id: Uuid,
    project_id: Uuid,
    actor_id: Uuid,
    actor_display: &str,
    rule_id: Uuid,
) -> Result<(), EditError> {
    revoke_rule_inner(
        t.tx_mut(),
        org_id,
        project_id,
        actor_id,
        actor_display,
        rule_id,
    )
    .await?;
    bump_project_permission_epoch(t.tx_mut(), org_id, project_id)
        .await
        .map_err(db_err)?;
    Ok(())
}

pub async fn batch_rules(
    t: &mut TenantTx<'_>,
    org_id: Uuid,
    project_id: Uuid,
    actor_id: Uuid,
    actor_display: &str,
    rules: &[RuleInput],
    revoke_ids: &[Uuid],
) -> Result<Value, EditError> {
    let mut created = 0usize;
    let mut unchanged = 0usize;
    for rule in rules {
        let (_, added) = insert_rule_inner(
            t.tx_mut(),
            org_id,
            project_id,
            actor_id,
            actor_display,
            rule,
            true,
        )
        .await?;
        if added {
            created += 1;
        } else {
            unchanged += 1;
        }
    }
    let mut seen_revoke = std::collections::HashSet::with_capacity(revoke_ids.len());
    for &rid in revoke_ids {
        if !seen_revoke.insert(rid) {
            continue;
        }
        revoke_rule_inner(
            t.tx_mut(),
            org_id,
            project_id,
            actor_id,
            actor_display,
            rid,
        )
        .await?;
    }
    if created > 0 || !seen_revoke.is_empty() {
        bump_project_permission_epoch(t.tx_mut(), org_id, project_id)
            .await
            .map_err(db_err)?;
    }
    Ok(json!({
        "created": created,
        "unchanged": unchanged,
        "revoked": seen_revoke.len(),
    }))
}

pub struct ExportRow {
    pub feature_id: String,
    pub attributes: Value,
    pub geometry: Option<Value>,
}

pub async fn export_layer_rows(
    scope: &mut EditScope<'_>,
    layer_id: Uuid,
) -> Result<Vec<ExportRow>, EditError> {
    let org_id = scope.organization_id();
    let project_id = scope.project_id();
    let sid: Option<Uuid> = sqlx::query_scalar(
        "SELECT source_id FROM spatial.layers
         WHERE organization_id = $1 AND project_id = $2 AND id = $3",
    )
    .bind(org_id)
    .bind(project_id)
    .bind(layer_id)
    .fetch_optional(&mut **scope.tx_mut())
    .await
    .map_err(db_err)?;
    let Some(sid) = sid else {
        return Err(EditError::detail(404, "layer not found"));
    };

    let allowed: bool = sqlx::query_scalar::<_, Option<bool>>(
        "SELECT gis_access.source_allowed($1, $2, $3, 'export')",
    )
    .bind(org_id)
    .bind(project_id)
    .bind(sid)
    .fetch_one(&mut **scope.tx_mut())
    .await
    .map_err(db_err)?
    .unwrap_or(false);
    if !allowed {
        return Err(EditError::detail(403, "export not granted"));
    }

    let rows = sqlx::query(
        "SELECT feature_id, attributes,
                CAST(ST_AsGeoJSON(ST_Transform(geometry, 4326)) AS jsonb) AS geometry
         FROM spatial.reference_features
         WHERE organization_id = $1 AND project_id = $2 AND source_id = $3
           AND gis_access.feature_allowed($1, $2, $3, feature_id, attributes, geometry, 'export')
         LIMIT 50001",
    )
    .bind(org_id)
    .bind(project_id)
    .bind(sid)
    .fetch_all(&mut **scope.tx_mut())
    .await
    .map_err(db_err)?;

    if rows.len() > 50_000 {
        return Err(EditError::detail(422, "download limit 50000 features"));
    }

    rows.into_iter()
        .map(|r| {
            Ok(ExportRow {
                feature_id: r.try_get("feature_id").map_err(db_err)?,
                attributes: r.try_get("attributes").map_err(db_err)?,
                geometry: r.try_get("geometry").map_err(db_err)?,
            })
        })
        .collect()
}

const REVIEW_CHECK_SQL: &str = "NOT EXISTS(
    SELECT 1 FROM editing.changeset_operations o
    WHERE o.organization_id = $1 AND o.project_id = $2 AND o.changeset_id = c.id AND (
        (o.op_type <> 'create' AND NOT (
            gis_access.feature_allowed($1, $2, c.source_id, o.feature_id, o.before_properties, ST_GeomFromEWKB(o.before_geometry_wkb), 'approve')
            AND gis_access.feature_allowed($1, $2, c.source_id, o.feature_id, o.before_properties, ST_GeomFromEWKB(o.before_geometry_wkb), 'read')
        ))
        OR (o.op_type <> 'delete' AND NOT (
            gis_access.feature_allowed($1, $2, c.source_id, o.feature_id, o.after_properties, ST_GeomFromEWKB(o.after_geometry_wkb), 'approve')
            AND gis_access.feature_allowed($1, $2, c.source_id, o.feature_id, o.after_properties, ST_GeomFromEWKB(o.after_geometry_wkb), 'read')
        ))
    )
)";

pub async fn list_reviews(scope: &mut EditScope<'_>) -> Result<Value, EditError> {
    let org_id = scope.organization_id();
    let project_id = scope.project_id();
    let sql = format!(
        "SELECT c.id, c.source_id, c.summary, c.actor_display_snapshot,
                c.operation_count::bigint AS operation_count, c.created_at,
                ds.name AS source_name,
                (SELECT jsonb_agg(
                    jsonb_build_object(
                        'decision', r.decision,
                        'note', r.note,
                        'created_at', r.created_at,
                        'reviewer_id', r.reviewer_id
                    ) ORDER BY r.created_at DESC
                 )
                 FROM gis_access.reviews r
                 WHERE r.organization_id = $1 AND r.project_id = $2 AND r.changeset_id = c.id) AS decisions
         FROM editing.changesets c
         JOIN spatial.data_sources ds
           ON ds.organization_id = $1 AND ds.project_id = $2 AND ds.id = c.source_id
         WHERE c.organization_id = $1 AND c.project_id = $2 AND {REVIEW_CHECK_SQL}
         ORDER BY c.created_at DESC LIMIT 100"
    );
    let rows = sqlx::query(&sql)
        .bind(org_id)
        .bind(project_id)
        .fetch_all(&mut **scope.tx_mut())
        .await
        .map_err(db_err)?;

    let items: Vec<Value> = rows
        .into_iter()
        .map(|r| {
            Ok(json!({
                "id": r.try_get::<Uuid, _>("id").map_err(db_err)?.to_string(),
                "source_id": r.try_get::<Uuid, _>("source_id").map_err(db_err)?.to_string(),
                "summary": r.try_get::<Option<String>, _>("summary").map_err(db_err)?,
                "actor_display_snapshot": r.try_get::<String, _>("actor_display_snapshot").map_err(db_err)?,
                "operation_count": r.try_get::<i64, _>("operation_count").map_err(db_err)?,
                "created_at": iso(r.try_get::<DateTime<Utc>, _>("created_at").map_err(db_err)?),
                "source_name": r.try_get::<String, _>("source_name").map_err(db_err)?,
                "decisions": r.try_get::<Option<Value>, _>("decisions").map_err(db_err)?,
            }))
        })
        .collect::<Result<_, EditError>>()?;
    Ok(json!({ "items": items }))
}

pub async fn decide_review(
    scope: &mut EditScope<'_>,
    changeset_id: Uuid,
    decision: &str,
    note: &str,
) -> Result<Uuid, EditError> {
    let org_id = scope.organization_id();
    let project_id = scope.project_id();
    let actor_id = scope.actor_id();
    let actor_display = scope.display_name().to_owned();

    let sql = format!(
        "SELECT c.id, c.actor_id FROM editing.changesets c
         WHERE c.organization_id = $1 AND c.project_id = $2 AND c.id = $3
           AND {REVIEW_CHECK_SQL}"
    );
    let rec = sqlx::query(&sql)
        .bind(org_id)
        .bind(project_id)
        .bind(changeset_id)
        .fetch_optional(&mut **scope.tx_mut())
        .await
        .map_err(db_err)?;
    let Some(rec) = rec else {
        return Err(EditError::detail(403, "changeset outside review scope"));
    };
    let cs_actor: Uuid = rec.try_get("actor_id").map_err(db_err)?;
    if cs_actor == actor_id {
        return Err(EditError::detail(422, "cannot review own changeset"));
    }

    let rid = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO gis_access.reviews(organization_id, project_id, id, changeset_id, reviewer_id, decision, note)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(org_id)
    .bind(project_id)
    .bind(rid)
    .bind(changeset_id)
    .bind(actor_id)
    .bind(decision)
    .bind(note.trim())
    .execute(&mut **scope.tx_mut())
    .await
    .map_err(db_err)?;

    append_gis_audit(
        scope.tx_mut(),
        org_id,
        actor_id,
        &actor_display,
        project_id,
        "change_reviewed",
        rid,
        json!({ "changeset_id": changeset_id.to_string(), "decision": decision }),
    )
    .await
    .map_err(db_err)?;
    Ok(rid)
}
