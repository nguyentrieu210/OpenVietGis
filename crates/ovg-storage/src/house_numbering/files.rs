//! E15 file-operation persistence. Callers hold a scoped, authorized ProjectRead.
use super::{append_audit, db_error, wire, wire_py, AuditActor};
use crate::catalog::ProjectRead;
use chrono::{NaiveDate, Utc};
use ovg_core::AppError;
use serde_json::{json, Map, Value};
use sqlx::Row;
use std::collections::HashMap;
use uuid::Uuid;

pub async fn import_lock(
    edit: &mut ProjectRead<'_>,
    fingerprint: &str,
) -> Result<Option<Value>, AppError> {
    // Serialize the same source fingerprint within one project before replay check.
    let key = format!(
        "hn-import:{}:{}:{fingerprint}",
        edit.organization_id(),
        edit.project_id()
    );
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(key)
        .execute(&mut **edit.transaction())
        .await
        .map_err(db_error)?;
    let row = sqlx::query("SELECT to_jsonb(i) AS wire FROM house_numbering.import_batches i WHERE organization_id=$1 AND project_id=$2 AND source_fingerprint=$3")
        .bind(edit.organization_id()).bind(edit.project_id()).bind(fingerprint)
        .fetch_optional(&mut **edit.transaction()).await.map_err(db_error)?;
    row.as_ref().map(wire_py).transpose()
}

pub async fn import_crs_exists(edit: &mut ProjectRead<'_>, srid: i32) -> Result<bool, AppError> {
    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM spatial_ref_sys WHERE srid=$1)")
        .bind(srid)
        .fetch_one(&mut **edit.transaction())
        .await
        .map_err(db_error)
}

pub struct ImportEvidence<'a> {
    pub filename: &'a str,
    pub fingerprint: &'a str,
    pub format: &'a str,
    pub origin: &'a str,
    pub source_crs: Option<i32>,
    pub mapping: &'a HashMap<String, String>,
    pub default_reference: &'a str,
    pub default_confidence: &'a str,
    pub encoding: &'a Value,
    pub rows: &'a [Value],
    pub artifact_id: Uuid,
}

fn value_text(v: &Value) -> Option<String> {
    match v {
        Value::Null => None,
        Value::String(s) => Some(s.clone()),
        Value::Bool(b) => Some(if *b { "True" } else { "False" }.to_owned()),
        other => Some(other.to_string()),
    }
}

