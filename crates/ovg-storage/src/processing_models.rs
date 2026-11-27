//! E08 reusable processing-model repository over processing.models.

use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{json, Value};
use sqlx::postgres::PgRow;
use sqlx::Row;
use uuid::Uuid;

use crate::catalog::ProjectRead;
use crate::editing::EditScope;

#[derive(Debug)]
pub enum ModelStoreError {
    Quota,
    Conflict,
    Database,
}

fn db_error(error: sqlx::Error) -> ModelStoreError {
    tracing::error!(%error, "processing model database operation failed");
    ModelStoreError::Database
}

fn iso(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::AutoSi, false)
}

fn wire(row: &PgRow) -> Result<Value, ModelStoreError> {
    Ok(json!({
        "id": row.try_get::<Uuid, _>("id").map_err(db_error)?,
        "name": row.try_get::<String, _>("name").map_err(db_error)?,
        "graph": row.try_get::<Value, _>("graph").map_err(db_error)?,
        "revision": row.try_get::<i32, _>("revision").map_err(db_error)?,
        "created_at": iso(row.try_get::<DateTime<Utc>, _>("created_at").map_err(db_error)?),
        "updated_at": iso(row.try_get::<DateTime<Utc>, _>("updated_at").map_err(db_error)?),
    }))
}

struct ModelActor<'a> {
    actor_type: &'a str,
    actor_id: Uuid,
    actor_display: &'a str,
}

async fn audit(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    organization_id: Uuid,
    actor: ModelActor<'_>,
    resource_id: Uuid,
    action: &str,
    name: &str,
) -> Result<(), ModelStoreError> {
    sqlx::query(
        "INSERT INTO audit.audit_events (
            id, organization_id, actor_type, actor_id, actor_display_snapshot,
            action, resource_type, resource_id, outcome, metadata, created_at
         ) VALUES ($1,$2,$3,$4,$5,$6,'processing_model',$7,'success',$8,$9)",
    )
    .bind(Uuid::new_v4())
    .bind(organization_id)
    .bind(actor.actor_type)
    .bind(actor.actor_id)
    .bind(actor.actor_display)
    .bind(action)
    .bind(resource_id)
    .bind(json!({"name": name}))
    .bind(Utc::now())
    .execute(&mut **tx)
    .await
    .map_err(db_error)?;
    Ok(())
}

pub async fn list_models(read: &mut ProjectRead<'_>) -> Result<Value, ModelStoreError> {
    let org = read.organization_id();
    let proj = read.project_id();
    list_models_tx(read.transaction(), org, proj).await
}

pub async fn list_models_scoped(scope: &mut EditScope<'_>) -> Result<Value, ModelStoreError> {
    let org = scope.organization_id();
    let proj = scope.project_id();
    list_models_tx(scope.tx_mut(), org, proj).await
}

async fn list_models_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    organization_id: Uuid,
    project_id: Uuid,
) -> Result<Value, ModelStoreError> {
    let rows = sqlx::query(
        "SELECT id,name,graph,revision,created_at,updated_at
         FROM processing.models
         WHERE organization_id=$1 AND project_id=$2
         ORDER BY updated_at DESC,id",
    )
    .bind(organization_id)
    .bind(project_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(db_error)?;
    let items = rows.iter().map(wire).collect::<Result<Vec<_>, _>>()?;
    Ok(json!({"items": items}))
}

pub struct ModelWrite {
    pub name: String,
    pub graph: Value,
}

pub async fn create_model(
    edit: &mut ProjectRead<'_>,
    actor_id: Uuid,
    actor_display: &str,
    input: ModelWrite,
) -> Result<Value, ModelStoreError> {
    let org = edit.organization_id();
    let proj = edit.project_id();
    create_model_tx(
        edit.transaction(),
        org,
        proj,
        "human",
        actor_id,
        actor_display,
        input,
    )
    .await
}

pub async fn create_model_scoped(
    scope: &mut EditScope<'_>,
    input: ModelWrite,
) -> Result<Value, ModelStoreError> {
    let org = scope.organization_id();
    let proj = scope.project_id();
    let actor_type = scope.actor_kind();
    let actor_id = scope.actor_id();
    let actor_display = scope.display_name().to_owned();
    create_model_tx(
        scope.tx_mut(),
        org,
        proj,
        actor_type,
        actor_id,
        &actor_display,
        input,
    )
    .await
}

async fn create_model_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    organization_id: Uuid,
    project_id: Uuid,
    actor_type: &str,
    actor_id: Uuid,
    actor_display: &str,
    input: ModelWrite,
) -> Result<Value, ModelStoreError> {
    let lock_key = format!("processing-model:{project_id}");
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(lock_key)
        .execute(&mut **tx)
        .await
        .map_err(db_error)?;
    let count = sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM processing.models
         WHERE organization_id=$1 AND project_id=$2",
    )
    .bind(organization_id)
    .bind(project_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(db_error)?;
    if count >= 50 {
        return Err(ModelStoreError::Quota);
    }

    let id = Uuid::new_v4();
    let row = sqlx::query(
        "INSERT INTO processing.models(organization_id,project_id,id,name,graph)
         VALUES($1,$2,$3,$4,$5)
         RETURNING id,name,graph,revision,created_at,updated_at",
    )
    .bind(organization_id)
    .bind(project_id)
    .bind(id)
    .bind(&input.name)
    .bind(input.graph)
    .fetch_one(&mut **tx)
    .await
    .map_err(db_error)?;
    audit(
        tx,
        organization_id,
        ModelActor {
            actor_type,
            actor_id,
            actor_display,
        },
        id,
        "processing.model.create",
        &input.name,
    )
    .await?;
    wire(&row)
}

