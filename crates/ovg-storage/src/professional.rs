//! E08 professional GIS tools over PostGIS (`/pro/geometry`, `/pro/topology`,
//! `/pro/schema`, `/pro/tiles/{layer_id}/{z}/{x}/{y}.mvt`), matching
//! `apps/api/openvietgis_api/adapters/http/professional.py`.

use std::collections::HashSet;

use chrono::{SubsecRound, Utc};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::Row;
use uuid::Uuid;

use crate::editing::{EditError, EditScope};
use crate::spatial_pins::{schema_revision_digest, validate_ready_geojson, FieldSpec};

const STATEMENT_TIMEOUT: &str = "12s";
const MAX_REVISION: i64 = (1_i64 << 53) - 1;

#[derive(Debug, Clone)]
pub struct Selection {
    pub layer_id: Uuid,
    pub content_revision: String,
    pub schema_revision: String,
    pub feature_ids: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct GeometryParams {
    pub operation: String,
    pub primary: Selection,
    pub secondary: Option<Selection>,
    pub distance: f64,
    pub dx: f64,
    pub dy: f64,
    pub angle: f64,
    pub factor: f64,
    pub vertex: i32,
}

#[derive(Debug, Clone)]
pub struct TopologyParams {
    pub check: String,
    pub primary: Selection,
    pub secondary: Option<Selection>,
}

#[derive(Debug, Clone)]
pub struct SchemaParams {
    pub primary: Selection,
    pub action: String,
    pub field_id: String,
    pub name: String,
    pub field_type: String,
}

#[derive(Debug, Clone)]
struct PinnedSource {
    id: Uuid,
    stored_crs: i32,
    schema_state: String,
    geometry_family: String,
    dimensions: String,
    feature_count: i64,
    catalog_revision: i64,
    content_revision: String,
    schema_revision: String,
}

fn db_err(err: sqlx::Error) -> EditError {
    tracing::error!(%err, "professional GIS database error");
    EditError::Internal("professional GIS database error".into())
}

fn op_err(_: sqlx::Error) -> EditError {
    EditError::Detail(
        422,
        "Spatial operation failed or exceeded its time budget".into(),
    )
}

async fn set_timeout(scope: &mut EditScope<'_>) -> Result<(), EditError> {
    sqlx::query("SET LOCAL statement_timeout = '12s'")
        .execute(&mut **scope.tx_mut())
        .await
        .map_err(db_err)?;
    let _ = STATEMENT_TIMEOUT;
    Ok(())
}

async fn pinned(
    scope: &mut EditScope<'_>,
    spec: &Selection,
    editable: bool,
    tile_read: bool,
) -> Result<PinnedSource, EditError> {
    let lock = if editable { "FOR UPDATE" } else { "FOR SHARE" };
    let org_id = scope.organization_id();
    let project_id = scope.project_id();

    let layer_sql = format!(
        "SELECT source_id, lifecycle FROM spatial.layers
         WHERE organization_id = $1 AND project_id = $2 AND id = $3 {lock}"
    );
    let layer_row = sqlx::query(&layer_sql)
        .bind(org_id)
        .bind(project_id)
        .bind(spec.layer_id)
        .fetch_optional(&mut **scope.tx_mut())
        .await
        .map_err(db_err)?;
    let Some(layer_row) = layer_row else {
        return Err(EditError::detail(404, "Layer not found"));
    };
    let layer_lifecycle: String = layer_row.try_get("lifecycle").map_err(db_err)?;
    if layer_lifecycle != "active" {
        return Err(EditError::detail(404, "Layer not found"));
    }
    let source_id: Uuid = layer_row.try_get("source_id").map_err(db_err)?;

    let src_sql = format!(
        "SELECT id, kind, lifecycle, validation_state, stored_crs,
                schema_state, geometry_family, dimensions, feature_count,
                catalog_revision, content_revision, schema_revision
         FROM spatial.data_sources
         WHERE organization_id = $1 AND project_id = $2 AND id = $3 {lock}"
    );
    let src_row = sqlx::query(&src_sql)
        .bind(org_id)
        .bind(project_id)
        .bind(source_id)
        .fetch_optional(&mut **scope.tx_mut())
        .await
        .map_err(db_err)?;
    let Some(src_row) = src_row else {
        return Err(EditError::detail(422, "Source is not ready"));
    };

    let lifecycle: String = src_row.try_get("lifecycle").map_err(db_err)?;
    let validation_state: String = src_row.try_get("validation_state").map_err(db_err)?;
    if lifecycle != "active" || validation_state != "ready" {
        return Err(EditError::detail(422, "Source is not ready"));
    }
    let content_revision: String = src_row.try_get("content_revision").map_err(db_err)?;
    let schema_revision: String = src_row.try_get("schema_revision").map_err(db_err)?;
    if (content_revision.as_str(), schema_revision.as_str())
        != (
            spec.content_revision.as_str(),
            spec.schema_revision.as_str(),
        )
    {
        return Err(EditError::detail(409, "Source revision changed; reload"));
    }

    let stored_crs: Option<i32> = src_row.try_get("stored_crs").map_err(db_err)?;
    let crs_ok = matches!(
        (tile_read, stored_crs),
        (true, Some(4326 | 3857)) | (false, Some(4326))
    );
    if !crs_ok {
        return Err(EditError::detail(
            422,
            "Unsupported native CRS for this tool",
        ));
    }
    let kind: String = src_row.try_get("kind").map_err(db_err)?;
    if editable && kind == "reference_fixture" {
        return Err(EditError::detail(403, "Reference source is read-only"));
    }
    let unique: HashSet<&String> = spec.feature_ids.iter().collect();
    if unique.len() != spec.feature_ids.len() {
        return Err(EditError::detail(422, "Duplicate feature identifiers"));
    }

    Ok(PinnedSource {
        id: src_row.try_get("id").map_err(db_err)?,
        stored_crs: stored_crs.unwrap_or(4326),
        schema_state: src_row.try_get("schema_state").map_err(db_err)?,
        geometry_family: src_row.try_get("geometry_family").map_err(db_err)?,
        dimensions: src_row.try_get("dimensions").map_err(db_err)?,
        feature_count: i64::from(
            src_row
                .try_get::<Option<i32>, _>("feature_count")
                .map_err(db_err)?
                .unwrap_or(0),
        ),
        catalog_revision: src_row.try_get("catalog_revision").map_err(db_err)?,
        content_revision,
        schema_revision,
    })
}

pub fn is_cad_op(op: &str) -> bool {
    matches!(
        op,
        "move_batch"
            | "copy"
            | "offset"
            | "rotate"
            | "scale"
            | "stretch"
            | "extend"
            | "trim"
            | "fillet"
    )
}

fn is_binary_op(op: &str) -> bool {
    matches!(
        op,
        "trim" | "intersection" | "union" | "difference" | "nearest" | "spatial_join"
    )
}

fn fillet_points(
    points: &[[f64; 2]],
    index: usize,
    radius: f64,
) -> Result<Vec<[f64; 2]>, EditError> {
    if index == 0 || index + 1 >= points.len() || radius <= 0.0 {
        return Err(EditError::detail(
            422,
            "Fillet requires an interior vertex and positive radius",
        ));
    }
    let a = points[index - 1];
    let b = points[index];
    let c = points[index + 1];
    let u = (a[0] - b[0], a[1] - b[1]);
    let v = (c[0] - b[0], c[1] - b[1]);
    let lu = u.0.hypot(u.1);
    let lv = v.0.hypot(v.1);
    if lu == 0.0 || lv == 0.0 {
        return Err(EditError::detail(422, "Repeated vertices"));
    }
    let u = (u.0 / lu, u.1 / lu);
    let v = (v.0 / lv, v.1 / lv);
    let theta = (u.0 * v.0 + u.1 * v.1).clamp(-1.0, 1.0).acos();
    if !(0.01..=std::f64::consts::PI - 0.01).contains(&theta) {
        return Err(EditError::detail(422, "Fillet requires a valid corner"));
    }
    let tangent = radius / (theta / 2.0).tan();
    if tangent >= lu.min(lv) {
        return Err(EditError::detail(422, "Radius exceeds adjacent segments"));
    }
    let bis = (u.0 + v.0, u.1 + v.1);
    let norm = bis.0.hypot(bis.1);
    let sin_half = (theta / 2.0).sin();
    let center = (
        b[0] + bis.0 / norm * radius / sin_half,
        b[1] + bis.1 / norm * radius / sin_half,
    );
    let p = (b[0] + u.0 * tangent, b[1] + u.1 * tangent);
    let q = (b[0] + v.0 * tangent, b[1] + v.1 * tangent);
    let start = (p.1 - center.1).atan2(p.0 - center.0);
    let end = (q.1 - center.1).atan2(q.0 - center.0);
    let delta = (end - start + std::f64::consts::PI).rem_euclid(2.0 * std::f64::consts::PI)
        - std::f64::consts::PI;
    let count = ((delta.abs() / (std::f64::consts::PI / 36.0)).ceil() as usize).max(4);
    let mut out = Vec::with_capacity(points.len() + count);
    out.extend_from_slice(&points[..index]);
    for i in 0..=count {
        let ang = start + delta * (i as f64) / (count as f64);
        out.push([center.0 + radius * ang.cos(), center.1 + radius * ang.sin()]);
    }
    out.extend_from_slice(&points[index + 1..]);
    Ok(out)
}

fn selection_sql(src_param: &str, ids_param: Option<&str>) -> String {
    let mut sql = format!(
        "SELECT feature_id, geometry, attributes FROM spatial.reference_features \
         WHERE organization_id = $1 AND project_id = $2 AND source_id = {src_param} \
         AND geometry_state = 'present'"
    );
    if let Some(ids) = ids_param {
        sql.push_str(&format!(" AND feature_id = ANY({ids})"));
    }
    sql
}

pub async fn geometry_preview(
    scope: &mut EditScope<'_>,
    req: GeometryParams,
) -> Result<Value, EditError> {
    set_timeout(scope).await?;
    let op = req.operation.as_str();
    let cad = is_cad_op(op);
    let src = pinned(scope, &req.primary, cad, false).await?;

    if req.primary.feature_ids.is_empty()
        || (cad && op != "move_batch" && req.primary.feature_ids.len() != 1)
    {
        return Err(EditError::detail(
            422,
            "Select a feature; CAD needs one feature",
        ));
    }
    if op == "move_batch" {
        if scope.is_scoped() {
            return Err(EditError::detail(
                403,
                "Batch move requires project-wide edit access",
            ));
        }
        if req.secondary.is_some() || (req.dx == 0.0 && req.dy == 0.0) {
            return Err(EditError::detail(
                422,
                "Select a nonzero vector and one source layer",
            ));
        }
    }
    if is_binary_op(op) && req.secondary.is_none() {
        return Err(EditError::detail(
            422,
            "Select a second layer and feature(s)",
        ));
    }

    let org_id = scope.organization_id();
    let project_id = scope.project_id();
    let primary_sql = selection_sql("$3", Some("$4"));
    let check_primary = format!("SELECT feature_id FROM ({primary_sql}) q");
    let primary_rows = sqlx::query(&check_primary)
        .bind(org_id)
        .bind(project_id)
        .bind(src.id)
        .bind(&req.primary.feature_ids)
        .fetch_all(&mut **scope.tx_mut())
        .await
        .map_err(op_err)?;
    if primary_rows.len() != req.primary.feature_ids.len() {
        return Err(EditError::detail(404, "Selected feature missing"));
    }

    let mut target_src_id: Option<Uuid> = None;
    let mut target_ids: Vec<String> = Vec::new();
    let mut secondary_sql = String::new();
    if let Some(ref sec) = req.secondary {
        let target = pinned(scope, sec, false, false).await?;
        if sec.feature_ids.is_empty() && target.feature_count > 2000 && op != "nearest" {
            return Err(EditError::detail(
                422,
                "Select target features; whole-layer operation limited to 2000",
            ));
        }
        secondary_sql = selection_sql(
            "$5",
            if sec.feature_ids.is_empty() {
                None
            } else {
                Some("$6")
            },
        );
        if !sec.feature_ids.is_empty() {
            let check_sec = format!("SELECT feature_id FROM ({secondary_sql}) q");
            let sec_rows = sqlx::query(&check_sec)
                .bind(org_id)
                .bind(project_id)
                .bind(src.id)
                .bind(&req.primary.feature_ids)
                .bind(target.id)
                .bind(&sec.feature_ids)
                .fetch_all(&mut **scope.tx_mut())
                .await
                .map_err(op_err)?;
            if sec_rows.len() != sec.feature_ids.len() {
                return Err(EditError::detail(404, "Target feature missing"));
            }
        }
        target_src_id = Some(target.id);
        target_ids = sec.feature_ids.clone();
    }

    if matches!(op, "offset" | "trim" | "extend" | "fillet" | "stretch")
        && src.geometry_family != "LineString"
    {
        return Err(EditError::detail(422, "This tool requires a LineString"));
    }
    if matches!(op, "offset" | "simplify" | "extend" | "fillet")
        && (req.distance == 0.0 || (op != "offset" && req.distance < 0.0))
    {
        return Err(EditError::detail(422, "Enter a nonzero valid distance"));
    }

    let cte = format!(
        "WITH a AS ({primary_sql}), \
         u AS (SELECT ST_Centroid(ST_Collect(geometry)) AS g, \
               ST_XMax(ST_Extent(geometry)) - ST_XMin(ST_Extent(geometry)) AS span FROM a), \
         crs AS (SELECT (CASE WHEN ST_Y(g) >= 0 THEN 32600 ELSE 32700 END) + \
                 LEAST(60, GREATEST(1, FLOOR((ST_X(g) + 180)/6)::int + 1)) AS srid \
                 FROM u WHERE span <= 6 AND ST_Y(g) BETWEEN -80 AND 84), \
         m AS (SELECT feature_id, ST_Transform(geometry, srid) AS g, srid FROM a CROSS JOIN crs)"
    );

    if op == "move_batch" {
        let sql = format!(
            "{cte}, result AS (SELECT feature_id, ST_Transform(ST_Translate(g, $5, $6), 4326) AS g FROM m) \
             SELECT feature_id, ST_AsGeoJSON(g, 15) AS geo, ST_IsValid(g) AS valid, ST_NPoints(g) AS n \
             FROM result ORDER BY feature_id"
        );
        let qrows = sqlx::query(&sql)
            .bind(org_id)
            .bind(project_id)
            .bind(src.id)
            .bind(&req.primary.feature_ids)
            .bind(req.dx)
            .bind(req.dy)
            .fetch_all(&mut **scope.tx_mut())
            .await
            .map_err(op_err)?;
        if qrows.len() != req.primary.feature_ids.len() {
            return Err(EditError::detail(
                422,
                "Invalid or unsupported batch extent",
            ));
        }
        let mut items = Vec::with_capacity(qrows.len());
        for qr in &qrows {
            let valid: bool = qr
                .try_get::<Option<bool>, _>("valid")
                .map_err(op_err)?
                .unwrap_or(false);
            let n: i32 = qr
                .try_get::<Option<i32>, _>("n")
                .map_err(op_err)?
                .unwrap_or(i32::MAX);
            if !valid || n > 4096 {
                return Err(EditError::detail(
                    422,
                    "Invalid or unsupported batch extent",
                ));
            }
            let geo_str: String = qr.try_get("geo").map_err(op_err)?;
            let g: Value = serde_json::from_str(&geo_str)
                .map_err(|_| EditError::detail(422, "Batch move result cannot be stored"))?;
            if g.get("type").and_then(Value::as_str) != Some(src.geometry_family.as_str()) {
                return Err(EditError::detail(
                    422,
                    "Batch move must preserve geometry type",
                ));
            }
            validate_ready_geojson(&g, None, 4326)
                .map_err(|_| EditError::detail(422, "Batch move result cannot be stored"))?;
            let fid: String = qr.try_get("feature_id").map_err(op_err)?;
            items.push(json!({ "feature_id": fid, "geometry": g }));
        }
        return Ok(json!({
            "source_id": src.id,
            "content_revision": src.content_revision,
            "items": items,
        }));
    }

    if op == "nearest" || op == "spatial_join" {
        let relation = if op == "nearest" {
            "TRUE"
        } else {
            "ST_Intersects(a.geometry, b.geometry)"
        };
        let inner_limit = if op == "nearest" { 1 } else { 101 };
        let sql = format!(
            "WITH a AS ({primary_sql}), b AS ({secondary_sql}) \
             SELECT a.feature_id AS source_fid, b.feature_id AS target_fid, \
                    ST_Distance(a.geometry::geography, b.geometry::geography) AS distance_m, \
                    ST_AsGeoJSON(a.geometry, 15) AS geo \
             FROM a CROSS JOIN LATERAL ( \
                 SELECT b.* FROM b WHERE {relation} \
                 AND NOT (b.feature_id = a.feature_id AND $3 = $5) \
                 ORDER BY ST_Distance(a.geometry::geography, b.geometry::geography), b.feature_id \
                 LIMIT {inner_limit} \
             ) b LIMIT 101"
        );
        let mut q = sqlx::query(&sql)
            .bind(org_id)
            .bind(project_id)
            .bind(src.id)
            .bind(&req.primary.feature_ids)
            .bind(target_src_id.unwrap_or_default());
        if !target_ids.is_empty() {
            q = q.bind(&target_ids);
        }
        let qrows = q.fetch_all(&mut **scope.tx_mut()).await.map_err(op_err)?;
        if qrows.len() > 100 {
            return Err(EditError::detail(
                422,
                "More than 100 results; narrow the selection",
            ));
        }
        let mut items = Vec::with_capacity(qrows.len());
        for qr in &qrows {
            let geo_str: String = qr.try_get("geo").map_err(op_err)?;
            let geo: Value = serde_json::from_str(&geo_str)
                .map_err(|_| EditError::detail(422, "Invalid spatial result"))?;
            items.push(json!({
                "geometry": geo,
                "source_fid": qr.try_get::<String, _>("source_fid").map_err(op_err)?,
                "target_fid": qr.try_get::<String, _>("target_fid").map_err(op_err)?,
                "distance_m": qr.try_get::<f64, _>("distance_m").map_err(op_err)?,
            }));
        }
        return Ok(json!({
            "source_id": src.id,
            "content_revision": src.content_revision,
            "items": items,
        }));
    }

    let result: Vec<Value> = if op == "fillet" {
        let sql = format!("{cte} SELECT ST_AsGeoJSON(g, 9) AS g, srid FROM m");
        let r = sqlx::query(&sql)
            .bind(org_id)
            .bind(project_id)
            .bind(src.id)
            .bind(&req.primary.feature_ids)
            .fetch_all(&mut **scope.tx_mut())
            .await
            .map_err(op_err)?;
        if r.len() != 1 {
            return Err(EditError::detail(422, "Unsupported metric extent"));
        }
        let g_str: String = r[0].try_get("g").map_err(op_err)?;
        let srid: i32 = r[0].try_get("srid").map_err(op_err)?;
        let parsed: Value = serde_json::from_str(&g_str)
            .map_err(|_| EditError::detail(422, "Unsupported metric extent"))?;
        let coords_arr = parsed
            .get("coordinates")
            .and_then(Value::as_array)
            .ok_or_else(|| EditError::detail(422, "Unsupported metric extent"))?;
        let mut pts = Vec::with_capacity(coords_arr.len());
        for item in coords_arr {
            let arr = item
                .as_array()
                .ok_or_else(|| EditError::detail(422, "Unsupported metric extent"))?;
            if arr.len() < 2 {
                return Err(EditError::detail(422, "Unsupported metric extent"));
            }
            let x = arr[0]
                .as_f64()
                .ok_or_else(|| EditError::detail(422, "Unsupported metric extent"))?;
            let y = arr[1]
                .as_f64()
                .ok_or_else(|| EditError::detail(422, "Unsupported metric extent"))?;
            pts.push([x, y]);
        }
        let vertex_idx = usize::try_from(req.vertex).unwrap_or(usize::MAX);
        let filleted = fillet_points(&pts, vertex_idx, req.distance)?;
        let geo_payload = json!({
            "type": "LineString",
            "coordinates": filleted,
        })
        .to_string();
        let q = sqlx::query(
            "SELECT ST_AsGeoJSON(ST_Transform(ST_SetSRID(ST_GeomFromGeoJSON($1), $2), 4326), 15) AS geo",
        )
        .bind(geo_payload)
        .bind(srid)
        .fetch_all(&mut **scope.tx_mut())
        .await
        .map_err(op_err)?;
        q.iter()
            .filter_map(|row| row.try_get::<String, _>("geo").ok())
            .filter_map(|s| serde_json::from_str::<Value>(&s).ok())
            .collect()
    } else {
        let parts_suffix = ", parts AS (SELECT (ST_Dump(g)).geom AS g FROM result \
             WHERE g IS NOT NULL AND NOT ST_IsEmpty(g)) \
             SELECT ST_AsGeoJSON(g, 15) AS geo, ST_IsValid(g) AS valid, ST_NPoints(g) AS n \
             FROM parts LIMIT 101";

        let qrows = match op {
            "copy" => {
                let sql = format!(
                    "{cte}, result AS (SELECT ST_Transform(ST_Translate(g, $5, $6), 4326) AS g FROM m){parts_suffix}"
                );
                sqlx::query(&sql)
                    .bind(org_id)
                    .bind(project_id)
                    .bind(src.id)
                    .bind(&req.primary.feature_ids)
                    .bind(req.dx)
                    .bind(req.dy)
                    .fetch_all(&mut **scope.tx_mut())
                    .await
                    .map_err(op_err)?
            }
            "offset" | "simplify" | "extend" => {
                let expr = match op {
                    "offset" => "ST_OffsetCurve(g, $5)",
                    "simplify" => "ST_SimplifyPreserveTopology(g, $5)",
                    "extend" => {
                        "ST_SetPoint(g, ST_NPoints(g) - 1, \
                         ST_Translate(ST_EndPoint(g), \
                         $5 * sin(ST_Azimuth(ST_PointN(g, ST_NPoints(g) - 1), ST_EndPoint(g))), \
                         $5 * cos(ST_Azimuth(ST_PointN(g, ST_NPoints(g) - 1), ST_EndPoint(g)))))"
                    }
                    _ => unreachable!(),
                };
                let sql = format!(
                    "{cte}, result AS (SELECT ST_Transform({expr}, 4326) AS g FROM m){parts_suffix}"
                );
                sqlx::query(&sql)
                    .bind(org_id)
                    .bind(project_id)
                    .bind(src.id)
                    .bind(&req.primary.feature_ids)
                    .bind(req.distance)
                    .fetch_all(&mut **scope.tx_mut())
                    .await
                    .map_err(op_err)?
            }
            "rotate" => {
                let sql = format!(
                    "{cte}, result AS (SELECT ST_Transform(ST_Rotate(g, radians($5), ST_Centroid(g)), 4326) AS g FROM m){parts_suffix}"
                );
                sqlx::query(&sql)
                    .bind(org_id)
                    .bind(project_id)
                    .bind(src.id)
                    .bind(&req.primary.feature_ids)
                    .bind(req.angle)
                    .fetch_all(&mut **scope.tx_mut())
                    .await
                    .map_err(op_err)?
            }
            "scale" => {
                let expr = "ST_Translate(ST_Scale(ST_Translate(g, -ST_X(ST_Centroid(g)), \
                    -ST_Y(ST_Centroid(g))), $5, $5), ST_X(ST_Centroid(g)), ST_Y(ST_Centroid(g)))";
                let sql = format!(
                    "{cte}, result AS (SELECT ST_Transform({expr}, 4326) AS g FROM m){parts_suffix}"
                );
                sqlx::query(&sql)
                    .bind(org_id)
                    .bind(project_id)
                    .bind(src.id)
                    .bind(&req.primary.feature_ids)
                    .bind(req.factor)
                    .fetch_all(&mut **scope.tx_mut())
                    .await
                    .map_err(op_err)?
            }
            "stretch" => {
                let expr = "CASE WHEN $7 < ST_NPoints(g) THEN \
                    ST_SetPoint(g, $7, ST_Translate(ST_PointN(g, $7 + 1), $5, $6)) END";
                let sql = format!(
                    "{cte}, result AS (SELECT ST_Transform({expr}, 4326) AS g FROM m){parts_suffix}"
                );
                sqlx::query(&sql)
                    .bind(org_id)
                    .bind(project_id)
                    .bind(src.id)
                    .bind(&req.primary.feature_ids)
                    .bind(req.dx)
                    .bind(req.dy)
                    .bind(req.vertex)
                    .fetch_all(&mut **scope.tx_mut())
                    .await
                    .map_err(op_err)?
            }
            "trim" => {
                let sql = format!(
                    "WITH a AS ({primary_sql}), b AS ({secondary_sql}), \
                     cut AS (SELECT ST_UnaryUnion(ST_Collect(geometry)) AS cutter FROM b), \
                     pieces AS (SELECT (ST_Dump(ST_Split(a.geometry, \
                       CASE WHEN ST_Dimension(cutter) = 2 THEN ST_Boundary(cutter) ELSE cutter END))).geom AS g, \
                       a.geometry AS original FROM a CROSS JOIN cut), \
                     result AS (SELECT g FROM pieces WHERE ST_GeometryType(g) = 'ST_LineString' \
                       AND (SELECT count(*) FROM pieces) > 1 \
                       ORDER BY ST_Distance(g, ST_StartPoint(original)), ST_Length(g) DESC LIMIT 1){parts_suffix}"
                );
                let mut q = sqlx::query(&sql)
                    .bind(org_id)
                    .bind(project_id)
                    .bind(src.id)
                    .bind(&req.primary.feature_ids)
                    .bind(target_src_id.unwrap_or_default());
                if !target_ids.is_empty() {
                    q = q.bind(&target_ids);
                }
                q.fetch_all(&mut **scope.tx_mut()).await.map_err(op_err)?
            }
            "intersection" | "difference" | "union" => {
                let fn_name = match op {
                    "intersection" => "ST_Intersection",
                    "difference" => "ST_Difference",
                    "union" => "ST_Union",
                    _ => unreachable!(),
                };
                let sql = format!(
                    "WITH a AS ({primary_sql}), b AS ({secondary_sql}), \
                     u AS (SELECT ST_UnaryUnion(ST_Collect(geometry)) AS g FROM a), \
                     v AS (SELECT ST_UnaryUnion(ST_Collect(geometry)) AS g FROM b), \
                     result AS (SELECT {fn_name}(u.g, v.g) AS g FROM u CROSS JOIN v){parts_suffix}"
                );
                let mut q = sqlx::query(&sql)
                    .bind(org_id)
                    .bind(project_id)
                    .bind(src.id)
                    .bind(&req.primary.feature_ids)
                    .bind(target_src_id.unwrap_or_default());
                if !target_ids.is_empty() {
                    q = q.bind(&target_ids);
                }
                q.fetch_all(&mut **scope.tx_mut()).await.map_err(op_err)?
            }
            "centroid" | "hull" => {
                let fn_name = if op == "centroid" {
                    "ST_Centroid"
                } else {
                    "ST_ConvexHull"
                };
                let sql = format!(
                    "WITH a AS ({primary_sql}), \
                     result AS (SELECT {fn_name}(ST_UnaryUnion(ST_Collect(geometry))) AS g FROM a){parts_suffix}"
                );
                sqlx::query(&sql)
                    .bind(org_id)
                    .bind(project_id)
                    .bind(src.id)
                    .bind(&req.primary.feature_ids)
                    .fetch_all(&mut **scope.tx_mut())
                    .await
                    .map_err(op_err)?
            }
            _ => return Err(EditError::InvalidRequest),
        };

        if qrows.len() > 100 {
            return Err(EditError::detail(
                422,
                "Invalid or oversized result; narrow your selection",
            ));
        }
        let mut out = Vec::with_capacity(qrows.len());
        for qr in &qrows {
            let valid: bool = qr
                .try_get::<Option<bool>, _>("valid")
                .map_err(op_err)?
                .unwrap_or(false);
            let n: i32 = qr
                .try_get::<Option<i32>, _>("n")
                .map_err(op_err)?
                .unwrap_or(i32::MAX);
            if !valid || n > 4096 {
                return Err(EditError::detail(
                    422,
                    "Invalid or oversized result; narrow your selection",
                ));
            }
            let geo_str: String = qr.try_get("geo").map_err(op_err)?;
            let geo: Value = serde_json::from_str(&geo_str).map_err(|_| {
                EditError::detail(422, "Invalid or oversized result; narrow your selection")
            })?;
            out.push(geo);
        }
        out
    };

    if result.is_empty() {
        return Err(EditError::detail(422, "No geometry result"));
    }
    if cad
        && (result.len() != 1
            || result[0].get("type").and_then(Value::as_str) != Some(src.geometry_family.as_str()))
    {
        return Err(EditError::detail(
            422,
            "CAD result must be one geometry of the same type",
        ));
    }
    for g in &result {
        validate_ready_geojson(g, None, 4326).map_err(|_| {
            EditError::detail(
                422,
                "Result cannot be stored as a supported simple XY geometry",
            )
        })?;
    }
    Ok(json!({
        "source_id": src.id,
        "content_revision": src.content_revision,
        "items": result.into_iter().map(|g| json!({ "geometry": g })).collect::<Vec<_>>(),
    }))
}

/// Check the exact pinned population before running a topology rule.
async fn topology_population(
    scope: &mut EditScope<'_>,
    source: &PinnedSource,
    selection: &Selection,
) -> Result<(i64, i64), EditError> {
    if source.feature_count > 50_000 {
        return Err(EditError::detail(
            422,
            "Topology supports at most 50000 source features",
        ));
    }
    let row = sqlx::query(
        "SELECT count(*) AS total,
            count(*) FILTER (WHERE geometry_state = 'present' AND geometry IS NOT NULL) AS present,
            count(*) FILTER (WHERE content_revision <> $5 OR schema_revision <> $6) AS stale
         FROM spatial.reference_features
         WHERE organization_id = $1 AND project_id = $2 AND source_id = $3
           AND (cardinality($4::text[]) = 0 OR feature_id = ANY($4))",
    )
    .bind(scope.organization_id())
    .bind(scope.project_id())
    .bind(source.id)
    .bind(&selection.feature_ids)
    .bind(&source.content_revision)
    .bind(&source.schema_revision)
    .fetch_one(&mut **scope.tx_mut())
    .await
    .map_err(op_err)?;
    let total: i64 = row.try_get("total").map_err(db_err)?;
    let present: i64 = row.try_get("present").map_err(db_err)?;
    let stale: i64 = row.try_get("stale").map_err(db_err)?;
    let expected = if selection.feature_ids.is_empty() {
        source.feature_count
    } else {
        selection.feature_ids.len() as i64
    };
    if total != expected || stale != 0 {
        return Err(EditError::detail(
            409,
            "Topology population changed or requested features are missing",
        ));
    }
    Ok((total, present))
}

pub async fn topology(scope: &mut EditScope<'_>, req: TopologyParams) -> Result<Value, EditError> {
    set_timeout(scope).await?;
    let src = pinned(scope, &req.primary, false, false).await?;
    let org_id = scope.organization_id();
    let project_id = scope.project_id();
    let check = req.check.as_str();
    let (requested_count, scanned_count) = topology_population(scope, &src, &req.primary).await?;
    let skipped_count = requested_count - scanned_count;
    if req.secondary.is_some() && !matches!(check, "outside" | "gap") {
        return Err(EditError::InvalidRequest);
    }

    let primary_sql = selection_sql(
        "$3",
        if req.primary.feature_ids.is_empty() {
            None
        } else {
            Some("$4")
        },
    );

    let mut target_src_id: Option<Uuid> = None;
    let mut target_ids: Vec<String> = Vec::new();
    let mut secondary_pin = Value::Null;
    if matches!(check, "outside" | "gap") {
        let Some(ref sec) = req.secondary else {
            return Err(EditError::detail(422, "Reference boundary layer required"));
        };
        let target = pinned(scope, sec, false, false).await?;
        if target.geometry_family != "Polygon" || target.feature_count > 5000 {
            return Err(EditError::detail(
                422,
                "Reference boundary must be polygon with <=5000 objects",
            ));
        }
        if target.id == src.id {
            return Err(EditError::detail(
                422,
                "Reference boundary must be a different source",
            ));
        }
        let (requested, present) = topology_population(scope, &target, sec).await?;
        if requested == 0 || present != requested {
            return Err(EditError::detail(
                422,
                "Reference boundary has missing geometry or is empty",
            ));
        }
        if check == "gap" && skipped_count != 0 {
            return Err(EditError::detail(
                422,
                "Gap check needs complete primary geometry",
            ));
        }
        secondary_pin = json!({"source_id":target.id,"content_revision":target.content_revision,"schema_revision":target.schema_revision});
        target_src_id = Some(target.id);
        target_ids = sec.feature_ids.clone();
    }

    let qrows = match check {
        "duplicate" => {
            let sql = format!(
                "WITH a AS ({primary_sql}), \
                 g AS (SELECT ST_AsEWKB(geometry) AS k, min(feature_id) AS keeper, count(*) AS n \
                       FROM a GROUP BY ST_AsEWKB(geometry) HAVING count(*) > 1) \
                 SELECT a.feature_id, g.keeper AS other, ST_AsGeoJSON(a.geometry, 15) AS geo \
                 FROM a JOIN g ON ST_AsEWKB(a.geometry) = g.k WHERE a.feature_id <> g.keeper LIMIT 201"
            );
            let mut q = sqlx::query(&sql).bind(org_id).bind(project_id).bind(src.id);
            if !req.primary.feature_ids.is_empty() {
                q = q.bind(&req.primary.feature_ids);
            }
            q.fetch_all(&mut **scope.tx_mut()).await.map_err(op_err)?
        }
        "invalid" => {
            let sql = format!(
                "WITH a AS ({primary_sql}) \
                 SELECT feature_id, NULL::text AS other, ST_AsGeoJSON(geometry, 15) AS geo, \
                        ST_IsValidReason(geometry) AS reason \
                 FROM a WHERE NOT ST_IsValid(geometry) LIMIT 201"
            );
            let mut q = sqlx::query(&sql).bind(org_id).bind(project_id).bind(src.id);
            if !req.primary.feature_ids.is_empty() {
                q = q.bind(&req.primary.feature_ids);
            }
            q.fetch_all(&mut **scope.tx_mut()).await.map_err(op_err)?
        }
        "overlap" => {
            if src.geometry_family != "Polygon" {
                return Err(EditError::detail(422, "Polygon required"));
            }
            let pair = if req.primary.feature_ids.is_empty() {
                "b.feature_id > a.feature_id"
            } else {
                "b.feature_id <> a.feature_id AND (b.feature_id > a.feature_id OR NOT (b.feature_id = ANY($4)))"
            };
            let sql = format!(
                "WITH a AS ({primary_sql}) \
                 SELECT a.feature_id, b.feature_id AS other, \
                        ST_AsGeoJSON(ST_Intersection(a.geometry, b.geometry), 15) AS geo \
                 FROM a JOIN spatial.reference_features b \
                   ON b.organization_id = $1 AND b.project_id = $2 AND b.source_id = $3 \
                  AND ({pair}) AND b.geometry_state = 'present' \
                  AND b.geometry && a.geometry \
                 WHERE ST_Relate(a.geometry, b.geometry, '2********') LIMIT 201"
            );
            let mut q = sqlx::query(&sql).bind(org_id).bind(project_id).bind(src.id);
            if !req.primary.feature_ids.is_empty() {
                q = q.bind(&req.primary.feature_ids);
            }
            q.fetch_all(&mut **scope.tx_mut()).await.map_err(op_err)?
        }
        "dangling" => {
            if src.geometry_family != "LineString" {
                return Err(EditError::detail(422, "LineString required"));
            }
            let sql = format!(
                "WITH a AS ({primary_sql}), \
                 ends AS (SELECT feature_id, ST_StartPoint(geometry) AS p FROM a WHERE NOT ST_IsClosed(geometry) \
                          UNION ALL SELECT feature_id, ST_EndPoint(geometry) FROM a WHERE NOT ST_IsClosed(geometry)) \
                 SELECT e.feature_id, NULL::text AS other, ST_AsGeoJSON(e.p, 15) AS geo \
                 FROM ends e WHERE NOT EXISTS ( \
                     SELECT 1 FROM spatial.reference_features b \
                     WHERE b.organization_id = $1 AND b.project_id = $2 AND b.source_id = $3 \
                       AND b.feature_id <> e.feature_id AND b.geometry && e.p \
                       AND ST_Intersects(b.geometry, e.p) \
                 ) LIMIT 201"
            );
            let mut q = sqlx::query(&sql).bind(org_id).bind(project_id).bind(src.id);
            if !req.primary.feature_ids.is_empty() {
                q = q.bind(&req.primary.feature_ids);
            }
            q.fetch_all(&mut **scope.tx_mut()).await.map_err(op_err)?
        }
        "outside" | "gap" => {
            if check == "gap" && (src.geometry_family != "Polygon" || src.feature_count > 5000) {
                return Err(EditError::detail(422, "Gap check needs <=5000 polygons"));
            }
            let p_sql = selection_sql(
                "$3",
                if req.primary.feature_ids.is_empty() {
                    None
                } else {
                    Some("$4")
                },
            );
            let sec_src_param = if req.primary.feature_ids.is_empty() {
                "$4"
            } else {
                "$5"
            };
            let sec_ids_param = if target_ids.is_empty() {
                None
            } else if req.primary.feature_ids.is_empty() {
                Some("$5")
            } else {
                Some("$6")
            };
            let s_sql = selection_sql(sec_src_param, sec_ids_param);
            let sql = if check == "outside" {
                format!(
                    "WITH a AS ({p_sql}), b AS ({s_sql}), \
                     mask AS (SELECT ST_UnaryUnion(ST_Collect(geometry)) AS g FROM b) \
                     SELECT a.feature_id, NULL::text AS other, ST_AsGeoJSON(a.geometry, 15) AS geo \
                     FROM a CROSS JOIN mask WHERE NOT ST_CoveredBy(a.geometry, mask.g) LIMIT 201"
                )
            } else {
                format!(
                    "WITH a AS ({p_sql}), b AS ({s_sql}), \
                     coverage AS (SELECT ST_UnaryUnion(ST_Collect(geometry)) AS g FROM a), \
                     mask AS (SELECT ST_UnaryUnion(ST_Collect(geometry)) AS g FROM b), \
                     parts AS (SELECT (ST_Dump(ST_Difference(mask.g, COALESCE(coverage.g, ST_GeomFromText('POLYGON EMPTY',4326))))).geom AS g \
                               FROM coverage CROSS JOIN mask) \
                     SELECT NULL::text AS feature_id, NULL::text AS other, ST_AsGeoJSON(g, 15) AS geo \
                     FROM parts WHERE NOT ST_IsEmpty(g) LIMIT 201"
                )
            };
            let mut q = sqlx::query(&sql).bind(org_id).bind(project_id).bind(src.id);
            if !req.primary.feature_ids.is_empty() {
                q = q.bind(&req.primary.feature_ids);
            }
            q = q.bind(target_src_id.unwrap_or_default());
            if !target_ids.is_empty() {
                q = q.bind(&target_ids);
            }
            q.fetch_all(&mut **scope.tx_mut()).await.map_err(op_err)?
        }
        _ => return Err(EditError::InvalidRequest),
    };

    let complete = qrows.len() <= 200 && skipped_count == 0;
    let mut items = Vec::with_capacity(qrows.len().min(200));
    for qr in qrows.iter().take(200) {
        let feature_id: Option<String> = qr.try_get("feature_id").map_err(op_err)?;
        let other: Option<String> = qr.try_get("other").map_err(op_err)?;
        let geo_str: String = qr.try_get("geo").map_err(op_err)?;
        let geo: Value = serde_json::from_str(&geo_str)
            .map_err(|_| EditError::detail(422, "Invalid spatial result"))?;
        let mut item = json!({
            "feature_id": feature_id,
            "other": other,
            "geometry": geo,
        });
        if check == "invalid" {
            let reason: Option<String> = qr.try_get("reason").map_err(op_err)?;
            item["reason"] = json!(reason);
        }
        items.push(item);
    }

    Ok(json!({
        "source_id": src.id,
        "content_revision": src.content_revision,
        "schema_revision": src.schema_revision,
        "secondary_pin": secondary_pin,
        "requested_count": requested_count,
        "scanned_count": scanned_count,
        "skipped_count": skipped_count,
        "truncated": qrows.len() > 200,
        "complete": complete,
        "items": items,
    }))
}

pub async fn schema_change(
    scope: &mut EditScope<'_>,
    req: SchemaParams,
) -> Result<Value, EditError> {
    set_timeout(scope).await?;
    let src = pinned(scope, &req.primary, true, false).await?;
    let org_id = scope.organization_id();
    let project_id = scope.project_id();

    let field_rows = sqlx::query(
        "SELECT field_id, name, ordinal, type, nullable, schema_revision
         FROM spatial.source_fields
         WHERE organization_id = $1 AND project_id = $2 AND source_id = $3
         ORDER BY ordinal",
    )
    .bind(org_id)
    .bind(project_id)
    .bind(src.id)
    .fetch_all(&mut **scope.tx_mut())
    .await
    .map_err(db_err)?;

    let mut fields: Vec<FieldSpec> = field_rows
        .iter()
        .map(|row| {
            Ok(FieldSpec {
                field_id: row.try_get("field_id").map_err(db_err)?,
                name: row.try_get("name").map_err(db_err)?,
                ordinal: row.try_get("ordinal").map_err(db_err)?,
                field_type: row.try_get("type").map_err(db_err)?,
                nullable: row.try_get("nullable").map_err(db_err)?,
                schema_revision: row.try_get("schema_revision").map_err(db_err)?,
            })
        })
        .collect::<Result<Vec<_>, EditError>>()?;

    if fields
        .iter()
        .any(|f| f.name == req.name && f.field_id != req.field_id)
    {
        return Err(EditError::detail(422, "Field name exists"));
    }

    let existing_pos = fields.iter().position(|f| f.field_id == req.field_id);
    if req.action == "add" {
        if existing_pos.is_some() || fields.len() >= 100 {
            return Err(EditError::detail(422, "Field exists or schema full"));
        }
        fields.push(FieldSpec {
            field_id: req.field_id.clone(),
            name: req.name.clone(),
            ordinal: fields.len() as i32,
            field_type: req.field_type.clone(),
            nullable: true,
            schema_revision: src.schema_revision.clone(),
        });
    } else {
        let Some(pos) = existing_pos else {
            return Err(EditError::detail(422, "Field not found or unchanged"));
        };
        if fields[pos].name == req.name {
            return Err(EditError::detail(422, "Field not found or unchanged"));
        }
        fields[pos].name = req.name.clone();
    }

    let schema = schema_revision_digest(
        &fields,
        &src.geometry_family,
        &src.dimensions,
        &src.schema_state,
    );
    let content = format!(
        "{:x}",
        Sha256::digest(format!("{}{}schema-v1", src.content_revision, schema).as_bytes())
    );
    if !(1..MAX_REVISION).contains(&src.catalog_revision) {
        return Err(EditError::validation(
            "VALIDATION_ERROR",
            "catalog revision exhausted",
        ));
    }
    let new_catalog_rev = src.catalog_revision + 1;
    let now = now_micros();

    sqlx::query(
        "UPDATE spatial.source_fields SET schema_revision = $4
         WHERE organization_id = $1 AND project_id = $2 AND source_id = $3",
    )
    .bind(org_id)
    .bind(project_id)
    .bind(src.id)
    .bind(&schema)
    .execute(&mut **scope.tx_mut())
    .await
    .map_err(db_err)?;

    if req.action == "add" {
        let ordinal = (fields.len() - 1) as i32;
        sqlx::query(
            "INSERT INTO spatial.source_fields(
                organization_id, project_id, source_id, field_id, name,
                ordinal, type, nullable, schema_revision
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, true, $8)",
        )
        .bind(org_id)
        .bind(project_id)
        .bind(src.id)
        .bind(&req.field_id)
        .bind(&req.name)
        .bind(ordinal)
        .bind(&req.field_type)
        .bind(&schema)
        .execute(&mut **scope.tx_mut())
        .await
        .map_err(db_err)?;

        sqlx::query(
            "UPDATE spatial.reference_features
             SET attributes = attributes || jsonb_build_object(CAST($4 AS text), NULL::text)
             WHERE organization_id = $1 AND project_id = $2 AND source_id = $3",
        )
        .bind(org_id)
        .bind(project_id)
        .bind(src.id)
        .bind(&req.field_id)
        .execute(&mut **scope.tx_mut())
        .await
        .map_err(db_err)?;
    } else {
        sqlx::query(
            "UPDATE spatial.source_fields SET name = $5
             WHERE organization_id = $1 AND project_id = $2 AND source_id = $3 AND field_id = $4",
        )
        .bind(org_id)
        .bind(project_id)
        .bind(src.id)
        .bind(&req.field_id)
        .bind(&req.name)
        .execute(&mut **scope.tx_mut())
        .await
        .map_err(db_err)?;
    }

