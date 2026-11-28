//! Numbering proposals: creation, queue, detail and the approval state machine.

use ovg_core::AppError;
use serde_json::{json, Value};
use sqlx::Row;
use std::collections::HashSet;
use uuid::Uuid;

use super::hierarchy::get_route;
use super::{append_audit, db_error, wire, wire_py, AuditActor};
use crate::catalog::ProjectRead;

/// Python `repr(str)` for messages that embed raw numbers.
pub(crate) fn py_repr(value: &str) -> String {
    let quote = if value.contains('\'') && !value.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::with_capacity(value.len() + 2);
    out.push(quote);
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}

pub async fn get_rule(edit: &mut ProjectRead<'_>, rule_id: Uuid) -> Result<Value, AppError> {
    let row = sqlx::query(
        "SELECT to_jsonb(r) - 'max_distance_m' AS wire, r.max_distance_m::float8 AS max_distance
         FROM house_numbering.rules r
         WHERE r.organization_id=$1 AND r.project_id=$2 AND r.id=$3",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(rule_id)
    .fetch_optional(&mut **edit.transaction())
    .await
    .map_err(db_error)?
    .ok_or_else(|| AppError::not_found("numbering rule not found"))?;
    let mut value = wire(&row)?;
    value["max_distance_m"] = json!(row.try_get::<f64, _>("max_distance").map_err(db_error)?);
    Ok(value)
}

// Stable scope identities: a route includes its descendant alleys and an area
// includes its descendant administrative units. Never accept out-of-scope rows.
async fn scoped_buildings(
    edit: &mut ProjectRead<'_>,
    scope_kind: &str,
    scope_id: Uuid,
    building_ids: &[Uuid],
) -> Result<(), AppError> {
    if building_ids.is_empty() || building_ids.iter().collect::<HashSet<_>>().len() != building_ids.len() {
        return Err(AppError::validation("INVALID_PROPOSAL_BUILDINGS", "proposal requires nonempty, unique buildings"));
    }
    let count: i64 = sqlx::query_scalar(
        "WITH RECURSIVE scope_tree(id) AS (
             SELECT $4::uuid
             UNION
             SELECT child.id FROM scope_tree parent JOIN (
                 SELECT id,parent_route_id AS parent_id FROM house_numbering.routes
                 WHERE organization_id=$1 AND project_id=$2 AND $3::text='route'
                 UNION ALL
                 SELECT id,parent_id FROM house_numbering.areas
                 WHERE organization_id=$1 AND project_id=$2 AND $3::text='area'
             ) child ON child.parent_id=parent.id
         ) SELECT count(*) FROM house_numbering.buildings b
         WHERE b.organization_id=$1 AND b.project_id=$2 AND b.id=ANY($5)
           AND b.state='active'
           AND CASE $3::text WHEN 'route' THEN b.route_id IN (SELECT id FROM scope_tree)
                            WHEN 'area' THEN b.area_id IN (SELECT id FROM scope_tree)
                            ELSE false END",
    ).bind(edit.organization_id()).bind(edit.project_id()).bind(scope_kind)
     .bind(scope_id).bind(building_ids).fetch_one(&mut **edit.transaction()).await.map_err(db_error)?;
    if count != building_ids.len() as i64 {
        return Err(AppError::conflict("PROPOSAL_SCOPE_MISMATCH", "every building must be active and within the proposal scope"));
    }
    Ok(())
}

