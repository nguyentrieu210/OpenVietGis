//! E04 RLS-enforced PostGIS repository for typed attribute & bounded spatial queries.
//!
//! Mirrors `apps/api/openvietgis_api/adapters/persistence/query_repository.py`.
//! Every statement runs inside the caller's [`ProjectRead`] transaction, so the
//! tenant GUCs and the `gis_access.feature_allowed` RLS policies always apply.
//!
//! Query shape (see `evidence/e04-implementation.md`): the reference
//! implementation scans the whole source three times (snapshot validation,
//! COUNT, ROW_NUMBER page). Here the snapshot validation and the matched count
//! share one scan, and the page is read in primary-key (or typed sort) order
//! with LIMIT so PostgreSQL can stop early.

pub mod domain;

use std::fmt::Write;

use chrono::{DateTime, Utc};
use ovg_core::crs::{
    reference_transformation, FixtureRequirement, WEB_MERCATOR_MAX_COORDINATE_METRE,
};
use ovg_core::AppError;
use serde_json::{json, Value};
use sqlx::postgres::{PgArguments, PgRow};
use sqlx::query::Query;
use sqlx::{Postgres, Row};
use uuid::Uuid;

use crate::catalog::ProjectRead;
use domain::{
    canonical_manifest_digest, compile_filter, compile_sort, invalid_spatial,
    is_canonical_attribute, parse_drawn_area, too_complex, valid_revision_token, validate_ready,
    FieldType, Geom, Param, SourceField, Sql, MAX_PAGE_POSITIONS, MAX_QUERY_FEATURES,
    MAX_QUERY_LIMIT, MAX_QUERY_OFFSET, MAX_QUERY_POSITIONS, MAX_REFERENCE_FEATURES,
    MAX_REFERENCE_POSITIONS, QUERY_STATEMENT_TIMEOUT,
};

const WEB_MERCATOR_PIPELINE: &str =
    "+proj=pipeline +step +proj=unitconvert +xy_in=deg +xy_out=rad +step +proj=webmerc +ellps=WGS84";

const SAVED_COLUMNS: &str = "id, organization_id, project_id, layer_id, name, description, \
    attribute_filter, spatial_filter, sort_field, sort_direction, created_by_actor_id, \
    created_at, updated_at";

/// Validated `ExecuteQueryRequest` body.
pub struct QueryRequest {
    /// Opt-in attribute-only mode includes rows with non-present geometry.
    pub attribute_only: bool,
    pub attribute_filter: Option<Value>,
    pub spatial_filter: Option<Value>,
    pub expected_content_revision: Option<String>,
    pub expected_schema_revision: Option<String>,
    pub expected_query_fingerprint: Option<String>,
    pub sort_field: Option<String>,
    pub sort_direction: String,
    pub limit: i64,
    pub offset: i64,
}

/// Validated `CreateSavedQueryRequest` body.
pub struct SavedQueryInput {
    pub layer_id: Uuid,
    pub name: String,
    pub description: String,
    pub attribute_filter: Option<Value>,
    pub spatial_filter: Option<Value>,
    pub sort_field: Option<String>,
    pub sort_direction: String,
}

fn bound(sql: &str, params: Vec<Param>) -> Query<'_, Postgres, PgArguments> {
    let mut query = sqlx::query(sql);
    for param in params {
        query = match param {
            Param::Text(v) | Param::Numeric(v) => query.bind(v),
            Param::TextArray(v) => query.bind(v),
            Param::Int(v) => query.bind(v),
            Param::I32(v) => query.bind(v),
            Param::Float(v) => query.bind(v),
            Param::Bool(v) => query.bind(v),
            Param::Timestamp(v) => query.bind(v),
            Param::Uuid(v) => query.bind(v),
            Param::Bytes(v) => query.bind(v),
        };
    }
    query
}

/// Database failures during a query never leak SQL; a timeout is distinguishable.
fn unavailable(error: sqlx::Error) -> AppError {
    let timeout =
        matches!(&error, sqlx::Error::Database(db) if db.code().as_deref() == Some("57014"));
    tracing::warn!(%error, "query statement failed");
    AppError::service_unavailable(
        if timeout {
            "QUERY_TIMEOUT"
        } else {
            "QUERY_UNAVAILABLE"
        },
        "query could not be completed safely; narrow or retry",
    )
}

fn internal(error: sqlx::Error) -> AppError {
    AppError::internal(format!("saved query database operation failed: {error}"))
}

fn invalid_source() -> AppError {
    AppError::validation("INVALID_QUERY_SOURCE", "invalid query snapshot or revision")
}

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut out, b| {
            let _ = write!(out, "{b:02x}");
            out
        })
}

struct Source {
    layer_id: Uuid,
    id: Uuid,
    stored_crs: i32,
    family: String,
    feature_count: Option<i64>,
    content_revision: String,
    schema_revision: String,
    fields: Vec<SourceField>,
}

impl Source {
    fn pin(&self) -> Value {
        json!({
            "layer_id": self.layer_id,
            "source_id": self.id,
            "content_revision": self.content_revision,
            "schema_revision": self.schema_revision,
            "stored_crs": self.stored_crs,
        })
    }