    sqlx::query(
        "UPDATE spatial.reference_features
         SET schema_revision = $4, content_revision = $5
         WHERE organization_id = $1 AND project_id = $2 AND source_id = $3",
    )
    .bind(org_id)
    .bind(project_id)
    .bind(src.id)
    .bind(&schema)
    .bind(&content)
    .execute(&mut **scope.tx_mut())
    .await
    .map_err(db_err)?;

    sqlx::query(
        "UPDATE spatial.data_sources
         SET schema_revision = $4, content_revision = $5, catalog_revision = $6, updated_at = $7
         WHERE organization_id = $1 AND project_id = $2 AND id = $3",
    )
    .bind(org_id)
    .bind(project_id)
    .bind(src.id)
    .bind(&schema)
    .bind(&content)
    .bind(new_catalog_rev)
    .bind(now)
    .execute(&mut **scope.tx_mut())
    .await
    .map_err(db_err)?;

    let change_id = Uuid::new_v4();
    let audit_id = Uuid::new_v4();
    let actor_type = scope.actor_kind();
    let actor_id = scope.actor_id();
    let actor_label: String = if scope.display_name().is_empty() {
        actor_id.to_string()
    } else {
        scope.display_name().chars().take(200).collect()
    };

    sqlx::query(
        "INSERT INTO audit.audit_events (
            id, organization_id, actor_type, actor_id, actor_display_snapshot,
            action, resource_type, resource_id, outcome, metadata, created_at
         ) VALUES ($1, $2, $3, $4, $5, 'spatial.catalog.update_schema', 'data_source', $6, 'success', $7, $8)",
    )
    .bind(audit_id)
    .bind(org_id)
    .bind(actor_type)
    .bind(actor_id)
    .bind(&actor_label)
    .bind(src.id)
    .bind(json!({
        "project_id": project_id.to_string(),
        "catalog_change_id": change_id.to_string(),
    }))
    .bind(now)
    .execute(&mut **scope.tx_mut())
    .await
    .map_err(db_err)?;

    let prior_rev = json!({
        "catalog_revision": src.catalog_revision,
        "content_revision": src.content_revision,
        "schema_revision": src.schema_revision,
    });
    let new_rev = json!({
        "catalog_revision": new_catalog_rev,
        "content_revision": content,
        "schema_revision": schema,
    });
    let changes = json!([
        {
            "field": "catalog_revision",
            "old_value": src.catalog_revision,
            "new_value": new_catalog_rev,
        },
        {
            "field": "content_revision",
            "old_value": src.content_revision,
            "new_value": content,
        },
        {
            "field": "schema_revision",
            "old_value": src.schema_revision,
            "new_value": schema,
        },
    ]);

    sqlx::query(
        "INSERT INTO spatial.catalog_changes (
            id, organization_id, project_id, resource_type, resource_id,
            source_id, layer_id, actor_type, actor_id, actor_display_snapshot,
            operation, created_at, audit_event_id, prior_revision, new_revision, changes
         ) VALUES (
            $1, $2, $3, 'data_source', $4,
            $4, NULL, $5, $6, $7,
            'update_schema', $8, $9, $10, $11, $12
         )",
    )
    .bind(change_id)
    .bind(org_id)
    .bind(project_id)
    .bind(src.id)
    .bind(actor_type)
    .bind(actor_id)
    .bind(&actor_label)
    .bind(now)
    .bind(audit_id)
    .bind(prior_rev)
    .bind(new_rev)
    .bind(changes)
    .execute(&mut **scope.tx_mut())
    .await
    .map_err(db_err)?;

    Ok(json!({
        "source_id": src.id,
        "schema_revision": schema,
        "content_revision": content,
        "field_id": req.field_id,
    }))
}

