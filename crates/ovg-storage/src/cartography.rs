//! E06 read-side cartography repository over canonical RLS tables.

use chrono::{DateTime, SecondsFormat, Utc};
use ovg_core::AppError;
use serde_json::{json, Value};
use sqlx::postgres::PgRow;
use sqlx::Row;
use uuid::Uuid;

use crate::catalog::ProjectRead;

fn db_error(error: sqlx::Error) -> AppError {
    tracing::error!(%error, "cartography database operation failed");
    AppError::internal("cartography database operation failed")
}

fn iso(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::AutoSi, false)
}

fn style_wire(row: &PgRow) -> Result<Value, AppError> {
    Ok(json!({
        "id": row.try_get::<Uuid, _>("id").map_err(db_error)?,
        "layer_id": row.try_get::<Uuid, _>("layer_id").map_err(db_error)?,
        "mode": row.try_get::<String, _>("mode").map_err(db_error)?,
        "style_spec": row.try_get::<Value, _>("style_spec").map_err(db_error)?,
        "label_field": row.try_get::<Option<String>, _>("label_field").map_err(db_error)?,
        "legend_items": row.try_get::<Value, _>("legend_items").map_err(db_error)?,
        "revision": row.try_get::<i32, _>("revision").map_err(db_error)?,
        "updated_at": iso(row.try_get::<DateTime<Utc>, _>("updated_at").map_err(db_error)?),
    }))
}

fn layout_wire(row: &PgRow) -> Result<Value, AppError> {
    Ok(json!({
        "id": row.try_get::<Uuid, _>("id").map_err(db_error)?,
        "name": row.try_get::<String, _>("name").map_err(db_error)?,
        "title": row.try_get::<String, _>("title").map_err(db_error)?,
        "paper_size": row.try_get::<String, _>("paper_size").map_err(db_error)?,
        "orientation": row.try_get::<String, _>("orientation").map_err(db_error)?,
        "crs_code": row.try_get::<i32, _>("crs_code").map_err(db_error)?,
        "attribution": row.try_get::<String, _>("attribution").map_err(db_error)?,
        "show_north_arrow": row.try_get::<bool, _>("show_north_arrow").map_err(db_error)?,
        "show_scale_bar": row.try_get::<bool, _>("show_scale_bar").map_err(db_error)?,
        "show_legend": row.try_get::<bool, _>("show_legend").map_err(db_error)?,
        "layer_ids": row.try_get::<Value, _>("layer_ids").map_err(db_error)?,
        "extent": row.try_get::<Option<Value>, _>("extent").map_err(db_error)?,
        "created_at": iso(row.try_get::<DateTime<Utc>, _>("created_at").map_err(db_error)?),
    }))
}