pub async fn create_proposal(
    edit: &mut ProjectRead<'_>,
    actor: AuditActor<'_>,
    scope_kind: &str,
    scope_id: Uuid,
    rule_id: Uuid,
    building_ids: &[Uuid],
) -> Result<Value, AppError> {
    if scope_kind == "route" {
        get_route(edit, scope_id, false).await?;
    } else {
        let area = sqlx::query(
            "SELECT 1 AS hit FROM house_numbering.areas
             WHERE organization_id=$1 AND project_id=$2 AND id=$3",
        )
        .bind(edit.organization_id())
        .bind(edit.project_id())
        .bind(scope_id)
        .fetch_optional(&mut **edit.transaction())
        .await
        .map_err(db_error)?;
        if area.is_none() {
            return Err(AppError::validation(
                "VALIDATION_ERROR",
                "proposal area does not exist",
            ));
        }
    }
    get_rule(edit, rule_id).await?;
    scoped_buildings(edit, scope_kind, scope_id, building_ids).await?;
    let mut numbers: Vec<(Uuid, Uuid, String)> = Vec::new();
    for building_id in building_ids {
        let row = sqlx::query(
            "SELECT id, building_id, number_text, kind, rule_id FROM house_numbering.address_numbers
             WHERE organization_id=$1 AND project_id=$2 AND building_id=$3
               AND retired_at IS NULL AND kind IN ('proposed','submitted','approved')
             ORDER BY CASE kind WHEN 'approved' THEN 1 WHEN 'submitted' THEN 2 ELSE 3 END
             LIMIT 1",
        )
        .bind(edit.organization_id())
        .bind(edit.project_id())
        .bind(building_id)
        .fetch_optional(&mut **edit.transaction())
        .await
        .map_err(db_error)?;
        let Some(row) = row else {
            return Err(missing_proposed());
        };
        if row.try_get::<String, _>("kind").map_err(db_error)? != "proposed" {
            return Err(missing_proposed());
        }
        if row.try_get::<Option<Uuid>, _>("rule_id").map_err(db_error)? != Some(rule_id) {
            return Err(AppError::conflict("PROPOSAL_RULE_MISMATCH", "number must be generated using the proposal rule"));
        }
        numbers.push((
            row.try_get("id").map_err(db_error)?,
            row.try_get("building_id").map_err(db_error)?,
            row.try_get("number_text").map_err(db_error)?,
        ));
    }
    let now = chrono::Utc::now();
    let proposal_id = Uuid::new_v4();
    let proposal = sqlx::query(
        "WITH inserted AS (
            INSERT INTO house_numbering.proposals (
                id,organization_id,project_id,scope_kind,scope_id,rule_id,status,revision,
                created_by_actor_id,updated_by_actor_id,created_at,updated_at
            ) VALUES ($1,$2,$3,$4,$5,$6,'draft',1,$7,$7,$8,$8)
            RETURNING *
         )
         SELECT to_jsonb(inserted) AS wire FROM inserted",
    )
    .bind(proposal_id)
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(scope_kind)
    .bind(scope_id)
    .bind(rule_id)
    .bind(actor.id)
    .bind(now)
    .fetch_one(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    let proposal = wire(&proposal)?;
    let mut items = Vec::new();
    for (number_id, building_id, number_text) in &numbers {
        let row = sqlx::query(
            "WITH inserted AS (
                INSERT INTO house_numbering.proposal_items (
                    id,organization_id,project_id,proposal_id,building_id,proposed_number,
                    source_number_id,created_at
                ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
                RETURNING *
             )
             SELECT to_jsonb(inserted) AS wire FROM inserted",
        )
        .bind(Uuid::new_v4())
        .bind(edit.organization_id())
        .bind(edit.project_id())
        .bind(proposal_id)
        .bind(building_id)
        .bind(number_text)
        .bind(number_id)
        .bind(now)
        .fetch_one(&mut **edit.transaction())
        .await
        .map_err(db_error)?;
        items.push(wire(&row)?);
    }
    append_audit(
        edit,
        actor,
        "house_numbering.proposal.create",
        "house_numbering_proposal",
        proposal_id,
        json!({ "items": items.len() }),
    )
    .await?;
    Ok(json!({ "proposal": proposal, "items": items }))
}

fn missing_proposed() -> AppError {
    AppError::conflict(
        "MISSING_PROPOSED_NUMBER",
        "every proposal item needs a proposed number",
    )
}

