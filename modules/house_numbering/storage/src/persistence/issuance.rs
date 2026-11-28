//! Externally issued certificate registry. Approval alone never issues a certificate.
use crate::catalog::ProjectRead;
use ovg_core::AppError;
use serde_json::{json,Value};
use uuid::Uuid;
use super::{append_audit,db_error,AuditActor};
use super::proposals::verify_approved_snapshot;

pub struct IssuanceInput<'a>{
    pub proposal_id:Uuid,pub number_id:Uuid,
    pub authority_name:&'a str,pub decision_ref:&'a str,pub certificate_ref:&'a str,
    pub effective_date:chrono::NaiveDate,pub template_revision:&'a str,
}

pub async fn record_issuance(edit:&mut ProjectRead<'_>,actor:AuditActor<'_>,input:IssuanceInput<'_>)->Result<Value,AppError>{
    if input.effective_date>chrono::Utc::now().date_naive(){
        return Err(AppError::validation("FUTURE_ISSUANCE_DATE","register only an already issued certificate"));
    }
    verify_approved_snapshot(edit,input.proposal_id).await?;
    let id=Uuid::new_v4();
    let record:Option<Value>=sqlx::query_scalar(
        "WITH inserted AS (
          INSERT INTO house_numbering.issuance_records(id,organization_id,project_id,proposal_id,building_id,number_id,route_id,
            number_text,route_name,authority_name,decision_ref,certificate_ref,effective_date,template_revision,recorded_by)
          SELECT $3,$1,$2,p.id,b.id,n.id,r.id,n.number_text,r.name,$6,$7,$8,$9,$10,$11
          FROM house_numbering.proposals p JOIN house_numbering.proposal_items i
            ON (i.organization_id,i.project_id,i.proposal_id)=(p.organization_id,p.project_id,p.id)
          JOIN house_numbering.address_numbers n ON (n.organization_id,n.project_id,n.id)=(i.organization_id,i.project_id,i.source_number_id)
          JOIN house_numbering.buildings b ON (b.organization_id,b.project_id,b.id)=(n.organization_id,n.project_id,n.building_id)
          JOIN house_numbering.routes r ON (r.organization_id,r.project_id,r.id)=(n.organization_id,n.project_id,n.route_id)
          WHERE p.organization_id=$1 AND p.project_id=$2 AND p.id=$4 AND n.id=$5
            AND p.status='approved' AND n.kind='approved' AND n.retired_at IS NULL
            AND b.state='active' AND b.eligibility='eligible' AND b.id=i.building_id
            AND r.status='confirmed' AND b.route_id=r.id
          ON CONFLICT DO NOTHING RETURNING *
        ) SELECT to_jsonb(inserted) FROM inserted"
    ).bind(edit.organization_id()).bind(edit.project_id()).bind(id).bind(input.proposal_id).bind(input.number_id)
        .bind(input.authority_name).bind(input.decision_ref).bind(input.certificate_ref).bind(input.effective_date)
        .bind(input.template_revision).bind(actor.id).fetch_optional(&mut **edit.transaction()).await.map_err(db_error)?;
    if record.is_none(){return Err(AppError::conflict("ISSUANCE_NOT_RECORDABLE","number not approved in this proposal or certificate/number already registered"));}
    insert_event(edit,actor.id,id,1,"recorded",input.decision_ref).await?;
    append_audit(edit,actor,"house_numbering.issuance.record","house_numbering_issuance",id,json!({"proposal_id":input.proposal_id,"number_id":input.number_id})).await?;
    get_issuance(edit,id).await
}

async fn insert_event(edit:&mut ProjectRead<'_>,actor:Uuid,id:Uuid,revision:i64,state:&str,evidence:&str)->Result<(),AppError>{
    sqlx::query("INSERT INTO house_numbering.issuance_events(organization_id,project_id,issuance_id,revision,state,evidence_ref,actor_id) VALUES($1,$2,$3,$4,$5,$6,$7)")
        .bind(edit.organization_id()).bind(edit.project_id()).bind(id).bind(revision).bind(state).bind(evidence).bind(actor)
        .execute(&mut **edit.transaction()).await.map_err(db_error)?;
    Ok(())
}

