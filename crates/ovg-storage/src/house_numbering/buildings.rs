//! Building registry writes, plaque lifecycle and split/merge lineage.

use std::collections::BTreeSet;

use ovg_core::AppError;
use serde_json::{json, Value};
use uuid::Uuid;

use super::hierarchy::{get_area, get_route};
use super::{append_audit, db_error, spatial_feature, wire, AuditActor};
use crate::catalog::ProjectRead;

const BUILDING_GEOMETRY: &[&str] = &["ST_Point", "ST_Polygon"];

#[derive(Debug, Clone, Copy)]
pub struct BuildingCreateInput<'a> {
    pub route_id: Option<Uuid>,
    pub area_id: Option<Uuid>,
    pub parcel_ref: Option<&'a str>,
    pub source_id: Uuid,
    pub feature_id: &'a str,
    pub eligibility: &'a str,
    pub ineligibility_reason: Option<&'a str>,
}

#[derive(Debug, Clone, Copy)]
pub struct BuildingUpdateInput<'a> {
    pub expected_revision: i64,
    pub route_id: Option<Uuid>,
    pub area_id: Option<Uuid>,
    pub parcel_ref: Option<&'a str>,
    pub source_id: Uuid,
    pub feature_id: &'a str,
}

#[derive(Debug, Clone)]
pub struct LineageInput<'a> {
    pub relation_kind: &'a str,
    pub operation_id: Uuid,
    pub predecessor_ids: Vec<Uuid>,
    pub successor_ids: Vec<Uuid>,
    /// Request order is preserved: the first stale building is reported.
    pub expected_revisions: Vec<(Uuid, i64)>,
}

fn revision(row: &Value) -> i64 {
    row.get("revision").and_then(Value::as_i64).unwrap_or(0)
}

pub(crate) async fn get_building(
    edit: &mut ProjectRead<'_>,
    building_id: Uuid,
    for_update: bool,
) -> Result<Value, AppError> {
    let sql = if for_update {
        "SELECT to_jsonb(b) AS wire FROM house_numbering.buildings b
         WHERE b.organization_id=$1 AND b.project_id=$2 AND b.id=$3 FOR UPDATE"
    } else {
        "SELECT to_jsonb(b) AS wire FROM house_numbering.buildings b
         WHERE b.organization_id=$1 AND b.project_id=$2 AND b.id=$3"
    };
    let row = sqlx::query(sql)
        .bind(edit.organization_id())
        .bind(edit.project_id())
        .bind(building_id)
        .fetch_optional(&mut **edit.transaction())
        .await
        .map_err(db_error)?
        .ok_or_else(|| AppError::not_found("building not found"))?;
    wire(&row)
}