    fn check_feature_limit(&self) -> Result<(), AppError> {
        if self.feature_count.is_none_or(|c| c > MAX_QUERY_FEATURES) {
            return Err(too_complex("target exceeds query feature limit"));
        }
        Ok(())
    }
}

async fn set_timeout(read: &mut ProjectRead<'_>) -> Result<(), AppError> {
    sqlx::query("SELECT set_config('statement_timeout', $1, true)")
        .bind(QUERY_STATEMENT_TIMEOUT)
        .execute(&mut **read.transaction())
        .await
        .map_err(unavailable)?;
    Ok(())
}

async fn resolve(read: &mut ProjectRead<'_>, layer_id: Uuid) -> Result<Source, AppError> {
    let (organization_id, project_id) = (read.organization_id(), read.project_id());
    let row = sqlx::query(
        "SELECT s.id, s.lifecycle, s.validation_state, s.stored_crs, s.geometry_family,
                s.feature_count::bigint AS feature_count, s.content_revision, s.schema_revision
         FROM spatial.layers l
         JOIN spatial.data_sources s
           ON s.organization_id = l.organization_id
          AND s.project_id = l.project_id
          AND s.id = l.source_id
         WHERE l.organization_id = $1 AND l.project_id = $2 AND l.id = $3
           AND l.lifecycle = 'active'
         FOR SHARE OF s, l",
    )
    .bind(organization_id)
    .bind(project_id)
    .bind(layer_id)
    .fetch_optional(&mut **read.transaction())
    .await
    .map_err(unavailable)?
    .ok_or_else(|| AppError::not_found("layer not found"))?;
    let lifecycle: String = row.try_get("lifecycle").map_err(unavailable)?;
    let state: String = row.try_get("validation_state").map_err(unavailable)?;
    if lifecycle != "active" || state != "ready" {
        return Err(AppError::validation(
            "SOURCE_NOT_READY",
            "layer source is not ready",
        ));
    }
    let id: Uuid = row.try_get("id").map_err(unavailable)?;
    let schema_revision: String = row.try_get("schema_revision").map_err(unavailable)?;
    let field_rows = sqlx::query(
        "SELECT field_id, type, nullable, schema_revision
         FROM spatial.source_fields
         WHERE organization_id = $1 AND project_id = $2 AND source_id = $3
         ORDER BY ordinal ASC",
    )
    .bind(organization_id)
    .bind(project_id)
    .bind(id)
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(unavailable)?;
    let mut fields = Vec::with_capacity(field_rows.len());
    for field in &field_rows {
        let revision: String = field.try_get("schema_revision").map_err(unavailable)?;
        if revision != schema_revision {
            return Err(AppError::conflict(
                "STALE_SCHEMA_REVISION",
                "source schema pins disagree",
            ));
        }
        let kind: String = field.try_get("type").map_err(unavailable)?;
        fields.push(SourceField {
            field_id: field.try_get("field_id").map_err(unavailable)?,
            field_type: FieldType::parse(&kind)
                .ok_or_else(|| AppError::internal("unsupported persisted field type"))?,
            nullable: field.try_get("nullable").map_err(unavailable)?,
        });
    }
    Ok(Source {
        layer_id,
        id,
        stored_crs: row
            .try_get::<Option<i32>, _>("stored_crs")
            .map_err(unavailable)?
            .ok_or_else(|| AppError::internal("ready source without stored CRS"))?,
        family: row.try_get("geometry_family").map_err(unavailable)?,
        feature_count: row.try_get("feature_count").map_err(unavailable)?,
        content_revision: row.try_get("content_revision").map_err(unavailable)?,
        schema_revision,
        fields,
    })
}