pub async fn get_issuance(read:&mut ProjectRead<'_>,id:Uuid)->Result<Value,AppError>{
    sqlx::query_scalar(
        "SELECT jsonb_build_object('record',to_jsonb(r),'history',
          (SELECT coalesce(jsonb_agg(to_jsonb(e) ORDER BY e.revision),'[]'::jsonb)
           FROM house_numbering.issuance_events e WHERE (e.organization_id,e.project_id,e.issuance_id)=(r.organization_id,r.project_id,r.id)))
         FROM house_numbering.issuance_records r WHERE r.organization_id=$1 AND r.project_id=$2 AND r.id=$3"
    ).bind(read.organization_id()).bind(read.project_id()).bind(id).fetch_optional(&mut **read.transaction()).await.map_err(db_error)?
        .ok_or_else(||AppError::not_found("issuance record not found"))
}

pub async fn list_issuances(read:&mut ProjectRead<'_>,building:Uuid)->Result<Value,AppError>{
    let items:Vec<Value>=sqlx::query_scalar(
        "SELECT to_jsonb(r)||jsonb_build_object('state',e.state,'revision',e.revision)
         FROM house_numbering.issuance_records r JOIN LATERAL (
           SELECT state,revision FROM house_numbering.issuance_events e WHERE (e.organization_id,e.project_id,e.issuance_id)=(r.organization_id,r.project_id,r.id)
           ORDER BY revision DESC LIMIT 1) e ON true
         WHERE r.organization_id=$1 AND r.project_id=$2 AND r.building_id=$3 ORDER BY r.recorded_at DESC,r.id"
    ).bind(read.organization_id()).bind(read.project_id()).bind(building).fetch_all(&mut **read.transaction()).await.map_err(db_error)?;
    Ok(json!({"items":items}))
}

pub async fn transition_issuance(edit:&mut ProjectRead<'_>,actor:AuditActor<'_>,id:Uuid,expected:i64,state:&str,evidence:&str)->Result<Value,AppError>{
    let current:Option<(i64,String,bool)>=sqlx::query_as(
        "SELECT e.revision,e.state,coalesce(n.kind='approved' AND n.retired_at IS NULL AND b.state='active' AND b.eligibility='eligible'
           AND n.building_id=b.id AND n.route_id=r.route_id AND b.route_id=r.route_id AND n.number_text=r.number_text
           AND EXISTS(SELECT 1 FROM house_numbering.routes route WHERE (route.organization_id,route.project_id,route.id)=(r.organization_id,r.project_id,r.route_id) AND route.status='confirmed' AND route.name=r.route_name),false)
         FROM house_numbering.issuance_records r JOIN LATERAL (
           SELECT revision,state FROM house_numbering.issuance_events e WHERE (e.organization_id,e.project_id,e.issuance_id)=(r.organization_id,r.project_id,r.id)
           ORDER BY revision DESC LIMIT 1) e ON true
         JOIN house_numbering.address_numbers n ON (n.organization_id,n.project_id,n.id)=(r.organization_id,r.project_id,r.number_id)
         JOIN house_numbering.buildings b ON (b.organization_id,b.project_id,b.id)=(r.organization_id,r.project_id,r.building_id)
         WHERE r.organization_id=$1 AND r.project_id=$2 AND r.id=$3"
    ).bind(edit.organization_id()).bind(edit.project_id()).bind(id).fetch_optional(&mut **edit.transaction()).await.map_err(db_error)?;
    let (revision,previous,active)=current.ok_or_else(||AppError::not_found("issuance record not found"))?;
    if revision!=expected{return Err(AppError::conflict("STALE_ISSUANCE","issuance revision changed"));}
    if !active && state!="void"{return Err(AppError::conflict("ISSUANCE_INACTIVE","approved number or building is inactive"));}
    let next=revision.checked_add(1).ok_or_else(||AppError::internal("issuance revision overflow"))?;
    insert_event(edit,actor.id,id,next,state,evidence).await?;
    append_audit(edit,actor,"house_numbering.issuance.transition","house_numbering_issuance",id,json!({"previous":previous,"state":state,"revision":next})).await?;
    get_issuance(edit,id).await
}