pub async fn update_model(
    edit: &mut ProjectRead<'_>,
    actor_id: Uuid,
    actor_display: &str,
    model_id: Uuid,
    revision: i32,
    input: ModelWrite,
) -> Result<Value, ModelStoreError> {
    let org = edit.organization_id();
    let proj = edit.project_id();
    update_model_tx(
        edit.transaction(),
        org,
        proj,
        "human",
        actor_id,
        actor_display,
        model_id,
        revision,
        input,
    )
    .await
}

pub async fn update_model_scoped(
    scope: &mut EditScope<'_>,
    model_id: Uuid,
    revision: i32,
    input: ModelWrite,
) -> Result<Value, ModelStoreError> {
    let org = scope.organization_id();
    let proj = scope.project_id();
    let actor_type = scope.actor_kind();
    let actor_id = scope.actor_id();
    let actor_display = scope.display_name().to_owned();
    update_model_tx(
        scope.tx_mut(),
        org,
        proj,
        actor_type,
        actor_id,
        &actor_display,
        model_id,
        revision,
        input,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn update_model_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    organization_id: Uuid,
    project_id: Uuid,
    actor_type: &str,
    actor_id: Uuid,
    actor_display: &str,
    model_id: Uuid,
    revision: i32,
    input: ModelWrite,
) -> Result<Value, ModelStoreError> {
    let row = sqlx::query(
        "UPDATE processing.models
         SET name=$5,graph=$6,revision=revision+1,updated_at=now()
         WHERE organization_id=$1 AND project_id=$2 AND id=$3 AND revision=$4
         RETURNING id,name,graph,revision,created_at,updated_at",
    )
    .bind(organization_id)
    .bind(project_id)
    .bind(model_id)
    .bind(revision)
    .bind(&input.name)
    .bind(input.graph)
    .fetch_optional(&mut **tx)
    .await
    .map_err(db_error)?
    .ok_or(ModelStoreError::Conflict)?;
    audit(
        tx,
        organization_id,
        ModelActor {
            actor_type,
            actor_id,
            actor_display,
        },
        model_id,
        "processing.model.update",
        &input.name,
    )
    .await?;
    wire(&row)
}

pub async fn delete_model(
    edit: &mut ProjectRead<'_>,
    actor_id: Uuid,
    actor_display: &str,
    model_id: Uuid,
    revision: i32,
) -> Result<(), ModelStoreError> {
    let org = edit.organization_id();
    let proj = edit.project_id();
    delete_model_tx(
        edit.transaction(),
        org,
        proj,
        "human",
        actor_id,
        actor_display,
        model_id,
        revision,
    )
    .await
}

pub async fn delete_model_scoped(
    scope: &mut EditScope<'_>,
    model_id: Uuid,
    revision: i32,
) -> Result<(), ModelStoreError> {
    let org = scope.organization_id();
    let proj = scope.project_id();
    let actor_type = scope.actor_kind();
    let actor_id = scope.actor_id();
    let actor_display = scope.display_name().to_owned();
    delete_model_tx(
        scope.tx_mut(),
        org,
        proj,
        actor_type,
        actor_id,
        &actor_display,
        model_id,
        revision,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn delete_model_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    organization_id: Uuid,
    project_id: Uuid,
    actor_type: &str,
    actor_id: Uuid,
    actor_display: &str,
    model_id: Uuid,
    revision: i32,
) -> Result<(), ModelStoreError> {
    let name = sqlx::query_scalar::<_, String>(
        "DELETE FROM processing.models
         WHERE organization_id=$1 AND project_id=$2 AND id=$3 AND revision=$4
         RETURNING name",
    )
    .bind(organization_id)
    .bind(project_id)
    .bind(model_id)
    .bind(revision)
    .fetch_optional(&mut **tx)
    .await
    .map_err(db_error)?
    .ok_or(ModelStoreError::Conflict)?;
    audit(
        tx,
        organization_id,
        ModelActor {
            actor_type,
            actor_id,
            actor_display,
        },
        model_id,
        "processing.model.delete",
        &name,
    )
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamp_wire_matches_python_isoformat() {
        let value = DateTime::parse_from_rfc3339("2026-11-26T08:09:10.123456+00:00")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(iso(value), "2026-11-26T08:09:10.123456+00:00");
    }
}
