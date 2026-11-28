//! E05 revision-pinned map presentation reads: bounded viewport, display
//! extent and Mapbox vector tiles. Mirrors
//! `apps/api/openvietgis_api/adapters/persistence/map_viewport.py` inside the
//! caller's RLS-scoped `ProjectRead` transaction. SQL is built from fixed code
//! constants only; every request value is bound.

use std::io::Write as _;

use ovg_core::crs::{
    reference_coordinate_wire, reference_transformation, FixtureRequirement, TransformationProfile,
    WEB_MERCATOR_MAX_COORDINATE_METRE, WEB_MERCATOR_MAX_LATITUDE,
};
use ovg_core::AppError;
use serde::Serialize;
use serde_json::{json, Value};
use sqlx::postgres::PgArguments;
use sqlx::query::Query;
use sqlx::{Postgres, Row};
use uuid::Uuid;

use crate::catalog::{db_error, ProjectRead};

pub const MAX_VIEWPORT_FEATURES: i64 = 1000;
pub const MAX_VIEWPORT_POSITIONS: i64 = 20_000;
pub const MAX_VIEWPORT_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_RENDER_SOURCE_FEATURES: i64 = 50_000;
pub const MAX_TILE_ZOOM: u32 = 22;
pub const MAX_TILE_FEATURES: i64 = 10_000;
const MAX_TILE_POSITIONS: i64 = 200_000;
const MAX_TILE_BYTES: usize = 4 * 1024 * 1024;
const TILE_EXTENT: i32 = 4096;
const TILE_BUFFER: i32 = 64;
const MAP_STATEMENT_TIMEOUT: &str = "2000ms";
const WEB_MERCATOR_PIPELINE: &str =
    "+proj=pipeline +step +proj=unitconvert +xy_in=deg +xy_out=rad +step +proj=webmerc +ellps=WGS84";

/// The pair of catalog read pins every presentation read is bound to.
pub struct ReadPins<'a> {
    pub content_revision: &'a str,
    pub schema_revision: &'a str,
}

impl ReadPins<'_> {
    /// `^[a-f0-9]{64}$`, the FastAPI query pattern.
    #[must_use]
    pub fn well_formed(&self) -> bool {
        is_pin(self.content_revision) && is_pin(self.schema_revision)
    }
}

fn is_pin(pin: &str) -> bool {
    pin.len() == 64 && pin.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// A finite, nonwrapping geographic viewport inside the Web Mercator domain.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewportBounds {
    pub west: f64,
    pub south: f64,
    pub east: f64,
    pub north: f64,
}

impl ViewportBounds {
    #[must_use]
    pub fn new(west: f64, south: f64, east: f64, north: f64) -> Option<Self> {
        // NaN fails every comparison and infinities fail the closed domain.
        let latitude = -WEB_MERCATOR_MAX_LATITUDE..=WEB_MERCATOR_MAX_LATITUDE;
        let valid = (-180.0..=180.0).contains(&west)
            && west < east
            && east <= 180.0
            && latitude.contains(&south)
            && south < north
            && north <= WEB_MERCATOR_MAX_LATITUDE;
        valid.then_some(Self {
            west,
            south,
            east,
            north,
        })
    }
}

#[derive(Clone, Copy)]
enum RenderRead {
    WholeSource,
    Tile,
}

struct Snapshot {
    context: Value,
    source_id: Uuid,
    native_srid: i32,
    geometry_type: String,
    expected_count: Option<i64>,
    extent: Option<[f64; 4]>,
    transformation: Option<&'static TransformationProfile>,
}