/// Authenticated lookup target for a QR containing only the random issuance UUID.
/// A voided or no-longer-approved record is not a current address.
pub async fn lookup_issuance(read:&mut ProjectRead<'_>,id:Uuid)->Result<Value,AppError>{
    sqlx::query_scalar(
        "SELECT jsonb_build_object('id',r.id,'number_text',r.number_text,'route_name',r.route_name,
           'effective_date',r.effective_date,'plate_state',e.state,'status','recorded')
         FROM house_numbering.issuance_records r JOIN LATERAL (
           SELECT state FROM house_numbering.issuance_events e WHERE (e.organization_id,e.project_id,e.issuance_id)=(r.organization_id,r.project_id,r.id)
           ORDER BY revision DESC LIMIT 1) e ON true
         JOIN house_numbering.address_numbers n ON (n.organization_id,n.project_id,n.id)=(r.organization_id,r.project_id,r.number_id)
         JOIN house_numbering.buildings b ON (b.organization_id,b.project_id,b.id)=(r.organization_id,r.project_id,r.building_id)
         JOIN house_numbering.routes route ON (route.organization_id,route.project_id,route.id)=(n.organization_id,n.project_id,n.route_id)
         WHERE r.organization_id=$1 AND r.project_id=$2 AND r.id=$3 AND e.state<>'void'
           AND n.kind='approved' AND n.retired_at IS NULL AND n.number_text=r.number_text AND b.state='active'
           AND b.eligibility='eligible' AND b.id=n.building_id AND b.route_id=n.route_id AND n.route_id=r.route_id AND route.name=r.route_name AND route.status='confirmed'"
    ).bind(read.organization_id()).bind(read.project_id()).bind(id).fetch_optional(&mut **read.transaction()).await.map_err(db_error)?
        .ok_or_else(||AppError::not_found("current registered address not found"))
}

pub struct ReplacementInput<'a>{
    pub expected_revision:i64,pub authority_name:&'a str,pub decision_ref:&'a str,
    pub certificate_ref:&'a str,pub effective_date:chrono::NaiveDate,pub template_revision:&'a str,
    pub reason:&'a str,
}

/// Atomically record an externally issued replacement of the same approved address.
/// The predecessor's history remains intact. No new address number is allocated.
pub async fn replace_issuance(edit:&mut ProjectRead<'_>,actor:AuditActor<'_>,old_id:Uuid,input:ReplacementInput<'_>)->Result<Value,AppError>{
    if input.effective_date>chrono::Utc::now().date_naive(){
        return Err(AppError::validation("FUTURE_ISSUANCE_DATE","register only an already issued certificate"));
    }
    let old:Option<(Uuid,i64,String)>=sqlx::query_as(
        "SELECT r.proposal_id,e.revision,e.state FROM house_numbering.issuance_records r
         JOIN LATERAL (SELECT revision,state FROM house_numbering.issuance_events e
           WHERE (e.organization_id,e.project_id,e.issuance_id)=(r.organization_id,r.project_id,r.id) ORDER BY revision DESC LIMIT 1) e ON true
         WHERE r.organization_id=$1 AND r.project_id=$2 AND r.id=$3"
    ).bind(edit.organization_id()).bind(edit.project_id()).bind(old_id).fetch_optional(&mut **edit.transaction()).await.map_err(db_error)?;
    let (proposal,revision,state)=old.ok_or_else(||AppError::not_found("issuance record not found"))?;
    if revision!=input.expected_revision{return Err(AppError::conflict("STALE_ISSUANCE","issuance revision changed"));}
    verify_approved_snapshot(edit,proposal).await?;
    if state!="void"{
        transition_issuance(edit,actor,old_id,revision,"void",input.reason).await?;
    }
    let id=Uuid::new_v4();
    let record:Option<Uuid>=sqlx::query_scalar(
        "INSERT INTO house_numbering.issuance_records(id,organization_id,project_id,proposal_id,building_id,number_id,route_id,
         number_text,route_name,authority_name,decision_ref,certificate_ref,effective_date,template_revision,recorded_by,replaces_issuance_id,replacement_reason)
         SELECT $4,$1,$2,r.proposal_id,r.building_id,r.number_id,r.route_id,r.number_text,r.route_name,$5,$6,$7,$8,$9,$10,r.id,$11
         FROM house_numbering.issuance_records r WHERE r.organization_id=$1 AND r.project_id=$2 AND r.id=$3
         ON CONFLICT DO NOTHING RETURNING id"
    ).bind(edit.organization_id()).bind(edit.project_id()).bind(old_id).bind(id).bind(input.authority_name)
        .bind(input.decision_ref).bind(input.certificate_ref).bind(input.effective_date).bind(input.template_revision)
        .bind(actor.id).bind(input.reason).fetch_optional(&mut **edit.transaction()).await.map_err(db_error)?;
    if record.is_none(){return Err(AppError::conflict("ISSUANCE_REPLACEMENT_CONFLICT","certificate or successor already registered"));}
    insert_event(edit,actor.id,id,1,"recorded",input.decision_ref).await?;
    append_audit(edit,actor,"house_numbering.issuance.replace","house_numbering_issuance",id,json!({"replaces_issuance_id":old_id})).await?;
    get_issuance(edit,id).await
}

