//! E10 immutable 3D Tiles catalog, manifest, content, generation, and picking storage.

use std::collections::HashMap;

use chrono::{DateTime, SecondsFormat, Utc};
use ovg_core::AppError;
use serde_json::{json, Value};
use sqlx::postgres::PgRow;
use sqlx::Row;
use uuid::Uuid;

use crate::catalog::ProjectRead;

const MAX_TILESETS_PER_PROJECT: i64 = 16;
const MAX_TILESET_QUOTA_BYTES: i64 = 256 * 1024 * 1024;
const MAX_GENERATOR_FEATURES: i64 = 3000;
const MAX_GENERATOR_VERTICES: i64 = 60_000;

fn db(e: sqlx::Error) -> AppError {
    tracing::error!(%e, "tileset database operation failed");
    AppError::internal("tileset database operation failed")
}

fn iso(v: DateTime<Utc>) -> String {
    v.to_rfc3339_opts(SecondsFormat::AutoSi, false)
}

fn tileset_row(r: &PgRow) -> Result<Value, AppError> {
    Ok(json!({
        "id": r.try_get::<Uuid, _>("id").map_err(db)?,
        "name": r.try_get::<String, _>("name").map_err(db)?,
        "byte_size": r.try_get::<i64, _>("byte_size").map_err(db)?,
        "tile_count": r.try_get::<i32, _>("tile_count").map_err(db)?,
        "created_at": iso(r.try_get::<DateTime<Utc>, _>("created_at").map_err(db)?),
        "provenance": r.try_get::<Value, _>("provenance").map_err(db)?,
    }))
}

#[derive(Debug)]
pub enum TilesetStoreError {
    Detail(u16, &'static str),
    Internal(AppError),
}

impl From<AppError> for TilesetStoreError {
    fn from(value: AppError) -> Self {
        Self::Internal(value)
    }
}

fn store_db(e: sqlx::Error) -> TilesetStoreError {
    if let sqlx::Error::Database(ref db_err) = e {
        if matches!(db_err.code().as_deref(), Some("57014" | "42883")) {
            return TilesetStoreError::Detail(503, "Tile generation unavailable or timed out");
        }
    }
    TilesetStoreError::Internal(db(e))
}

pub struct NewTileResource {
    pub uri: String,
    pub sha256: String,
    pub payload: Vec<u8>,
    pub feature_index: Value,
}

pub struct PersistTilesetArgs<'a> {
    pub actor_id: Uuid,
    pub actor_display: &'a str,
    pub name: &'a str,
    pub manifest: Value,
    pub byte_size: i64,
    pub resources: Vec<NewTileResource>,
    pub provenance: Value,
    pub action: &'a str,
}

pub struct TriangulatedRow {
    pub feature_id: String,
    pub height: Value,
    pub geometry: Value,
    pub triangles: Value,
}

pub struct GenerationContext {
    pub source_id: Uuid,
    pub layer_revision: i64,
    pub content_revision: String,
    pub schema_revision: String,
    pub stored_crs: i32,
    pub rows: Vec<TriangulatedRow>,
}

pub struct PrepareGenerationArgs<'a> {
    pub layer_id: Uuid,
    pub expected_layer_revision: i64,
    pub expected_content_revision: &'a str,
    pub expected_schema_revision: &'a str,
    pub height_field: Option<&'a str>,
    pub uniform_height_m: Option<f64>,
}

pub struct CurrentFeatureContext {
    pub layer_name: String,
    pub attributes: Option<Value>,
    pub field_names: HashMap<String, String>,
    pub cad_links: Vec<Value>,
}