async fn load_proposal(
    edit: &mut ProjectRead<'_>,
    proposal_id: Uuid,
    for_update: bool,
) -> Result<(Value, Vec<Value>), AppError> {
    let sql = if for_update {
        "SELECT to_jsonb(p) || jsonb_build_object('rule_status',r.status,'authority_ref',r.authority_ref) AS wire
         FROM house_numbering.proposals p
         JOIN house_numbering.rules r
           ON r.organization_id=p.organization_id AND r.project_id=p.project_id AND r.id=p.rule_id
         WHERE p.organization_id=$1 AND p.project_id=$2 AND p.id=$3 FOR UPDATE"
    } else {
        "SELECT to_jsonb(p) || jsonb_build_object('rule_status',r.status,'authority_ref',r.authority_ref) AS wire
         FROM house_numbering.proposals p
         JOIN house_numbering.rules r
           ON r.organization_id=p.organization_id AND r.project_id=p.project_id AND r.id=p.rule_id
         WHERE p.organization_id=$1 AND p.project_id=$2 AND p.id=$3"
    };
    let proposal = sqlx::query(sql)
        .bind(edit.organization_id())
        .bind(edit.project_id())
        .bind(proposal_id)
        .fetch_optional(&mut **edit.transaction())
        .await
        .map_err(db_error)?
        .ok_or_else(|| AppError::not_found("proposal not found"))?;
    let proposal = wire(&proposal)?;
    let rows = sqlx::query(
        "SELECT to_jsonb(i) || jsonb_build_object(
            'eligibility',b.eligibility,'number_kind',n.kind,'route_id',n.route_id) AS wire
         FROM house_numbering.proposal_items i
         JOIN house_numbering.buildings b
           ON b.organization_id=i.organization_id AND b.project_id=i.project_id AND b.id=i.building_id
         LEFT JOIN house_numbering.address_numbers n
           ON n.organization_id=i.organization_id AND n.project_id=i.project_id AND n.id=i.source_number_id
         WHERE i.organization_id=$1 AND i.project_id=$2 AND i.proposal_id=$3
         ORDER BY i.created_at,i.id",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(proposal_id)
    .fetch_all(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    let items = rows.iter().map(wire).collect::<Result<Vec<_>, _>>()?;
    Ok((proposal, items))
}

pub async fn get_proposal(
    read: &mut ProjectRead<'_>,
    proposal_id: Uuid,
) -> Result<Value, AppError> {
    let (proposal, items) = load_proposal(read, proposal_id, false).await?;
    Ok(json!({ "proposal": proposal, "items": items }))
}

pub async fn list_proposals(
    read: &mut ProjectRead<'_>,
    status: Option<&str>,
    offset: i64,
    limit: i64,
) -> Result<Value, AppError> {
    let rows = sqlx::query(
        "SELECT jsonb_build_object(
            'id',p.id,'scope_kind',p.scope_kind,'scope_id',p.scope_id,'status',p.status,
            'revision',p.revision,'rule_id',p.rule_id,'created_at',p.created_at,
            'updated_at',p.updated_at,'submitted_at',p.submitted_at,'decided_at',p.decided_at,
            'decision_comment',p.decision_comment,'rule_version',r.version,'rule_status',r.status,
            'authority_ref',r.authority_ref,
            'item_count',(SELECT count(*) FROM house_numbering.proposal_items i
               WHERE i.organization_id=p.organization_id AND i.project_id=p.project_id
                 AND i.proposal_id=p.id)) AS wire
         FROM house_numbering.proposals p JOIN house_numbering.rules r
           ON (r.organization_id,r.project_id,r.id)=(p.organization_id,p.project_id,p.rule_id)
         WHERE p.organization_id=$1 AND p.project_id=$2 AND ($3::text IS NULL OR p.status=$3::text)
         ORDER BY p.updated_at DESC,p.id LIMIT $4 OFFSET $5",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(status)
    .bind(limit + 1)
    .bind(offset)
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(db_error)?;
    let has_more = i64::try_from(rows.len()).unwrap_or(i64::MAX) > limit;
    let items = rows
        .iter()
        .take(usize::try_from(limit).unwrap_or(0))
        .map(wire_py)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({
        "items": items,
        "next_offset": if has_more { json!(offset + limit) } else { Value::Null },
    }))
}