impl ProjectRead<'_> {
    pub async fn viewport(
        &mut self,
        layer_id: Uuid,
        pins: &ReadPins<'_>,
        bounds: ViewportBounds,
        limit: i64,
    ) -> Result<Value, AppError> {
        let (context, features) = self.viewport_rows(layer_id, pins, bounds, limit).await?;
        build_viewport_response(context, &features, limit)
    }

    /// Pre-serialized UTF-8 JSON bytes for the viewport response, avoiding
    /// intermediate `Value` coordinate allocations and redundant encoding passes.
    pub async fn viewport_json(
        &mut self,
        layer_id: Uuid,
        pins: &ReadPins<'_>,
        bounds: ViewportBounds,
        limit: i64,
    ) -> Result<Vec<u8>, AppError> {
        let (context, features) = self.viewport_rows(layer_id, pins, bounds, limit).await?;
        build_viewport_json_bytes(&context, &features, limit)
    }

    async fn viewport_rows(
        &mut self,
        layer_id: Uuid,
        pins: &ReadPins<'_>,
        bounds: ViewportBounds,
        limit: i64,
    ) -> Result<(Value, Vec<RenderRow>), AppError> {
        if !(1..=MAX_VIEWPORT_FEATURES).contains(&limit) {
            return Err(AppError::internal("viewport feature limit outside bounds"));
        }
        let snapshot = self
            .render_snapshot(layer_id, pins, RenderRead::WholeSource)
            .await?;
        let expected_count = snapshot.expected_count.ok_or_else(read_unavailable)?;
        // One statement snapshot: the summary validates every source row
        // (count and display domain) and gates the bounded candidate prefix.
        let sql = format!(
            "{}, summary AS MATERIALIZED (
                SELECT count(*) AS snapshot_count,
                    COALESCE(bool_and(valid IS TRUE), true) AS snapshot_valid
                FROM display
            ), candidate AS MATERIALIZED (
                SELECT feature_id, geometry, ST_NPoints(geometry) AS positions
                FROM display CROSS JOIN summary
                WHERE CASE WHEN snapshot_count = $8 AND snapshot_valid AND valid THEN
                    ST_Intersects(geometry, ST_MakeEnvelope($9, $10, $11, $12, 4326))
                    ELSE false END
                ORDER BY feature_id COLLATE \"C\" LIMIT $13
            ), prefix AS MATERIALIZED (
                SELECT *, SUM(positions) OVER (
                    ORDER BY feature_id COLLATE \"C\" ROWS UNBOUNDED PRECEDING
                ) AS total_positions FROM candidate
            )
            SELECT summary.snapshot_count, summary.snapshot_valid,
                prefix.feature_id, prefix.positions,
                CASE WHEN total_positions <= $14
                    THEN ST_AsBinary(prefix.geometry, 'NDR') END AS wkb
            FROM summary LEFT JOIN prefix ON total_positions - positions <= $14
            ORDER BY prefix.feature_id COLLATE \"C\"",
            display_ctes(snapshot.transformation)
        );
        let rows = self
            .bind_snapshot(sqlx::query(&sql), &snapshot, pins)
            .bind(expected_count)
            .bind(bounds.west)
            .bind(bounds.south)
            .bind(bounds.east)
            .bind(bounds.north)
            .bind(limit + 1)
            .bind(MAX_VIEWPORT_POSITIONS)
            .fetch_all(&mut **self.tx.tx_mut())
            .await
            .map_err(unavailable)?;
        let first = rows.first().ok_or_else(read_unavailable)?;
        validate_summary(
            expected_count,
            first.try_get("snapshot_count").map_err(unavailable)?,
            first.try_get("snapshot_valid").map_err(unavailable)?,
        )?;
        let mut features = Vec::with_capacity(rows.len());
        for row in &rows {
            // The LEFT JOIN summary marker is not a feature.
            let feature_id: Option<String> = row.try_get("feature_id").map_err(unavailable)?;
            if let Some(feature_id) = feature_id {
                features.push(RenderRow {
                    feature_id,
                    positions: row.try_get("positions").map_err(unavailable)?,
                    wkb: row.try_get("wkb").map_err(unavailable)?,
                });
            }
        }
        Ok((snapshot.context, features))
    }

    pub async fn display_extent(
        &mut self,
        layer_id: Uuid,
        pins: &ReadPins<'_>,
    ) -> Result<Value, AppError> {
        let snapshot = self
            .render_snapshot(layer_id, pins, RenderRead::WholeSource)
            .await?;
        let expected_count = snapshot.expected_count.ok_or_else(read_unavailable)?;
        let sql = format!(
            "{}
            SELECT snapshot_count, snapshot_valid,
                ST_XMin(extent) AS west, ST_YMin(extent) AS south,
                ST_XMax(extent) AS east, ST_YMax(extent) AS north
            FROM (
                SELECT count(*) AS snapshot_count,
                    COALESCE(bool_and(valid IS TRUE), true) AS snapshot_valid,
                    ST_Extent(geometry) FILTER (WHERE valid IS TRUE) AS extent
                FROM display
            ) AS actual",
            display_ctes(snapshot.transformation)
        );
        let row = self
            .bind_snapshot(sqlx::query(&sql), &snapshot, pins)
            .fetch_one(&mut **self.tx.tx_mut())
            .await
            .map_err(unavailable)?;
        validate_summary(
            expected_count,
            row.try_get("snapshot_count").map_err(unavailable)?,
            row.try_get("snapshot_valid").map_err(unavailable)?,
        )?;
        let corners: [Option<f64>; 4] = [
            row.try_get("west").map_err(unavailable)?,
            row.try_get("south").map_err(unavailable)?,
            row.try_get("east").map_err(unavailable)?,
            row.try_get("north").map_err(unavailable)?,
        ];
        let (state, bounds) = match corners {
            [Some(west), Some(south), Some(east), Some(north)] => {
                ("available", json!([west, south, east, north]))
            }
            _ => ("empty", Value::Null),
        };
        Ok(json!({ "context": snapshot.context, "state": state, "bounds": bounds }))
    }

    /// One Mapbox vector tile (`features` layer, `feature_id` property) for a
    /// native EPSG:4326 source, bounded by feature, position and byte budgets.
    /// Returns tile bytes and whether the candidate prefix was truncated.
    pub async fn vector_tile(
        &mut self,
        layer_id: Uuid,
        pins: &ReadPins<'_>,
        z: u32,
        x: u32,
        y: u32,
    ) -> Result<(Vec<u8>, bool), AppError> {
        if z > MAX_TILE_ZOOM || x >= 1 << z || y >= 1 << z {
            return Err(AppError::internal("tile address outside bounds"));
        }
        let snapshot = self
            .render_snapshot(layer_id, pins, RenderRead::Tile)
            .await?;
        if snapshot.transformation.is_some() {
            // shortcut: projected sources are viewport-only; add a fixed pipeline per CRS when tiles need them.
            return Err(unsupported_transformation());
        }
        let (west, south, east, north) = tile_geographic_bounds(z, x, y);
        let covers_world = z == 0
            || (west <= -180.0
                && east >= 180.0
                && south <= -WEB_MERCATOR_MAX_LATITUDE
                && north >= WEB_MERCATOR_MAX_LATITUDE);
        let extent_in_display_domain = snapshot.extent.is_some_and(|[x0, y0, x1, y1]| {
            x0 >= -180.0
                && x1 <= 180.0
                && y0 >= -WEB_MERCATOR_MAX_LATITUDE
                && y1 <= WEB_MERCATOR_MAX_LATITUDE
        });
        let covers_source_extent = !self.gis_scoped
            && snapshot.extent.is_some_and(|[min_x, min_y, max_x, max_y]| {
                west <= min_x && east >= max_x && south <= min_y && north >= max_y
            });
        // When a tile spans the world or the entire source extent, scanning the
        // global GIST index would visit every layer in the DB and sort up to 1M
        // rows. When the source's extent lies within the Web Mercator display
        // domain, materialize the first LIMIT rows in PK order first so
        // PostgreSQL never estimates low spatial/SRID selectivity and falls back
        // to a Parallel Seq Scan + Sort on 1M rows.
        let area_expr = if covers_world {
            format!(
                "ST_MakeEnvelope(-180, {:?}, 180, {:?}, 4326)",
                -WEB_MERCATOR_MAX_LATITUDE, WEB_MERCATOR_MAX_LATITUDE
            )
        } else {
            let max_m = WEB_MERCATOR_MAX_COORDINATE_METRE;
            format!(
                "ST_InverseTransformPipeline(
                    ST_Intersection(
                        ST_TileEnvelope($4, $5, $6, margin => $7),
                        ST_MakeEnvelope({:?}, {:?}, {max_m:?}, {max_m:?}, 3857)
                    ), '{WEB_MERCATOR_PIPELINE}', 4326
                )",
                -max_m, -max_m
            )
        };
        let candidate_cte = if !self.gis_scoped
            && extent_in_display_domain
            && (covers_world || covers_source_extent)
        {
            "page AS MATERIALIZED (
                SELECT f.feature_id, f.geometry, ST_NPoints(f.geometry) AS positions
                FROM spatial.reference_features AS f
                WHERE f.organization_id = $1 AND f.project_id = $2 AND f.source_id = $3
                    AND f.geometry_state = 'present'
                    AND f.content_revision = $10 AND f.schema_revision = $11
                ORDER BY f.feature_id COLLATE \"C\" LIMIT $12
            ), candidate AS MATERIALIZED (
                SELECT p.feature_id, p.geometry, p.positions
                FROM page AS p CROSS JOIN bounds
                WHERE ST_SRID(p.geometry) = 4326
                    AND _ST_Intersects(p.geometry, bounds.area)
            )"
            .to_owned()
        } else {
            let spatial_pred = if covers_world || covers_source_extent {
                "_ST_Intersects(f.geometry, bounds.area)"
            } else {
                "f.geometry && bounds.area AND ST_Intersects(f.geometry, bounds.area)"
            };
            format!(
                "candidate AS MATERIALIZED (
                    SELECT f.feature_id, f.geometry, ST_NPoints(f.geometry) AS positions
                    FROM spatial.reference_features AS f CROSS JOIN bounds
                    WHERE f.organization_id = $1 AND f.project_id = $2 AND f.source_id = $3
                        AND f.geometry_state = 'present'
                        AND f.content_revision = $10 AND f.schema_revision = $11
                        AND ST_SRID(f.geometry) = 4326
                        AND {spatial_pred}
                    ORDER BY f.feature_id COLLATE \"C\" LIMIT $12
                )"
            )
        };
        // Bound vertex work before projection and encoding, not only after serialization.
        let sql = format!(
            "WITH bounds AS (
                SELECT ST_TileEnvelope($4, $5, $6) AS tile,
                    {area_expr} AS area
            ), {candidate_cte}, ranked AS (
                SELECT feature_id, geometry,
                    row_number() OVER (ORDER BY feature_id COLLATE \"C\") AS ordinal,
                    sum(positions) OVER (ORDER BY feature_id COLLATE \"C\"
                        ROWS UNBOUNDED PRECEDING) AS cumulative_positions
                FROM candidate
            ), prefix AS MATERIALIZED (
                SELECT feature_id, geometry FROM ranked
                WHERE ordinal <= $13 AND cumulative_positions <= $14
            ), encoded AS (
                SELECT feature_id, ST_AsMVTGeom(
                        ST_TransformPipeline(
                            ST_ClipByBox2D(prefix.geometry, bounds.area::box2d),
                            '{WEB_MERCATOR_PIPELINE}', 3857
                        ),
                        bounds.tile, $8, $9, true
                    ) AS geom
                FROM prefix CROSS JOIN bounds
            )
            SELECT (
                SELECT ST_AsMVT(t, 'features', $8, 'geom' ORDER BY t.feature_id COLLATE \"C\")
                FROM (SELECT feature_id, geom FROM encoded WHERE geom IS NOT NULL) AS t
            ) AS tile, (SELECT count(*) FROM candidate) AS candidates,
                (SELECT count(*) FROM prefix) AS included"
        );
        let row = sqlx::query(&sql)
            .bind(self.tx.organization_id())
            .bind(self.project_id)
            .bind(snapshot.source_id)
            .bind(z as i32)
            .bind(x as i32)
            .bind(y as i32)
            .bind(f64::from(TILE_BUFFER) / f64::from(TILE_EXTENT))
            .bind(TILE_EXTENT)
            .bind(TILE_BUFFER)
            .bind(pins.content_revision.to_owned())
            .bind(pins.schema_revision.to_owned())
            .bind(MAX_TILE_FEATURES + 1)
            .bind(MAX_TILE_FEATURES)
            .bind(MAX_TILE_POSITIONS)
            .fetch_one(&mut **self.tx.tx_mut())
            .await
            .map_err(unavailable)?;
        let tile: Option<Vec<u8>> = row.try_get("tile").map_err(unavailable)?;
        let candidates: i64 = row.try_get("candidates").map_err(unavailable)?;
        let included: i64 = row.try_get("included").map_err(unavailable)?;
        let tile = tile.unwrap_or_default();
        if tile.len() > MAX_TILE_BYTES {
            return Err(read_unavailable());
        }
        Ok((tile, candidates == included))
    }

    /// Layer -> source -> pins -> CRS -> render profile, in the Python order.
    async fn render_snapshot(
        &mut self,
        layer_id: Uuid,
        pins: &ReadPins<'_>,
        read: RenderRead,
    ) -> Result<Snapshot, AppError> {
        // Its own statement: a GUC set inside a query only applies to later statements.
        sqlx::query("SELECT set_config('statement_timeout', $1, true)")
            .bind(MAP_STATEMENT_TIMEOUT)
            .execute(&mut **self.tx.tx_mut())
            .await
            .map_err(unavailable)?;
        let source_id: Uuid = sqlx::query_scalar(
            "SELECT l.source_id
             FROM spatial.layers AS l
             WHERE l.organization_id = $1 AND l.project_id = $2 AND l.id = $3 AND l.lifecycle = 'active'
             FOR SHARE OF l",
        )
        .bind(self.tx.organization_id())
        .bind(self.project_id)
        .bind(layer_id)
        .fetch_optional(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?
        .ok_or_else(|| AppError::not_found("resource not found"))?;
        let source = self.get_source_record(source_id).await?.wire;

        if source["validation_state"] != "ready" {
            return Err(AppError::conflict(
                "SOURCE_NOT_READY",
                "The source is not ready.",
            ));
        }
        if source["content_revision"] != pins.content_revision
            || source["schema_revision"] != pins.schema_revision
        {
            return Err(AppError::conflict(
                "STALE_READ_PIN",
                "Read pins changed; reload the source and start a new traversal.",
            ));
        }
        let native = source["stored_crs"]["code"]
            .as_u64()
            .and_then(|code| u32::try_from(code).ok())
            .ok_or_else(|| AppError::internal("ready source requires native CRS"))?;
        let reference = reference_coordinate_wire(4326)
            .ok_or_else(|| AppError::internal("EPSG:4326 must be readable"))?;
        let transformation = match native {
            4326 => None,
            _ => Some(
                reference_transformation(native, 4326).ok_or_else(unsupported_transformation)?,
            ),
        };

        let geometry_family = source["schema"]["geometry_family"]
            .as_str()
            .unwrap_or_default();
        let renderable_kind = matches!(
            source["kind"].as_str(),
            Some("reference_fixture" | "imported_file" | "derived_processing")
        );
        let supported = renderable_kind
            && source["schema"]["dimensions"] == "XY"
            && matches!(geometry_family, "Point" | "LineString" | "Polygon");
        if !supported {
            return Err(unsupported_transformation());
        }
        let expected_count = match read {
            // The tile candidate query already applies the actor's RLS scope.
            // A whole-source count or size cap here would defeat bounded tile reads.
            RenderRead::Tile => None,
            RenderRead::WholeSource => Some(self.whole_source_count(source_id, &source).await?),
        };
        if let Some(edge)=transformation.filter(|edge|requires_fixture(edge)) {
            if self.gis_scoped {return Err(out_of_area());}
            let org=self.tx.organization_id();
            crate::catalog::regional_crs::verify_regional_source(self.tx.tx_mut(),org,self.project_id,source_id,edge).await?;
        }
        let extent = source["extent"].as_object().and_then(|e| {
            Some([
                e.get("min_x")?.as_f64()?,
                e.get("min_y")?.as_f64()?,
                e.get("max_x")?.as_f64()?,
                e.get("max_y")?.as_f64()?,
            ])
        });
        Ok(Snapshot {
            context: json!({
                "organization_id": self.tx.organization_id(),
                "project_id": self.project_id,
                "layer_id": layer_id,
                "source_id": source_id,
                "content_revision": pins.content_revision,
                "schema_revision": pins.schema_revision,
                "geometry_included": true,
                "response_crs": reference,
                "transformation": transformation.map(TransformationProfile::to_wire)
            }),
            source_id,
            native_srid: i32::try_from(native)
                .map_err(|_| AppError::internal("native CRS code out of range"))?,
            geometry_type: format!("ST_{geometry_family}"),
            expected_count,
            extent,
            transformation,
        })
    }

    async fn whole_source_count(
        &mut self,
        source_id: Uuid,
        source: &Value,
    ) -> Result<i64, AppError> {
        let expected_count = if self.gis_scoped {
            // Object RLS hides features; count exactly what this actor may read.
            let count: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM spatial.reference_features
                 WHERE organization_id = $1 AND project_id = $2 AND source_id = $3",
            )
            .bind(self.tx.organization_id())
            .bind(self.project_id)
            .bind(source_id)
            .fetch_one(&mut **self.tx.tx_mut())
            .await
            .map_err(unavailable)?;
            Some(count)
        } else {
            source["feature_count"].as_i64()
        };
        expected_count
            .filter(|count| *count <= MAX_RENDER_SOURCE_FEATURES)
            .ok_or_else(unsupported_transformation)
    }

    /// Binds $1 organization, $2 project, $3 source, $4 content pin,
    /// $5 schema pin, $6 native SRID and $7 geometry type for `display_ctes`.
    fn bind_snapshot<'q>(
        &self,
        query: Query<'q, Postgres, PgArguments>,
        snapshot: &Snapshot,
        pins: &ReadPins<'_>,
    ) -> Query<'q, Postgres, PgArguments> {
        query
            .bind(self.tx.organization_id())
            .bind(self.project_id)
            .bind(snapshot.source_id)
            .bind(pins.content_revision.to_owned())
            .bind(pins.schema_revision.to_owned())
            .bind(snapshot.native_srid)
            .bind(snapshot.geometry_type.clone())
    }
}