pub async fn stage_evidence(
    edit: &mut ProjectRead<'_>,
    actor: AuditActor<'_>,
    input: ImportEvidence<'_>,
) -> Result<Value, AppError> {
    let now = Utc::now();
    let mut rejected = Vec::new();
    let mut accepted = 0_i64;
    for (index, row) in input.rows.iter().enumerate() {
        let get = |name: &str| {
            input
                .mapping
                .get(name)
                .and_then(|column| row.get(column))
                .unwrap_or(&Value::Null)
        };
        let building = value_text(get("building_id"));
        let route = value_text(get("route_id"));
        let building_id = building
            .as_deref()
            .and_then(|s| Uuid::parse_str(s.trim()).ok());
        let route_id = route
            .as_deref()
            .and_then(|s| Uuid::parse_str(s.trim()).ok());
        let raw = value_text(get("raw_number"))
            .or_else(|| value_text(get("old_number")).filter(|s| !s.is_empty()))
            .or_else(|| value_text(get("new_number")))
            .unwrap_or_default();
        let reference = value_text(get("source_reference"))
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| input.default_reference.to_owned());
        let confidence = value_text(get("confidence"))
            .filter(|s| !s.trim().is_empty())
            .map_or_else(
                || input.default_confidence.to_owned(),
                |s| s.trim().to_lowercase(),
            );
        let date = value_text(get("evidence_date")).and_then(|s| {
            ["%Y-%m-%d", "%d/%m/%Y", "%d-%m-%Y"]
                .iter()
                .find_map(|fmt| NaiveDate::parse_from_str(s.trim(), fmt).ok())
        });
        let mut reasons = Vec::new();
        if reference.trim().is_empty() {
            reasons.push("missing_source_reference");
        }
        if !matches!(
            confidence.as_str(),
            "authoritative" | "observed" | "estimated"
        ) {
            reasons.push("invalid_confidence");
        }
        if raw.trim().is_empty() {
            reasons.push("missing_number");
        }
        if building.as_ref().is_some_and(|s| !s.is_empty()) && building_id.is_none() {
            reasons.push("invalid_building_id");
        }
        if route.as_ref().is_some_and(|s| !s.is_empty()) && route_id.is_none() {
            reasons.push("invalid_route_id");
        }
        if !reasons.is_empty() {
            rejected.push(json!({"row":index+2,"reasons":reasons}));
            continue;
        }
        sqlx::query("INSERT INTO house_numbering.evidence
            (id,organization_id,project_id,building_id,route_id,origin,raw_number,source_reference,evidence_date,confidence,private_ref,created_by_actor_id,created_at)
            VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,NULL,$11,$12)")
            .bind(Uuid::new_v4()).bind(edit.organization_id()).bind(edit.project_id())
            .bind(building_id).bind(route_id).bind(input.origin).bind(raw.trim())
            .bind(reference.trim()).bind(date).bind(confidence).bind(actor.id).bind(now)
            .execute(&mut **edit.transaction()).await.map_err(db_error)?;
        accepted += 1;
    }
    let mut mapping =
        serde_json::to_value(input.mapping).map_err(|_| AppError::internal("invalid mapping"))?;
    mapping["source_format"] = json!(input.format);
    mapping["origin"] = json!(input.origin);
    mapping["encoding"] = input.encoding.clone();
    let read_count =
        i64::try_from(input.rows.len()).map_err(|_| AppError::internal("import count overflow"))?;
    let rejected_count = read_count - accepted;
    let row = sqlx::query("WITH inserted AS (
        INSERT INTO house_numbering.import_batches
        (id,organization_id,project_id,source_fingerprint,source_name,source_crs,mapping,read_count,accepted_count,rejected_count,original_artifact_id,status,created_by_actor_id,created_at,committed_at)
        VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,'committed',$12,$13,$13) RETURNING *)
        SELECT to_jsonb(inserted) AS wire FROM inserted")
        .bind(Uuid::new_v4()).bind(edit.organization_id()).bind(edit.project_id())
        .bind(input.fingerprint).bind(input.filename).bind(input.source_crs).bind(mapping)
        .bind(i32::try_from(read_count).map_err(|_| AppError::internal("import count overflow"))?)
        .bind(i32::try_from(accepted).map_err(|_| AppError::internal("import count overflow"))?)
        .bind(i32::try_from(rejected_count).map_err(|_| AppError::internal("import count overflow"))?)
        .bind(input.artifact_id).bind(actor.id).bind(now)
        .fetch_one(&mut **edit.transaction()).await.map_err(db_error)?;
    Ok(json!({"batch":wire_py(&row)?,"replayed":false,"rejections":rejected}))
}