/// Per-row snapshot validity: pins, native CRS/family, topology, domain and
/// declared attribute types (never turn malformed input into a zero match).
fn row_valid(source: &Source, sql: &mut Sql, attribute_only: bool) -> String {
    let crev = sql.bind(Param::Text(source.content_revision.clone()));
    let srev = sql.bind(Param::Text(source.schema_revision.clone()));
    let geometry_check = if attribute_only {
        String::new()
    } else {
        let srid = sql.bind(Param::I32(source.stored_crs));
        let gtype = sql.bind(Param::Text(format!("ST_{}", source.family)));
        let domain = match source.stored_crs {
            4326 | 4756 => format!(
                " AND ST_CoveredBy(rf.geometry, ST_MakeEnvelope(-180, -90, 180, 90, {srid})) \
                 AND ST_XMax(rf.geometry) - ST_XMin(rf.geometry) <= 180"
            ),
            3857 => {
                let m = sql.bind(Param::Float(WEB_MERCATOR_MAX_COORDINATE_METRE));
                format!(
                    " AND ST_CoveredBy(rf.geometry, ST_MakeEnvelope(-{m}, -{m}, {m}, {m}, {srid}))"
                )
            }
            _ => String::new(),
        };
        format!(" AND rf.geometry_state = 'present' AND ST_SRID(rf.geometry) = {srid} AND ST_GeometryType(rf.geometry) = {gtype} AND ST_Zmflag(rf.geometry) = 0 AND ST_NPoints(rf.geometry) BETWEEN 1 AND 4096 AND NOT ST_IsEmpty(rf.geometry) AND ST_IsValid(rf.geometry){domain}")
    };
    let mut checks = vec!["jsonb_typeof(rf.attributes) = 'object'".to_owned()];
    if !source.fields.is_empty() {
        let keys = source.fields.iter().map(|f| f.field_id.clone()).collect();
        let keys = sql.bind(Param::TextArray(keys));
        checks.push(format!("rf.attributes ?& {keys}"));
    }
    let count = sql.bind(Param::Int(source.fields.len() as i64));
    checks.push(format!(
        "(SELECT count(*) FROM jsonb_object_keys(CASE WHEN jsonb_typeof(rf.attributes) = 'object' THEN rf.attributes ELSE '{{}}'::jsonb END)) = {count}"
    ));
    for field in &source.fields {
        let key = sql.bind(Param::Text(field.field_id.clone()));
        let value = format!("(rf.attributes -> {key})");
        let scalar = format!("(rf.attributes ->> {key})");
        let check = match field.field_type {
            FieldType::Text => {
                format!("jsonb_typeof({value}) = 'string' AND char_length({scalar}) <= 4096")
            }
            FieldType::Integer => format!(
                "jsonb_typeof({value}) = 'number' AND CASE WHEN {scalar} ~ '^-?[0-9]{{1,10}}$' \
                 THEN CAST({scalar} AS numeric) BETWEEN -2147483648 AND 2147483647 ELSE false END"
            ),
            FieldType::Decimal => format!(
                "jsonb_typeof({value}) = 'string' \
                 AND {scalar} ~ '^-?(0|[1-9][0-9]*)([.][0-9]*[1-9])?$' AND {scalar} <> '-0' \
                 AND char_length(replace(replace({scalar}, '-', ''), '.', '')) <= 38 \
                 AND char_length(split_part({scalar}, '.', 2)) <= 18"
            ),
            FieldType::Boolean => format!("jsonb_typeof({value}) = 'boolean'"),
            FieldType::Timestamp => {
                format!("jsonb_typeof({value}) = 'string' AND {scalar} ~ '^[0-9]{{4}}-.*Z$'")
            }
        };
        let check = if field.nullable {
            format!("jsonb_typeof({value}) = 'null' OR ({check})")
        } else {
            check
        };
        checks.push(format!("({check})"));
    }
    format!(
        "rf.content_revision = {crev} AND rf.schema_revision = {srev} \
         {geometry_check} AND ({})",
        checks.join(" AND ")
    )
}

/// One RLS-filtered scan: validate the whole ready snapshot and, for a target,
/// count matches of the compiled filter in the same pass.
async fn scan(
    read: &mut ProjectRead<'_>,
    source: &Source,
    target: Option<(Option<&Value>, &Spatial)>,
    attribute_only: bool,
) -> Result<i64, AppError> {
    source.check_feature_limit()?;
    let mut sql = Sql::default();
    let org = sql.bind(Param::Uuid(read.organization_id()));
    let project = sql.bind(Param::Uuid(read.project_id()));
    let src = sql.bind(Param::Uuid(source.id));
    let valid = row_valid(source, &mut sql, attribute_only);
    let position_count = if attribute_only {
        "0::bigint"
    } else {
        "COALESCE(sum(ST_NPoints(rf.geometry)), 0)::bigint"
    };
    let (cte, hit) = match target {
        Some((filter, spatial)) => {
            let filter = compile_filter(filter, &source.fields, &mut sql)?;
            let (cte, clause) = spatial.render(&mut sql);
            (cte, format!("({filter}) AND ({clause})"))
        }
        None => (String::new(), "false".to_owned()),
    };
    // OFFSET 0 keeps the validity expression evaluated once per row; CASE
    // guarantees typed casts only run on rows that already passed validation.
    let text = format!(
        "{cte} SELECT count(*) AS cnt,
                {position_count} AS positions,
                COALESCE(bool_and(v.ok), true) AS valid,
                count(*) FILTER (WHERE CASE WHEN v.ok THEN {hit} ELSE false END) AS matched
         FROM spatial.reference_features rf
         CROSS JOIN LATERAL (SELECT ({valid}) IS TRUE AS ok OFFSET 0) v
         WHERE rf.organization_id = {org} AND rf.project_id = {project} AND rf.source_id = {src}"
    );
    let row = bound(&text, sql.params)
        .fetch_one(&mut **read.transaction())
        .await
        .map_err(unavailable)?;
    let count: i64 = row.try_get("cnt").map_err(unavailable)?;
    let positions: i64 = row.try_get("positions").map_err(unavailable)?;
    let valid: bool = row.try_get("valid").map_err(unavailable)?;
    if Some(count) != source.feature_count || !valid {
        return Err(AppError::validation(
            "INVALID_QUERY_SOURCE",
            "source does not match ready snapshot",
        ));
    }
    if positions > MAX_QUERY_POSITIONS {
        return Err(too_complex("target exceeds position limit"));
    }
    row.try_get("matched").map_err(unavailable)
}

#[derive(Default)]
struct Spatial {
    function: Option<&'static str>,
    geometries: Vec<Vec<u8>>,
    srid: i32,
    reference_pin: Value,
    operation: Option<&'static str>,
}