/// `display(feature_id, geometry, valid)` over all scoped source rows in
/// EPSG:4326.
///
/// The table CHECK `ck_feature_geometry_state` already guarantees present
/// rows are 2D, valid, nonempty and at most 4096 positions, so native rows only
/// repeat the pin/SRID/type equality; topology is rechecked after a transform.
/// The bound is the 50k render ceiling (not Python's `LIMIT 10001`), so only a
/// larger snapshot fails the count check, never a valid layer above 10k.
fn display_ctes(transformation: Option<&TransformationProfile>) -> String {
    let lat = WEB_MERCATOR_MAX_LATITUDE;
    let native = format!(
        "SELECT feature_id, geometry,
            content_revision = $4 AND schema_revision = $5 AND geometry_state = 'present'
            AND ST_SRID(geometry) = $6 AND ST_GeometryType(geometry) = $7 AS valid
        FROM spatial.reference_features
        WHERE organization_id = $1 AND project_id = $2 AND source_id = $3
        LIMIT {}",
        MAX_RENDER_SOURCE_FEATURES + 1
    );
    let display_domain = format!(
        "ST_CoveredBy(geometry, ST_MakeEnvelope(-180, {:?}, 180, {lat:?}, 4326))",
        -lat
    );
    let Some(edge) = transformation else {
        return format!(
            "WITH display AS MATERIALIZED (
                SELECT feature_id, geometry, valid AND {display_domain} AS valid
                FROM ({native}) AS native
            )"
        );
    };
    let transform=crate::catalog::regional_crs::profile_expression(edge,"geometry");
    let area = edge.geographic_bounds;
    format!(
        "WITH native AS NOT MATERIALIZED ({native}),
        step0 AS NOT MATERIALIZED (
            SELECT feature_id, CASE WHEN valid
                THEN {transform} END
                AS geometry, valid
            FROM native
        ),
        display AS MATERIALIZED (
            SELECT feature_id, geometry, valid AND geometry IS NOT NULL
                AND ST_SRID(geometry) = 4326 AND ST_Zmflag(geometry) = 0
                AND ST_NPoints(geometry) BETWEEN 1 AND 4096
                AND NOT ST_IsEmpty(geometry) AND ST_IsValid(geometry)
                AND {display_domain}
                AND ST_CoveredBy(geometry, ST_MakeEnvelope({:?}, {:?}, {:?}, {:?}, 4326)) AS valid
            FROM step0
        )",
        area.min_x, area.min_y, area.max_x, area.max_y
    )
}

