//! Module-specific allowlists constrain, never elevate, an already verified Core role.
use super::{append_project_audit, db_error};
use crate::catalog::ProjectRead;
use ovg_core::AppError;
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

pub async fn policy(read: &mut ProjectRead<'_>, key: &str, actor_type: &str, actor: Uuid) -> Result<Value, AppError> {
    let row=sqlx::query("SELECT permissions,revision FROM extensions.module_permission_policies
      WHERE organization_id=$1 AND project_id=$2 AND extension_key=$3 AND actor_type=$4 AND actor_id=$5")
        .bind(read.organization_id()).bind(read.project_id()).bind(key).bind(actor_type).bind(actor)
        .fetch_optional(&mut **read.transaction()).await.map_err(db_error)?;
    match row {
        None => Ok(json!({"permissions":null,"revision":0})),
        Some(row) => Ok(json!({"permissions":row.try_get::<Option<Vec<String>>,_>("permissions").map_err(db_error)?,"revision":row.try_get::<i64,_>("revision").map_err(db_error)?})),
    }
}

pub async fn allows(read: &mut ProjectRead<'_>, key:&str, permission:&str)->Result<bool,AppError>{
    let actor=read.actor_id();let actor_type=read.actor_type();
    let row=policy(read,key,actor_type,actor).await?;
    Ok(row["permissions"].is_null() || row["permissions"].as_array().is_some_and(|a|a.iter().any(|v|v.as_str()==Some(permission))))
}

/// The caller proves project.manage, validates the active manifest and handles CSRF.
/// None restores legacy role inheritance; an empty array explicitly denies every module permission.
pub async fn replace_policy(edit:&mut ProjectRead<'_>, key:&str, target_type:&str, target:Uuid,
    permissions:Option<&[String]>, expected_revision:i64, actor_display:&str)->Result<Value,AppError>{
    let old=policy(edit,key,target_type,target).await?;
    if old["revision"].as_i64()!=Some(expected_revision){return Err(AppError::conflict("STALE_MODULE_PERMISSION_POLICY","module permission policy changed"));}
    let actor=edit.actor_id();let org=edit.organization_id();let project=edit.project_id();
    // Preserve a monotonic revision when restoring inheritance; never reset a row to revision zero.
    let changed=sqlx::query("INSERT INTO extensions.module_permission_policies(organization_id,project_id,extension_key,actor_type,actor_id,permissions,updated_by_actor_id)
        SELECT $1,$2,$3,$4,$5,$6,$7 WHERE $8=0 OR EXISTS (SELECT 1 FROM extensions.module_permission_policies WHERE organization_id=$1 AND project_id=$2 AND extension_key=$3 AND actor_type=$4 AND actor_id=$5 AND revision=$8) ON CONFLICT(organization_id,project_id,extension_key,actor_type,actor_id)
        DO UPDATE SET permissions=EXCLUDED.permissions,revision=extensions.module_permission_policies.revision+1,updated_by_actor_id=EXCLUDED.updated_by_actor_id,updated_at=now() WHERE extensions.module_permission_policies.revision=$8")
        .bind(org).bind(project).bind(key).bind(target_type).bind(target).bind(permissions).bind(actor).bind(expected_revision)
        .execute(&mut **edit.transaction()).await.map_err(db_error)?.rows_affected();
    if changed!=1{return Err(AppError::conflict("STALE_MODULE_PERMISSION_POLICY","module permission policy changed"));}
    let new=policy(edit,key,target_type,target).await?;
    append_project_audit(edit,actor,actor_display,"extension.permissions.replace","project",project,
        json!({"extension_key":key,"actor_type":target_type,"actor_id":target,"previous":old,"current":new}),chrono::Utc::now()).await?;
    Ok(new)
}