pub async fn list(read: &mut ProjectRead<'_>) -> Result<Value, AppError> {
    let rows = sqlx::query(
        "SELECT id,name,byte_size,tile_count,created_at,provenance
         FROM spatial.tilesets
         WHERE organization_id=$1 AND project_id=$2
         ORDER BY created_at DESC,id",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(db)?;
    let items = rows
        .iter()
        .map(tileset_row)
        .collect::<Result<Vec<Value>, AppError>>()?;
    Ok(json!({ "items": items }))
}

pub async fn manifest(read: &mut ProjectRead<'_>, id: Uuid) -> Result<Option<Value>, AppError> {
    sqlx::query_scalar::<_, Value>(
        "SELECT manifest FROM spatial.tilesets
         WHERE organization_id=$1 AND project_id=$2 AND id=$3",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(id)
    .fetch_optional(&mut **read.transaction())
    .await
    .map_err(db)
}

pub async fn get_tile_payload(
    read: &mut ProjectRead<'_>,
    tileset_id: Uuid,
    uri: &str,
) -> Result<Option<(String, Vec<u8>)>, AppError> {
    let row = sqlx::query(
        "SELECT sha256, payload FROM spatial.tileset_contents
         WHERE organization_id=$1 AND project_id=$2 AND tileset_id=$3 AND uri=$4",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(tileset_id)
    .bind(uri)
    .fetch_optional(&mut **read.transaction())
    .await
    .map_err(db)?;
    let Some(row) = row else {
        return Ok(None);
    };
    let sha256 = row.try_get::<String, _>("sha256").map_err(db)?;
    let payload = row.try_get::<Vec<u8>, _>("payload").map_err(db)?;
    Ok(Some((sha256, payload)))
}

pub async fn persist_tileset(
    edit: &mut ProjectRead<'_>,
    args: PersistTilesetArgs<'_>,
) -> Result<Value, TilesetStoreError> {
    let tileset_id = Uuid::new_v4();
    let lock_key = format!("tilesets:{}", edit.project_id());
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(&lock_key)
        .execute(&mut **edit.transaction())
        .await
        .map_err(store_db)?;

    let quota_row = sqlx::query(
        "SELECT count(*)::bigint AS count, coalesce(sum(byte_size), 0)::bigint AS total
         FROM spatial.tilesets WHERE organization_id=$1 AND project_id=$2",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .fetch_one(&mut **edit.transaction())
    .await
    .map_err(store_db)?;
    let count: i64 = quota_row.try_get("count").map_err(store_db)?;
    let total: i64 = quota_row.try_get("total").map_err(store_db)?;
    if count >= MAX_TILESETS_PER_PROJECT || total.saturating_add(args.byte_size) > MAX_TILESET_QUOTA_BYTES
    {
        return Err(TilesetStoreError::Detail(413, "Project tileset quota exceeded"));
    }

    let tile_count = i32::try_from(args.resources.len()).map_err(|_| {
        TilesetStoreError::Internal(AppError::internal("tile count overflow"))
    })?;

    sqlx::query(
        "INSERT INTO spatial.tilesets
         (organization_id, project_id, id, name, manifest, byte_size, tile_count, actor_id, provenance)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(tileset_id)
    .bind(args.name)
    .bind(&args.manifest)
    .bind(args.byte_size)
    .bind(tile_count)
    .bind(args.actor_id)
    .bind(&args.provenance)
    .execute(&mut **edit.transaction())
    .await
    .map_err(store_db)?;

    for res in &args.resources {
        sqlx::query(
            "INSERT INTO spatial.tileset_contents
             (organization_id, project_id, tileset_id, uri, sha256, payload, feature_index)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(edit.organization_id())
        .bind(edit.project_id())
        .bind(tileset_id)
        .bind(&res.uri)
        .bind(&res.sha256)
        .bind(&res.payload)
        .bind(&res.feature_index)
        .execute(&mut **edit.transaction())
        .await
        .map_err(store_db)?;
    }

    let mut audit_meta = match args.provenance {
        Value::Object(map) => map,
        _ => serde_json::Map::new(),
    };
    audit_meta.insert("tile_count".to_owned(), json!(tile_count));
    audit_meta.insert("byte_size".to_owned(), json!(args.byte_size));
    audit_meta.insert("crs".to_owned(), json!("EPSG:4979/EPSG:4978"));

    sqlx::query(
        "INSERT INTO audit.audit_events (
            id, organization_id, actor_type, actor_id, actor_display_snapshot,
            action, resource_type, resource_id, outcome, metadata, created_at
         ) VALUES ($1, $2, 'human', $3, $4, $5, 'tileset', $6, 'success', $7, $8)",
    )
    .bind(Uuid::new_v4())
    .bind(edit.organization_id())
    .bind(args.actor_id)
    .bind(args.actor_display)
    .bind(args.action)
    .bind(tileset_id)
    .bind(Value::Object(audit_meta))
    .bind(Utc::now())
    .execute(&mut **edit.transaction())
    .await
    .map_err(store_db)?;

    let inserted = sqlx::query(
        "SELECT id,name,byte_size,tile_count,created_at,provenance
         FROM spatial.tilesets
         WHERE organization_id=$1 AND project_id=$2 AND id=$3",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(tileset_id)
    .fetch_one(&mut **edit.transaction())
    .await
    .map_err(store_db)?;

    tileset_row(&inserted).map_err(TilesetStoreError::Internal)
}

pub async fn prepare_generation(
    edit: &mut ProjectRead<'_>,
    args: PrepareGenerationArgs<'_>,
) -> Result<GenerationContext, TilesetStoreError> {
    sqlx::query("SET LOCAL statement_timeout = '20s'")
        .execute(&mut **edit.transaction())
        .await
        .map_err(store_db)?;

    let source = sqlx::query(
        "SELECT s.id, s.content_revision, s.schema_revision, s.stored_crs,
                s.feature_count::bigint AS feature_count,
                s.geometry_family, s.dimensions, s.validation_state,
                l.layer_revision::bigint AS layer_revision
         FROM spatial.layers l
         JOIN spatial.data_sources s
           ON s.organization_id = l.organization_id
          AND s.project_id = l.project_id
          AND s.id = l.source_id
         WHERE l.organization_id = $1 AND l.project_id = $2 AND l.id = $3
           AND l.lifecycle = 'active' AND s.lifecycle = 'active'
         FOR SHARE OF s, l",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(args.layer_id)
    .fetch_optional(&mut **edit.transaction())
    .await
    .map_err(store_db)?
    .ok_or(TilesetStoreError::Detail(404, "Layer not found"))?;

    let source_id: Uuid = source.try_get("id").map_err(store_db)?;
    let content_revision: String = source.try_get("content_revision").map_err(store_db)?;
    let schema_revision: String = source.try_get("schema_revision").map_err(store_db)?;
    let stored_crs: i32 = source.try_get("stored_crs").map_err(store_db)?;
    let feature_count: i64 = source.try_get("feature_count").map_err(store_db)?;
    let geometry_family: String = source.try_get("geometry_family").map_err(store_db)?;
    let dimensions: String = source.try_get("dimensions").map_err(store_db)?;
    let validation_state: String = source.try_get("validation_state").map_err(store_db)?;
    let layer_revision: i64 = source.try_get("layer_revision").map_err(store_db)?;

    if layer_revision != args.expected_layer_revision
        || content_revision != args.expected_content_revision
        || schema_revision != args.expected_schema_revision
    {
        return Err(TilesetStoreError::Detail(
            409,
            "Source revision changed; reload the layer",
        ));
    }

    if validation_state != "ready"
        || geometry_family != "Polygon"
        || dimensions != "XY"
        || !matches!(stored_crs, 4326 | 3857)
    {
        return Err(TilesetStoreError::Detail(
            422,
            "Only ready XY Polygon EPSG:4326 / EPSG:3857 supported",
        ));
    }

    if !(1..=MAX_GENERATOR_FEATURES).contains(&feature_count) {
        return Err(TilesetStoreError::Detail(
            413,
            "Layer must have 1\u{2013}3000 features",
        ));
    }

    let field_kind = if let Some(field) = args.height_field {
        let found = sqlx::query_scalar::<_, String>(
            "SELECT type FROM spatial.source_fields
             WHERE organization_id = $1 AND project_id = $2
               AND source_id = $3 AND field_id = $4 AND schema_revision = $5",
        )
        .bind(edit.organization_id())
        .bind(edit.project_id())
        .bind(source_id)
        .bind(field)
        .bind(&schema_revision)
        .fetch_optional(&mut **edit.transaction())
        .await
        .map_err(store_db)?;
        if !matches!(found.as_deref(), Some("integer" | "decimal")) {
            return Err(TilesetStoreError::Detail(422, "Height field must be numeric"));
        }
        found
    } else {
        None
    };

    let field_type = if field_kind.as_deref() == Some("integer") {
        "number"
    } else {
        "string"
    };
    let field_name = args.height_field.unwrap_or("");

    let stats = sqlx::query(
        "SELECT count(*)::bigint AS n,
                coalesce(sum(ST_NPoints(geometry)), 0)::bigint AS vertices,
                bool_and(coalesce(
                    geometry_state = 'present'
                    AND ST_SRID(geometry) = $5
                    AND ST_GeometryType(geometry) = 'ST_Polygon'
                    AND ST_NDims(geometry) = 2
                    AND ST_IsValid(geometry)
                    AND NOT ST_IsEmpty(geometry)
                    AND ($6::double precision IS NOT NULL OR jsonb_typeof(attributes -> $7) = $8),
                    false
                )) AS valid
         FROM spatial.reference_features
         WHERE organization_id = $1 AND project_id = $2
           AND source_id = $3 AND content_revision = $4 AND schema_revision = $9",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(source_id)
    .bind(&content_revision)
    .bind(stored_crs)
    .bind(args.uniform_height_m)
    .bind(field_name)
    .bind(field_type)
    .bind(&schema_revision)
    .fetch_one(&mut **edit.transaction())
    .await
    .map_err(store_db)?;

    let n: i64 = stats.try_get("n").map_err(store_db)?;
    let vertices: i64 = stats.try_get("vertices").map_err(store_db)?;
    let valid: Option<bool> = stats.try_get("valid").map_err(store_db)?;

    if n != feature_count {
        return Err(TilesetStoreError::Detail(409, "Feature count changed"));
    }
    if vertices > MAX_GENERATOR_VERTICES {
        return Err(TilesetStoreError::Detail(413, "60,000 vertex budget exceeded"));
    }
    if !valid.unwrap_or(false) {
        return Err(TilesetStoreError::Detail(
            422,
            "All features require valid XY polygons and numeric heights",
        ));
    }

    let raw_rows = sqlx::query(
        "WITH polygons AS MATERIALIZED (
             SELECT feature_id,
                    COALESCE(to_jsonb($6::double precision), attributes -> $7) AS height,
                    CASE WHEN $5 = 4326 THEN geometry ELSE ST_Transform(geometry, 4326) END AS geom
             FROM spatial.reference_features
             WHERE organization_id = $1 AND project_id = $2
               AND source_id = $3 AND content_revision = $4 AND schema_revision = $8
         )
         SELECT feature_id, height,
                ST_AsGeoJSON(geom, 15)::jsonb AS geometry,
                ST_AsGeoJSON(ST_TriangulatePolygon(geom), 15)::jsonb AS triangles
         FROM polygons ORDER BY feature_id",
    )
    .bind(edit.organization_id())
    .bind(edit.project_id())
    .bind(source_id)
    .bind(&content_revision)
    .bind(stored_crs)
    .bind(args.uniform_height_m)
    .bind(field_name)
    .bind(&schema_revision)
    .fetch_all(&mut **edit.transaction())
    .await
    .map_err(store_db)?;

    let rows = raw_rows
        .into_iter()
        .map(|row| {
            Ok(TriangulatedRow {
                feature_id: row.try_get("feature_id").map_err(store_db)?,
                height: row.try_get("height").map_err(store_db)?,
                geometry: row.try_get("geometry").map_err(store_db)?,
                triangles: row.try_get("triangles").map_err(store_db)?,
            })
        })
        .collect::<Result<Vec<_>, TilesetStoreError>>()?;

    Ok(GenerationContext {
        source_id,
        layer_revision,
        content_revision,
        schema_revision,
        stored_crs,
        rows,
    })
}

pub async fn load_tile_feature_index(
    read: &mut ProjectRead<'_>,
    tileset_id: Uuid,
    uri: &str,
) -> Result<Option<(Value, Value)>, AppError> {
    let row = sqlx::query(
        "SELECT t.provenance, c.feature_index
         FROM spatial.tilesets t
         JOIN spatial.tileset_contents c
           ON c.organization_id = t.organization_id
          AND c.project_id = t.project_id
          AND c.tileset_id = t.id
         WHERE t.organization_id = $1 AND t.project_id = $2
           AND t.id = $3 AND c.uri = $4",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(tileset_id)
    .bind(uri)
    .fetch_optional(&mut **read.transaction())
    .await
    .map_err(db)?;
    let Some(row) = row else {
        return Ok(None);
    };
    let provenance: Value = row.try_get("provenance").map_err(db)?;
    let feature_index: Value = row.try_get("feature_index").map_err(db)?;
    Ok(Some((provenance, feature_index)))
}

pub async fn load_current_feature_context(
    read: &mut ProjectRead<'_>,
    source_id: Uuid,
    layer_id: Uuid,
    feature_id: &str,
    expected_layer_revision: i64,
    expected_content_revision: &str,
    expected_schema_revision: &str,
) -> Result<Option<CurrentFeatureContext>, TilesetStoreError> {
    let current = sqlx::query(
        "SELECT l.name AS layer_name,
                l.layer_revision::bigint AS layer_revision,
                s.content_revision,
                s.schema_revision
         FROM spatial.layers l
         JOIN spatial.data_sources s
           ON s.organization_id = l.organization_id
          AND s.project_id = l.project_id
          AND s.id = l.source_id
         WHERE l.organization_id = $1 AND l.project_id = $2
           AND l.id = $3 AND s.id = $4
           AND l.lifecycle = 'active' AND s.lifecycle = 'active'
           AND s.validation_state = 'ready'
         FOR SHARE OF l, s",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(layer_id)
    .bind(source_id)
    .fetch_optional(&mut **read.transaction())
    .await
    .map_err(store_db)?;

    let Some(current) = current else {
        return Ok(None);
    };
    let layer_name: String = current.try_get("layer_name").map_err(store_db)?;
    let layer_revision: i64 = current.try_get("layer_revision").map_err(store_db)?;
    let content_revision: String = current.try_get("content_revision").map_err(store_db)?;
    let schema_revision: String = current.try_get("schema_revision").map_err(store_db)?;

    if layer_revision != expected_layer_revision
        || content_revision != expected_content_revision
        || schema_revision != expected_schema_revision
    {
        return Ok(None);
    }

    let attributes = sqlx::query_scalar::<_, Value>(
        "SELECT attributes FROM spatial.reference_features
         WHERE organization_id = $1 AND project_id = $2
           AND source_id = $3 AND feature_id = $4
           AND content_revision = $5 AND schema_revision = $6
           AND geometry_state = 'present'",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(source_id)
    .bind(feature_id)
    .bind(&content_revision)
    .bind(&schema_revision)
    .fetch_optional(&mut **read.transaction())
    .await
    .map_err(store_db)?;

    let Some(attributes) = attributes else {
        return Ok(None);
    };

    let field_rows = sqlx::query(
        "SELECT field_id, name FROM spatial.source_fields
         WHERE organization_id = $1 AND project_id = $2
           AND source_id = $3 AND schema_revision = $4
         ORDER BY ordinal",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(source_id)
    .bind(&schema_revision)
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(store_db)?;

    let mut field_names = HashMap::with_capacity(field_rows.len());
    for row in field_rows {
        let fid: String = row.try_get("field_id").map_err(store_db)?;
        let fname: String = row.try_get("name").map_err(store_db)?;
        field_names.insert(fid, fname);
    }

    let cad_links = crate::cad_documents::linked_features(
        read.transaction(),
        read.organization_id(),
        read.project_id(),
        crate::cad_documents::LinkedFilter::ByFeature {
            source_id,
            feature_id,
        },
    )
    .await
    .map_err(|err| match err {
        AppError::Conflict { .. } => {
            TilesetStoreError::Detail(409, "Too many linked conversions; narrow selection")
        }
        other => TilesetStoreError::Internal(other),
    })?;

    Ok(Some(CurrentFeatureContext {
        layer_name,
        attributes: Some(attributes),
        field_names,
        cad_links,
    }))
}

pub async fn find_feature_by_id(
    read: &mut ProjectRead<'_>,
    tileset_id: Uuid,
    source_id: Uuid,
    layer_id: Uuid,
    feature_id: &str,
) -> Result<(String, i64), TilesetStoreError> {
    let rows = sqlx::query(
        "SELECT c.uri, e.item->'first_triangle' AS triangle
         FROM spatial.tilesets t
         JOIN spatial.tileset_contents c
           ON c.organization_id = t.organization_id
          AND c.project_id = t.project_id
          AND c.tileset_id = t.id
         CROSS JOIN LATERAL jsonb_array_elements(c.feature_index) e(item)
         WHERE t.organization_id = $1 AND t.project_id = $2 AND t.id = $3
           AND t.provenance->>'feature_picking' = 'true'
           AND t.provenance->>'source_id' = $4
           AND t.provenance->>'layer_id' = $5
           AND e.item->>'feature_id' = $6
         LIMIT 2",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(tileset_id)
    .bind(source_id.to_string())
    .bind(layer_id.to_string())
    .bind(feature_id)
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(store_db)?;

    if rows.is_empty() {
        return Err(TilesetStoreError::Detail(
            404,
            "Feature not indexed in this tileset",
        ));
    }
    if rows.len() != 1 {
        return Err(TilesetStoreError::Detail(409, "Feature index integrity failure"));
    }
    let uri: String = rows[0].try_get("uri").map_err(store_db)?;
    let tri_val: Value = rows[0].try_get("triangle").map_err(store_db)?;
    let Some(triangle) = tri_val.as_i64() else {
        return Err(TilesetStoreError::Detail(409, "Feature index integrity failure"));
    };
    if tri_val.is_f64() || !(0..100_000).contains(&triangle) {
        return Err(TilesetStoreError::Detail(409, "Feature index integrity failure"));
    }
    Ok((uri, triangle))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_matches_python() {
        let t = DateTime::parse_from_rfc3339("2026-11-26T10:11:12.123456+00:00")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(iso(t), "2026-11-26T10:11:12.123456+00:00");
    }
}