impl Spatial {
    /// Returns an optional statement prefix (CTE) and the predicate.
    fn render(&self, sql: &mut Sql) -> (String, String) {
        let Some(function) = self.function else {
            return (String::new(), "TRUE".to_owned());
        };
        let srid = sql.bind(Param::I32(self.srid));
        if let [single] = self.geometries.as_slice() {
            // A constant geometry operand keeps the predicate index-eligible.
            let wkb = sql.bind(Param::Bytes(single.clone()));
            return (
                String::new(),
                format!("{function}(rf.geometry, ST_GeomFromWKB({wkb}, {srid}))"),
            );
        }
        let list = sql.bind(Param::TextArray(
            self.geometries.iter().map(|g| hex(g.as_slice())).collect(),
        ));
        (
            format!(
                "WITH ref_geometries AS MATERIALIZED (
                    SELECT ST_GeomFromWKB(decode(r.wkb, 'hex'), {srid}) AS geometry
                    FROM unnest({list}) AS r(wkb)
                 )"
            ),
            format!(
                "EXISTS (SELECT 1 FROM ref_geometries rg WHERE {function}(rf.geometry, rg.geometry))"
            ),
        )
    }
}

/// Only explicit, non-fixture, single-step verified edges; never an implicit
/// ST_Transform or guessed CRS.
async fn checked_geometry(
    read: &mut ProjectRead<'_>,
    geometry: &Geom,
    srid: i32,
    target: i32,
) -> Result<(Vec<u8>, Option<&'static str>), AppError> {
    let unsupported = |message| AppError::validation("UNSUPPORTED_CRS", message);
    let mut profile = None;
    if srid != target {
        let edge = u32::try_from(srid)
            .ok()
            .zip(u32::try_from(target).ok())
            .and_then(|(s, t)| reference_transformation(s, t))
            .filter(|edge| edge.fixture_requirement == FixtureRequirement::None)
            .ok_or_else(|| unsupported("no accepted query CRS transformation"))?;
        if matches!(srid, 4326 | 4756) {
            let positions =
                validate_ready(geometry, None, srid, false).map_err(|()| invalid_source())?;
            if positions
                .iter()
                .any(|&(x, y)| !edge.geographic_bounds.contains(x, y, 0.0))
            {
                return Err(unsupported("geometry outside transformation area"));
            }
        }
        profile = Some(edge);
    }
    let mut sql = Sql::default();
    let wkb = sql.bind(Param::Bytes(geometry.to_wkb()));
    let source_srid = sql.bind(Param::I32(srid));
    let expr = if profile.is_some() {
        let pipeline = sql.bind(Param::Text(WEB_MERCATOR_PIPELINE.to_owned()));
        let target_srid = sql.bind(Param::I32(target));
        let function = if srid == 3857 {
            "ST_InverseTransformPipeline"
        } else {
            "ST_TransformPipeline"
        };
        format!("{function}(native.geom, {pipeline}, {target_srid})")
    } else {
        "native.geom".to_owned()
    };
    let text = format!(
        "WITH native AS MATERIALIZED (
             SELECT ST_GeomFromWKB({wkb}, {source_srid}) AS geom
         ), result AS MATERIALIZED (
             SELECT CASE WHEN ST_IsValid(native.geom) THEN {expr} END AS geom FROM native
         )
         SELECT ST_IsValid(geom) AS valid, ST_SRID(geom) AS srid,
             CASE WHEN ST_NPoints(geom) BETWEEN 1 AND 4096 AND ST_Zmflag(geom) = 0
                 AND NOT ST_IsEmpty(geom) THEN ST_AsBinary(geom, 'NDR') END AS wkb
         FROM result"
    );
    let row = bound(&text, sql.params)
        .fetch_one(&mut **read.transaction())
        .await
        .map_err(unavailable)?;
    let valid: Option<bool> = row.try_get("valid").map_err(unavailable)?;
    let out_srid: Option<i32> = row.try_get("srid").map_err(unavailable)?;
    let out: Option<Vec<u8>> = row.try_get("wkb").map_err(unavailable)?;
    let invalid = || invalid_spatial("invalid query geometry");
    if valid != Some(true) || out_srid != Some(target) {
        return Err(invalid());
    }
    let checked = out
        .as_deref()
        .and_then(Geom::from_wkb)
        .ok_or_else(invalid)?;
    let positions = validate_ready(&checked, Some(geometry.family()), target, profile.is_some())
        .map_err(|()| invalid())?;
    if let Some(edge) = profile {
        if matches!(target, 4326 | 4756)
            && positions
                .iter()
                .any(|&(x, y)| !edge.geographic_bounds.contains(x, y, 0.0))
        {
            return Err(unsupported("geometry outside transformation area"));
        }
    }
    Ok((checked.to_wkb(), profile.map(|edge| edge.operation_id)))
}