pub enum TileResult {
    NotModified {
        etag: String,
    },
    Data {
        bytes: Vec<u8>,
        etag: Option<String>,
    },
}

#[allow(clippy::too_many_arguments)]
pub async fn tile(
    scope: &mut EditScope<'_>,
    layer_id: Uuid,
    z: i64,
    x: i64,
    y: i64,
    content_revision: &str,
    schema_revision: &str,
    if_none_match: Option<&str>,
) -> Result<TileResult, EditError> {
    if !(0..=22).contains(&z) {
        return Err(EditError::detail(422, "Invalid tile"));
    }
    let limit = 1_i64 << z;
    if !(0..limit).contains(&x) || !(0..limit).contains(&y) {
        return Err(EditError::detail(422, "Invalid tile"));
    }
    set_timeout(scope).await?;
    let spec = Selection {
        layer_id,
        content_revision: content_revision.to_owned(),
        schema_revision: schema_revision.to_owned(),
        feature_ids: Vec::new(),
    };
    let src = pinned(scope, &spec, false, true).await?;

    let restricted = scope.is_scoped();
    let etag = if !restricted {
        let cred_str = scope
            .credential_id()
            .map_or_else(|| "None".to_owned(), |id| id.to_string());
        // Match Python `json.dumps(identity)` default separators `(", ", ": ")`.
        let identity = format!(
            "[\"mvt-v3\", \"{}\", \"{}\", \"{}\", \"{}\", \"{}\", \"{}\", \"{}\", \"{}\", {}, {}, {}]",
            scope.actor_id(),
            scope.actor_kind(),
            cred_str,
            scope.organization_id(),
            scope.project_id(),
            layer_id,
            content_revision,
            schema_revision,
            z,
            x,
            y,
        );
        let tag = format!("\"{:x}\"", Sha256::digest(identity.as_bytes()));
        if let Some(supplied) = if_none_match {
            let matched = supplied
                .split(',')
                .map(|v| {
                    let s = v.trim();
                    s.strip_prefix("W/").unwrap_or(s)
                })
                .any(|v| v == tag);
            if matched {
                return Ok(TileResult::NotModified { etag: tag });
            }
        }
        Some(tag)
    } else {
        None
    };

    let org_id = scope.organization_id();
    let project_id = scope.project_id();
    let display = if src.stored_crs == 3857 {
        "geometry"
    } else {
        "ST_Transform(geometry,3857)"
    };
    let native_bounds = if src.stored_crs == 3857 {
        "b"
    } else {
        "ST_Transform(b,4326)"
    };
    let base = format!(
        "WITH bounds AS (SELECT ST_TileEnvelope($4, $5, $6) AS b), \
         source AS (SELECT feature_id, {display} AS g, b \
                    FROM spatial.reference_features CROSS JOIN bounds \
                    WHERE organization_id = $1 AND project_id = $2 \
                      AND source_id = $3 AND geometry_state = 'present' \
                      AND geometry && {native_bounds})"
    );
    let sql = if src.geometry_family == "Point" && z < 17 {
        format!(
            "{base}, clusters AS (SELECT ST_SnapToGrid(g, (ST_XMax(b) - ST_XMin(b))/32) AS p, \
             ST_Centroid(ST_Collect(g)) AS g, count(*) AS count, min(feature_id) AS fid, b \
             FROM source GROUP BY p, b), \
             tile AS (SELECT CASE WHEN count = 1 THEN fid ELSE '' END AS feature_id, \
             count AS cluster_count, ST_AsMVTGeom(g, b, 4096, 64, true) AS geom FROM clusters) \
             SELECT ST_AsMVT(tile, 'features', 4096, 'geom') AS tile FROM tile"
        )
    } else {
        format!(
            "{base}, tile AS (SELECT feature_id, 1 AS cluster_count, \
             ST_AsMVTGeom(ST_SimplifyPreserveTopology(g, (ST_XMax(b) - ST_XMin(b))/4096), b, 4096, 64, true) AS geom \
             FROM source ORDER BY feature_id LIMIT 20000) \
             SELECT ST_AsMVT(tile, 'features', 4096, 'geom') AS tile FROM tile"
        )
    };
    let row = sqlx::query(&sql)
        .bind(org_id)
        .bind(project_id)
        .bind(src.id)
        .bind(z as i32)
        .bind(x as i32)
        .bind(y as i32)
        .fetch_one(&mut **scope.tx_mut())
        .await
        .map_err(op_err)?;
    let data: Vec<u8> = row
        .try_get::<Option<Vec<u8>>, _>("tile")
        .map_err(op_err)?
        .unwrap_or_default();
    Ok(TileResult::Data { bytes: data, etag })
}

fn now_micros() -> chrono::DateTime<Utc> {
    Utc::now().trunc_subsecs(6)
}