pub async fn document_snapshot(
    edit: &mut ProjectRead<'_>,
    building: Uuid,
) -> Result<Value, AppError> {
    let row = sqlx::query("SELECT jsonb_build_object(
        'building_id',b.id,'parcel_ref',b.parcel_ref,'eligibility',b.eligibility,
        'route_id',r.id,'route_name',r.name,'route_source_id',r.linked_source_id,'route_feature_id',r.linked_feature_id,
        'number_id',n.id,'number_text',n.number_text,'number_kind',n.kind,'rule_id',n.rule_id,
        'rule_version',rule.version,'rule_status',rule.status,'authority_ref',rule.authority_ref,
        'left_odd',rule.left_odd,'suffix_style',rule.suffix_style,'max_distance_m',rule.max_distance_m,
        'crs_code',ST_SRID(rf.geometry),'route_geometry',ST_AsGeoJSON(rf.geometry)::jsonb,
        'building_geometry',ST_AsGeoJSON(bf.geometry)::jsonb) AS wire
        FROM house_numbering.buildings b
        LEFT JOIN house_numbering.routes r ON (r.organization_id,r.project_id,r.id)=(b.organization_id,b.project_id,b.route_id)
        LEFT JOIN house_numbering.route_lines rf ON (rf.organization_id,rf.project_id,rf.route_id)=(r.organization_id,r.project_id,r.id)
        LEFT JOIN spatial.reference_features bf ON (bf.organization_id,bf.project_id,bf.source_id,bf.feature_id)=(b.organization_id,b.project_id,b.linked_source_id,b.linked_feature_id)
        LEFT JOIN LATERAL (SELECT * FROM house_numbering.address_numbers n
           WHERE (n.organization_id,n.project_id,n.building_id)=(b.organization_id,b.project_id,b.id) AND n.retired_at IS NULL
           ORDER BY CASE n.kind WHEN 'approved' THEN 1 WHEN 'submitted' THEN 2 ELSE 3 END,n.created_at DESC LIMIT 1) n ON true
        LEFT JOIN house_numbering.rules rule ON (rule.organization_id,rule.project_id,rule.id)=(b.organization_id,b.project_id,n.rule_id)
        WHERE b.organization_id=$1 AND b.project_id=$2 AND b.id=$3 FOR SHARE OF b")
        .bind(edit.organization_id()).bind(edit.project_id()).bind(building)
        .fetch_optional(&mut **edit.transaction()).await.map_err(db_error)?
        .ok_or_else(|| AppError::not_found("building dossier not found"))?;
    let mut snapshot = wire(&row)?;
    let route = snapshot["route_id"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok());
    let rows = sqlx::query("SELECT jsonb_build_object('id',b.id,'number_text',n.number_text,'kind',n.kind,
        'geometry',ST_AsGeoJSON(ST_PointOnSurface(f.geometry))::jsonb) AS wire
        FROM house_numbering.buildings b
        JOIN spatial.reference_features f ON (f.organization_id,f.project_id,f.source_id,f.feature_id)=(b.organization_id,b.project_id,b.linked_source_id,b.linked_feature_id)
        LEFT JOIN LATERAL (SELECT number_text,kind FROM house_numbering.address_numbers n
          WHERE (n.organization_id,n.project_id,n.building_id)=(b.organization_id,b.project_id,b.id) AND n.retired_at IS NULL
          ORDER BY CASE n.kind WHEN 'approved' THEN 1 WHEN 'submitted' THEN 2 ELSE 3 END,n.created_at DESC LIMIT 1) n ON true
        WHERE b.organization_id=$1 AND b.project_id=$2 AND b.route_id=$3 AND b.state='active' LIMIT 50001")
        .bind(edit.organization_id()).bind(edit.project_id()).bind(route)
        .fetch_all(&mut **edit.transaction()).await.map_err(db_error)?;
    if rows.len() > 50000 {
        return Err(AppError::validation(
            "REPORT_ROW_LIMIT_EXCEEDED",
            "Document map exceeds safe row limit",
        ));
    }
    snapshot["route_buildings"] = json!(rows.iter().map(wire).collect::<Result<Vec<_>, _>>()?);
    snapshot["detail_radius"] = json!(35.0);
    Ok(snapshot)
}

pub struct IssueDocument<'a> {
    pub building: Uuid,
    pub number: Option<Uuid>,
    pub doc_type: &'a str,
    pub template: &'a str,
    pub status: &'a str,
    pub replaces: Option<Uuid>,
    pub artifact_id: Uuid,
    pub artifact_sha: &'a str,
}

pub async fn publish_document(
    edit: &mut ProjectRead<'_>,
    actor: AuditActor<'_>,
    input: IssueDocument<'_>,
) -> Result<Value, AppError> {
    let now = Utc::now();
    if let Some(previous) = input.replaces {
        let updated=sqlx::query("UPDATE house_numbering.documents SET status='reissue_required',revision=revision+1,updated_by_actor_id=$4,updated_at=$5
           WHERE organization_id=$1 AND project_id=$2 AND id=$3 AND building_id=$6 AND status='official'")
           .bind(edit.organization_id()).bind(edit.project_id()).bind(previous).bind(actor.id).bind(now).bind(input.building)
           .execute(&mut **edit.transaction()).await.map_err(db_error)?;
        if updated.rows_affected() != 1 {
            return Err(AppError::conflict(
                "CONFLICT",
                "reissue target is not an official document",
            ));
        }
    }
    let id = Uuid::new_v4();
    let row=sqlx::query("WITH inserted AS (INSERT INTO house_numbering.documents
        (id,organization_id,project_id,building_id,number_id,doc_type,template_revision,status,artifact_id,issued_at,replaces_document_id,revision,created_by_actor_id,updated_by_actor_id,created_at,updated_at)
        VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,1,$12,$12,$13,$13) RETURNING *) SELECT to_jsonb(inserted) AS wire FROM inserted")
        .bind(id).bind(edit.organization_id()).bind(edit.project_id()).bind(input.building).bind(input.number)
        .bind(input.doc_type).bind(input.template).bind(input.status).bind(input.artifact_id)
        .bind((input.status=="official").then_some(now)).bind(input.replaces).bind(actor.id).bind(now)
        .fetch_one(&mut **edit.transaction()).await.map_err(db_error)?;
    append_audit(edit,actor,"house_numbering.document.issue","house_numbering_document",id,
        json!({"doc_type":input.doc_type,"status":input.status,"artifact_id":input.artifact_id,"sha256":input.artifact_sha,"template_revision":input.template})).await?;
    wire_py(&row)
}