// Lock the same entities used by allocation: route, then building, then number.
// The proposal/rule row is already locked. Re-read authoritative fields only
// after all entity locks have been acquired; never rely on the UI snapshot.
async fn validate_transition_items(
    edit: &mut ProjectRead<'_>, proposal_id: Uuid, proposal: &Value,
    expected_kind: &str, approval: bool,
) -> Result<(), AppError> {
    let org=edit.organization_id(); let project=edit.project_id();
    for statement in [
        "SELECT r.id FROM house_numbering.routes r JOIN house_numbering.address_numbers n
          ON (n.organization_id,n.project_id,n.route_id)=(r.organization_id,r.project_id,r.id)
          WHERE r.organization_id=$1 AND r.project_id=$2 AND n.id IN
          (SELECT source_number_id FROM house_numbering.proposal_items WHERE organization_id=$1 AND project_id=$2 AND proposal_id=$3)
          ORDER BY r.id FOR UPDATE OF r",
        "SELECT b.id FROM house_numbering.buildings b WHERE b.organization_id=$1 AND b.project_id=$2 AND b.id IN
          (SELECT building_id FROM house_numbering.proposal_items WHERE organization_id=$1 AND project_id=$2 AND proposal_id=$3)
          ORDER BY b.id FOR UPDATE OF b",
        "SELECT n.id FROM house_numbering.address_numbers n WHERE n.organization_id=$1 AND n.project_id=$2 AND n.id IN
          (SELECT source_number_id FROM house_numbering.proposal_items WHERE organization_id=$1 AND project_id=$2 AND proposal_id=$3)
          ORDER BY n.id FOR UPDATE OF n",
    ] {
        sqlx::query(statement).bind(org).bind(project).bind(proposal_id)
            .fetch_all(&mut **edit.transaction()).await.map_err(db_error)?;
    }
    let rows=sqlx::query(
        "SELECT i.building_id,i.proposed_number,b.state,b.eligibility,b.route_id AS building_route,
            n.route_id,n.building_id AS number_building,n.number_text,n.kind,n.rule_id,n.retired_at IS NOT NULL AS retired,r.status AS route_status
         FROM house_numbering.proposal_items i JOIN house_numbering.buildings b
           ON (b.organization_id,b.project_id,b.id)=(i.organization_id,i.project_id,i.building_id)
         LEFT JOIN house_numbering.address_numbers n ON (n.organization_id,n.project_id,n.id)=(i.organization_id,i.project_id,i.source_number_id)
         LEFT JOIN house_numbering.routes r ON (r.organization_id,r.project_id,r.id)=(n.organization_id,n.project_id,n.route_id)
         WHERE i.organization_id=$1 AND i.project_id=$2 AND i.proposal_id=$3 ORDER BY i.id"
    ).bind(org).bind(project).bind(proposal_id).fetch_all(&mut **edit.transaction()).await.map_err(db_error)?;
    let rule=proposal["rule_id"].as_str().and_then(|v|Uuid::parse_str(v).ok()).ok_or_else(||AppError::internal("proposal rule missing"))?;
    let scope=proposal["scope_id"].as_str().and_then(|v|Uuid::parse_str(v).ok()).ok_or_else(||AppError::internal("proposal scope missing"))?;
    let mut buildings=Vec::with_capacity(rows.len());
    for row in rows {
        let id:Uuid=row.try_get("building_id").map_err(db_error)?;
        let route:Option<Uuid>=row.try_get("route_id").map_err(db_error)?;
        let number:Option<String>=row.try_get("number_text").map_err(db_error)?;
        let snapshot:String=row.try_get("proposed_number").map_err(db_error)?;
        if route.is_none() || row.try_get::<Option<Uuid>,_>("building_route").map_err(db_error)?!=route
            || row.try_get::<Option<Uuid>,_>("number_building").map_err(db_error)?!=Some(id)
            || number.as_deref()!=Some(snapshot.as_str())
            || row.try_get::<Option<String>,_>("kind").map_err(db_error)?.as_deref()!=Some(expected_kind)
            || row.try_get::<Option<Uuid>,_>("rule_id").map_err(db_error)?!=Some(rule)
            || row.try_get::<bool,_>("retired").map_err(db_error)?
            || row.try_get::<String,_>("state").map_err(db_error)?!="active" {
            return Err(AppError::conflict("STALE_PROPOSAL_ITEM", "building, route, rule or proposed number changed"));
        }
        let eligibility:String=row.try_get("eligibility").map_err(db_error)?;
        if eligibility=="ineligible" || (approval && eligibility!="eligible") {
            return Err(AppError::conflict("BUILDING_ELIGIBILITY_UNVERIFIED", "approval requires every building to be explicitly eligible"));
        }
        if approval && row.try_get::<Option<String>,_>("route_status").map_err(db_error)?.as_deref()!=Some("confirmed") {
            return Err(AppError::conflict("ROUTE_UNVERIFIED", "approval requires confirmed routes"));
        }
        buildings.push(id);
    }
    scoped_buildings(edit,proposal["scope_kind"].as_str().unwrap_or_default(),scope,&buildings).await
}