fn requires_fixture(edge: &TransformationProfile) -> bool {
    edge.fixture_requirement != FixtureRequirement::None
}

fn validate_summary(expected: i64, count: i64, valid: bool) -> Result<(), AppError> {
    if count != expected || count > MAX_RENDER_SOURCE_FEATURES {
        return Err(read_unavailable());
    }
    if !valid {
        return Err(out_of_area());
    }
    Ok(())
}

fn unavailable(error: sqlx::Error) -> AppError {
    tracing::warn!(%error, "bounded map read failed");
    read_unavailable()
}

fn read_unavailable() -> AppError {
    AppError::service_unavailable("MAP_READ_UNAVAILABLE", "Map read unavailable; retry.")
}

fn out_of_area() -> AppError {
    AppError::validation(
        "TRANSFORMATION_OUT_OF_AREA",
        "The source is outside the approved operation area or evidence.",
    )
}

fn unsupported_transformation() -> AppError {
    AppError::validation(
        "UNSUPPORTED_TRANSFORMATION",
        "No approved direct operation exists.",
    )
}

struct RenderRow {
    feature_id: String,
    positions: i32,
    wkb: Option<Vec<u8>>,
}

fn tile_geographic_bounds(z: u32, x: u32, y: u32) -> (f64, f64, f64, f64) {
    let n = f64::from(1_u32 << z);
    let margin = f64::from(TILE_BUFFER) / f64::from(TILE_EXTENT);
    let x_min = ((f64::from(x) - margin) / n).clamp(0.0, 1.0);
    let x_max = ((f64::from(x) + 1.0 + margin) / n).clamp(0.0, 1.0);
    let y_min = ((f64::from(y) - margin) / n).clamp(0.0, 1.0);
    let y_max = ((f64::from(y) + 1.0 + margin) / n).clamp(0.0, 1.0);
    let west = x_min * 360.0 - 180.0;
    let east = x_max * 360.0 - 180.0;
    let merc_to_lat = |ty: f64| {
        let rad = std::f64::consts::PI * (1.0 - 2.0 * ty);
        rad.sinh().atan().to_degrees()
    };
    let north = merc_to_lat(y_min).clamp(-WEB_MERCATOR_MAX_LATITUDE, WEB_MERCATOR_MAX_LATITUDE);
    let south = merc_to_lat(y_max).clamp(-WEB_MERCATOR_MAX_LATITUDE, WEB_MERCATOR_MAX_LATITUDE);
    (west, south, east, north)
}

