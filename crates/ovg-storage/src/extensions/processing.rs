use super::{db_error, runtime_v2::ActivePin};
use crate::catalog::ProjectRead;
use ovg_core::AppError;
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;
pub struct ProposalBinding<'a> {
    pub release: &'a str,
    pub lock: &'a str,
    pub grant_revision: i64,
    pub epoch: i64,
}
pub async fn propose(
    read: &mut ProjectRead<'_>,
    pin: &ActivePin,
    id: Uuid,
    b: ProposalBinding<'_>,
    plan: &Value,
    digest: &str,
) -> Result<Value, AppError> {
    if read.is_gis_scoped() || read.actor_type() != "human" || id.is_nil() {
        return Err(AppError::forbidden("invalid processing scope"));
    }
    let row=sqlx::query("INSERT INTO extensions.plugin_processing_proposals(id,organization_id,project_id,actor_id,manifest_id,activation_id,activation_revision,release_digest,lock_digest,grant_revision,catalog_epoch,plan,plan_digest)
      VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13) ON CONFLICT(id) DO NOTHING RETURNING id")
        .bind(id).bind(read.organization_id()).bind(read.project_id()).bind(read.actor_id()).bind(pin.manifest_id).bind(pin.activation_id).bind(pin.revision)
        .bind(b.release).bind(b.lock).bind(b.grant_revision).bind(b.epoch).bind(plan).bind(digest)
        .fetch_optional(&mut **read.transaction()).await.map_err(db_error)?;
    if row.is_none() {
        let p = proposal(read, id, pin, b, digest).await?;
        if p != *plan {
            return Err(AppError::conflict(
                "PLUGIN_PROPOSAL_CONFLICT",
                "proposal changed",
            ));
        }
    }
    Ok(
        json!({"proposal_id":id,"plan_digest":digest,"plan":plan,"requires_human_confirmation":true}),
    )
}
pub async fn proposal(
    read: &mut ProjectRead<'_>,
    id: Uuid,
    pin: &ActivePin,
    b: ProposalBinding<'_>,
    digest: &str,
) -> Result<Value, AppError> {
    let row=sqlx::query("SELECT plan,plan_digest,manifest_id,activation_id,activation_revision,release_digest,lock_digest,grant_revision,catalog_epoch,
      created_at>clock_timestamp()-interval '5 minutes' AS fresh FROM extensions.plugin_processing_proposals WHERE id=$1 AND organization_id=$2 AND project_id=$3 AND actor_id=$4")
      .bind(id).bind(read.organization_id()).bind(read.project_id()).bind(read.actor_id()).fetch_optional(&mut **read.transaction()).await.map_err(db_error)?
      .ok_or_else(||AppError::not_found("processing proposal not found"))?;
    if !row.try_get::<bool, _>("fresh").map_err(db_error)?
        || row.try_get::<Uuid, _>("manifest_id").map_err(db_error)? != pin.manifest_id
        || row.try_get::<Uuid, _>("activation_id").map_err(db_error)? != pin.activation_id
        || row
            .try_get::<i64, _>("activation_revision")
            .map_err(db_error)?
            != pin.revision
        || row
            .try_get::<String, _>("release_digest")
            .map_err(db_error)?
            != b.release
        || row.try_get::<String, _>("lock_digest").map_err(db_error)? != b.lock
        || row.try_get::<i64, _>("grant_revision").map_err(db_error)? != b.grant_revision
        || row.try_get::<i64, _>("catalog_epoch").map_err(db_error)? != b.epoch
        || row.try_get::<String, _>("plan_digest").map_err(db_error)? != digest
    {
        return Err(AppError::conflict(
            "STALE_PLUGIN_PROPOSAL",
            "proposal expired or authorization changed",
        ));
    }
    row.try_get("plan").map_err(db_error)
}
pub async fn approved_job(read: &mut ProjectRead<'_>, id: Uuid) -> Result<Option<Uuid>, AppError> {
    sqlx::query_scalar("SELECT job_id FROM extensions.plugin_processing_approvals WHERE organization_id=$1 AND project_id=$2 AND proposal_id=$3 AND actor_id=$4")
      .bind(read.organization_id()).bind(read.project_id()).bind(id).bind(read.actor_id())
      .fetch_optional(&mut **read.transaction()).await.map_err(db_error)
}
pub async fn approve(read: &mut ProjectRead<'_>, id: Uuid, job: Uuid) -> Result<(), AppError> {
    if !read.is_project_manager() || read.is_gis_scoped() || read.actor_type() != "human" {
        return Err(AppError::forbidden("human manager required"));
    }
    sqlx::query("INSERT INTO extensions.plugin_processing_approvals(organization_id,project_id,proposal_id,actor_id,job_id) VALUES($1,$2,$3,$4,$5)")
      .bind(read.organization_id()).bind(read.project_id()).bind(id).bind(read.actor_id()).bind(job)
      .execute(&mut **read.transaction()).await.map_err(db_error)?;
    Ok(())
}