async fn spatial_clause(
    read: &mut ProjectRead<'_>,
    source: &Source,
    filter: Option<&Value>,
) -> Result<Spatial, AppError> {
    let Some(map) = filter.and_then(Value::as_object) else {
        return Ok(Spatial::default());
    };
    let predicate = map
        .get("predicate")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid_spatial("invalid spatial predicate"))?;
    let function = match predicate {
        "intersects" => "ST_Intersects",
        "within" => "ST_Within",
        "contains" => "ST_Contains",
        _ => return Err(invalid_spatial("unsupported spatial predicate")),
    };
    let mut spatial = Spatial {
        function: Some(function),
        srid: source.stored_crs,
        ..Spatial::default()
    };
    let drawn = map.len() == 3 && map.contains_key("geometry") && map.contains_key("crs_code");
    let reference = map.contains_key("reference_layer_id")
        && map.keys().all(|k| {
            matches!(
                k.as_str(),
                "predicate" | "reference_layer_id" | "reference_feature_id"
            )
        });
    if drawn {
        let (area, srid) = parse_drawn_area(&map["geometry"], &map["crs_code"])?;
        let (wkb, operation) = checked_geometry(read, &area, srid, source.stored_crs).await?;
        spatial.geometries.push(wkb);
        spatial.operation = operation;
        return Ok(spatial);
    }
    if !reference {
        return Err(invalid_spatial(
            "provide exactly one spatial input with explicit CRS if drawn",
        ));
    }
    let reference_layer = map["reference_layer_id"]
        .as_str()
        .and_then(|id| Uuid::parse_str(id).ok())
        .ok_or_else(|| invalid_spatial("invalid reference layer identifier"))?;
    let reference = resolve(read, reference_layer).await?;
    spatial.reference_pin = reference.pin();
    let feature_id = match map.get("reference_feature_id") {
        None | Some(Value::Null) => None,
        Some(Value::String(id)) if (1..=128).contains(&id.chars().count()) => Some(id.clone()),
        Some(_) => return Err(invalid_spatial("invalid reference feature identifier")),
    };
    scan(read, &reference, None, false).await?;
    if feature_id.is_none()
        && reference
            .feature_count
            .is_none_or(|c| c > MAX_REFERENCE_FEATURES)
    {
        return Err(too_complex("reference set exceeds feature limit"));
    }
    let rows = sqlx::query(
        "WITH refs AS MATERIALIZED (
             SELECT geometry, feature_id FROM spatial.reference_features
             WHERE organization_id = $1 AND project_id = $2 AND source_id = $3
               AND content_revision = $4 AND schema_revision = $5
               AND ($6::text IS NULL OR feature_id = $6)
             ORDER BY feature_id LIMIT 501
         )
         SELECT (SUM(ST_NPoints(geometry)) OVER ())::bigint AS positions,
             COUNT(*) OVER () AS cnt,
             CASE WHEN SUM(ST_NPoints(geometry)) OVER () <= 100000
               AND COUNT(*) OVER () <= 500
               THEN ST_AsBinary(geometry, 'NDR') END AS wkb
         FROM refs",
    )
    .bind(read.organization_id())
    .bind(read.project_id())
    .bind(reference.id)
    .bind(&reference.content_revision)
    .bind(&reference.schema_revision)
    .bind(feature_id.as_deref())
    .fetch_all(&mut **read.transaction())
    .await
    .map_err(unavailable)?;
    if feature_id.is_some() && rows.is_empty() {
        return Err(AppError::not_found("reference feature not found"));
    }
    for row in &rows {
        let count: i64 = row.try_get("cnt").map_err(unavailable)?;
        let positions: i64 = row.try_get("positions").map_err(unavailable)?;
        if count > MAX_REFERENCE_FEATURES || positions > MAX_REFERENCE_POSITIONS {
            return Err(too_complex("reference set exceeds position limit"));
        }
        let wkb: Option<Vec<u8>> = row.try_get("wkb").map_err(unavailable)?;
        let native = wkb
            .as_deref()
            .and_then(Geom::from_wkb)
            .filter(|g| {
                validate_ready(g, Some(&reference.family), reference.stored_crs, false).is_ok()
            })
            .ok_or_else(|| invalid_spatial("invalid reference geometry"))?;
        if reference.stored_crs == source.stored_crs {
            spatial.geometries.push(native.to_wkb());
        } else {
            let (wkb, operation) =
                checked_geometry(read, &native, reference.stored_crs, source.stored_crs).await?;
            spatial.geometries.push(wkb);
            spatial.operation = operation;
        }
    }
    Ok(spatial)
}