pub async fn packet_data(
    read: &mut ProjectRead<'_>,
    id: Uuid,
) -> Result<(Value, Vec<Value>), AppError> {
    let p = super::get_proposal(read, id).await?["proposal"].clone();
    let rows=sqlx::query("SELECT jsonb_build_object('building_id',i.building_id,'proposed_number',i.proposed_number,
        'number_text',n.number_text,'number_kind',n.kind,'retired_at',n.retired_at,
        'route_id',b.route_id,'parcel_ref',b.parcel_ref,'building_state',b.state,
        'linked_source_id',b.linked_source_id,'linked_feature_id',b.linked_feature_id,
        'route_name',r.name,'prefix',r.prefix,'area_name',a.name) AS wire
        FROM house_numbering.proposal_items i
        JOIN house_numbering.buildings b ON (b.organization_id,b.project_id,b.id)=(i.organization_id,i.project_id,i.building_id)
        LEFT JOIN house_numbering.routes r ON (r.organization_id,r.project_id,r.id)=(b.organization_id,b.project_id,b.route_id)
        LEFT JOIN house_numbering.areas a ON (a.organization_id,a.project_id,a.id)=(b.organization_id,b.project_id,b.area_id)
        LEFT JOIN house_numbering.address_numbers n ON (n.organization_id,n.project_id,n.id)=(i.organization_id,i.project_id,i.source_number_id)
        WHERE i.organization_id=$1 AND i.project_id=$2 AND i.proposal_id=$3 ORDER BY r.name,i.proposed_number,i.id LIMIT 5001")
        .bind(read.organization_id()).bind(read.project_id()).bind(id).fetch_all(&mut **read.transaction()).await.map_err(db_error)?;
    if rows.len() > 5000 {
        return Err(AppError::validation(
            "REPORT_ROW_LIMIT_EXCEEDED",
            "Proposal exceeds safe row limit",
        ));
    }
    Ok((p, rows.iter().map(wire).collect::<Result<Vec<_>, _>>()?))
}

const PUBLIC_FIELDS: [&str; 11] = [
    "building_id",
    "house_number",
    "old_number",
    "proposed_number",
    "approved_number",
    "route_name",
    "area_name",
    "parcel_number",
    "sheet_number",
    "state",
    "crs",
];