fn json_size(value: &Value) -> Result<usize, AppError> {
    serde_json::to_vec(value)
        .map(|bytes| bytes.len())
        .map_err(|_| AppError::internal("viewport serialization failed"))
}

fn serialization_failed() -> AppError {
    AppError::internal("viewport serialization failed")
}

/// Port of `domains/maps/viewport.py::build_viewport_response`: a stable
/// prefix bounded by feature count, positions and the whole JSON envelope size.
fn build_viewport_response(
    context: Value,
    features: &[RenderRow],
    limit: i64,
) -> Result<Value, AppError> {
    let reference = context["response_crs"].clone();
    let mut items: Vec<Value> = Vec::new();
    let mut limitation: Option<&str> = None;
    let overhead = json_size(&json!({
        "context": context,
        "items": [],
        "complete": false,
        "limitation": "position_limit"
    }))?;
    let (mut positions, mut item_bytes) = (0_i64, 0_usize);
    let mut previous: Option<&str> = None;
    for feature in features {
        if previous.is_some_and(|id| feature.feature_id.as_str() <= id) {
            return Err(AppError::internal("viewport order must be strictly stable"));
        }
        previous = Some(feature.feature_id.as_str());
        if items.len() as i64 == limit {
            limitation = Some("feature_limit");
            break;
        }
        let count = i64::from(feature.positions);
        if count < 1 || positions + count > MAX_VIEWPORT_POSITIONS {
            limitation = Some("position_limit");
            break;
        }
        let wkb = feature.wkb.as_deref().ok_or_else(|| {
            AppError::internal("bounded geometry marker has inconsistent positions")
        })?;
        let (kind, coordinates) = decode_display_wkb(wkb, count).ok_or_else(|| {
            AppError::internal("geometry cannot be presented in map display domain")
        })?;
        let wire = json!({
            "feature_id": feature.feature_id,
            "geometry": {
                "type": kind,
                "coordinates": coordinates,
                "coordinate_reference": reference
            }
        });
        let size = json_size(&wire)? + usize::from(!items.is_empty());
        if overhead + item_bytes + size > MAX_VIEWPORT_BYTES {
            limitation = Some("byte_limit");
            break;
        }
        items.push(wire);
        positions += count;
        item_bytes += size;
    }
    let result = json!({
        "context": context,
        "items": items,
        "complete": limitation.is_none(),
        "limitation": limitation
    });
    if json_size(&result)? > MAX_VIEWPORT_BYTES {
        return Err(read_unavailable());
    }
    Ok(result)
}

