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