/// Building-based lifecycle KPI, deliberately not a count of certificates or
/// events: replacement history must never inflate the completed population.
pub async fn lifecycle_kpi(read:&mut ProjectRead<'_>)->Result<Value,AppError>{
    let breakdown=lifecycle_breakdown(read).await?;
    let mut total=breakdown["total"].clone();
    // Preserve the older endpoint response contract. New dimensions use breakdown.
    if let Some(fields)=total.as_object_mut(){fields.remove("surveyed_buildings");fields.remove("awaiting_review_buildings");}
    Ok(total)
}

/// Each dimension is a separate partition of the active-building population.
/// Area IDs are direct assignment, not a recursive rollup. Null group retained.
pub async fn lifecycle_breakdown(read:&mut ProjectRead<'_>)->Result<Value,AppError>{
    sqlx::query_scalar(
        "WITH buildings AS (
           SELECT id,eligibility,route_id,area_id FROM house_numbering.buildings
           WHERE organization_id=$1 AND project_id=$2 AND state='active'
         ), approved AS (
           SELECT DISTINCT n.building_id FROM house_numbering.address_numbers n
           JOIN buildings b ON b.id=n.building_id AND b.route_id=n.route_id
           JOIN house_numbering.routes r ON (r.organization_id,r.project_id,r.id)=(n.organization_id,n.project_id,n.route_id)
           WHERE n.organization_id=$1 AND n.project_id=$2 AND n.kind='approved' AND n.retired_at IS NULL
             AND b.eligibility='eligible' AND r.status='confirmed'
         ), latest AS (
           SELECT DISTINCT ON (issuance_id) issuance_id,state FROM house_numbering.issuance_events
           WHERE organization_id=$1 AND project_id=$2 ORDER BY issuance_id,revision DESC
         ), current_issuance AS (
           SELECT r.building_id,e.state FROM house_numbering.issuance_records r
           JOIN latest e ON e.issuance_id=r.id AND e.state<>'void'
           JOIN house_numbering.address_numbers n ON (n.organization_id,n.project_id,n.id)=(r.organization_id,r.project_id,r.number_id)
           JOIN buildings b ON b.id=r.building_id AND b.id=n.building_id AND b.route_id=n.route_id
           JOIN house_numbering.routes route ON (route.organization_id,route.project_id,route.id)=(n.organization_id,n.project_id,n.route_id)
           WHERE r.organization_id=$1 AND r.project_id=$2 AND b.eligibility='eligible'
             AND n.kind='approved' AND n.retired_at IS NULL AND n.number_text=r.number_text
             AND n.route_id=r.route_id AND route.name=r.route_name AND route.status='confirmed'
         ), pending AS (
           SELECT DISTINCT c.building_id FROM house_numbering.reconcile_cases c JOIN buildings b ON b.id=c.building_id
           WHERE c.organization_id=$1 AND c.project_id=$2 AND c.status='open' AND c.case_class<>'matched'
         ), survey AS (
           SELECT s.building_id,bool_or(s.state='accepted') AS accepted,bool_or(s.state='submitted') AS submitted
           FROM house_numbering.surveys s JOIN buildings b ON b.id=s.building_id
           WHERE s.organization_id=$1 AND s.project_id=$2 GROUP BY s.building_id
         ), flags AS (
           SELECT b.*,a.building_id IS NOT NULL AS approved,
             coalesce(i.registered,false) AS registered,coalesce(i.installed,false) AS installed,
             p.building_id IS NOT NULL AS unresolved,coalesce(s.accepted,false) AS surveyed,
             coalesce(s.submitted,false) AND NOT coalesce(s.accepted,false) AS awaiting_review
           FROM buildings b LEFT JOIN approved a ON a.building_id=b.id
           LEFT JOIN (SELECT building_id,true AS registered,bool_or(state='installed') AS installed FROM current_issuance GROUP BY building_id) i ON i.building_id=b.id
           LEFT JOIN pending p ON p.building_id=b.id LEFT JOIN survey s ON s.building_id=b.id
         ), grouped AS (
           SELECT CASE WHEN GROUPING(area_id)=0 THEN 'area' WHEN GROUPING(route_id)=0 THEN 'route' ELSE 'total' END AS dimension,
             CASE WHEN GROUPING(area_id)=0 THEN area_id WHEN GROUPING(route_id)=0 THEN route_id ELSE NULL END AS group_id,
             jsonb_build_object('active_buildings',count(*),'eligible',count(*) FILTER(WHERE eligibility='eligible'),
               'unknown',count(*) FILTER(WHERE eligibility='unknown'),'ineligible',count(*) FILTER(WHERE eligibility='ineligible'),
               'approved_buildings',count(*) FILTER(WHERE approved),'registered_buildings',count(*) FILTER(WHERE registered),
               'installed_buildings',count(*) FILTER(WHERE installed),'unresolved_buildings',count(*) FILTER(WHERE unresolved),
               'surveyed_buildings',count(*) FILTER(WHERE surveyed),'awaiting_review_buildings',count(*) FILTER(WHERE awaiting_review)) AS metrics
           FROM flags GROUP BY GROUPING SETS ((),(area_id),(route_id))
         ), labeled AS (
           SELECT g.dimension,g.group_id,g.metrics,CASE WHEN g.dimension='area' THEN coalesce(a.name,'Chưa gán địa bàn')
             WHEN g.dimension='route' THEN coalesce(r.name,'Chưa gán tuyến') ELSE 'Tổng' END AS name
           FROM grouped g LEFT JOIN house_numbering.areas a ON g.dimension='area' AND a.organization_id=$1 AND a.project_id=$2 AND a.id=g.group_id
           LEFT JOIN house_numbering.routes r ON g.dimension='route' AND r.organization_id=$1 AND r.project_id=$2 AND r.id=g.group_id
         )
         SELECT jsonb_build_object('total',(SELECT metrics FROM labeled WHERE dimension='total'),
           'areas',coalesce((SELECT jsonb_agg(jsonb_build_object('id',group_id,'name',name)||metrics ORDER BY name,group_id) FROM labeled WHERE dimension='area'),'[]'::jsonb),
           'routes',coalesce((SELECT jsonb_agg(jsonb_build_object('id',group_id,'name',name)||metrics ORDER BY name,group_id) FROM labeled WHERE dimension='route'),'[]'::jsonb))"
    ).bind(read.organization_id()).bind(read.project_id()).fetch_one(&mut **read.transaction()).await.map_err(db_error)
}

