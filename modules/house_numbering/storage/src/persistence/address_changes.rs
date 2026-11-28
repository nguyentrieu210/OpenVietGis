//! Record external address retirement and successor decisions. No allocation bypass.
use crate::catalog::ProjectRead;
use ovg_core::AppError;
use serde_json::{json,Value};
use uuid::Uuid;
use super::{append_audit,db_error,AuditActor,transition_issuance,proposals::verify_approved_snapshot};

pub async fn retire_address(edit:&mut ProjectRead<'_>,actor:AuditActor<'_>,id:Uuid,expected:i64,date:chrono::NaiveDate,decision:&str,reason:&str)->Result<Value,AppError>{
    if date>chrono::Utc::now().date_naive(){return Err(AppError::validation("FUTURE_DECISION","record only effective retirement decisions"));}
    let current:Option<(i64,String,bool)>=sqlx::query_as(
        "SELECT revision,kind,retired_at IS NOT NULL FROM house_numbering.address_numbers WHERE organization_id=$1 AND project_id=$2 AND id=$3 FOR UPDATE"
    ).bind(edit.organization_id()).bind(edit.project_id()).bind(id).fetch_optional(&mut **edit.transaction()).await.map_err(db_error)?;
    let (revision,kind,retired)=current.ok_or_else(||AppError::not_found("address number not found"))?;
    if revision!=expected||kind!="approved"||retired{return Err(AppError::conflict("STALE_ADDRESS","active approved number and matching revision required"));}
    let issued:Vec<(Uuid,i64)>=sqlx::query_as(
      "SELECT r.id,e.revision FROM house_numbering.issuance_records r JOIN LATERAL(
        SELECT revision,state FROM house_numbering.issuance_events e WHERE (e.organization_id,e.project_id,e.issuance_id)=(r.organization_id,r.project_id,r.id)
        ORDER BY revision DESC LIMIT 1) e ON e.state<>'void'
       WHERE r.organization_id=$1 AND r.project_id=$2 AND r.number_id=$3 ORDER BY r.id"
    ).bind(edit.organization_id()).bind(edit.project_id()).bind(id).fetch_all(&mut **edit.transaction()).await.map_err(db_error)?;
    for (iid,rev) in issued{transition_issuance(edit,actor,iid,rev,"void",reason).await?;}
    sqlx::query("INSERT INTO house_numbering.address_retirements(organization_id,project_id,number_id,effective_date,decision_ref,reason,actor_id) VALUES($1,$2,$3,$4,$5,$6,$7)")
      .bind(edit.organization_id()).bind(edit.project_id()).bind(id).bind(date).bind(decision).bind(reason).bind(actor.id)
      .execute(&mut **edit.transaction()).await.map_err(db_error)?;
    let result:Value=sqlx::query_scalar(
      "WITH changed AS (UPDATE house_numbering.address_numbers SET retired_at=now(),revision=revision+1,updated_by_actor_id=$4,updated_at=now()
        WHERE organization_id=$1 AND project_id=$2 AND id=$3 AND retired_at IS NULL AND kind='approved' AND revision=$5 RETURNING *)
       SELECT to_jsonb(changed) FROM changed"
    ).bind(edit.organization_id()).bind(edit.project_id()).bind(id).bind(actor.id).bind(expected)
      .fetch_optional(&mut **edit.transaction()).await.map_err(db_error)?.ok_or_else(||AppError::conflict("STALE_ADDRESS","address changed"))?;
    append_audit(edit,actor,"house_numbering.address.retire","house_numbering_address",id,json!({"effective_date":date,"revision":result["revision"]})).await?;
    Ok(result)
}

pub async fn link_successor(edit:&mut ProjectRead<'_>,actor:AuditActor<'_>,old:Uuid,new:Uuid,proposal:Uuid,date:chrono::NaiveDate,decision:&str)->Result<Value,AppError>{
    if date>chrono::Utc::now().date_naive(){return Err(AppError::validation("FUTURE_DECISION","record only effective decisions"));}
    verify_approved_snapshot(edit,proposal).await?;
    let result:Option<Value>=sqlx::query_scalar(
      "WITH inserted AS (INSERT INTO house_numbering.address_successors(organization_id,project_id,old_number_id,new_number_id,effective_date,decision_ref,actor_id)
        SELECT $1,$2,$3,$4,$5,$6,$7 WHERE EXISTS(SELECT 1 FROM house_numbering.proposal_items i WHERE i.organization_id=$1 AND i.project_id=$2 AND i.proposal_id=$8 AND i.source_number_id=$4)
        ON CONFLICT DO NOTHING RETURNING *) SELECT to_jsonb(inserted) FROM inserted"
    ).bind(edit.organization_id()).bind(edit.project_id()).bind(old).bind(new).bind(date).bind(decision).bind(actor.id).bind(proposal)
     .fetch_optional(&mut **edit.transaction()).await.map_err(db_error)?;
    let result=result.ok_or_else(||AppError::conflict("ADDRESS_SUCCESSOR_CONFLICT","successor must be in verified proposal and not already linked"))?;
    append_audit(edit,actor,"house_numbering.address.successor","house_numbering_address",old,json!({"new_number_id":new})).await?;
    Ok(result)
}

pub async fn address_history(read:&mut ProjectRead<'_>,building:Uuid)->Result<Value,AppError>{
    let items:Vec<Value>=sqlx::query_scalar(
      "SELECT jsonb_build_object('id',n.id,'number_text',n.number_text,'route_id',n.route_id,'route_name',r.name,
        'revision',n.revision,'retired_at',n.retired_at,'retirement',to_jsonb(t),'successor',to_jsonb(s))
       FROM house_numbering.address_numbers n JOIN house_numbering.routes r ON (r.organization_id,r.project_id,r.id)=(n.organization_id,n.project_id,n.route_id)
       LEFT JOIN house_numbering.address_retirements t ON (t.organization_id,t.project_id,t.number_id)=(n.organization_id,n.project_id,n.id)
       LEFT JOIN house_numbering.address_successors s ON (s.organization_id,s.project_id,s.old_number_id)=(n.organization_id,n.project_id,n.id)
       WHERE n.organization_id=$1 AND n.project_id=$2 AND n.building_id=$3 AND n.kind='approved' ORDER BY n.created_at,n.id"
    ).bind(read.organization_id()).bind(read.project_id()).bind(building).fetch_all(&mut **read.transaction()).await.map_err(db_error)?;
    Ok(json!({"items":items}))
}