pub async fn export_rows(
    read: &mut ProjectRead<'_>,
    actor: AuditActor<'_>,
    fields: &[String],
    private: bool,
    purpose: &str,
) -> Result<(Vec<Value>, String), AppError> {
    if !private {
        let mut unknown: Vec<_> = fields
            .iter()
            .filter(|f| !PUBLIC_FIELDS.contains(&f.as_str()))
            .cloned()
            .collect();
        unknown.sort();
        if !unknown.is_empty() {
            return Err(AppError::validation(
                "EXPORT_FIELDS",
                format!("public export fields not allowed: {}", unknown.join(", ")),
            ));
        }
    } else if purpose.trim().is_empty() {
        return Err(AppError::validation(
            "VALIDATION_ERROR",
            "private export requires explicit purpose",
        ));
    }
    let rows=sqlx::query("SELECT jsonb_build_object('building_id',b.id,'parcel_number',b.parcel_ref,
        'route_name',r.name,'area_name',a.name,'house_number',current_n.number_text,'state',current_n.kind,
        'old_number',old_n.number_text,'proposed_number',CASE WHEN current_n.kind='proposed' THEN current_n.number_text END,
        'approved_number',CASE WHEN current_n.kind='approved' THEN current_n.number_text END,
        'crs','EPSG:'||ST_SRID(f.geometry), '_geometry',ST_AsGeoJSON(ST_Transform(ST_PointOnSurface(f.geometry),4326))::jsonb,
        '_native_x',ST_X(ST_PointOnSurface(f.geometry)),'_native_y',ST_Y(ST_PointOnSurface(f.geometry)),'_native_crs',ST_SRID(f.geometry)) AS wire
        FROM house_numbering.buildings b
        LEFT JOIN house_numbering.routes r ON (r.organization_id,r.project_id,r.id)=(b.organization_id,b.project_id,b.route_id)
        LEFT JOIN house_numbering.areas a ON (a.organization_id,a.project_id,a.id)=(b.organization_id,b.project_id,b.area_id)
        JOIN spatial.reference_features f ON (f.organization_id,f.project_id,f.source_id,f.feature_id)=(b.organization_id,b.project_id,b.linked_source_id,b.linked_feature_id)
        LEFT JOIN LATERAL (SELECT number_text,kind FROM house_numbering.address_numbers n
          WHERE (n.organization_id,n.project_id,n.building_id)=(b.organization_id,b.project_id,b.id) AND n.retired_at IS NULL
          ORDER BY CASE n.kind WHEN 'approved' THEN 1 WHEN 'submitted' THEN 2 WHEN 'proposed' THEN 3 ELSE 4 END,n.created_at DESC LIMIT 1) current_n ON true
        LEFT JOIN LATERAL (SELECT number_text FROM house_numbering.address_numbers n
          WHERE (n.organization_id,n.project_id,n.building_id)=(b.organization_id,b.project_id,b.id) AND n.retired_at IS NULL
          AND n.kind IN ('old','certificate','cad','survey') ORDER BY n.created_at DESC LIMIT 1) old_n ON true
        WHERE b.organization_id=$1 AND b.project_id=$2 AND b.state='active' ORDER BY r.name,current_n.number_text,b.id LIMIT 50001")
        .bind(read.organization_id()).bind(read.project_id()).fetch_all(&mut **read.transaction()).await.map_err(db_error)?;
    if rows.len() > 50000 {
        return Err(AppError::validation(
            "REPORT_ROW_LIMIT_EXCEEDED",
            "Export exceeds safe row limit",
        ));
    }
    let base = rows.iter().map(wire).collect::<Result<Vec<_>, _>>()?;
    let mut output = Vec::new();
    let mut projection = String::new();
    if private {
        super::enable_private_access(read).await?;
        let by_id: HashMap<_, _> = base
            .iter()
            .filter_map(|v| Some((v["building_id"].as_str()?, v)))
            .collect();
        let rows=sqlx::query("SELECT building_id,protected_data FROM house_numbering.restricted_households
            WHERE organization_id=$1 AND project_id=$2 AND (expires_at IS NULL OR expires_at>now()) ORDER BY created_at DESC,id DESC LIMIT 50001")
            .bind(read.organization_id()).bind(read.project_id()).fetch_all(&mut **read.transaction()).await.map_err(db_error)?;
        if rows.len() > 50000 {
            return Err(AppError::validation(
                "REPORT_ROW_LIMIT_EXCEEDED",
                "Private export exceeds safe row limit",
            ));
        }
        for row in rows {
            let id: Uuid = row.try_get("building_id").map_err(db_error)?;
            let id = id.to_string();
            let data: Value = row.try_get("protected_data").map_err(db_error)?;
            let mut record = Map::new();
            for key in fields {
                let value = if key == "building_id" {
                    json!(id)
                } else {
                    data.get(key)
                        .cloned()
                        .or_else(|| by_id.get(id.as_str()).and_then(|v| v.get(key).cloned()))
                        .unwrap_or(Value::Null)
                };
                record.insert(key.clone(), value);
            }
            output.push(Value::Object(record));
        }
        sqlx::query("INSERT INTO house_numbering.access_log(id,organization_id,project_id,actor_id,action,resource_type,resource_id,metadata,created_at)
          VALUES($1,$2,$3,$4,'private.export','restricted_household',NULL,$5,now())")
            .bind(Uuid::new_v4()).bind(read.organization_id()).bind(read.project_id()).bind(actor.id)
            .bind(json!({"purpose":purpose.chars().take(100).collect::<String>(),"fields":fields,"count":output.len()}))
            .execute(&mut **read.transaction()).await.map_err(db_error)?;
    } else {
        if let Some(srid) = base
            .iter()
            .find_map(|v| v["_native_crs"].as_i64())
            .and_then(|n| i32::try_from(n).ok())
        {
            projection = sqlx::query_scalar::<_, String>(
                "SELECT COALESCE(srtext,'') FROM spatial_ref_sys WHERE srid=$1",
            )
            .bind(srid)
            .fetch_optional(&mut **read.transaction())
            .await
            .map_err(db_error)?
            .unwrap_or_default();
        }
        for item in base {
            let mut record = Map::new();
            for key in fields {
                let value = item[key].clone();
                if value.is_null() || value.is_string() || value.is_i64() {
                    record.insert(key.clone(), value);
                }
            }
            for key in ["_geometry", "_native_x", "_native_y", "_native_crs"] {
                record.insert(key.to_owned(), item[key].clone());
            }
            output.push(Value::Object(record));
        }
    }
    Ok((output, projection))
}

pub async fn audit_export(
    read: &mut ProjectRead<'_>,
    actor: AuditActor<'_>,
    artifact: Uuid,
    metadata: Value,
) -> Result<(), AppError> {
    append_audit(
        read,
        actor,
        "house_numbering.export",
        "house_numbering_export",
        artifact,
        metadata,
    )
    .await
}
