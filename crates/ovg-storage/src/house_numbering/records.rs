//! Reconciliation queue, protected household data, search, import batches,
//! document history and stored module artifacts.

use std::collections::HashMap;
use std::path::Path;

use ovg_core::AppError;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::Row;
use uuid::Uuid;

use super::buildings::get_building;
use super::{append_audit, db_error, wire_py, AuditActor};
use crate::catalog::ProjectRead;

fn validation(message: &str) -> AppError {
    AppError::validation("VALIDATION_ERROR", message)
}

fn rows_py(rows: &[sqlx::postgres::PgRow]) -> Result<Vec<Value>, AppError> {
    rows.iter().map(wire_py).collect()
}

pub async fn list_imports(edit: &mut ProjectRead<'_>) -> Result<Value, AppError> {
    let rows = sqlx::query(
        "SELECT to_jsonb(i) AS wire FROM house_numbering.import_batches i
         WHERE i.organization_id=$1 AND i.project_id=$2
         ORDER BY i.created_at DESC,i.id DESC",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .fetch_all(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    Ok(json!({ "items": rows_py(&rows)? }))
}

pub async fn document_history(
    read: &mut ProjectRead<'_>,
    building_id: Uuid,
) -> Result<Value, AppError> {
    let rows = sqlx::query(
        "SELECT to_jsonb(d) AS wire FROM house_numbering.documents d
         WHERE d.organization_id=$1 AND d.project_id=$2 AND d.building_id=$3
         ORDER BY d.created_at DESC,d.id DESC",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(building_id)
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(db_error)?;
    Ok(json!({ "items": rows_py(&rows)? }))
}

// ---------------------------------------------------------------- reconciliation

struct Evidence {
    id: Uuid,
    building_id: Option<Uuid>,
    origin: String,
    raw_number: Option<String>,
}

async fn case_exists(
    edit: &mut ProjectRead<'_>,
    building_id: Option<Uuid>,
    case_class: &str,
    evidence_ids: &[Uuid],
) -> Result<Option<Value>, AppError> {
    let rows = sqlx::query(
        "SELECT to_jsonb(c) AS wire FROM house_numbering.reconcile_cases c
         WHERE c.organization_id=$1 AND c.project_id=$2
           AND c.building_id IS NOT DISTINCT FROM $3
           AND c.case_class=$4 AND c.status='open'
         ORDER BY c.created_at DESC",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(building_id)
    .bind(case_class)
    .fetch_all(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    let wanted: std::collections::BTreeSet<String> =
        evidence_ids.iter().map(Uuid::to_string).collect();
    for row in &rows {
        let value = wire_py(row)?;
        let have: std::collections::BTreeSet<String> = value["evidence_ids"]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .map(|item| {
                        item.as_str()
                            .map_or_else(|| item.to_string(), str::to_owned)
                    })
                    .collect()
            })
            .unwrap_or_default();
        if have == wanted {
            return Ok(Some(value));
        }
    }
    Ok(None)
}

async fn insert_case(
    edit: &mut ProjectRead<'_>,
    actor: AuditActor<'_>,
    building_id: Option<Uuid>,
    evidence_ids: &[Uuid],
    case_class: &str,
    estimated: bool,
) -> Result<Value, AppError> {
    if let Some(existing) = case_exists(edit, building_id, case_class, evidence_ids).await? {
        return Ok(existing);
    }
    let ids: Vec<String> = evidence_ids.iter().map(Uuid::to_string).collect();
    let row = sqlx::query(
        "WITH inserted AS (
            INSERT INTO house_numbering.reconcile_cases (
                id,organization_id,project_id,building_id,evidence_ids,case_class,
                estimated,status,resolution,revision,created_by_actor_id,updated_by_actor_id,
                created_at,updated_at
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,'open',NULL,1,$8,$8,$9,$9)
            RETURNING *
         )
         SELECT to_jsonb(inserted) AS wire FROM inserted",
    )
    .bind(Uuid::new_v4())
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(building_id)
    .bind(json!(ids))
    .bind(case_class)
    .bind(estimated)
    .bind(actor.id)
    .bind(chrono::Utc::now())
    .fetch_one(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    wire_py(&row)
}

/// Port of `HouseNumberingReconciliation.rebuild_cases`; `classify` is the module rule.
pub async fn rebuild_reconciliation<F>(
    edit: &mut ProjectRead<'_>,
    actor: AuditActor<'_>,
    removed: &[Uuid],
    estimated: &[Uuid],
    classify: F,
) -> Result<Value, AppError>
where
    F: Fn(Option<&str>, Option<&str>, Option<&str>, bool, bool) -> Result<&'static str, String>,
{
    let buildings: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM house_numbering.buildings
         WHERE organization_id=$1 AND project_id=$2 AND state='active'
         ORDER BY id",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .fetch_all(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    let evidence_rows = sqlx::query(
        "SELECT id, building_id, origin, raw_number FROM house_numbering.evidence
         WHERE organization_id=$1 AND project_id=$2
         ORDER BY evidence_date DESC NULLS LAST,created_at DESC,id DESC",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .fetch_all(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    let mut by_building: HashMap<Uuid, Vec<Evidence>> = HashMap::new();
    let mut unmatched = Vec::new();
    for row in &evidence_rows {
        let evidence = Evidence {
            id: row.try_get("id").map_err(db_error)?,
            building_id: row.try_get("building_id").map_err(db_error)?,
            origin: row.try_get("origin").map_err(db_error)?,
            raw_number: row.try_get("raw_number").map_err(db_error)?,
        };
        match evidence.building_id {
            None if evidence.origin == "certificate" => unmatched.push(evidence.id),
            None => {}
            Some(building) => by_building.entry(building).or_default().push(evidence),
        }
    }
    // Python `str(raw_number)` renders a missing number as the text "None".
    let text = |evidence: Option<&Evidence>| {
        evidence.map(|row| row.raw_number.clone().unwrap_or_else(|| "None".to_owned()))
    };
    let mut results = Vec::new();
    for building_id in buildings {
        let rows = by_building
            .get(&building_id)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let latest = |origin: &str| rows.iter().find(|row| row.origin == origin);
        let (certificate, cad, survey) = (latest("certificate"), latest("cad"), latest("survey"));
        let (cert_text, cad_text, survey_text) = (text(certificate), text(cad), text(survey));
        let case_class = classify(
            cert_text.as_deref(),
            cad_text.as_deref(),
            survey_text.as_deref(),
            survey.is_some(),
            !removed.contains(&building_id),
        )
        .map_err(AppError::internal)?;
        let evidence_ids: Vec<Uuid> = [certificate, cad, survey]
            .into_iter()
            .flatten()
            .map(|row| row.id)
            .collect();
        results.push(
            insert_case(
                edit,
                actor,
                Some(building_id),
                &evidence_ids,
                case_class,
                estimated.contains(&building_id),
            )
            .await?,
        );
    }
    for evidence_id in unmatched {
        results.push(
            insert_case(
                edit,
                actor,
                None,
                &[evidence_id],
                "certificate_unmatched",
                false,
            )
            .await?,
        );
    }
    Ok(json!({ "items": results }))
}

pub async fn list_cases(
    read: &mut ProjectRead<'_>,
    status: Option<&str>,
    case_class: Option<&str>,
) -> Result<Value, AppError> {
    let rows = sqlx::query(
        "SELECT to_jsonb(c) AS wire FROM house_numbering.reconcile_cases c
         WHERE c.organization_id=$1 AND c.project_id=$2
           AND ($3::text IS NULL OR c.status=$3) AND ($4::text IS NULL OR c.case_class=$4)
         ORDER BY c.created_at DESC,c.id DESC",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(status.filter(|value| !value.is_empty()))
    .bind(case_class.filter(|value| !value.is_empty()))
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(db_error)?;
    Ok(json!({ "items": rows_py(&rows)? }))
}

/// Candidate `(building_id, old_number, new_number)` rows for certificate matching.
pub struct CertificateScope {
    pub route_id: Option<Uuid>,
    pub raw_number: Option<String>,
    pub candidates: Vec<(Uuid, Option<String>, Option<String>)>,
}

pub async fn certificate_scope(
    read: &mut ProjectRead<'_>,
    evidence_id: Uuid,
) -> Result<CertificateScope, AppError> {
    let evidence = sqlx::query(
        "SELECT route_id, raw_number FROM house_numbering.evidence
         WHERE organization_id=$1 AND project_id=$2 AND id=$3
           AND origin='certificate' AND building_id IS NULL",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(evidence_id)
    .fetch_optional(&mut **read.transaction())
    .await
    .map_err(db_error)?
    .ok_or_else(|| AppError::not_found("unmatched certificate evidence not found"))?;
    let route_id: Option<Uuid> = evidence.try_get("route_id").map_err(db_error)?;
    let raw_number: Option<String> = evidence.try_get("raw_number").map_err(db_error)?;
    let Some(route) = route_id else {
        return Ok(CertificateScope {
            route_id,
            raw_number,
            candidates: Vec::new(),
        });
    };
    let rows = sqlx::query(
        "SELECT b.id AS building_id,n.number_text,n.kind
         FROM house_numbering.buildings b
         JOIN house_numbering.address_numbers n
           ON n.organization_id=b.organization_id AND n.project_id=b.project_id
          AND n.building_id=b.id AND n.retired_at IS NULL
         WHERE b.organization_id=$1 AND b.project_id=$2 AND n.route_id=$3",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(route)
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(db_error)?;
    let mut order: Vec<Uuid> = Vec::new();
    let mut grouped: HashMap<Uuid, (Option<String>, Option<String>)> = HashMap::new();
    for row in &rows {
        let building: Uuid = row.try_get("building_id").map_err(db_error)?;
        let number: String = row.try_get("number_text").map_err(db_error)?;
        let kind: String = row.try_get("kind").map_err(db_error)?;
        let entry = grouped.entry(building).or_insert_with(|| {
            order.push(building);
            (None, None)
        });
        if matches!(kind.as_str(), "old" | "certificate" | "cad" | "survey") && entry.0.is_none() {
            entry.0 = Some(number.clone());
        }
        if matches!(kind.as_str(), "proposed" | "submitted" | "approved") && entry.1.is_none() {
            entry.1 = Some(number);
        }
    }
    let candidates = order
        .into_iter()
        .map(|building| {
            let (old, new) = grouped.remove(&building).unwrap_or_default();
            (building, old, new)
        })
        .collect();
    Ok(CertificateScope {
        route_id,
        raw_number,
        candidates,
    })
}

pub async fn resolve_case(
    edit: &mut ProjectRead<'_>,
    actor: AuditActor<'_>,
    case_id: Uuid,
    expected_revision: i64,
    status: &str,
    resolution: &str,
    building_id: Option<Uuid>,
) -> Result<Value, AppError> {
    let row = sqlx::query(
        "WITH updated AS (
            UPDATE house_numbering.reconcile_cases
            SET status=$4,resolution=$5,building_id=COALESCE($6,building_id),revision=revision+1,
                updated_by_actor_id=$7,updated_at=$8
            WHERE organization_id=$1 AND project_id=$2 AND id=$3
              AND revision=$9 AND status='open'
            RETURNING *
         )
         SELECT to_jsonb(updated) AS wire FROM updated",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(case_id)
    .bind(status)
    .bind(resolution.trim())
    .bind(building_id)
    .bind(actor.id)
    .bind(chrono::Utc::now())
    .bind(expected_revision)
    .fetch_optional(&mut **edit.transaction())
    .await
    .map_err(db_error)?
    .ok_or_else(|| {
        AppError::conflict("STALE_RECONCILE_CASE", "reconcile case is stale or closed")
    })?;
    wire_py(&row)
}

// ---------------------------------------------------------- protected households

/// Bind the transaction-local private-data namespace (`set_extension_private_access`).
pub async fn enable_private_access(edit: &mut ProjectRead<'_>) -> Result<(), AppError> {
    sqlx::query("SELECT set_config('openvietgis.extension_private_access', $1, true)")
        .bind("house_numbering")
        .execute(&mut **edit.transaction())
        .await
        .map_err(db_error)?;
    Ok(())
}

async fn access_log(
    edit: &mut ProjectRead<'_>,
    actor: AuditActor<'_>,
    action: &str,
    resource_id: Option<Uuid>,
    metadata: Value,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO house_numbering.access_log (
            id,organization_id,project_id,actor_id,action,resource_type,
            resource_id,metadata,created_at
         ) VALUES ($1,$2,$3,$4,$5,'restricted_household',$6,$7,$8)",
    )
    .bind(Uuid::new_v4())
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(actor.id)
    .bind(action)
    .bind(resource_id)
    .bind(metadata)
    .bind(chrono::Utc::now())
    .execute(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    Ok(())
}

/// Character length of Python `json.dumps(value, ensure_ascii=False)`.
pub fn py_json_len(value: &Value) -> usize {
    let compact = serde_json::to_string(value)
        .unwrap_or_default()
        .chars()
        .count();
    fn separators(value: &Value) -> usize {
        match value {
            Value::Array(items) => {
                items.len().saturating_sub(1) + items.iter().map(separators).sum::<usize>()
            }
            Value::Object(map) => {
                map.len() * 2 - usize::from(!map.is_empty())
                    + map.values().map(separators).sum::<usize>()
            }
            _ => 0,
        }
    }
    compact + separators(value)
}

fn py_prefix(text: &str, count: usize) -> String {
    text.chars().take(count).collect()
}

pub struct PrivateInput<'a> {
    pub building_id: Uuid,
    pub purpose: &'a str,
    pub consent: bool,
    pub lawful_basis_ref: Option<&'a str>,
    pub protected_data: &'a Value,
    pub expires_at: Option<&'a str>,
    pub record_id: Option<Uuid>,
    pub expected_revision: Option<i64>,
}

pub async fn upsert_private(
    edit: &mut ProjectRead<'_>,
    actor: AuditActor<'_>,
    input: PrivateInput<'_>,
) -> Result<Value, AppError> {
    enable_private_access(edit).await?;
    get_building(edit, input.building_id, false).await?;
    if input.purpose.trim().is_empty() || input.purpose.chars().count() > 500 {
        return Err(validation("private-data purpose is required"));
    }
    if py_json_len(input.protected_data) > 64_000 {
        return Err(validation("protected household payload is too large"));
    }
    let now = chrono::Utc::now();
    let (row, record_id, action) = match input.record_id {
        None => {
            let record_id = Uuid::new_v4();
            let row = sqlx::query(
                "WITH inserted AS (
                    INSERT INTO house_numbering.restricted_households (
                        id,organization_id,project_id,building_id,purpose,consent,lawful_basis_ref,
                        protected_data,expires_at,revision,created_by_actor_id,updated_by_actor_id,
                        created_at,updated_at
                    ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9::timestamptz,1,$10,$10,$11,$11)
                    RETURNING *
                 )
                 SELECT to_jsonb(inserted) AS wire FROM inserted",
            )
            .bind(record_id)
            .bind(edit.organization_id())
            .bind(edit.project_id())
            .bind(input.building_id)
            .bind(input.purpose.trim())
            .bind(input.consent)
            .bind(input.lawful_basis_ref)
            .bind(input.protected_data)
            .bind(input.expires_at)
            .bind(actor.id)
            .bind(now)
            .fetch_one(&mut **edit.transaction())
            .await
            .map_err(db_error)?;
            (row, record_id, "private.create")
        }
        Some(record_id) => {
            let Some(expected) = input.expected_revision else {
                return Err(validation(
                    "expected_revision required for private-data update",
                ));
            };
            let row = sqlx::query(
                "WITH updated AS (
                    UPDATE house_numbering.restricted_households
                    SET building_id=$4,purpose=$5,consent=$6,lawful_basis_ref=$7,protected_data=$8,
                        expires_at=$9::timestamptz,revision=revision+1,updated_by_actor_id=$10,
                        updated_at=$11
                    WHERE organization_id=$1 AND project_id=$2 AND id=$3 AND revision=$12
                    RETURNING *
                 )
                 SELECT to_jsonb(updated) AS wire FROM updated",
            )
            .bind(edit.organization_id())
            .bind(edit.project_id())
            .bind(record_id)
            .bind(input.building_id)
            .bind(input.purpose.trim())
            .bind(input.consent)
            .bind(input.lawful_basis_ref)
            .bind(input.protected_data)
            .bind(input.expires_at)
            .bind(actor.id)
            .bind(now)
            .bind(expected)
            .fetch_optional(&mut **edit.transaction())
            .await
            .map_err(db_error)?
            .ok_or_else(|| {
                AppError::conflict(
                    "STALE_PRIVATE_RECORD",
                    "protected household record is stale",
                )
            })?;
            (row, record_id, "private.update")
        }
    };
    access_log(
        edit,
        actor,
        action,
        Some(record_id),
        json!({
            "building_id": input.building_id.to_string(),
            "purpose": py_prefix(input.purpose, 100),
        }),
    )
    .await?;
    wire_py(&row)
}

pub async fn read_private(
    edit: &mut ProjectRead<'_>,
    actor: AuditActor<'_>,
    building_id: Uuid,
    purpose: &str,
) -> Result<Value, AppError> {
    enable_private_access(edit).await?;
    if purpose.trim().is_empty() {
        return Err(validation("access purpose required"));
    }
    let rows = sqlx::query(
        "SELECT to_jsonb(h) AS wire FROM house_numbering.restricted_households h
         WHERE h.organization_id=$1 AND h.project_id=$2 AND h.building_id=$3
           AND (h.expires_at IS NULL OR h.expires_at>now())
         ORDER BY h.created_at DESC,h.id DESC",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(building_id)
    .fetch_all(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    access_log(
        edit,
        actor,
        "private.read",
        None,
        json!({
            "building_id": building_id.to_string(),
            "purpose": py_prefix(purpose, 100),
            "count": rows.len(),
        }),
    )
    .await?;
    Ok(json!({ "items": rows_py(&rows)? }))
}

// ------------------------------------------------------------------------ search

pub struct SearchInput<'a> {
    pub query: Option<&'a str>,
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub crs_code: Option<i32>,
    pub max_distance: f64,
    pub limit: i64,
}

fn text_hit(row: &sqlx::postgres::PgRow) -> Result<Value, AppError> {
    let building: Uuid = row.try_get("building_id").map_err(db_error)?;
    Ok(json!({
        "building_id": building.to_string(),
        "route_name": row.try_get::<Option<String>, _>("route_name").map_err(db_error)?,
        "house_number": row.try_get::<Option<String>, _>("house_number").map_err(db_error)?,
        "state": row.try_get::<Option<String>, _>("state").map_err(db_error)?,
        "old_number": row.try_get::<Option<String>, _>("evidence_number").map_err(db_error)?,
        "evidence_origin": row.try_get::<Option<String>, _>("evidence_origin").map_err(db_error)?,
        "parcel_number": row.try_get::<Option<String>, _>("parcel_ref").map_err(db_error)?,
    }))
}

pub async fn search(read: &mut ProjectRead<'_>, input: SearchInput<'_>) -> Result<Value, AppError> {
    if let Some(query) = input.query.filter(|value| !value.is_empty()) {
        let q = query.trim();
        if q.is_empty() {
            return Err(validation("search query must not be blank"));
        }
        let exact = sqlx::query(
            "WITH candidates AS (
                SELECT building_id FROM house_numbering.address_numbers
                WHERE organization_id=$1 AND project_id=$2
                  AND number_text=$3 AND retired_at IS NULL AND building_id IS NOT NULL
                UNION
                SELECT building_id FROM house_numbering.evidence
                WHERE organization_id=$1 AND project_id=$2
                  AND raw_number=$3 AND building_id IS NOT NULL
                UNION
                SELECT id FROM house_numbering.buildings
                WHERE organization_id=$1 AND project_id=$2 AND parcel_ref=$3 AND state='active'
             )
             SELECT b.id AS building_id,r.name AS route_name,b.parcel_ref,
                    n.number_text AS house_number,n.kind AS state,
                    ev.raw_number AS evidence_number,ev.origin AS evidence_origin
             FROM candidates c
             JOIN house_numbering.buildings b
               ON b.organization_id=$1 AND b.project_id=$2 AND b.id=c.building_id AND b.state='active'
             LEFT JOIN house_numbering.routes r
               ON r.organization_id=b.organization_id AND r.project_id=b.project_id AND r.id=b.route_id
             LEFT JOIN LATERAL (
                 SELECT number_text,kind FROM house_numbering.address_numbers n
                 WHERE n.organization_id=b.organization_id AND n.project_id=b.project_id
                   AND n.building_id=b.id AND n.retired_at IS NULL
                 ORDER BY CASE n.kind WHEN 'approved' THEN 1 WHEN 'submitted' THEN 2
                          WHEN 'proposed' THEN 3 ELSE 4 END,n.created_at DESC,n.id
                 LIMIT 1
             ) n ON true
             LEFT JOIN LATERAL (
                 SELECT raw_number,origin FROM house_numbering.evidence ev
                 WHERE ev.organization_id=b.organization_id AND ev.project_id=b.project_id
                   AND ev.building_id=b.id
                 ORDER BY ev.evidence_date DESC NULLS LAST,ev.created_at DESC,ev.id DESC
                 LIMIT 1
             ) ev ON true
             ORDER BY r.name,n.number_text,b.id
             LIMIT $4",
        )
        .bind(read.organization_id())
        .bind(read.project_id())
        .bind(q)
        .bind(input.limit)
        .fetch_all(&mut **read.transaction())
        .await
        .map_err(db_error)?;
        let rows = if exact.is_empty() {
            let pattern = format!("%{}%", q.replace(['%', '_'], ""));
            sqlx::query(
                "SELECT DISTINCT b.id AS building_id,r.name AS route_name,b.parcel_ref,
                        n.number_text AS house_number,n.kind AS state,
                        ev.raw_number AS evidence_number,ev.origin AS evidence_origin
                 FROM house_numbering.buildings b
                 LEFT JOIN house_numbering.routes r
                   ON r.organization_id=b.organization_id AND r.project_id=b.project_id
                  AND r.id=b.route_id
                 LEFT JOIN house_numbering.address_numbers n
                   ON n.organization_id=b.organization_id AND n.project_id=b.project_id
                  AND n.building_id=b.id AND n.retired_at IS NULL
                 LEFT JOIN house_numbering.evidence ev
                   ON ev.organization_id=b.organization_id AND ev.project_id=b.project_id
                  AND ev.building_id=b.id
                 WHERE b.organization_id=$1 AND b.project_id=$2
                   AND b.state='active' AND (
                     n.number_text ILIKE $3 OR ev.raw_number ILIKE $3 OR r.name ILIKE $3
                     OR COALESCE(b.parcel_ref,'') ILIKE $3
                   )
                 ORDER BY r.name,n.number_text,b.id LIMIT $4",
            )
            .bind(read.organization_id())
            .bind(read.project_id())
            .bind(pattern)
            .bind(input.limit)
            .fetch_all(&mut **read.transaction())
            .await
            .map_err(db_error)?
        } else {
            exact
        };
        let items = rows.iter().map(text_hit).collect::<Result<Vec<_>, _>>()?;
        return Ok(json!({ "items": items }));
    }
    if input.x.is_some() || input.y.is_some() || input.crs_code.is_some() {
        let (Some(x), Some(y), Some(crs)) = (input.x, input.y, input.crs_code) else {
            return Err(validation("coordinate search requires x, y and crs_code"));
        };
        let rows = sqlx::query(
            "SELECT b.id AS building_id,r.name AS route_name,n.number_text AS house_number,
                    n.kind AS state,b.parcel_ref,
                    ST_Distance(
                      ST_Transform(ST_PointOnSurface(f.geometry),$3),
                      ST_SetSRID(ST_MakePoint($4,$5),$3)
                    ) AS distance
             FROM house_numbering.buildings b
             JOIN spatial.reference_features f
               ON f.organization_id=b.organization_id AND f.project_id=b.project_id
              AND f.source_id=b.linked_source_id AND f.feature_id=b.linked_feature_id
             LEFT JOIN house_numbering.routes r
               ON r.organization_id=b.organization_id AND r.project_id=b.project_id AND r.id=b.route_id
             LEFT JOIN LATERAL (
                 SELECT number_text,kind FROM house_numbering.address_numbers n
                 WHERE n.organization_id=b.organization_id AND n.project_id=b.project_id
                   AND n.building_id=b.id AND n.retired_at IS NULL
                 ORDER BY CASE n.kind WHEN 'approved' THEN 1 WHEN 'submitted' THEN 2 ELSE 3 END,
                          n.created_at DESC LIMIT 1
             ) n ON true
             WHERE b.organization_id=$1 AND b.project_id=$2 AND b.state='active'
               AND ST_DWithin(
                 ST_Transform(ST_PointOnSurface(f.geometry),$3),
                 ST_SetSRID(ST_MakePoint($4,$5),$3),$6
               )
             ORDER BY distance,b.id LIMIT $7",
        )
        .bind(read.organization_id())
        .bind(read.project_id())
        .bind(crs)
        .bind(x)
        .bind(y)
        .bind(input.max_distance)
        .bind(input.limit)
        .fetch_all(&mut **read.transaction())
        .await
        .map_err(db_error)?;
        let mut items = Vec::new();
        for row in &rows {
            let building: Uuid = row.try_get("building_id").map_err(db_error)?;
            items.push(json!({
                "building_id": building.to_string(),
                "route_name": row.try_get::<Option<String>, _>("route_name").map_err(db_error)?,
                "house_number": row.try_get::<Option<String>, _>("house_number").map_err(db_error)?,
                "state": row.try_get::<Option<String>, _>("state").map_err(db_error)?,
                "parcel_number": row.try_get::<Option<String>, _>("parcel_ref").map_err(db_error)?,
                "distance": row.try_get::<f64, _>("distance").map_err(db_error)?,
            }));
        }
        return Ok(json!({ "items": items }));
    }
    Err(validation("search requires query or coordinates"))
}

// --------------------------------------------------------------------- artifacts

pub struct StoredArtifact {
    pub filename: String,
    pub content_type: String,
    pub sha256: String,
    pub byte_size: u64,
    pub data: Vec<u8>,
}

/// Port of `read_module_artifact`: `Ok(None)` when missing, `Err` on integrity mismatch.
pub fn read_module_artifact(
    root: &Path,
    scope: &ProjectRead<'_>,
    namespace: &str,
    artifact_id: Uuid,
) -> Result<Option<StoredArtifact>, String> {
    let parent = root
        .join("extensions")
        .join(scope.organization_id().to_string())
        .join(scope.project_id().to_string())
        .join(namespace)
        .join(artifact_id.to_string());
    let Ok(entries) = std::fs::read_dir(&parent) else {
        return Ok(None);
    };
    let metas: Vec<_> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    let [meta_path] = metas.as_slice() else {
        return Ok(None);
    };
    let meta: Value = std::fs::read_to_string(meta_path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .ok_or("module artifact metadata unreadable")?;
    let text = |key: &str| meta[key].as_str().map(str::to_owned);
    let filename = text("filename").ok_or("module artifact metadata incomplete")?;
    let data = std::fs::read(parent.join(&filename)).map_err(|error| error.to_string())?;
    let sha256 = format!("{:x}", Sha256::digest(&data));
    let byte_size = u64::try_from(data.len()).unwrap_or(u64::MAX);
    if Some(sha256.as_str()) != meta["sha256"].as_str()
        || meta["byte_size"].as_u64() != Some(byte_size)
    {
        return Err("module artifact integrity mismatch".to_owned());
    }
    Ok(Some(StoredArtifact {
        filename,
        content_type: text("content_type").unwrap_or_default(),
        sha256,
        byte_size,
        data,
    }))
}

/// `HouseNumberingDocumentService.download` without the Python renderer.
pub async fn download_document(
    read: &mut ProjectRead<'_>,
    actor: AuditActor<'_>,
    root: &Path,
    document_id: Uuid,
) -> Result<StoredArtifact, AppError> {
    let artifact_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT artifact_id FROM house_numbering.documents
         WHERE organization_id=$1 AND project_id=$2 AND id=$3",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(document_id)
    .fetch_optional(&mut **read.transaction())
    .await
    .map_err(db_error)?
    .flatten();
    let artifact_id =
        artifact_id.ok_or_else(|| AppError::not_found("document artifact not found"))?;
    let artifact = read_module_artifact(root, read, "house-numbering-documents", artifact_id)
        .ok()
        .flatten()
        .ok_or_else(|| AppError::not_found("document artifact unavailable"))?;
    append_audit(
        read,
        actor,
        "house_numbering.document.download",
        "house_numbering_document",
        document_id,
        json!({ "artifact_id": artifact_id.to_string() }),
    )
    .await?;
    Ok(artifact)
}

/// `HouseNumberingExportService.download`: public namespace first, then the
/// private namespace (requires manage + private access, like Python).
pub async fn download_export(
    read: &mut ProjectRead<'_>,
    actor: AuditActor<'_>,
    root: &Path,
    artifact_id: Uuid,
    can_manage: bool,
) -> Result<(StoredArtifact, bool), AppError> {
    let integrity = |error: String| AppError::internal(error);
    let (artifact, private) =
        match read_module_artifact(root, read, "house-numbering-exports", artifact_id)
            .map_err(integrity)?
        {
            Some(artifact) => (artifact, false),
            None => {
                if !can_manage {
                    return Err(super::field::manage_required());
                }
                enable_private_access(read).await?;
                let artifact = read_module_artifact(
                    root,
                    read,
                    "house-numbering-private-exports",
                    artifact_id,
                )
                .map_err(integrity)?
                .ok_or_else(|| AppError::internal("module artifact not found"))?;
                (artifact, true)
            }
        };
    append_audit(
        read,
        actor,
        "house_numbering.export.download",
        "house_numbering_export",
        artifact_id,
        json!({ "sha256": artifact.sha256, "include_private": private }),
    )
    .await?;
    Ok((artifact, private))
}

/// Port of `list_geometry_reviews` (staged geometry candidates and promotion state).
pub async fn list_geometry_reviews(
    read: &mut ProjectRead<'_>,
    offset: i64,
    limit: i64,
) -> Result<Value, AppError> {
    let rows = sqlx::query(
        "SELECT jsonb_build_object(
            'id',g.id,'route_code',g.route_code,'snapshot',g.snapshot,'reason',g.reason,
            'revision',g.revision,'created_at',g.created_at,'updated_at',g.updated_at,
            'promoted_route_id',(SELECT r.id FROM house_numbering.routes r
                WHERE (r.organization_id,r.project_id)=(g.organization_id,g.project_id)
                  AND r.source_label='route-tree-import:'||g.route_code
                  AND r.name=g.snapshot->>'name' AND r.route_type=g.snapshot->>'route_type'
                  AND r.parent_route_id::text=g.snapshot->>'parent_id'
                  AND r.prefix=g.snapshot->>'resolved_prefix'
                  AND r.linked_source_id::text=g.snapshot->>'source_id'
                  AND r.linked_feature_id=g.snapshot->>'feature_id'
                  AND r.direction_state=g.snapshot->>'direction_state'
                  AND COALESCE(r.segments,'[]'::jsonb)=COALESCE(g.snapshot->'segments','[]'::jsonb)
                ORDER BY r.id LIMIT 1)) AS wire
         FROM house_numbering.geometry_reviews g
         WHERE g.organization_id=$1 AND g.project_id=$2
         ORDER BY g.updated_at DESC,g.id LIMIT $3 OFFSET $4",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(limit + 1)
    .bind(offset)
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(db_error)?;
    let has_more = i64::try_from(rows.len()).unwrap_or(i64::MAX) > limit;
    let mut items = Vec::new();
    for row in rows.iter().take(usize::try_from(limit).unwrap_or(0)) {
        let mut item = wire_py(row)?;
        let promoted = !item["promoted_route_id"].is_null();
        item["status"] = json!(if promoted { "promoted" } else { "pending" });
        items.push(item);
    }
    Ok(json!({
        "items": items,
        "next_offset": if has_more { json!(offset + limit) } else { Value::Null },
    }))
}