async fn move_numbers(
    edit: &mut ProjectRead<'_>,
    actor: AuditActor<'_>,
    proposal_id: Uuid,
    from: &str,
    to: &str,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<(), AppError> {
    let moved=sqlx::query(
        "UPDATE house_numbering.address_numbers n SET kind=$5,revision=revision+1,
             updated_by_actor_id=$6,updated_at=$7
         FROM house_numbering.proposal_items i
         WHERE i.organization_id=$1 AND i.project_id=$2 AND i.proposal_id=$3
           AND n.organization_id=i.organization_id AND n.project_id=i.project_id
           AND n.id=i.source_number_id AND n.kind=$4 AND n.retired_at IS NULL",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(proposal_id)
    .bind(from)
    .bind(to)
    .bind(actor.id)
    .bind(now)
    .execute(&mut **edit.transaction())
    .await
    .map_err(db_error)?.rows_affected();
    let expected:i64=sqlx::query_scalar("SELECT count(*) FROM house_numbering.proposal_items WHERE organization_id=$1 AND project_id=$2 AND proposal_id=$3")
        .bind(edit.organization_id()).bind(edit.project_id()).bind(proposal_id)
        .fetch_one(&mut **edit.transaction()).await.map_err(db_error)?;
    if moved != expected as u64 || expected == 0 {return Err(AppError::conflict("STALE_PROPOSAL_ITEM", "not every proposal number could transition"));}
    Ok(())
}

fn blank(comment: Option<&str>) -> bool {
    comment.is_none_or(|value| value.trim().is_empty())
}

// Bind the decision to the exact submitted domain revisions, resolved route
// geometry, linked feature content and evidence/case state, without storing PII.
async fn submission_digest(edit:&mut ProjectRead<'_>,proposal_id:Uuid,revision_offset:i64)->Result<String,AppError>{
    let org=edit.organization_id();let project=edit.project_id();
    let links:Vec<Value>=sqlx::query_scalar(
        "SELECT DISTINCT link FROM (
           SELECT jsonb_build_object('source_id',b.linked_source_id,'feature_id',b.linked_feature_id) AS link
           FROM house_numbering.proposal_items i JOIN house_numbering.buildings b
             ON (b.organization_id,b.project_id,b.id)=(i.organization_id,i.project_id,i.building_id)
           WHERE i.organization_id=$1 AND i.project_id=$2 AND i.proposal_id=$3
           UNION ALL
           SELECT segment FROM house_numbering.proposal_items i JOIN house_numbering.address_numbers n
             ON (n.organization_id,n.project_id,n.id)=(i.organization_id,i.project_id,i.source_number_id)
           JOIN house_numbering.routes r ON (r.organization_id,r.project_id,r.id)=(n.organization_id,n.project_id,n.route_id)
           CROSS JOIN LATERAL jsonb_array_elements(CASE WHEN jsonb_array_length(r.segments)>0 THEN r.segments
             ELSE jsonb_build_array(jsonb_build_object('source_id',r.linked_source_id,'feature_id',r.linked_feature_id)) END) segment
           WHERE i.organization_id=$1 AND i.project_id=$2 AND i.proposal_id=$3
         ) refs WHERE link->>'source_id' IS NOT NULL AND link->>'feature_id' IS NOT NULL"
    ).bind(org).bind(project).bind(proposal_id).fetch_all(&mut **edit.transaction()).await.map_err(db_error)?;
    let refs:Vec<ovg_storage::module_ports::SpatialFeatureRef>=serde_json::from_value(Value::Array(links))
        .map_err(|_|AppError::internal("invalid proposal feature reference"))?;
    let locked=ovg_storage::module_ports::lock_linked_features(edit,&refs,true).await?;
    if refs.iter().any(|r|!locked.contains(r)){
        return Err(AppError::conflict("STALE_PROPOSAL_GEOMETRY","proposal references a missing GIS feature"));
    }
    let digest:String=sqlx::query_scalar(
        "WITH items AS (
          SELECT i.id,i.building_id,n.route_id,
            jsonb_build_array(i.id,b.id,b.revision,n.id,n.revision-$4,r.id,r.revision,
              rule.id,rule.revision,rule.status,rule.authority_ref,
              encode(sha256(ST_AsEWKB(line.geometry)),'hex'),
              encode(sha256(convert_to(jsonb_build_array(f.attributes,encode(ST_AsEWKB(f.geometry),'hex'),f.geometry_state,f.schema_revision,s.lifecycle,s.validation_state)::text,'UTF8')),'hex'),
              (SELECT coalesce(jsonb_agg(jsonb_build_array(c.id,c.revision,c.status) ORDER BY c.id),'[]'::jsonb)
                 FROM house_numbering.reconcile_cases c WHERE (c.organization_id,c.project_id,c.building_id)=(b.organization_id,b.project_id,b.id)),
              (SELECT coalesce(jsonb_agg(encode(sha256(convert_to(to_jsonb(e)::text,'UTF8')),'hex') ORDER BY e.id),'[]'::jsonb)
                 FROM house_numbering.evidence e WHERE (e.organization_id,e.project_id,e.building_id)=(b.organization_id,b.project_id,b.id))
            ) AS pin,
            line.geometry IS NOT NULL AND f.geometry_state='present' AND s.lifecycle='active' AND s.validation_state='ready' AS ready
          FROM house_numbering.proposal_items i JOIN house_numbering.buildings b
            ON (b.organization_id,b.project_id,b.id)=(i.organization_id,i.project_id,i.building_id)
          JOIN house_numbering.address_numbers n ON (n.organization_id,n.project_id,n.id)=(i.organization_id,i.project_id,i.source_number_id)
          JOIN house_numbering.routes r ON (r.organization_id,r.project_id,r.id)=(n.organization_id,n.project_id,n.route_id)
          JOIN house_numbering.rules rule ON (rule.organization_id,rule.project_id,rule.id)=(n.organization_id,n.project_id,n.rule_id)
          LEFT JOIN house_numbering.route_lines line ON (line.organization_id,line.project_id,line.route_id)=(r.organization_id,r.project_id,r.id)
          LEFT JOIN extensions.spatial_features_v1 f ON (f.organization_id,f.project_id,f.source_id,f.feature_id)=(b.organization_id,b.project_id,b.linked_source_id,b.linked_feature_id)
          LEFT JOIN extensions.spatial_sources_v1 s ON (s.organization_id,s.project_id,s.id)=(f.organization_id,f.project_id,f.source_id)
          WHERE i.organization_id=$1 AND i.project_id=$2 AND i.proposal_id=$3
        ) SELECT CASE WHEN count(*)>0 AND bool_and(coalesce(ready,false)) THEN
          encode(sha256(convert_to(jsonb_agg(pin ORDER BY id)::text,'UTF8')),'hex') ELSE '' END FROM items"
    ).bind(org).bind(project).bind(proposal_id).bind(revision_offset).fetch_one(&mut **edit.transaction()).await.map_err(db_error)?;
    if digest.is_empty(){return Err(AppError::conflict("PROPOSAL_GEOMETRY_UNVERIFIED","every item requires a ready linked building and resolved route geometry"));}
    Ok(digest)
}