/// `POST .../layers/{layer_id}/query`: exact typed, revision-pinned, bounded page.
pub async fn execute_query(
    read: &mut ProjectRead<'_>,
    actor_id: Uuid,
    layer_id: Uuid,
    request: &QueryRequest,
) -> Result<Value, AppError> {
    if request.attribute_only && request.spatial_filter.is_some() {
        return Err(invalid_spatial(
            "attribute-only query cannot include spatial filter",
        ));
    }
    if !(1..=MAX_QUERY_LIMIT).contains(&request.limit) {
        return Err(AppError::validation(
            "INVALID_LIMIT",
            "invalid query page size",
        ));
    }
    if !(0..=MAX_QUERY_OFFSET).contains(&request.offset) {
        return Err(AppError::validation(
            "INVALID_OFFSET",
            "invalid query offset",
        ));
    }
    if request.offset != 0 && request.expected_query_fingerprint.is_none() {
        return Err(AppError::conflict(
            "MISSING_QUERY_PIN",
            "page requires previous query fingerprint; restart at offset 0",
        ));
    }
    let pins = [
        &request.expected_content_revision,
        &request.expected_schema_revision,
        &request.expected_query_fingerprint,
    ];
    if pins
        .into_iter()
        .flatten()
        .any(|token| !valid_revision_token(token))
    {
        return Err(AppError::validation(
            "INVALID_QUERY_PIN",
            "invalid query revision token",
        ));
    }
    set_timeout(read).await?;
    let source = resolve(read, layer_id).await?;
    if request
        .expected_content_revision
        .as_ref()
        .is_some_and(|pin| *pin != source.content_revision)
    {
        return Err(AppError::conflict(
            "STALE_CONTENT_REVISION",
            "layer content changed; rerun query",
        ));
    }
    if request
        .expected_schema_revision
        .as_ref()
        .is_some_and(|pin| *pin != source.schema_revision)
    {
        return Err(AppError::conflict(
            "STALE_SCHEMA_REVISION",
            "layer schema changed; rerun query",
        ));
    }
    let filter = request.attribute_filter.as_ref();
    let sort_field = request.sort_field.as_deref();
    compile_filter(filter, &source.fields, &mut Sql::default())?;
    compile_sort(
        &source.fields,
        sort_field,
        &request.sort_direction,
        &mut Sql::default(),
    )?;
    source.check_feature_limit()?;
    let spatial = spatial_clause(read, &source, request.spatial_filter.as_ref()).await?;
    let total = scan(
        read,
        &source,
        Some((filter, &spatial)),
        request.attribute_only,
    )
    .await?;
    let fingerprint = canonical_manifest_digest(&json!({
        "version": if request.attribute_only { "spec065-attribute-query-v1" } else { "r007-query-v1" },
        "scope": [
            read.organization_id().to_string(),
            read.project_id().to_string(),
            actor_id.to_string(),
            "None",
        ],
        "target": source.pin(),
        "reference": spatial.reference_pin,
        "attribute_filter": request.attribute_filter,
        "spatial_filter": request.spatial_filter,
        "sort_field": request.sort_field,
        "sort_direction": request.sort_direction,
        "transformation": spatial.operation,
    }));
    if request
        .expected_query_fingerprint
        .as_ref()
        .is_some_and(|pin| *pin != fingerprint)
    {
        return Err(AppError::conflict(
            "STALE_QUERY_PIN",
            "query or input revision changed; rerun from offset 0",
        ));
    }
    if total > MAX_QUERY_FEATURES {
        return Err(too_complex("query result exceeds feature limit"));
    }

    let mut sql = Sql::default();
    let org = sql.bind(Param::Uuid(read.organization_id()));
    let project = sql.bind(Param::Uuid(read.project_id()));
    let src = sql.bind(Param::Uuid(source.id));
    let crev = sql.bind(Param::Text(source.content_revision.clone()));
    let srev = sql.bind(Param::Text(source.schema_revision.clone()));
    let compiled = compile_filter(filter, &source.fields, &mut sql)?;
    let sort = compile_sort(
        &source.fields,
        sort_field,
        &request.sort_direction,
        &mut sql,
    )?;
    let (cte, clause) = spatial.render(&mut sql);
    let limit = sql.bind(Param::Int(request.limit));
    let offset = sql.bind(Param::Int(request.offset));
    let sort_key = sort
        .key
        .map(|key| format!(", {key} AS sort_key"))
        .unwrap_or_default();
    let (inner, outer) = (sort.inner, sort.outer);
    let text = if request.attribute_only {
        format!(
            "SELECT feature_id, attributes, geometry_state FROM (
             SELECT rf.feature_id, rf.attributes, rf.geometry_state{sort_key}
             FROM spatial.reference_features rf
             WHERE rf.organization_id = {org} AND rf.project_id = {project}
               AND rf.source_id = {src} AND rf.content_revision = {crev}
               AND rf.schema_revision = {srev} AND ({compiled})
             ORDER BY {inner} LIMIT {limit} OFFSET {offset}
         ) page ORDER BY {outer}"
        )
    } else {
        let max_positions = sql.bind(Param::Int(MAX_PAGE_POSITIONS));
        format!(
            "{cte} SELECT feature_id, attributes, (SUM(np) OVER ())::bigint AS page_positions,
             CASE WHEN SUM(np) OVER () <= {max_positions}
                 THEN ST_AsBinary(geometry, 'NDR') END AS wkb
         FROM (
             SELECT rf.feature_id, rf.attributes, rf.geometry,
                 ST_NPoints(rf.geometry) AS np{sort_key}
             FROM spatial.reference_features rf
             WHERE rf.organization_id = {org} AND rf.project_id = {project}
               AND rf.source_id = {src} AND rf.content_revision = {crev}
               AND rf.schema_revision = {srev} AND rf.geometry_state = 'present'
               AND ({compiled}) AND ({clause})
             ORDER BY {inner} LIMIT {limit} OFFSET {offset}
         ) page
         ORDER BY {outer}"
        )
    };
    let rows = bound(&text, sql.params)
        .fetch_all(&mut **read.transaction())
        .await
        .map_err(unavailable)?;
    let mut items = Vec::with_capacity(rows.len());
    let mut selection = Vec::with_capacity(rows.len());
    for row in &rows {
        let feature_id: String = row.try_get("feature_id").map_err(unavailable)?;
        let attributes: Value = row.try_get("attributes").map_err(unavailable)?;
        let geometry = if request.attribute_only {
            Value::Null
        } else {
            let positions: i64 = row.try_get("page_positions").map_err(unavailable)?;
            if positions > MAX_PAGE_POSITIONS {
                return Err(too_complex(
                    "page geometry exceeds position limit; reduce page size",
                ));
            }
            let wkb: Option<Vec<u8>> = row.try_get("wkb").map_err(unavailable)?;
            let geometry = wkb
                .as_deref()
                .and_then(Geom::from_wkb)
                .filter(|g| {
                    validate_ready(g, Some(&source.family), source.stored_crs, false).is_ok()
                })
                .ok_or_else(invalid_source)?;
            json!({"type": geometry.family(), "coordinates": geometry.coordinates()})
        };
        let canonical = attributes.as_object().is_some_and(|map| {
            map.len() == source.fields.len()
                && source.fields.iter().all(|field| {
                    map.get(&field.field_id)
                        .is_some_and(|value| is_canonical_attribute(field, value))
                })
        });
        if !canonical {
            return Err(invalid_source());
        }
        selection.push(feature_id.clone());
        let mut item =
            json!({"feature_id": feature_id, "attributes": attributes, "geometry": geometry});
        if request.attribute_only {
            let state: String = row.try_get("geometry_state").map_err(unavailable)?;
            item["geometry_state"] = json!(state);
        }
        items.push(item);
    }
    let mut body = source.pin();
    if request.attribute_only {
        body["attribute_only"] = json!(true);
    }
    body["reference_pin"] = spatial.reference_pin;
    body["query_fingerprint"] = json!(fingerprint);
    body["geometry_transformation"] = json!(spatial.operation);
    body["total_count"] = json!(total);
    body["limit"] = json!(request.limit);
    body["offset"] = json!(request.offset);
    body["has_more"] = json!(request.offset + (items.len() as i64) < total);
    body["selection_feature_ids"] = json!(selection);
    body["items"] = Value::Array(items);
    Ok(body)
}

