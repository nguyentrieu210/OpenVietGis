//! Field work: assignments, survey review, offline survey sync, progress and sign-offs.

use std::path::Path;

use ovg_core::AppError;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use sqlx::Row;
use uuid::Uuid;

use super::buildings::get_building;
use super::{append_audit, db_error, wire, wire_py, AuditActor};
use crate::catalog::ProjectRead;

/// Raised where Python calls `prepare("manage")` mid-operation; HTTP renders the
/// Core `{"detail": "missing permission: project.manage"}` 403.
pub const MANAGE_REQUIRED: &str = "PROJECT_MANAGE_REQUIRED";

pub fn manage_required() -> AppError {
    AppError::Forbidden {
        code: MANAGE_REQUIRED,
        message: "missing permission: project.manage".to_owned(),
    }
}

/// `double precision` columns are Python floats (`20.0`), not JSON integers.
pub(crate) fn floats(value: &mut Value, keys: &[&str]) {
    for key in keys {
        if let Some(number) = value.get(*key).and_then(Value::as_f64) {
            value[*key] = json!(number);
        }
    }
}

const SURVEY_FLOATS: &[&str] = &["point_x", "point_y", "gps_accuracy_m", "gps_deviation_m"];

fn survey_row(row: &sqlx::postgres::PgRow) -> Result<Value, AppError> {
    let mut value = wire_py(row)?;
    floats(&mut value, SURVEY_FLOATS);
    Ok(value)
}

fn validation(message: &str) -> AppError {
    AppError::validation("VALIDATION_ERROR", message)
}

#[derive(Debug, Clone, Copy)]
pub struct AssignmentInput {
    pub scope_kind: &'static str,
    pub scope_id: Uuid,
    pub assignee_actor_id: Uuid,
    pub starts_on: chrono::NaiveDate,
    pub ends_on: Option<chrono::NaiveDate>,
    pub target_count: Option<i32>,
}

pub async fn create_assignment(
    edit: &mut ProjectRead<'_>,
    actor: AuditActor<'_>,
    input: AssignmentInput,
) -> Result<Value, AppError> {
    if input.ends_on.is_some_and(|ends| ends < input.starts_on) {
        return Err(validation("assignment end date precedes start date"));
    }
    let now = chrono::Utc::now();
    let id = Uuid::new_v4();
    let row = sqlx::query(
        "WITH inserted AS (
            INSERT INTO house_numbering.assignments (
                id,organization_id,project_id,scope_kind,scope_id,assignee_actor_id,
                target_count,starts_on,ends_on,revision,created_by_actor_id,created_at,updated_at
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,1,$10,$11,$11)
            RETURNING *
         )
         SELECT to_jsonb(inserted) AS wire FROM inserted",
    )
    .bind(id)
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(input.scope_kind)
    .bind(input.scope_id)
    .bind(input.assignee_actor_id)
    .bind(input.target_count)
    .bind(input.starts_on)
    .bind(input.ends_on)
    .bind(actor.id)
    .bind(now)
    .fetch_one(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    let result = wire_py(&row)?;
    append_audit(
        edit,
        actor,
        "house_numbering.assignment.create",
        "house_numbering_assignment",
        id,
        json!({
            "assignee_actor_id": input.assignee_actor_id.to_string(),
            "scope_kind": input.scope_kind,
        }),
    )
    .await?;
    Ok(result)
}

pub async fn reassign_assignment(
    edit: &mut ProjectRead<'_>,
    actor: AuditActor<'_>,
    assignment_id: Uuid,
    expected_revision: i64,
    assignee_actor_id: Uuid,
) -> Result<Value, AppError> {
    let row = sqlx::query(
        "WITH updated AS (
            UPDATE house_numbering.assignments
            SET assignee_actor_id=$4,revision=revision+1,updated_at=$5
            WHERE organization_id=$1 AND project_id=$2 AND id=$3 AND revision=$6
            RETURNING *
         )
         SELECT to_jsonb(updated) AS wire FROM updated",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(assignment_id)
    .bind(assignee_actor_id)
    .bind(chrono::Utc::now())
    .bind(expected_revision)
    .fetch_optional(&mut **edit.transaction())
    .await
    .map_err(db_error)?
    .ok_or_else(|| AppError::conflict("STALE_ASSIGNMENT", "assignment is stale or missing"))?;
    let result = wire_py(&row)?;
    append_audit(
        edit,
        actor,
        "house_numbering.assignment.reassign",
        "house_numbering_assignment",
        assignment_id,
        json!({ "assignee_actor_id": assignee_actor_id.to_string() }),
    )
    .await?;
    Ok(result)
}