async fn check_quality(edit:&mut ProjectRead<'_>,proposal_id:Uuid)->Result<(),AppError>{
    let open:bool=sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM house_numbering.proposal_items i JOIN house_numbering.reconcile_cases c
          ON (c.organization_id,c.project_id,c.building_id)=(i.organization_id,i.project_id,i.building_id)
          WHERE i.organization_id=$1 AND i.project_id=$2 AND i.proposal_id=$3 AND c.status='open' AND c.case_class<>'matched')"
    ).bind(edit.organization_id()).bind(edit.project_id()).bind(proposal_id).fetch_one(&mut **edit.transaction()).await.map_err(db_error)?;
    if open{return Err(AppError::conflict("PROPOSAL_RECONCILIATION_PENDING","resolve outstanding address conflicts before approval"));}
    Ok(())
}

pub async fn transition_proposal(
    edit: &mut ProjectRead<'_>,
    actor: AuditActor<'_>,
    proposal_id: Uuid,
    expected_revision: i64,
    action: &str,
    comment: Option<&str>,
    reviewer: bool,
) -> Result<Value, AppError> {
    let (proposal, _) = load_proposal(edit, proposal_id, true).await?;
    if proposal["revision"].as_i64() != Some(expected_revision) {
        return Err(AppError::conflict(
            "STALE_PROPOSAL",
            "proposal revision is stale",
        ));
    }
    let state = proposal["status"].as_str().unwrap_or_default().to_owned();
    let now = chrono::Utc::now();
    let (new_state, submitted_at, keep_submitted, decided_at): (
        &str,
        Option<chrono::DateTime<chrono::Utc>>,
        bool,
        Option<chrono::DateTime<chrono::Utc>>,
    ) = match (action, state.as_str(), reviewer) {
        ("submit", "draft", _) => {
            validate_transition_items(edit, proposal_id, &proposal, "proposed", false).await?;
            let duplicate = sqlx::query(
                "SELECT n.number_text
                     FROM house_numbering.proposal_items i
                     JOIN house_numbering.address_numbers n
                       ON n.organization_id=i.organization_id AND n.project_id=i.project_id
                      AND n.id=i.source_number_id
                     WHERE i.organization_id=$1 AND i.project_id=$2 AND i.proposal_id=$3
                     GROUP BY n.route_id,n.number_text
                     HAVING count(*) > 1
                     LIMIT 1",
            )
            .bind(edit.organization_id())
            .bind(edit.project_id())
            .bind(proposal_id)
            .fetch_optional(&mut **edit.transaction())
            .await
            .map_err(db_error)?;
            let occupied = sqlx::query(
                "SELECT n.number_text
                     FROM house_numbering.proposal_items i
                     JOIN house_numbering.address_numbers n
                       ON n.organization_id=i.organization_id AND n.project_id=i.project_id
                      AND n.id=i.source_number_id
                     JOIN house_numbering.address_numbers existing
                       ON existing.organization_id=n.organization_id
                      AND existing.project_id=n.project_id
                      AND existing.route_id=n.route_id
                      AND existing.number_text=n.number_text
                      AND existing.id<>n.id
                      AND existing.retired_at IS NULL
                      AND existing.kind IN ('submitted','approved','reserved')
                     WHERE i.organization_id=$1 AND i.project_id=$2 AND i.proposal_id=$3
                     LIMIT 1",
            )
            .bind(edit.organization_id())
            .bind(edit.project_id())
            .bind(proposal_id)
            .fetch_optional(&mut **edit.transaction())
            .await
            .map_err(db_error)?;
            if let Some(row) = duplicate.or(occupied) {
                let number: String = row.try_get("number_text").map_err(db_error)?;
                return Err(AppError::conflict(
                    "NUMBER_OCCUPIED",
                    format!(
                        "number {} is already occupied on this route",
                        py_repr(&number)
                    ),
                ));
            }
            move_numbers(edit, actor, proposal_id, "proposed", "submitted", now).await?;
            let digest=submission_digest(edit,proposal_id,0).await?;
            sqlx::query("INSERT INTO house_numbering.proposal_submission_pins(organization_id,project_id,proposal_id,submitted_revision,submitted_by,input_digest) VALUES($1,$2,$3,$4,$5,$6)")
              .bind(edit.organization_id()).bind(edit.project_id()).bind(proposal_id).bind(expected_revision+1).bind(actor.id).bind(digest)
              .execute(&mut **edit.transaction()).await.map_err(db_error)?;
            ("submitted", Some(now), false, None)
        }
        ("withdraw", "submitted", false) => {
            if blank(comment) {
                return Err(AppError::validation(
                    "VALIDATION_ERROR",
                    "withdrawal requires a reason",
                ));
            }
            move_numbers(edit, actor, proposal_id, "submitted", "proposed", now).await?;
            ("draft", None, false, Some(now))
        }
        ("reject", "submitted", true) => {
            if blank(comment) {
                return Err(AppError::validation(
                    "VALIDATION_ERROR",
                    "rejection requires a reason",
                ));
            }
            move_numbers(edit, actor, proposal_id, "submitted", "proposed", now).await?;
            ("draft", None, false, Some(now))
        }
        ("approve", "submitted", true) => {
            let reviewer_id=actor.id.to_string();
            if ["created_by_actor_id", "updated_by_actor_id"].iter().any(|key| proposal[*key].as_str()==Some(reviewer_id.as_str())) {
                return Err(AppError::forbidden("proposal approver must be independent from creator and submitter"));
            }
            validate_transition_items(edit, proposal_id, &proposal, "submitted", true).await?;
            if proposal["rule_status"].as_str() != Some("authoritative")
                || blank(proposal["authority_ref"].as_str())
            {
                return Err(AppError::conflict(
                    "RULE_NOT_AUTHORITATIVE",
                    "approval requires authoritative rule",
                ));
            }
            check_quality(edit,proposal_id).await?;
            let pin:Option<(Uuid,String)>=sqlx::query_as("SELECT submitted_by,input_digest::text FROM house_numbering.proposal_submission_pins WHERE organization_id=$1 AND project_id=$2 AND proposal_id=$3 AND submitted_revision=$4")
                .bind(edit.organization_id()).bind(edit.project_id()).bind(proposal_id).bind(expected_revision)
                .fetch_optional(&mut **edit.transaction()).await.map_err(db_error)?;
            let (submitter,digest)=pin.ok_or_else(||AppError::conflict("PROPOSAL_REVIEW_REQUIRED","withdraw and resubmit the legacy proposal before approval"))?;
            if submitter==actor.id{return Err(AppError::forbidden("proposal approver must be independent from submitter"));}
            if submission_digest(edit,proposal_id,0).await?!=digest{
                return Err(AppError::conflict("STALE_PROPOSAL_REVIEW","proposal inputs changed; withdraw and resubmit"));
            }
            move_numbers(edit, actor, proposal_id, "submitted", "approved", now).await?;
            ("approved", None, true, Some(now))
        }
        _ => {
            return Err(AppError::conflict(
                "INVALID_PROPOSAL_TRANSITION",
                "invalid proposal transition",
            ))
        }
    };
    let row = sqlx::query(
        "WITH updated AS (
            UPDATE house_numbering.proposals
            SET status=$4,decision_comment=$5,revision=revision+1,updated_by_actor_id=$6,
                submitted_at=CASE WHEN $10 THEN submitted_at ELSE $7 END,decided_at=$8,updated_at=$9
            WHERE organization_id=$1 AND project_id=$2 AND id=$3
            RETURNING *
         )
         SELECT to_jsonb(updated) AS wire FROM updated",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(proposal_id)
    .bind(new_state)
    .bind(comment)
    .bind(actor.id)
    .bind(submitted_at)
    .bind(decided_at)
    .bind(now)
    .bind(keep_submitted)
    .fetch_one(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    let result = wire(&row)?;
    append_audit(
        edit,
        actor,
        &format!("house_numbering.proposal.{action}"),
        "house_numbering_proposal",
        proposal_id,
        json!({ "state": new_state }),
    )
    .await?;
    Ok(result)
}

/// Registration of an external certificate requires the same approved input
/// snapshot, not just an approved status flag. No mutation of the proposal.
pub(super) async fn verify_approved_snapshot(edit:&mut ProjectRead<'_>,proposal_id:Uuid)->Result<(),AppError>{
    let (proposal,_)=load_proposal(edit,proposal_id,true).await?;
    if proposal["status"]!="approved" || proposal["rule_status"]!="authoritative" || blank(proposal["authority_ref"].as_str()){
        return Err(AppError::conflict("ISSUANCE_REQUIRES_APPROVAL","a verified approved proposal is required"));
    }
    validate_transition_items(edit,proposal_id,&proposal,"approved",true).await?;
    check_quality(edit,proposal_id).await?;
    let revision=proposal["revision"].as_i64().ok_or_else(||AppError::internal("missing proposal revision"))?;
    let pin:Option<(Uuid,String)>=sqlx::query_as("SELECT submitted_by,input_digest::text FROM house_numbering.proposal_submission_pins WHERE organization_id=$1 AND project_id=$2 AND proposal_id=$3 AND submitted_revision=$4")
        .bind(edit.organization_id()).bind(edit.project_id()).bind(proposal_id).bind(revision-1)
        .fetch_optional(&mut **edit.transaction()).await.map_err(db_error)?;
    let (submitter,digest)=pin.ok_or_else(||AppError::conflict("ISSUANCE_REQUIRES_VERIFIED_APPROVAL","approved proposal has no verified submission pin"))?;
    if proposal["updated_by_actor_id"].as_str()==Some(submitter.to_string().as_str()) || submission_digest(edit,proposal_id,1).await?!=digest{
        return Err(AppError::conflict("ISSUANCE_REVIEW_REQUIRED","approved inputs or reviewer verification changed"));
    }
    Ok(())
}