/// Python `str.strip()` whitespace (Unicode White_Space plus U+001C..U+001F).
fn python_strip(value: &str) -> &str {
    value.trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
}

/// Python `datetime.isoformat()` of a UTC timestamp.
fn isoformat(value: DateTime<Utc>) -> String {
    if value.timestamp_subsec_micros() == 0 {
        value.format("%Y-%m-%dT%H:%M:%S+00:00").to_string()
    } else {
        value.format("%Y-%m-%dT%H:%M:%S%.6f+00:00").to_string()
    }
}

fn saved_wire(row: &PgRow) -> Result<Value, sqlx::Error> {
    Ok(json!({
        "id": row.try_get::<Uuid, _>("id")?,
        "organization_id": row.try_get::<Uuid, _>("organization_id")?,
        "project_id": row.try_get::<Uuid, _>("project_id")?,
        "layer_id": row.try_get::<Uuid, _>("layer_id")?,
        "name": row.try_get::<String, _>("name")?,
        "description": row.try_get::<String, _>("description")?,
        "attribute_filter": row.try_get::<Option<Value>, _>("attribute_filter")?,
        "spatial_filter": row.try_get::<Option<Value>, _>("spatial_filter")?,
        "sort_field": row.try_get::<Option<String>, _>("sort_field")?,
        "sort_direction": row.try_get::<String, _>("sort_direction")?,
        "created_by_actor_id": row.try_get::<Uuid, _>("created_by_actor_id")?,
        "created_at": isoformat(row.try_get("created_at")?),
        "updated_at": isoformat(row.try_get("updated_at")?),
    }))
}

async fn validate_definition(
    write: &mut ProjectRead<'_>,
    input: &SavedQueryInput,
) -> Result<(), AppError> {
    set_timeout(write).await?;
    let source = resolve(write, input.layer_id).await?;
    compile_filter(
        input.attribute_filter.as_ref(),
        &source.fields,
        &mut Sql::default(),
    )?;
    compile_sort(
        &source.fields,
        input.sort_field.as_deref(),
        &input.sort_direction,
        &mut Sql::default(),
    )?;
    spatial_clause(write, &source, input.spatial_filter.as_ref()).await?;
    Ok(())
}

