//! Stable semantic bindings. Core owns identity and version checks; modules own role semantics.
use super::{append_project_audit, db_error};
use crate::catalog::ProjectRead;
use ovg_core::AppError;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::Row;
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BindingInput {
    pub source_id: Uuid,
    pub role: String,
    pub schema_revision: String,
}

fn valid_role(role: &str) -> bool {
    let mut bytes=role.bytes();
    role.len()<=64 && bytes.next().is_some_and(|b|b.is_ascii_lowercase())
        && bytes.all(|b|b.is_ascii_lowercase()||b.is_ascii_digit()||b==b'_')
}

pub async fn get(read:&mut ProjectRead<'_>,key:&str)->Result<Value,AppError>{
    let revision:Option<i64>=sqlx::query_scalar("SELECT revision FROM extensions.module_binding_sets WHERE organization_id=$1 AND project_id=$2 AND extension_key=$3")
        .bind(read.organization_id()).bind(read.project_id()).bind(key).fetch_optional(&mut **read.transaction()).await.map_err(db_error)?;
    let rows=sqlx::query("SELECT b.source_id,b.role,b.schema_revision,s.schema_revision AS current_schema_revision,s.lifecycle,
      EXISTS(SELECT 1 FROM spatial.layers l WHERE l.organization_id=b.organization_id AND l.project_id=b.project_id AND l.source_id=b.source_id AND l.lifecycle='active') AS has_layer
      FROM extensions.module_layer_bindings b JOIN spatial.data_sources s
      ON (s.organization_id,s.project_id,s.id)=(b.organization_id,b.project_id,b.source_id)
      WHERE b.organization_id=$1 AND b.project_id=$2 AND b.extension_key=$3 ORDER BY b.role,b.source_id")
        .bind(read.organization_id()).bind(read.project_id()).bind(key).fetch_all(&mut **read.transaction()).await.map_err(db_error)?;
    let mut items=Vec::with_capacity(rows.len());
    for row in rows{
        let expected:String=row.try_get("schema_revision").map_err(db_error)?;
        let current:String=row.try_get("current_schema_revision").map_err(db_error)?;
        let active=row.try_get::<String,_>("lifecycle").map_err(db_error)?=="active" && row.try_get::<bool,_>("has_layer").map_err(db_error)?;
        let status=if !active{"unavailable"}else if expected!=current{"schema_changed"}else{"ready"};
        items.push(json!({"source_id":row.try_get::<Uuid,_>("source_id").map_err(db_error)?,"role":row.try_get::<String,_>("role").map_err(db_error)?,"schema_revision":expected,"current_schema_revision":current,"status":status}));
    }
    Ok(json!({"revision":revision.unwrap_or(0),"configured":revision.is_some(),"items":items}))
}

/// Read source contract inside the authorized project transaction. The runtime
/// validates its semantic role against these values before writing a binding.
pub async fn source_contract(edit:&mut ProjectRead<'_>,source:Uuid)->Result<(String,Vec<String>),AppError>{
    let geometry:Option<String>=sqlx::query_scalar("SELECT geometry_family FROM spatial.data_sources WHERE organization_id=$1 AND project_id=$2 AND id=$3 AND lifecycle='active' FOR SHARE")
        .bind(edit.organization_id()).bind(edit.project_id()).bind(source).fetch_optional(&mut **edit.transaction()).await.map_err(db_error)?;
    let geometry=geometry.ok_or_else(||AppError::conflict("STALE_BINDING_SOURCE","source missing or retired"))?;
    let fields=sqlx::query_scalar("SELECT field_id::text FROM spatial.source_fields WHERE organization_id=$1 AND project_id=$2 AND source_id=$3 ORDER BY ordinal")
        .bind(edit.organization_id()).bind(edit.project_id()).bind(source).fetch_all(&mut **edit.transaction()).await.map_err(db_error)?;
    Ok((geometry,fields))
}

pub async fn replace(edit:&mut ProjectRead<'_>,key:&str,expected_revision:i64,items:&[BindingInput],actor_display:&str)->Result<Value,AppError>{
    if items.len()>256||items.iter().map(|i|i.source_id).collect::<HashSet<_>>().len()!=items.len()||items.iter().any(|i|!valid_role(&i.role)||i.source_id.is_nil()||i.schema_revision.len()!=64||!i.schema_revision.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b))){
        return Err(AppError::validation("INVALID_LAYER_BINDINGS","bindings require unique sources, valid roles and schema revisions"));
    }
    let old=get(edit,key).await?;
    if old["revision"].as_i64()!=Some(expected_revision){return Err(AppError::conflict("STALE_LAYER_BINDINGS","module layer bindings changed"));}
    let org=edit.organization_id();let project=edit.project_id();let actor=edit.actor_id();
    let mut sorted:Vec<_>=items.iter().collect();sorted.sort_by_key(|i|i.source_id);
    for item in sorted{
        let schema:Option<String>=sqlx::query_scalar("SELECT s.schema_revision FROM spatial.data_sources s WHERE s.organization_id=$1 AND s.project_id=$2 AND s.id=$3 AND s.lifecycle='active' AND EXISTS(SELECT 1 FROM spatial.layers l WHERE l.organization_id=s.organization_id AND l.project_id=s.project_id AND l.source_id=s.id AND l.lifecycle='active') FOR SHARE OF s")
            .bind(org).bind(project).bind(item.source_id).fetch_optional(&mut **edit.transaction()).await.map_err(db_error)?;
        if schema.as_deref()!=Some(item.schema_revision.as_str()){return Err(AppError::conflict("STALE_BINDING_SOURCE","source missing, retired or schema changed"));}
    }
    let changed=sqlx::query("INSERT INTO extensions.module_binding_sets(organization_id,project_id,extension_key,revision,updated_by_actor_id) SELECT $1,$2,$3,1,$4 WHERE $5=0 OR EXISTS (SELECT 1 FROM extensions.module_binding_sets WHERE organization_id=$1 AND project_id=$2 AND extension_key=$3 AND revision=$5) ON CONFLICT(organization_id,project_id,extension_key) DO UPDATE SET revision=extensions.module_binding_sets.revision+1,updated_by_actor_id=EXCLUDED.updated_by_actor_id,updated_at=now() WHERE extensions.module_binding_sets.revision=$5")
        .bind(org).bind(project).bind(key).bind(actor).bind(expected_revision).execute(&mut **edit.transaction()).await.map_err(db_error)?.rows_affected();
    if changed!=1{return Err(AppError::conflict("STALE_LAYER_BINDINGS","module layer bindings changed"));}
    sqlx::query("DELETE FROM extensions.module_layer_bindings WHERE organization_id=$1 AND project_id=$2 AND extension_key=$3")
        .bind(org).bind(project).bind(key).execute(&mut **edit.transaction()).await.map_err(db_error)?;
    for i in items{
        sqlx::query("INSERT INTO extensions.module_layer_bindings(organization_id,project_id,extension_key,source_id,role,schema_revision) VALUES($1,$2,$3,$4,$5,$6)")
            .bind(org).bind(project).bind(key).bind(i.source_id).bind(&i.role).bind(&i.schema_revision).execute(&mut **edit.transaction()).await.map_err(db_error)?;
    }
    let new=get(edit,key).await?;
    append_project_audit(edit,actor,actor_display,"extension.bindings.replace","project",project,json!({"extension_key":key,"previous":old,"current":new}),chrono::Utc::now()).await?;
    Ok(new)
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn role_identifiers_are_bounded(){assert!(valid_role("survey_points"));for invalid in ["","1survey","Survey","a-b","a.b","a b"]{assert!(!valid_role(invalid));}assert!(!valid_role(&"a".repeat(65)));}
}