pub async fn list_assignments(read: &mut ProjectRead<'_>) -> Result<Value, AppError> {
    let rows = sqlx::query(
        "SELECT to_jsonb(a) AS wire FROM house_numbering.assignments a
         WHERE a.organization_id=$1 AND a.project_id=$2
         ORDER BY a.starts_on DESC,a.created_at DESC,a.id DESC",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(db_error)?;
    let items = rows.iter().map(wire_py).collect::<Result<Vec<_>, _>>()?;
    Ok(json!({ "items": items }))
}

pub async fn list_surveys(
    read: &mut ProjectRead<'_>,
    state: Option<&str>,
) -> Result<Value, AppError> {
    let rows = sqlx::query(
        "SELECT to_jsonb(s) AS wire FROM house_numbering.surveys s
         WHERE s.organization_id=$1 AND s.project_id=$2 AND ($3::text IS NULL OR s.state=$3)
         ORDER BY s.created_at DESC,s.id DESC",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(state.filter(|value| !value.is_empty()))
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(db_error)?;
    let items = rows.iter().map(survey_row).collect::<Result<Vec<_>, _>>()?;
    Ok(json!({ "items": items }))
}

pub async fn review_survey(
    edit: &mut ProjectRead<'_>,
    actor: AuditActor<'_>,
    survey_id: Uuid,
    expected_revision: i64,
    decision: &str,
) -> Result<Value, AppError> {
    let current = sqlx::query(
        "SELECT revision, state, building_id FROM house_numbering.surveys
         WHERE organization_id=$1 AND project_id=$2 AND id=$3 FOR UPDATE",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(survey_id)
    .fetch_optional(&mut **edit.transaction())
    .await
    .map_err(db_error)?
    .ok_or_else(|| AppError::not_found("survey not found"))?;
    let revision: i64 = current.try_get("revision").map_err(db_error)?;
    let state: String = current.try_get("state").map_err(db_error)?;
    if revision != expected_revision || state != "submitted" {
        return Err(AppError::conflict(
            "STALE_SURVEY",
            "survey is stale or not reviewable",
        ));
    }
    let building: Option<Uuid> = current.try_get("building_id").map_err(db_error)?;
    if let (true, Some(building)) = (decision == "accepted", building) {
        let other = sqlx::query(
            "SELECT id FROM house_numbering.surveys
             WHERE organization_id=$1 AND project_id=$2 AND building_id=$3
               AND state='accepted' AND id<>$4 LIMIT 1",
        )
        .bind(edit.organization_id())
        .bind(edit.project_id())
        .bind(building)
        .bind(survey_id)
        .fetch_optional(&mut **edit.transaction())
        .await
        .map_err(db_error)?;
        if other.is_some() {
            return Err(AppError::conflict(
                "SURVEY_REVIEW_CONFLICT",
                "another accepted survey exists for building",
            ));
        }
    }
    let row = sqlx::query(
        "WITH updated AS (
            UPDATE house_numbering.surveys SET state=$4,revision=revision+1,updated_at=$5
            WHERE organization_id=$1 AND project_id=$2 AND id=$3
            RETURNING *
         )
         SELECT to_jsonb(updated) AS wire FROM updated",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(survey_id)
    .bind(decision)
    .bind(chrono::Utc::now())
    .fetch_one(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    let result = survey_row(&row)?;
    append_audit(
        edit,
        actor,
        "house_numbering.survey.review",
        "house_numbering_survey",
        survey_id,
        json!({ "decision": decision }),
    )
    .await?;
    Ok(result)
}

pub async fn progress(read: &mut ProjectRead<'_>) -> Result<Value, AppError> {
    let target: i64 = sqlx::query_scalar(
        "SELECT COALESCE(sum(target_count),0)::bigint
         FROM house_numbering.assignments
         WHERE organization_id=$1 AND project_id=$2
           AND starts_on<=CURRENT_DATE AND (ends_on IS NULL OR ends_on>=CURRENT_DATE)",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .fetch_one(&mut **read.transaction())
    .await
    .map_err(db_error)?;
    let rows = sqlx::query(
        "SELECT DISTINCT ON (building_id) condition,state
         FROM house_numbering.surveys
         WHERE organization_id=$1 AND project_id=$2 AND building_id IS NOT NULL
         ORDER BY building_id,updated_at DESC,id DESC",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(db_error)?;
    let (mut positions, mut houses, mut vacant, mut other, mut removed, mut pending) =
        (0_i64, 0_i64, 0_i64, 0_i64, 0_i64, 0_i64);
    for row in &rows {
        let condition: String = row.try_get("condition").map_err(db_error)?;
        let state: String = row.try_get("state").map_err(db_error)?;
        if state == "submitted" {
            pending += 1;
        }
        if state != "accepted" {
            continue;
        }
        positions += 1;
        match condition.as_str() {
            "house" => houses += 1,
            "vacant" => vacant += 1,
            "removed" => {
                other += 1;
                removed += 1;
            }
            _ => other += 1,
        }
    }
    #[allow(clippy::cast_precision_loss)]
    let completion = (target != 0).then(|| positions as f64 / target as f64 * 100.0);
    Ok(json!({
        "positions": positions,
        "houses": houses,
        "vacant": vacant,
        "other": other,
        "completion": completion,
        "removed": removed,
        "submitted_pending": pending,
    }))
}

/// Python `round(x, 3)`: exact decimal rounding of the binary value.
fn round3(value: f64) -> f64 {
    format!("{value:.3}").parse().unwrap_or(value)
}

/// Verify a module artifact like `read_module_artifact` (metadata + sha256 integrity).
fn artifact_available(root: &Path, scope: &ProjectRead<'_>, artifact_id: Uuid) -> bool {
    let parent = root
        .join("extensions")
        .join(scope.organization_id().to_string())
        .join(scope.project_id().to_string())
        .join("house-numbering-survey-photos")
        .join(artifact_id.to_string());
    let Ok(entries) = std::fs::read_dir(&parent) else {
        return false;
    };
    let metas: Vec<_> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    let [meta_path] = metas.as_slice() else {
        return false;
    };
    let Some(meta) = std::fs::read_to_string(meta_path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
    else {
        return false;
    };
    let Some(filename) = meta["filename"].as_str() else {
        return false;
    };
    let Ok(data) = std::fs::read(parent.join(filename)) else {
        return false;
    };
    let digest = format!("{:x}", Sha256::digest(&data));
    Some(digest.as_str()) == meta["sha256"].as_str()
        && meta["byte_size"].as_u64() == u64::try_from(data.len()).ok()
}

fn id_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

struct Gps {
    x: Option<f64>,
    y: Option<f64>,
    crs: Option<i32>,
    accuracy: Option<f64>,
    deviation: Option<f64>,
    warnings: Vec<Value>,
}

fn py_float(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}

async fn gps_observation(
    edit: &mut ProjectRead<'_>,
    building_id: Option<Uuid>,
    item: &Map<String, Value>,
    client_uuid: Uuid,
) -> Result<Gps, AppError> {
    let get = |key: &str| item.get(key).filter(|value| !value.is_null());
    let (x, y, crs, accuracy) = (get("x"), get("y"), get("crs_code"), get("gps_accuracy_m"));
    if x.is_none() && y.is_none() && crs.is_none() {
        if accuracy.is_some() {
            return Err(AppError::validation(
                "GPS_ACCURACY_WITHOUT_POINT",
                "gps_accuracy_m requires x, y and crs_code",
            ));
        }
        return Ok(Gps {
            x: None,
            y: None,
            crs: None,
            accuracy: None,
            deviation: None,
            warnings: Vec::new(),
        });
    }
    let (Some(x), Some(y), Some(crs)) = (x, y, crs) else {
        return Err(validation("survey coordinate requires x, y and crs_code"));
    };
    let Some(building_id) = building_id else {
        return Err(AppError::validation(
            "GPS_BUILDING_REQUIRED",
            "GPS survey point requires a linked building",
        ));
    };
    let invalid = || AppError::validation("GPS_INVALID", "invalid GPS coordinate");
    let point_x = py_float(x).ok_or_else(invalid)?;
    let point_y = py_float(y).ok_or_else(invalid)?;
    // Python int(): truncates floats, parses decimal-integer strings.
    let crs_code = match crs {
        Value::Number(number) => number.as_i64().or_else(|| {
            number
                .as_f64()
                .filter(|v| v.is_finite())
                .map(|v| v.trunc() as i64)
        }),
        Value::String(text) => text.trim().parse::<i64>().ok(),
        _ => None,
    }
    .ok_or_else(invalid)?;
    if !point_x.is_finite() || !point_y.is_finite() || crs_code <= 0 {
        return Err(invalid());
    }
    let crs_code = i32::try_from(crs_code).map_err(|_| invalid())?;
    let accuracy_m = match accuracy {
        None => None,
        Some(value) => {
            let bad = || AppError::validation("GPS_ACCURACY", "invalid GPS accuracy");
            let accuracy = py_float(value).ok_or_else(bad)?;
            if !accuracy.is_finite() || accuracy <= 0.0 {
                return Err(bad());
            }
            Some(accuracy)
        }
    };
    let known = sqlx::query("SELECT 1 AS hit FROM spatial_ref_sys WHERE srid=$1")
        .bind(crs_code)
        .fetch_optional(&mut **edit.transaction())
        .await
        .map_err(db_error)?;
    if known.is_none() {
        return Err(AppError::validation("GPS_CRS_UNKNOWN", "unknown GPS CRS"));
    }
    let row = sqlx::query(
        "SELECT
            ST_X(ST_Transform(ST_SetSRID(ST_MakePoint($4,$5),$6), ST_SRID(rf.geometry))) AS stored_x,
            ST_Y(ST_Transform(ST_SetSRID(ST_MakePoint($4,$5),$6), ST_SRID(rf.geometry))) AS stored_y,
            ST_SRID(rf.geometry) AS stored_crs,
            ST_Distance(
                ST_Transform(rf.geometry,4326)::geography,
                ST_Transform(ST_SetSRID(ST_MakePoint($4,$5),$6),4326)::geography
            ) AS deviation_m
         FROM house_numbering.buildings b
         JOIN extensions.spatial_features_v1 rf
           ON rf.organization_id=b.organization_id AND rf.project_id=b.project_id
          AND rf.source_id=b.linked_source_id AND rf.feature_id=b.linked_feature_id
         WHERE b.organization_id=$1 AND b.project_id=$2 AND b.id=$3 AND b.state='active'
           AND rf.geometry_state='present'",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(building_id)
    .bind(point_x)
    .bind(point_y)
    .bind(crs_code)
    .fetch_optional(&mut **edit.transaction())
    .await
    .map_err(db_error)?
    .ok_or_else(|| {
        AppError::validation(
            "GPS_REFERENCE_UNAVAILABLE",
            "linked building geometry is unavailable for GPS validation",
        )
    })?;
    let deviation: f64 = row.try_get("deviation_m").map_err(db_error)?;
    let mut warnings = Vec::new();
    match accuracy_m {
        None => warnings.push(json!({
            "client_uuid": client_uuid.to_string(),
            "code": "GPS_ACCURACY_MISSING",
            "deviation_m": round3(deviation),
        })),
        Some(accuracy) if deviation > accuracy => warnings.push(json!({
            "client_uuid": client_uuid.to_string(),
            "code": "GPS_DEVIATION",
            "deviation_m": round3(deviation),
            "accuracy_m": round3(accuracy),
        })),
        Some(_) => {}
    }
    Ok(Gps {
        x: Some(row.try_get("stored_x").map_err(db_error)?),
        y: Some(row.try_get("stored_y").map_err(db_error)?),
        crs: Some(row.try_get("stored_crs").map_err(db_error)?),
        accuracy: accuracy_m,
        deviation: Some(deviation),
        warnings,
    })
}

/// Port of `HouseNumberingSurveyService.sync` (idempotent offline batches).
pub async fn sync_surveys(
    edit: &mut ProjectRead<'_>,
    actor: AuditActor<'_>,
    can_manage: bool,
    artifact_root: &Path,
    operations: &[Value],
    receipt_version: Option<u32>,
) -> Result<Value, AppError> {
    if operations.is_empty() || operations.len() > 500 {
        return Err(validation(
            "offline sync batch must contain 1..500 operations",
        ));
    }
    let (mut applied, mut replayed, mut conflicts, mut warnings_out) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::<Value>::new());
    let mut receipts=Vec::<Value>::new();
    let mut seen=std::collections::HashSet::new();
    let mut delegated = 0_i64;
    let now = chrono::Utc::now();
    let empty = Map::new();
    for operation in operations {
        let item = operation.as_object().unwrap_or(&empty);
        let identifiers = || validation("invalid offline survey identifiers");
        let client_uuid = item
            .get("client_uuid")
            .and_then(|value| Uuid::parse_str(&id_text(value)).ok())
            .ok_or_else(identifiers)?;
        let building_id = match item.get("building_id").filter(|value| py_truthy(value)) {
            Some(value) => Some(Uuid::parse_str(&id_text(value)).map_err(|_| identifiers())?),
            None => None,
        };
        let expected_revision=if receipt_version==Some(2){
            if !seen.insert(client_uuid){return Err(validation("duplicate client_uuid in batch"));}
            let queue_actor=item.get("queue_actor_id").and_then(Value::as_str).and_then(|v|Uuid::parse_str(v).ok());
            if queue_actor!=Some(actor.id){return Err(AppError::forbidden("offline queue belongs to another actor"));}
            Some(item.get("expected_building_revision").and_then(Value::as_i64).filter(|r|*r>0)
                .ok_or_else(||validation("offline survey requires expected building revision"))?)
        }else{None};
        if expected_revision.is_some()&&building_id.is_none(){return Err(validation("offline survey requires building identity"));}
        let recorded_by = match item.get("recorded_by_actor_id").filter(|v| py_truthy(v)) {
            Some(value) => Uuid::parse_str(&id_text(value)).map_err(|_| identifiers())?,
            None => actor.id,
        };
        if recorded_by != actor.id {
            if !can_manage {
                return Err(manage_required());
            }
            let member = sqlx::query(
                "SELECT 1 AS hit FROM org.organization_memberships
                 WHERE organization_id=$1 AND user_id=$2 AND status='active'",
            )
            .bind(edit.organization_id())
            .bind(recorded_by)
            .fetch_optional(&mut **edit.transaction())
            .await
            .map_err(db_error)?;
            if member.is_none() {
                return Err(AppError::validation(
                    "DELEGATED_ACTOR_INVALID",
                    "delegated survey recorder is not an active organization member",
                ));
            }
        }
        let condition = match item.get("condition") {
            None => String::new(),
            Some(value) => id_text(value),
        };
        if !matches!(condition.as_str(), "house" | "vacant" | "removed" | "other") {
            return Err(validation("invalid survey condition"));
        }
        let photo_values = match item.get("photo_artifact_ids") {
            None => Vec::new(),
            Some(Value::Array(values)) => values.clone(),
            // shortcut: Python iterates any iterable (e.g. a string); only lists are accepted here.
            Some(_) => return Err(AppError::internal("survey photo list is not an array")),
        };
        let mut photo_ids = Vec::new();
        for value in &photo_values {
            photo_ids.push(
                Uuid::parse_str(&id_text(value))
                    .map_err(|_| AppError::internal("invalid survey photo identifier"))?,
            );
        }
        let unique: std::collections::HashSet<&Uuid> = photo_ids.iter().collect();
        if photo_ids.len() > 12 || unique.len() != photo_ids.len() {
            return Err(AppError::validation(
                "PHOTO_LIST",
                "invalid survey photo list",
            ));
        }
        let canonical_photos: Vec<String> = photo_ids.iter().map(Uuid::to_string).collect();
        let raw = |key: &str| item.get(key).cloned().unwrap_or(Value::Null);
        let mut semantic = json!({
            "building_id": building_id.map(|id| id.to_string()),
            "condition": condition,
            "x": raw("x"),
            "y": raw("y"),
            "crs_code": raw("crs_code"),
            "gps_accuracy_m": raw("gps_accuracy_m"),
            "photo_artifact_ids": canonical_photos,
            "recorded_by_actor_id": recorded_by.to_string(),
        });
        if let Some(revision)=expected_revision {
            semantic["expected_building_revision"]=json!(revision);
            semantic["queue_actor_id"]=json!(actor.id);
            semantic["protocol"]=json!(2);
        }
        // serde_json maps are key-sorted and compact, like json.dumps(sort_keys, separators).
        let fingerprint = format!(
            "{:x}",
            Sha256::digest(
                serde_json::to_string(&semantic)
                    .unwrap_or_default()
                    .as_bytes()
            )
        );
        let existing = sqlx::query(
            "SELECT id, payload_fingerprint, warnings FROM house_numbering.surveys
             WHERE organization_id=$1 AND project_id=$2 AND client_uuid=$3",
        )
        .bind(edit.organization_id())
        .bind(edit.project_id())
        .bind(client_uuid)
        .fetch_optional(&mut **edit.transaction())
        .await
        .map_err(db_error)?;
        if let Some(existing) = existing {
            let stored: Option<String> =
                existing.try_get("payload_fingerprint").map_err(db_error)?;
            // shortcut: legacy rows without a fingerprint are treated as conflicts; all
            // rows written since hn_0004 carry one.
            if stored.as_deref() == Some(fingerprint.as_str()) {
                let id: Uuid = existing.try_get("id").map_err(db_error)?;
                replayed.push(json!(id.to_string()));
                receipts.push(json!({"client_uuid":client_uuid,"survey_id":id,"status":"replayed"}));
                if let Value::Array(items) =
                    existing.try_get::<Value, _>("warnings").map_err(db_error)?
                {
                    warnings_out.extend(items);
                }
                continue;
            }
            conflicts.push(json!({
                "client_uuid": client_uuid.to_string(),
                "code": "CLIENT_UUID_PAYLOAD_CONFLICT",
            }));
            continue;
        }
        for photo in &photo_ids {
            if !artifact_available(artifact_root, edit, *photo) {
                return Err(AppError::validation(
                    "PHOTO_MISSING",
                    format!("survey photo {photo} is unavailable"),
                ));
            }
        }
        if let Some(building) = building_id {
            let current=get_building(edit,building,true).await?;
            if expected_revision.is_some() && (current["state"]!="active" || current["revision"].as_i64()!=expected_revision){
                conflicts.push(json!({"client_uuid":client_uuid,"code":"STALE_BUILDING","current_revision":current["revision"]}));
                continue;
            }
            let accepted = sqlx::query(
                "SELECT id FROM house_numbering.surveys
                 WHERE organization_id=$1 AND project_id=$2 AND building_id=$3
                   AND state='accepted' LIMIT 1",
            )
            .bind(edit.organization_id())
            .bind(edit.project_id())
            .bind(building)
            .fetch_optional(&mut **edit.transaction())
            .await
            .map_err(db_error)?;
            if let Some(accepted) = accepted {
                let id: Uuid = accepted.try_get("id").map_err(db_error)?;
                conflicts.push(json!({
                    "client_uuid": client_uuid.to_string(),
                    "code": "BUILDING_ALREADY_ACCEPTED",
                    "existing_survey_id": id.to_string(),
                }));
                continue;
            }
        }
        let gps = gps_observation(edit, building_id, item, client_uuid).await?;
        let survey_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO house_numbering.surveys (
                id,organization_id,project_id,client_uuid,building_id,point_x,point_y,
                crs_code,gps_accuracy_m,gps_deviation_m,warnings,payload_fingerprint,
                photo_artifact_ids,condition,state,recorded_by_actor_id,
                submitted_by_actor_id,revision,created_at,updated_at
             ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,'submitted',$15,$16,1,$17,$17)",
        )
        .bind(survey_id)
        .bind(edit.organization_id())
        .bind(edit.project_id())
        .bind(client_uuid)
        .bind(building_id)
        .bind(gps.x)
        .bind(gps.y)
        .bind(gps.crs)
        .bind(gps.accuracy)
        .bind(gps.deviation)
        .bind(Value::Array(gps.warnings.clone()))
        .bind(&fingerprint)
        .bind(json!(canonical_photos))
        .bind(&condition)
        .bind(recorded_by)
        .bind(actor.id)
        .bind(now)
        .execute(&mut **edit.transaction())
        .await
        .map_err(db_error)?;
        applied.push(json!(survey_id.to_string()));
        receipts.push(json!({"client_uuid":client_uuid,"survey_id":survey_id,"status":"applied"}));
        warnings_out.extend(gps.warnings);
        if recorded_by != actor.id {
            delegated += 1;
        }
    }
    append_audit(
        edit,
        actor,
        "house_numbering.survey.sync",
        "house_numbering_survey_batch",
        Uuid::new_v4(),
        json!({
            "applied": applied.len(),
            "replayed": replayed.len(),
            "conflicts": conflicts.len(),
            "warnings": warnings_out.len(),
            "delegated": delegated,
        }),
    )
    .await?;
    let mut response=json!({"applied":applied,"replayed":replayed,"conflicts":conflicts,"warnings":warnings_out});
    if receipt_version==Some(2){response["receipts"]=json!(receipts);}
    Ok(response)
}

/// Python truthiness for JSON values read with `item.get(...)`.
fn py_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(flag) => *flag,
        Value::Number(number) => number.as_f64().is_some_and(|v| v != 0.0),
        Value::String(text) => !text.is_empty(),
        Value::Array(items) => !items.is_empty(),
        Value::Object(map) => !map.is_empty(),
    }
}

const PENDING: &str = "btrim(coalesce(attributes->>'trang_thai','')) <> 'khop' \
     AND coalesce(nullif(btrim(attributes->>'xu_ly'),''),'chua_xu_ly')='chua_xu_ly'";

fn signoff_row(row: &sqlx::postgres::PgRow) -> Result<Value, AppError> {
    wire(row)
}

pub async fn list_signoffs(read: &mut ProjectRead<'_>, source_id: Uuid) -> Result<Value, AppError> {
    let rows = sqlx::query(
        "SELECT to_jsonb(s) - 'organization_id' - 'project_id' AS wire
         FROM house_numbering.survey_signoffs s
         WHERE s.organization_id=$1 AND s.project_id=$2 AND s.source_id=$3 AND s.revoked_at IS NULL
         ORDER BY s.ap, s.ten_duong NULLS FIRST",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(source_id)
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(db_error)?;
    let items = rows
        .iter()
        .map(signoff_row)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({ "items": items }))
}

pub async fn sign_scope(
    edit: &mut ProjectRead<'_>,
    actor: AuditActor<'_>,
    source_ids: &[Uuid],
    ap: &str,
    ten_duong: Option<&str>,
    note: Option<&str>,
) -> Result<Value, AppError> {
    let label = match ten_duong {
        Some(road) => format!("{ap} · {road}"),
        None => ap.to_owned(),
    };
    let mut sources: Vec<Uuid> = source_ids.to_vec();
    sources.sort();
    sources.dedup();
    let mut counted = Vec::new();
    let (mut total, mut pending) = (0_i64, 0_i64);
    for source_id in sources {
        sqlx::query(
            "SELECT pg_advisory_xact_lock(hashtextextended('hn_signoff:'||CAST($1 AS text),0))",
        )
        .bind(source_id)
        .execute(&mut **edit.transaction())
        .await
        .map_err(db_error)?;
        let row = sqlx::query(&format!(
            "SELECT count(*)::bigint AS total, count(*) FILTER (WHERE {PENDING})::bigint AS pending
             FROM extensions.spatial_features_v1
             WHERE organization_id=$1 AND project_id=$2 AND source_id=$3
               AND attributes ? 'xu_ly' AND btrim(attributes->>'ap')=$4
               AND ($5::text IS NULL OR btrim(attributes->>'ten_duong')=$5)"
        ))
        .bind(edit.organization_id())
        .bind(edit.project_id())
        .bind(source_id)
        .bind(ap)
        .bind(ten_duong)
        .fetch_one(&mut **edit.transaction())
        .await
        .map_err(db_error)?;
        let n: i64 = row.try_get("total").map_err(db_error)?;
        let p: i64 = row.try_get("pending").map_err(db_error)?;
        if n > 0 {
            counted.push((source_id, n));
            total += n;
            pending += p;
        }
    }
    if total == 0 {
        return Err(AppError::NotFound {
            code: "NOT_FOUND",
            message: format!("{label} không có điểm khảo sát nào"),
        });
    }
    if pending > 0 {
        return Err(AppError::Conflict {
            code: "SURVEY_SCOPE_INCOMPLETE",
            message: format!("{label} còn {pending} điểm chưa đối chiếu"),
            details: Some(json!({ "pending": pending, "total": total })),
        });
    }
    let mut out = Vec::new();
    for (source_id, n) in counted {
        let exists = sqlx::query(
            "SELECT 1 AS hit FROM house_numbering.survey_signoffs
             WHERE organization_id=$1 AND project_id=$2 AND source_id=$3 AND revoked_at IS NULL
               AND ap=$4 AND (ten_duong IS NULL OR ten_duong=$5::text) LIMIT 1",
        )
        .bind(edit.organization_id())
        .bind(edit.project_id())
        .bind(source_id)
        .bind(ap)
        .bind(ten_duong)
        .fetch_optional(&mut **edit.transaction())
        .await
        .map_err(db_error)?;
        if exists.is_some() {
            return Err(AppError::conflict(
                "SURVEY_SCOPE_SIGNED",
                format!("{label} đã được ký duyệt"),
            ));
        }
        let signoff_id = Uuid::new_v4();
        let row = sqlx::query(
            "WITH inserted AS (
                INSERT INTO house_numbering.survey_signoffs
                  (id,organization_id,project_id,source_id,ap,ten_duong,point_count,
                   signed_by_actor_id,signed_at,note)
                VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
                RETURNING *
             )
             SELECT to_jsonb(inserted) - 'organization_id' - 'project_id' AS wire FROM inserted",
        )
        .bind(signoff_id)
        .bind(edit.organization_id())
        .bind(edit.project_id())
        .bind(source_id)
        .bind(ap)
        .bind(ten_duong)
        .bind(i32::try_from(n).unwrap_or(i32::MAX))
        .bind(actor.id)
        .bind(chrono::Utc::now())
        .bind(note)
        .fetch_one(&mut **edit.transaction())
        .await
        .map_err(db_error)?;
        append_audit(
            edit,
            actor,
            "house_numbering.survey_signoff.sign",
            "survey_signoff",
            signoff_id,
            json!({
                "source_id": source_id.to_string(),
                "ap": ap,
                "ten_duong": ten_duong,
                "point_count": n,
            }),
        )
        .await?;
        out.push(signoff_row(&row)?);
    }
    Ok(json!({ "items": out }))
}

pub async fn revoke_signoff(
    edit: &mut ProjectRead<'_>,
    actor: AuditActor<'_>,
    signoff_id: Uuid,
    reason: &str,
) -> Result<Value, AppError> {
    let row = sqlx::query(
        "WITH updated AS (
            UPDATE house_numbering.survey_signoffs
            SET revoked_by_actor_id=$4, revoked_at=$5, revoke_reason=$6
            WHERE organization_id=$1 AND project_id=$2 AND id=$3 AND revoked_at IS NULL
            RETURNING *
         )
         SELECT to_jsonb(updated) - 'organization_id' - 'project_id' AS wire FROM updated",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(signoff_id)
    .bind(actor.id)
    .bind(chrono::Utc::now())
    .bind(reason)
    .fetch_optional(&mut **edit.transaction())
    .await
    .map_err(db_error)?
    .ok_or_else(|| AppError::not_found("không tìm thấy chữ ký đang hiệu lực"))?;
    let result = signoff_row(&row)?;
    append_audit(
        edit,
        actor,
        "house_numbering.survey_signoff.revoke",
        "survey_signoff",
        signoff_id,
        json!({"ap": result["ap"].clone(), "ten_duong": result["ten_duong"].clone(), "reason": reason}),
    )
    .await?;
    Ok(result)
}

/// Port of `HouseNumberingSurveyService.upload_photo` over the module artifact store.
pub async fn upload_photo(
    edit: &mut ProjectRead<'_>,
    actor: AuditActor<'_>,
    root: &Path,
    content_type: &str,
    data: &[u8],
) -> Result<Value, AppError> {
    let ext = match content_type {
        "image/jpeg" => "jpg",
        "image/png" => "png",
        _ => {
            return Err(AppError::validation(
                "PHOTO_TYPE",
                "survey photo must be JPEG or PNG",
            ))
        }
    };
    if data.is_empty() || data.len() > 15 * 1024 * 1024 {
        return Err(AppError::validation(
            "PHOTO_SIZE",
            "survey photo size is invalid",
        ));
    }
    let artifact_id = Uuid::new_v4();
    let filename = format!("survey-photo.{ext}");
    let storage_key = format!(
        "extensions/{}/{}/house-numbering-survey-photos/{artifact_id}/{filename}",
        edit.organization_id(),
        edit.project_id()
    );
    let target = root.join(&storage_key);
    let digest = format!("{:x}", Sha256::digest(data));
    let meta = json!({
        "id": artifact_id.to_string(),
        "filename": filename,
        "content_type": content_type,
        "byte_size": data.len(),
        "sha256": digest,
        "storage_key": storage_key,
    });
    let write = || -> std::io::Result<()> {
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&target, data)?;
        let mut meta_path = target.clone().into_os_string();
        meta_path.push(".json");
        std::fs::write(meta_path, super::numbering::py_json_dumps(&meta))
    };
    write().map_err(|error| AppError::internal(format!("survey photo write failed: {error}")))?;
    append_audit(
        edit,
        actor,
        "house_numbering.survey.photo_upload",
        "house_numbering_photo",
        artifact_id,
        json!({ "sha256": digest, "byte_size": data.len() }),
    )
    .await?;
    Ok(json!({
        "id": artifact_id.to_string(),
        "content_type": content_type,
        "byte_size": data.len(),
        "sha256": digest,
    }))
}