pub(crate) async fn official_number(
    edit: &mut ProjectRead<'_>,
    building_id: Uuid,
    kinds: &[&str],
) -> Result<bool, AppError> {
    let kinds: Vec<String> = kinds.iter().map(|kind| (*kind).to_owned()).collect();
    let row = sqlx::query(
        "SELECT 1 AS hit FROM house_numbering.address_numbers
         WHERE organization_id=$1 AND project_id=$2
           AND building_id=$3 AND retired_at IS NULL
           AND kind = ANY($4)
         LIMIT 1",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(building_id)
    .bind(kinds)
    .fetch_optional(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    Ok(row.is_some())
}

fn building_snapshot(current: &Value) -> Value {
    json!({
        "route_id": current["route_id"].clone(),
        "area_id": current["area_id"].clone(),
        "parcel_ref": current["parcel_ref"].clone(),
        "linked_source_id": current["linked_source_id"].clone(),
        "linked_feature_id": current["linked_feature_id"].clone(),
    })
}

pub async fn create_building(
    edit: &mut ProjectRead<'_>,
    actor: AuditActor<'_>,
    input: BuildingCreateInput<'_>,
) -> Result<Value, AppError> {
    spatial_feature(edit, input.source_id, input.feature_id, BUILDING_GEOMETRY).await?;
    let id = Uuid::new_v4();
    let row = sqlx::query(
        "WITH inserted AS (
            INSERT INTO house_numbering.buildings (
                id,organization_id,project_id,route_id,area_id,parcel_ref,
                linked_source_id,linked_feature_id,eligibility,ineligibility_reason,
                state,revision,created_by_actor_id,updated_by_actor_id,created_at,updated_at
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,'active',1,$11,$11,$12,$12)
            RETURNING *
         )
         SELECT to_jsonb(inserted) AS wire FROM inserted",
    )
    .bind(id)
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(input.route_id)
    .bind(input.area_id)
    .bind(input.parcel_ref)
    .bind(input.source_id)
    .bind(input.feature_id)
    .bind(input.eligibility)
    .bind(input.ineligibility_reason)
    .bind(actor.id)
    .bind(chrono::Utc::now())
    .fetch_one(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    let result = wire(&row)?;
    append_audit(
        edit,
        actor,
        "house_numbering.building.create",
        "house_numbering_building",
        id,
        json!({}),
    )
    .await?;
    Ok(result)
}

pub async fn update_building(
    edit: &mut ProjectRead<'_>,
    actor: AuditActor<'_>,
    building_id: Uuid,
    input: BuildingUpdateInput<'_>,
) -> Result<Value, AppError> {
    let current = get_building(edit, building_id, true).await?;
    if current["state"].as_str() != Some("active") {
        return Err(AppError::conflict(
            "BUILDING_NOT_ACTIVE",
            "building is not active",
        ));
    }
    if revision(&current) != input.expected_revision {
        return Err(AppError::conflict(
            "STALE_BUILDING",
            "building revision is stale",
        ));
    }
    spatial_feature(edit, input.source_id, input.feature_id, BUILDING_GEOMETRY).await?;
    if let Some(route_id) = input.route_id {
        get_route(edit, route_id, false).await?;
    }
    if let Some(area_id) = input.area_id {
        get_area(edit, area_id, false).await?;
    }
    let current_route = current["route_id"]
        .as_str()
        .and_then(|value| Uuid::parse_str(value).ok());
    if input.route_id != current_route
        && official_number(edit, building_id, &["submitted", "approved", "reserved"]).await?
    {
        return Err(AppError::conflict(
            "BUILDING_ROUTE_LOCKED",
            "building route is locked by an official number",
        ));
    }
    // Python strips a non-empty parcel_ref and stores None for "".
    let parcel = input
        .parcel_ref
        .filter(|value| !value.is_empty())
        .map(str::trim);
    let row = sqlx::query(
        "WITH updated AS (
            UPDATE house_numbering.buildings
            SET route_id=$4,area_id=$5,parcel_ref=$6,
                linked_source_id=$7,linked_feature_id=$8,
                revision=revision+1,updated_by_actor_id=$9,updated_at=$10
            WHERE organization_id=$1 AND project_id=$2 AND id=$3
            RETURNING *
         )
         SELECT to_jsonb(updated) AS wire FROM updated",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(building_id)
    .bind(input.route_id)
    .bind(input.area_id)
    .bind(parcel)
    .bind(input.source_id)
    .bind(input.feature_id)
    .bind(actor.id)
    .bind(chrono::Utc::now())
    .fetch_one(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    let result = wire(&row)?;
    append_audit(
        edit,
        actor,
        "house_numbering.building.update",
        "house_numbering_building",
        building_id,
        json!({"before": building_snapshot(&current), "after_revision": revision(&result)}),
    )
    .await?;
    Ok(result)
}

async fn mark_removed(
    edit: &mut ProjectRead<'_>,
    actor: AuditActor<'_>,
    building_id: Uuid,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE house_numbering.buildings
         SET state='removed',
             plaque_status=CASE WHEN plaque_status='not_planned' THEN plaque_status ELSE 'removed' END,
             plaque_updated_at=CASE WHEN plaque_status='not_planned' THEN plaque_updated_at ELSE $4 END,
             revision=revision+1,updated_by_actor_id=$5,updated_at=$4
         WHERE organization_id=$1 AND project_id=$2 AND id=$3",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(building_id)
    .bind(now)
    .bind(actor.id)
    .execute(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    Ok(())
}

pub async fn retire_building(
    edit: &mut ProjectRead<'_>,
    actor: AuditActor<'_>,
    building_id: Uuid,
    expected_revision: i64,
) -> Result<(), AppError> {
    let current = get_building(edit, building_id, true).await?;
    if current["state"].as_str() != Some("active") {
        return Err(AppError::conflict(
            "BUILDING_NOT_ACTIVE",
            "building is not active",
        ));
    }
    if revision(&current) != expected_revision {
        return Err(AppError::conflict(
            "STALE_BUILDING",
            "building revision is stale",
        ));
    }
    if official_number(edit, building_id, &["submitted", "approved", "reserved"]).await? {
        return Err(AppError::conflict(
            "BUILDING_OFFICIAL_NUMBER_LOCKED",
            "building with an official number must use the split/merge or approval workflow",
        ));
    }
    mark_removed(edit, actor, building_id, chrono::Utc::now()).await?;
    let mut snapshot = building_snapshot(&current);
    snapshot["revision"] = json!(revision(&current));
    append_audit(
        edit,
        actor,
        "house_numbering.building.retire",
        "house_numbering_building",
        building_id,
        json!({ "snapshot": snapshot }),
    )
    .await
}

pub async fn set_building_eligibility(
    edit: &mut ProjectRead<'_>,
    actor: AuditActor<'_>,
    building_id: Uuid,
    expected_revision: i64,
    eligibility: &str,
    reason: Option<&str>,
) -> Result<Value, AppError> {
    let current = get_building(edit, building_id, true).await?;
    if revision(&current) != expected_revision {
        return Err(AppError::conflict(
            "STALE_BUILDING",
            "building revision is stale",
        ));
    }
    let row = sqlx::query(
        "WITH updated AS (
            UPDATE house_numbering.buildings
            SET eligibility=$4,ineligibility_reason=$5,
                revision=revision+1,updated_by_actor_id=$6,updated_at=$7
            WHERE organization_id=$1 AND project_id=$2 AND id=$3
            RETURNING *
         )
         SELECT to_jsonb(updated) AS wire FROM updated",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(building_id)
    .bind(eligibility)
    .bind(reason)
    .bind(actor.id)
    .bind(chrono::Utc::now())
    .fetch_one(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    let result = wire(&row)?;
    append_audit(
        edit,
        actor,
        "house_numbering.building.eligibility",
        "house_numbering_building",
        building_id,
        json!({ "eligibility": eligibility }),
    )
    .await?;
    Ok(result)
}

pub async fn set_plaque_status(
    edit: &mut ProjectRead<'_>,
    actor: AuditActor<'_>,
    building_id: Uuid,
    expected_revision: i64,
    status: &str,
) -> Result<Value, AppError> {
    let current = get_building(edit, building_id, true).await?;
    if revision(&current) != expected_revision {
        return Err(AppError::conflict(
            "STALE_BUILDING",
            "building revision is stale",
        ));
    }
    if matches!(status, "installed" | "replaced")
        && !official_number(edit, building_id, &["approved"]).await?
    {
        return Err(AppError::conflict(
            "PLAQUE_REQUIRES_APPROVED_NUMBER",
            "plaque installation requires an approved house number",
        ));
    }
    let row = sqlx::query(
        "WITH updated AS (
            UPDATE house_numbering.buildings
            SET plaque_status=$4,plaque_updated_at=$6,
                revision=revision+1,updated_by_actor_id=$5,updated_at=$6
            WHERE organization_id=$1 AND project_id=$2 AND id=$3
            RETURNING *
         )
         SELECT to_jsonb(updated) AS wire FROM updated",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(building_id)
    .bind(status)
    .bind(actor.id)
    .bind(chrono::Utc::now())
    .fetch_one(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    let result = wire(&row)?;
    append_audit(
        edit,
        actor,
        "house_numbering.building.plaque_status",
        "house_numbering_building",
        building_id,
        json!({"old_status": current["plaque_status"].clone(), "new_status": status}),
    )
    .await?;
    Ok(result)
}

fn lineage_rows(rows: &[sqlx::postgres::PgRow]) -> Result<Vec<Value>, AppError> {
    rows.iter().map(wire).collect()
}

pub async fn record_building_lineage(
    edit: &mut ProjectRead<'_>,
    actor: AuditActor<'_>,
    input: LineageInput<'_>,
) -> Result<Value, AppError> {
    let predecessors: BTreeSet<Uuid> = input.predecessor_ids.iter().copied().collect();
    let successors: BTreeSet<Uuid> = input.successor_ids.iter().copied().collect();
    if input.relation_kind == "split" && (predecessors.len() != 1 || successors.len() < 2) {
        return Err(AppError::validation(
            "VALIDATION_ERROR",
            "split requires one predecessor and at least two successors",
        ));
    }
    if input.relation_kind == "merge" && (predecessors.len() < 2 || successors.len() != 1) {
        return Err(AppError::validation(
            "VALIDATION_ERROR",
            "merge requires at least two predecessors and one successor",
        ));
    }
    if !predecessors.is_disjoint(&successors) {
        return Err(AppError::validation(
            "VALIDATION_ERROR",
            "lineage predecessor and successor sets must be disjoint",
        ));
    }
    let all_ids: BTreeSet<Uuid> = predecessors.union(&successors).copied().collect();
    let revision_ids: BTreeSet<Uuid> = input.expected_revisions.iter().map(|(id, _)| *id).collect();
    if revision_ids != all_ids {
        return Err(AppError::validation(
            "LINEAGE_REVISION_SET",
            "expected_revisions must cover every lineage building",
        ));
    }

    let existing = sqlx::query(
        "SELECT to_jsonb(l) AS wire, l.relation_kind,
                l.predecessor_building_id, l.successor_building_id
         FROM house_numbering.building_lineage l
         WHERE l.organization_id=$1 AND l.project_id=$2 AND l.operation_id=$3
         ORDER BY l.predecessor_building_id,l.successor_building_id",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(input.operation_id)
    .fetch_all(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    let wrap = |items: Vec<Value>| {
        json!({
            "operation_id": input.operation_id.to_string(),
            "relation_kind": input.relation_kind,
            "items": items,
        })
    };
    if !existing.is_empty() {
        use sqlx::Row;
        let mut kinds = BTreeSet::new();
        let mut pre = BTreeSet::new();
        let mut succ = BTreeSet::new();
        for row in &existing {
            kinds.insert(
                row.try_get::<String, _>("relation_kind")
                    .map_err(db_error)?,
            );
            pre.insert(
                row.try_get::<Uuid, _>("predecessor_building_id")
                    .map_err(db_error)?,
            );
            succ.insert(
                row.try_get::<Uuid, _>("successor_building_id")
                    .map_err(db_error)?,
            );
        }
        if kinds.len() == 1
            && kinds.contains(input.relation_kind)
            && pre == predecessors
            && succ == successors
        {
            return Ok(wrap(lineage_rows(&existing)?));
        }
        return Err(AppError::conflict(
            "LINEAGE_OPERATION_CONFLICT",
            "lineage operation_id was already used with different participants",
        ));
    }

    let ids: Vec<Uuid> = all_ids.iter().copied().collect();
    let rows = sqlx::query(
        "SELECT to_jsonb(b) AS wire FROM house_numbering.buildings b
         WHERE b.organization_id=$1 AND b.project_id=$2 AND b.id = ANY($3)
         ORDER BY b.id
         FOR UPDATE",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(&ids)
    .fetch_all(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    let mut by_id = std::collections::HashMap::new();
    for row in &rows {
        let value = wire(row)?;
        if let Some(id) = value["id"].as_str().and_then(|id| Uuid::parse_str(id).ok()) {
            by_id.insert(id, value);
        }
    }
    if by_id.len() != all_ids.len() {
        return Err(AppError::not_found(
            "one or more lineage buildings were not found",
        ));
    }
    for (building_id, expected) in &input.expected_revisions {
        let row = &by_id[building_id];
        if revision(row) != *expected {
            return Err(AppError::conflict(
                "STALE_BUILDING",
                format!("building {building_id} revision is stale"),
            ));
        }
        if row["state"].as_str() != Some("active") {
            return Err(AppError::conflict(
                "BUILDING_NOT_ACTIVE",
                format!("building {building_id} is not active"),
            ));
        }
    }

    let now = chrono::Utc::now();
    for predecessor in &predecessors {
        mark_removed(edit, actor, *predecessor, now).await?;
    }
    let mut inserted = Vec::new();
    for predecessor in &predecessors {
        for successor in &successors {
            let row = sqlx::query(
                "WITH inserted AS (
                    INSERT INTO house_numbering.building_lineage (
                        id,organization_id,project_id,operation_id,relation_kind,
                        predecessor_building_id,successor_building_id,
                        created_by_actor_id,created_at
                    ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
                    RETURNING *
                 )
                 SELECT to_jsonb(inserted) AS wire FROM inserted",
            )
            .bind(Uuid::new_v4())
            .bind(edit.organization_id())
            .bind(edit.project_id())
            .bind(input.operation_id)
            .bind(input.relation_kind)
            .bind(predecessor)
            .bind(successor)
            .bind(actor.id)
            .bind(now)
            .fetch_one(&mut **edit.transaction())
            .await
            .map_err(db_error)?;
            inserted.push(wire(&row)?);
        }
    }
    let as_strings = |set: &BTreeSet<Uuid>| -> Vec<String> {
        let mut values: Vec<String> = set.iter().map(Uuid::to_string).collect();
        values.sort();
        values
    };
    append_audit(
        edit,
        actor,
        "house_numbering.building.lineage",
        "house_numbering_building_lineage",
        input.operation_id,
        json!({
            "relation_kind": input.relation_kind,
            "predecessor_ids": as_strings(&predecessors),
            "successor_ids": as_strings(&successors),
        }),
    )
    .await?;
    Ok(wrap(inserted))
}

pub async fn list_building_lineage(
    read: &mut ProjectRead<'_>,
    building_id: Uuid,
) -> Result<Value, AppError> {
    get_building(read, building_id, false).await?;
    let rows = sqlx::query(
        "SELECT to_jsonb(l) AS wire FROM house_numbering.building_lineage l
         WHERE l.organization_id=$1 AND l.project_id=$2
           AND (l.predecessor_building_id=$3 OR l.successor_building_id=$3)
         ORDER BY l.created_at,l.id",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(building_id)
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(db_error)?;
    Ok(json!({ "items": lineage_rows(&rows)? }))
}