/// `POST .../saved-queries` (project.edit): validate then persist a definition.
pub async fn create_saved_query(
    write: &mut ProjectRead<'_>,
    actor_id: Uuid,
    input: &SavedQueryInput,
) -> Result<Value, AppError> {
    let name = python_strip(&input.name);
    if !(1..=200).contains(&name.chars().count()) {
        return Err(AppError::validation(
            "INVALID_NAME",
            "name must be 1..200 characters",
        ));
    }
    validate_definition(write, input)
        .await
        .map_err(|error| match error.code() {
            "QUERY_TIMEOUT" | "QUERY_UNAVAILABLE" => AppError::service_unavailable(
                "QUERY_UNAVAILABLE",
                "saved definition could not be validated",
            ),
            "INVALID_QUERY_SOURCE" if error.to_string() == "invalid query snapshot or revision" => {
                AppError::validation("INVALID_QUERY_SOURCE", "invalid saved definition")
            }
            _ => error,
        })?;
    let description: String = python_strip(&input.description).chars().take(500).collect();
    let text = format!(
        "INSERT INTO queries.saved_queries (
             id, organization_id, project_id, layer_id, name, description,
             attribute_filter, spatial_filter, sort_field, sort_direction,
             created_by_actor_id, created_at, updated_at
         ) VALUES (
             $1, $2, $3, $4, $5, $6, CAST($7 AS jsonb), CAST($8 AS jsonb), $9, $10, $11, $12, $12
         )
         RETURNING {SAVED_COLUMNS}"
    );
    let row = sqlx::query(&text)
        .bind(Uuid::new_v4())
        .bind(write.organization_id())
        .bind(write.project_id())
        .bind(input.layer_id)
        .bind(name)
        .bind(description)
        .bind(input.attribute_filter.as_ref().map(Value::to_string))
        .bind(input.spatial_filter.as_ref().map(Value::to_string))
        .bind(input.sort_field.as_deref())
        .bind(&input.sort_direction)
        .bind(actor_id)
        .bind(Utc::now())
        .fetch_one(&mut **write.transaction())
        .await
        .map_err(internal)?;
    saved_wire(&row).map_err(internal)
}

/// `GET .../saved-queries` (project.read), newest first, bounded to 500.
pub async fn list_saved_queries(
    read: &mut ProjectRead<'_>,
    layer_id: Option<Uuid>,
) -> Result<Value, AppError> {
    let text = format!(
        "SELECT {SAVED_COLUMNS} FROM queries.saved_queries
         WHERE organization_id = $1 AND project_id = $2
           AND ($3::uuid IS NULL OR layer_id = $3)
         ORDER BY created_at DESC, id ASC LIMIT 501"
    );
    let rows = sqlx::query(&text)
        .bind(read.organization_id())
        .bind(read.project_id())
        .bind(layer_id)
        .fetch_all(&mut **read.transaction())
        .await
        .map_err(internal)?;
    if rows.len() > 500 {
        return Err(too_complex("saved definition list exceeds limit"));
    }
    let items = rows
        .iter()
        .map(saved_wire)
        .collect::<Result<Vec<_>, _>>()
        .map_err(internal)?;
    Ok(json!({ "items": items }))
}

/// `DELETE .../saved-queries/{query_id}` (project.edit).
pub async fn delete_saved_query(
    write: &mut ProjectRead<'_>,
    query_id: Uuid,
) -> Result<(), AppError> {
    let deleted: Option<Uuid> = sqlx::query_scalar(
        "DELETE FROM queries.saved_queries
         WHERE organization_id = $1 AND project_id = $2 AND id = $3
         RETURNING id",
    )
    .bind(write.organization_id())
    .bind(write.project_id())
    .bind(query_id)
    .fetch_optional(&mut **write.transaction())
    .await
    .map_err(internal)?;
    deleted
        .map(drop)
        .ok_or_else(|| AppError::not_found("saved query not found"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attribute_snapshot_validation_does_not_read_geometry() {
        let source = Source {
            layer_id: Uuid::nil(),
            id: Uuid::nil(),
            stored_crs: 4326,
            family: "Point".to_owned(),
            feature_count: Some(2),
            content_revision: "content".to_owned(),
            schema_revision: "schema".to_owned(),
            fields: vec![SourceField {
                field_id: "rank".to_owned(),
                field_type: FieldType::Integer,
                nullable: true,
            }],
        };
        let mut sql = Sql::default();
        let attribute = row_valid(&source, &mut sql, true);
        assert!(!attribute.contains("rf.geometry"));
        assert!(attribute.contains("rf.content_revision"));
        assert!(attribute.contains("jsonb_object_keys"));
        assert!(attribute.contains("2147483647"));
        let spatial = row_valid(&source, &mut Sql::default(), false);
        assert!(spatial.contains("ST_IsValid"));
        assert!(spatial.contains("geometry_state = 'present'"));
    }

    #[test]
    fn strips_like_python_and_formats_isoformat() {
        assert_eq!(python_strip("\u{1f} tên\u{3000}\n"), "tên");
        let ts = DateTime::parse_from_rfc3339("2026-11-26T01:02:03.004500Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(isoformat(ts), "2026-11-26T01:02:03.004500+00:00");
        assert_eq!(hex(&[0, 255, 16]), "00ff10");
    }

    #[test]
    fn spatial_render_binds_geometry_and_uses_direct_predicate() {
        let mut sql = Sql::default();
        let spatial = Spatial {
            function: Some("ST_Intersects"),
            geometries: vec![vec![1, 2]],
            srid: 4326,
            ..Spatial::default()
        };
        let (cte, clause) = spatial.render(&mut sql);
        assert!(cte.is_empty());
        assert_eq!(clause, "ST_Intersects(rf.geometry, ST_GeomFromWKB($2, $1))");
        let none = Spatial::default().render(&mut sql);
        assert_eq!(none.1, "TRUE");
    }
}