/// Stream-encode the viewport response into a single UTF-8 JSON buffer, matching
/// `build_viewport_response` bounds and schema without per-vertex `Value` trees
/// or repeated `serde_json::to_vec` size probes.
fn build_viewport_json_bytes(
    context: &Value,
    features: &[RenderRow],
    limit: i64,
) -> Result<Vec<u8>, AppError> {
    let reference =
        serde_json::to_vec(&context["response_crs"]).map_err(|_| serialization_failed())?;
    let context_bytes = serde_json::to_vec(context).map_err(|_| serialization_failed())?;
    let overhead = json_size(&json!({
        "context": context,
        "items": [],
        "complete": false,
        "limitation": "position_limit"
    }))?;
    let mut items_buf = Vec::with_capacity(64 * 1024);
    let mut item_scratch = Vec::with_capacity(1024);
    let mut limitation: Option<&str> = None;
    let (mut positions, mut item_count) = (0_i64, 0_i64);
    let mut previous: Option<&str> = None;

    for feature in features {
        if previous.is_some_and(|id| feature.feature_id.as_str() <= id) {
            return Err(AppError::internal("viewport order must be strictly stable"));
        }
        previous = Some(feature.feature_id.as_str());
        if item_count == limit {
            limitation = Some("feature_limit");
            break;
        }
        let count = i64::from(feature.positions);
        if count < 1 || positions + count > MAX_VIEWPORT_POSITIONS {
            limitation = Some("position_limit");
            break;
        }
        let wkb = feature.wkb.as_deref().ok_or_else(|| {
            AppError::internal("bounded geometry marker has inconsistent positions")
        })?;
        let geom = decode_display_geometry(wkb, count).ok_or_else(|| {
            AppError::internal("geometry cannot be presented in map display domain")
        })?;

        item_scratch.clear();
        item_scratch.extend_from_slice(br#"{"feature_id":"#);
        serde_json::to_writer(&mut item_scratch, &feature.feature_id)
            .map_err(|_| serialization_failed())?;
        item_scratch.extend_from_slice(br#","geometry":{"coordinate_reference":"#);
        item_scratch.extend_from_slice(&reference);
        item_scratch.extend_from_slice(br#","coordinates":"#);
        serde_json::to_writer(&mut item_scratch, &geom).map_err(|_| serialization_failed())?;
        item_scratch.extend_from_slice(br#","type":""#);
        item_scratch.extend_from_slice(geom.kind().as_bytes());
        item_scratch.extend_from_slice(br#""}}"#);

        let comma = usize::from(item_count > 0);
        if overhead + items_buf.len() + comma + item_scratch.len() > MAX_VIEWPORT_BYTES {
            limitation = Some("byte_limit");
            break;
        }
        if comma == 1 {
            items_buf.push(b',');
        }
        items_buf.extend_from_slice(&item_scratch);
        positions += count;
        item_count += 1;
    }

    let complete_str = if limitation.is_none() {
        "true"
    } else {
        "false"
    };
    let limitation_json = match limitation {
        Some(lim) => serde_json::to_string(lim).map_err(|_| serialization_failed())?,
        None => "null".to_owned(),
    };
    let mut out = Vec::with_capacity(
        64 + context_bytes.len() + items_buf.len() + complete_str.len() + limitation_json.len(),
    );
    write!(out, r#"{{"complete":{complete_str},"context":"#).map_err(|_| serialization_failed())?;
    out.extend_from_slice(&context_bytes);
    out.extend_from_slice(br#","items":["#);
    out.extend_from_slice(&items_buf);
    write!(out, r#"],"limitation":{limitation_json}}}"#).map_err(|_| serialization_failed())?;

    if out.len() > MAX_VIEWPORT_BYTES {
        return Err(read_unavailable());
    }
    Ok(out)
}

enum DecodedGeometry {
    Point([f64; 2]),
    LineString(Vec<[f64; 2]>),
    Polygon(Vec<Vec<[f64; 2]>>),
}

impl DecodedGeometry {
    fn kind(&self) -> &'static str {
        match self {
            Self::Point(_) => "Point",
            Self::LineString(_) => "LineString",
            Self::Polygon(_) => "Polygon",
        }
    }

    fn into_value(self) -> Value {
        match self {
            Self::Point(pt) => json!(pt),
            Self::LineString(line) => json!(line),
            Self::Polygon(rings) => json!(rings),
        }
    }
}

impl Serialize for DecodedGeometry {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Point(pt) => pt.serialize(serializer),
            Self::LineString(line) => line.serialize(serializer),
            Self::Polygon(rings) => rings.serialize(serializer),
        }
    }
}

struct WkbReader<'a> {
    bytes: &'a [u8],
    offset: usize,
    remaining: u32,
}

impl<'a> WkbReader<'a> {
    fn take(&mut self, len: usize) -> Option<&'a [u8]> {
        let slice = self.bytes.get(self.offset..self.offset.checked_add(len)?)?;
        self.offset += len;
        Some(slice)
    }

    fn word(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    /// Positions of one part, checked like `validate_ready_geometry` for
    /// EPSG:4326 display: XY domain, display latitude, no longitude wrap.
    fn part(&mut self, count: u32) -> Option<Vec<[f64; 2]>> {
        if !(1..=self.remaining).contains(&count) {
            return None;
        }
        self.remaining -= count;
        let bytes = self.take(usize::try_from(count).ok()?.checked_mul(16)?)?;
        let mut positions = Vec::with_capacity(bytes.len() / 16);
        for chunk in bytes.chunks_exact(16) {
            let (x, y) = chunk.split_at(8);
            let x = f64::from_le_bytes(x.try_into().ok()?);
            let y = f64::from_le_bytes(y.try_into().ok()?);
            if !(-180.0..=180.0).contains(&x) || !(-90.0..=90.0).contains(&y) {
                return None;
            }
            if y.abs() > WEB_MERCATOR_MAX_LATITUDE {
                return None;
            }
            positions.push([x, y]);
        }
        let wraps = positions
            .windows(2)
            .any(|pair| (pair[0][0] - pair[1][0]).abs() > 180.0);
        (!wraps).then_some(positions)
    }
}

fn decode_display_geometry(wkb: &[u8], expected_positions: i64) -> Option<DecodedGeometry> {
    let mut reader = WkbReader {
        bytes: wkb,
        offset: 0,
        remaining: 4096,
    };
    if reader.take(1)?[0] != 1 {
        return None;
    }
    let (geom, total) = match reader.word()? {
        1 => {
            let part = reader.part(1)?;
            (DecodedGeometry::Point(part[0]), 1_usize)
        }
        2 => {
            let count = reader.word()?;
            let part = reader.part(count)?;
            let total = part.len();
            (DecodedGeometry::LineString(part), total)
        }
        3 => {
            let rings = reader.word()?;
            if !(1..=1024).contains(&rings) {
                return None;
            }
            let mut parts = Vec::with_capacity(usize::try_from(rings).ok()?);
            let mut total = 0_usize;
            for _ in 0..rings {
                let count = reader.word()?;
                let part = reader.part(count)?;
                total += part.len();
                parts.push(part);
            }
            (DecodedGeometry::Polygon(parts), total)
        }
        _ => return None,
    };
    if reader.offset != wkb.len() || i64::try_from(total).ok()? != expected_positions {
        return None;
    }
    Some(geom)
}

/// Decode little-endian 2D OGC WKB (Point, LineString, Polygon) without
/// rounding, like `_read_native_wkb`, and require the exact position count.
fn decode_display_wkb(wkb: &[u8], expected_positions: i64) -> Option<(&'static str, Value)> {
    let geom = decode_display_geometry(wkb, expected_positions)?;
    let kind = geom.kind();
    Some((kind, geom.into_value()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(x: f64, y: f64) -> Vec<u8> {
        let mut wkb = vec![1, 1, 0, 0, 0];
        wkb.extend_from_slice(&x.to_le_bytes());
        wkb.extend_from_slice(&y.to_le_bytes());
        wkb
    }

    fn polygon(ring: &[[f64; 2]]) -> Vec<u8> {
        let mut wkb = vec![1, 3, 0, 0, 0, 1, 0, 0, 0];
        wkb.extend_from_slice(&(ring.len() as u32).to_le_bytes());
        for [x, y] in ring {
            wkb.extend_from_slice(&x.to_le_bytes());
            wkb.extend_from_slice(&y.to_le_bytes());
        }
        wkb
    }

    fn context() -> Value {
        json!({ "response_crs": reference_coordinate_wire(4326) })
    }

    fn row(id: &str, wkb: Vec<u8>, positions: i32) -> RenderRow {
        RenderRow {
            feature_id: id.into(),
            positions,
            wkb: Some(wkb),
        }
    }

    #[test]
    fn bounds_reject_nonfinite_wrapped_and_degenerate_viewports() {
        assert!(ViewportBounds::new(105.0, 20.0, 106.0, 21.0).is_some());
        assert!(ViewportBounds::new(-180.0, -85.0, 180.0, 85.0).is_some());
        assert!(ViewportBounds::new(f64::NAN, 20.0, 106.0, 21.0).is_none());
        assert!(ViewportBounds::new(105.0, 20.0, f64::INFINITY, 21.0).is_none());
        assert!(ViewportBounds::new(170.0, 20.0, -170.0, 21.0).is_none());
        assert!(ViewportBounds::new(105.0, 21.0, 106.0, 21.0).is_none());
        assert!(ViewportBounds::new(105.0, 20.0, 106.0, 86.0).is_none());
    }

    #[test]
    fn read_pins_follow_the_fastapi_pattern() {
        let good = "a".repeat(64);
        let pins = ReadPins {
            content_revision: &good,
            schema_revision: &good,
        };
        assert!(pins.well_formed());
        let upper = "A".repeat(64);
        let pins = ReadPins {
            content_revision: &upper,
            schema_revision: &good,
        };
        assert!(!pins.well_formed());
    }

    #[test]
    fn wkb_decodes_exactly_and_rejects_display_violations() {
        let (kind, coordinates) = decode_display_wkb(&point(105.5, 21.0), 1).unwrap();
        assert_eq!((kind, coordinates), ("Point", json!([105.5, 21.0])));
        let ring = [[105.0, 20.0], [106.0, 20.0], [106.0, 21.0], [105.0, 20.0]];
        let (kind, coordinates) = decode_display_wkb(&polygon(&ring), 4).unwrap();
        assert_eq!(kind, "Polygon");
        assert_eq!(coordinates, json!([ring]));
        assert!(decode_display_wkb(&polygon(&ring), 5).is_none());
        assert!(decode_display_wkb(&point(105.0, 86.0), 1).is_none());
        let wrap = [[-179.0, 0.0], [179.0, 0.0], [179.0, 1.0], [-179.0, 0.0]];
        assert!(decode_display_wkb(&polygon(&wrap), 4).is_none());
        let mut trailing = point(1.0, 1.0);
        trailing.push(0);
        assert!(decode_display_wkb(&trailing, 1).is_none());
    }

    #[test]
    fn viewport_prefix_reports_feature_and_position_limits() {
        let rows = [
            row("a", point(105.0, 20.0), 1),
            row("b", point(105.1, 20.1), 1),
        ];
        let body = build_viewport_response(context(), &rows, 1).unwrap();
        assert_eq!(body["items"].as_array().unwrap().len(), 1);
        assert_eq!(body["complete"], false);
        assert_eq!(body["limitation"], "feature_limit");

        let body = build_viewport_response(context(), &rows, 2).unwrap();
        assert_eq!(body["complete"], true);
        assert_eq!(body["limitation"], Value::Null);

        let over = [
            row("a", point(105.0, 20.0), 1),
            RenderRow {
                feature_id: "b".into(),
                positions: 20_000,
                wkb: None,
            },
        ];
        let body = build_viewport_response(context(), &over, 10).unwrap();
        assert_eq!(body["items"].as_array().unwrap().len(), 1);
        assert_eq!(body["limitation"], "position_limit");

        let unordered = [row("b", point(1.0, 1.0), 1), row("a", point(1.0, 1.0), 1)];
        assert!(build_viewport_response(context(), &unordered, 10).is_err());
    }

    #[test]
    fn viewport_json_bytes_matches_value_response_and_tile_bounds() {
        let ring = [[105.0, 20.0], [106.0, 20.0], [106.0, 21.0], [105.0, 20.0]];
        let rows = [
            row("a", point(105.123456, 20.654321), 1),
            row("b", polygon(&ring), 4),
        ];
        for limit in [1, 2] {
            let expected = build_viewport_response(context(), &rows, limit).unwrap();
            let raw = build_viewport_json_bytes(&context(), &rows, limit).unwrap();
            let parsed: Value = serde_json::from_slice(&raw).unwrap();
            assert_eq!(parsed, expected);
            assert_eq!(raw.len(), json_size(&expected).unwrap());
        }
        let (w, s, e, n) = tile_geographic_bounds(0, 0, 0);
        assert_eq!((w, e), (-180.0, 180.0));
        assert!((n - WEB_MERCATOR_MAX_LATITUDE).abs() < 1e-9);
        assert!((s + WEB_MERCATOR_MAX_LATITUDE).abs() < 1e-9);
    }
}