/// Recorded survey cohort (server submission timestamps, UTC). Not a measure of
/// actual hours worked or field capture date. Actor totals may overlap buildings.
pub async fn survey_staff_kpi(read:&mut ProjectRead<'_>,start:chrono::NaiveDate,end:chrono::NaiveDate)->Result<Value,AppError>{
    sqlx::query_scalar(
        "WITH cohort AS (
           SELECT recorded_by_actor_id AS actor_id,building_id,state
           FROM house_numbering.surveys WHERE organization_id=$1 AND project_id=$2 AND state IN ('submitted','accepted','rejected')
             AND created_at>=($3::date::timestamp AT TIME ZONE 'UTC')
             AND created_at<($4::date::timestamp AT TIME ZONE 'UTC')
         ), per_actor AS (
           SELECT actor_id,count(*) AS submitted_forms,
             count(*) FILTER(WHERE state='accepted') AS accepted_forms,
             count(*) FILTER(WHERE state='rejected') AS rejected_forms,
             count(*) FILTER(WHERE state='submitted') AS pending_forms,
             count(DISTINCT building_id) AS observed_buildings,
             count(DISTINCT building_id) FILTER(WHERE state='accepted') AS accepted_buildings
           FROM cohort GROUP BY actor_id
         ), labeled AS (
           SELECT p.*,coalesce(u.display_name,p.actor_id::text) AS display_name
           FROM per_actor p LEFT JOIN org.organization_memberships m ON m.organization_id=$1 AND m.user_id=p.actor_id AND m.status='active'
           LEFT JOIN identity.users u ON u.id=m.user_id
         )
         SELECT jsonb_build_object('start_date',$3::date,'end_date_exclusive',$4::date,
           'distinct_observed_buildings',(SELECT count(DISTINCT building_id) FROM cohort),
           'distinct_accepted_buildings',(SELECT count(DISTINCT building_id) FROM cohort WHERE state='accepted'),
           'items',coalesce((SELECT jsonb_agg(to_jsonb(labeled) ORDER BY accepted_buildings DESC,display_name,actor_id) FROM labeled),'[]'::jsonb))"
    ).bind(read.organization_id()).bind(read.project_id()).bind(start).bind(end).fetch_one(&mut **read.transaction()).await.map_err(db_error)
}