pub async fn get_layer_style(
    read: &mut ProjectRead<'_>,
    layer_id: Uuid,
) -> Result<Value, AppError> {
    let row = sqlx::query(
        "SELECT id, layer_id, mode, style_spec, label_field, legend_items, revision, updated_at
         FROM cartography.layer_styles
         WHERE organization_id = $1 AND project_id = $2 AND layer_id = $3",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(layer_id)
    .fetch_optional(&mut **read.transaction())
    .await
    .map_err(db_error)?;
    let style = row.as_ref().map(style_wire).transpose()?;
    Ok(json!({ "style": style }))
}

pub async fn list_layouts(read: &mut ProjectRead<'_>) -> Result<Value, AppError> {
    let rows = sqlx::query(
        "SELECT id, name, title, paper_size, orientation, crs_code, attribution,
                show_north_arrow, show_scale_bar, show_legend, layer_ids, extent, created_at
         FROM cartography.print_layouts
         WHERE organization_id = $1 AND project_id = $2
         ORDER BY created_at DESC",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(db_error)?;
    let items = rows
        .iter()
        .map(layout_wire)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({ "items": items }))
}

#[derive(Debug, Clone)]
pub struct StyleTarget {
    pub name: String,
    pub source_id: Uuid,
}

pub async fn style_target(
    edit: &mut ProjectRead<'_>,
    layer_id: Uuid,
) -> Result<StyleTarget, AppError> {
    let row = sqlx::query(
        "SELECT id, name, source_id
         FROM spatial.layers
         WHERE organization_id = $1 AND project_id = $2 AND id = $3
           AND lifecycle = 'active'
         FOR UPDATE",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(layer_id)
    .fetch_optional(&mut **edit.transaction())
    .await
    .map_err(db_error)?
    .ok_or_else(|| AppError::not_found("layer not found"))?;
    Ok(StyleTarget {
        name: row.try_get("name").map_err(db_error)?,
        source_id: row.try_get("source_id").map_err(db_error)?,
    })
}

#[derive(Debug, Clone)]
pub struct StyleInsert {
    pub actor_id: Uuid,
    pub layer_id: Uuid,
    pub source_id: Uuid,
    pub mode: String,
    pub style_spec: Value,
    pub label_field: Option<String>,
    pub legend_items: Value,
}

pub async fn upsert_layer_style(
    edit: &mut ProjectRead<'_>,
    input: StyleInsert,
) -> Result<Value, AppError> {
    if let Some(field_id) = input.label_field.as_deref() {
        let exists = sqlx::query_scalar::<_, i32>(
            "SELECT 1 FROM spatial.source_fields
             WHERE organization_id = $1 AND project_id = $2
               AND source_id = $3 AND field_id = $4",
        )
        .bind(edit.organization_id())
        .bind(edit.project_id())
        .bind(input.source_id)
        .bind(field_id)
        .fetch_optional(&mut **edit.transaction())
        .await
        .map_err(db_error)?;
        if exists.is_none() {
            return Err(AppError::validation(
                "UNKNOWN_FIELD",
                format!("label_field '{field_id}' not in layer schema"),
            ));
        }
    }

    let now = Utc::now();
    let row = sqlx::query(
        "INSERT INTO cartography.layer_styles (
            id, organization_id, project_id, layer_id, mode, style_spec,
            label_field, legend_items, revision, updated_by_actor_id,
            created_at, updated_at
         ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,1,$9,$10,$10)
         ON CONFLICT (organization_id, project_id, layer_id) DO UPDATE
         SET mode = EXCLUDED.mode,
             style_spec = EXCLUDED.style_spec,
             label_field = EXCLUDED.label_field,
             legend_items = EXCLUDED.legend_items,
             revision = cartography.layer_styles.revision + 1,
             updated_by_actor_id = EXCLUDED.updated_by_actor_id,
             updated_at = EXCLUDED.updated_at
         RETURNING id, layer_id, mode, style_spec, label_field, legend_items,
                   revision, updated_at",
    )
    .bind(Uuid::new_v4())
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(input.layer_id)
    .bind(input.mode)
    .bind(input.style_spec)
    .bind(input.label_field)
    .bind(input.legend_items)
    .bind(input.actor_id)
    .bind(now)
    .fetch_one(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    style_wire(&row)
}

#[derive(Debug, Clone)]
pub struct LayoutInsert {
    pub name: String,
    pub title: String,
    pub paper_size: String,
    pub orientation: String,
    pub crs_code: i32,
    pub attribution: String,
    pub show_north_arrow: bool,
    pub show_scale_bar: bool,
    pub show_legend: bool,
    pub layer_ids: Vec<Uuid>,
    pub extent: Option<Value>,
}

pub async fn create_layout(
    edit: &mut ProjectRead<'_>,
    actor_id: Uuid,
    input: LayoutInsert,
) -> Result<Value, AppError> {
    for layer_id in &input.layer_ids {
        let available = sqlx::query_scalar::<_, Uuid>(
            "SELECT l.id
             FROM spatial.layers l
             JOIN spatial.data_sources s
               ON l.source_id = s.id
              AND l.project_id = s.project_id
              AND l.organization_id = s.organization_id
             WHERE l.organization_id = $1
               AND l.project_id = $2
               AND l.id = $3
               AND l.lifecycle = 'active'
               AND s.lifecycle = 'active'
               AND s.validation_state = 'ready'",
        )
        .bind(edit.organization_id())
        .bind(edit.project_id())
        .bind(layer_id)
        .fetch_optional(&mut **edit.transaction())
        .await
        .map_err(db_error)?;
        if available.is_none() {
            return Err(AppError::not_found("layout layer unavailable"));
        }
    }

    let now = Utc::now();
    let layer_ids = Value::Array(
        input
            .layer_ids
            .iter()
            .map(|id| Value::String(id.to_string()))
            .collect(),
    );
    let row = sqlx::query(
        "INSERT INTO cartography.print_layouts (
            id, organization_id, project_id, name, title, paper_size, orientation,
            crs_code, attribution, show_north_arrow, show_scale_bar, show_legend,
            layer_ids, extent, created_by_actor_id, created_at, updated_at
         ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$16)
         RETURNING id, name, title, paper_size, orientation, crs_code, attribution,
                   show_north_arrow, show_scale_bar, show_legend, layer_ids, extent,
                   created_at",
    )
    .bind(Uuid::new_v4())
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(input.name)
    .bind(input.title)
    .bind(input.paper_size)
    .bind(input.orientation)
    .bind(input.crs_code)
    .bind(input.attribution)
    .bind(input.show_north_arrow)
    .bind(input.show_scale_bar)
    .bind(input.show_legend)
    .bind(layer_ids)
    .bind(input.extent)
    .bind(actor_id)
    .bind(now)
    .fetch_one(&mut **edit.transaction())
    .await
    .map_err(db_error)?;
    layout_wire(&row)
}

fn export_wire(row: &PgRow) -> Result<Value, AppError> {
    Ok(json!({
        "id": row.try_get::<Uuid, _>("id").map_err(db_error)?,
        "layout_id": row.try_get::<Uuid, _>("layout_id").map_err(db_error)?,
        "output_format": row.try_get::<String, _>("output_format").map_err(db_error)?,
        "dpi": row.try_get::<i32, _>("dpi").map_err(db_error)?,
        "scale_denominator": row.try_get::<i32, _>("scale_denominator").map_err(db_error)?,
        "scale_accuracy_note": row.try_get::<String, _>("scale_accuracy_note").map_err(db_error)?,
        "revision_manifest": row.try_get::<Value, _>("revision_manifest").map_err(db_error)?,
        "sha256": row.try_get::<String, _>("sha256").map_err(db_error)?,
        "byte_size": row.try_get::<i64, _>("byte_size").map_err(db_error)?,
        "created_at": iso(row.try_get::<DateTime<Utc>, _>("created_at").map_err(db_error)?),
    }))
}

pub async fn list_exports(read: &mut ProjectRead<'_>) -> Result<Value, AppError> {
    let rows = sqlx::query(
        "SELECT id, layout_id, output_format, dpi, scale_denominator,
                scale_accuracy_note, revision_manifest, sha256, byte_size, created_at
         FROM cartography.print_exports
         WHERE organization_id = $1 AND project_id = $2
         ORDER BY created_at DESC
         LIMIT 50",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(db_error)?;
    let items = rows
        .iter()
        .map(export_wire)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({ "items": items }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn python_isoformat_uses_explicit_utc_offset() {
        let value = DateTime::parse_from_rfc3339("2026-11-26T01:02:03+00:00")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(iso(value), "2026-11-26T01:02:03+00:00");
    }
}
