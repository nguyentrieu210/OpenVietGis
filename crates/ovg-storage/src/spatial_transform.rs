//! Server-side CRS transformation through the fixed PostGIS pipelines.
//!
//! Reads must use this path, never the portable kernel or a bare `ST_Transform`:
//! PROJ's default VN-2000 operation departs from EPSG:6960 by about 0.8 m in
//! southern provinces (Spec 064 evidence `e02-implementation.md`). As in the
//! Python reference, callers check fixture provenance and the operation area of
//! the approved edge before and after the steps.

use ovg_core::crs::TransformationProfile;
use ovg_core::AppError;
use sqlx::PgConnection;

/// Apply every explicit step of an approved edge to `(x, y)` coordinates.
pub async fn apply_steps(
    conn: &mut PgConnection,
    edge: &TransformationProfile,
    points: &[(f64, f64)],
) -> Result<Vec<(f64, f64)>, AppError> {
    let (mut xs, mut ys): (Vec<f64>, Vec<f64>) = points.iter().copied().unzip();
    for step in edge.steps {
        let input = srid(step.source_code)?;
        let output = srid(step.target_code)?;
        // The function name comes only from this boolean, the pipeline only
        // from the static registry; no caller value reaches the SQL text.
        let function = if step.inverse {
            "ST_InverseTransformPipeline"
        } else {
            "ST_TransformPipeline"
        };
        let sql = format!(
            "SELECT ST_X(g.geom), ST_Y(g.geom), ST_SRID(g.geom)
             FROM unnest($1::float8[], $2::float8[]) WITH ORDINALITY AS p(x, y, n),
                  LATERAL {function}(ST_SetSRID(ST_MakePoint(p.x, p.y), $3), $4, $5) AS g(geom)
             ORDER BY p.n"
        );
        let rows: Vec<(f64, f64, i32)> = sqlx::query_as(&sql)
            .bind(&xs)
            .bind(&ys)
            .bind(input)
            .bind(step.pipeline)
            .bind(output)
            .fetch_all(&mut *conn)
            .await
            .map_err(|_| unavailable())?;
        let valid = rows.len() == xs.len()
            && rows
                .iter()
                .all(|&(x, y, srid)| x.is_finite() && y.is_finite() && srid == output);
        if !valid {
            return Err(unavailable());
        }
        (xs, ys) = rows.into_iter().map(|(x, y, _)| (x, y)).unzip();
    }
    Ok(xs.into_iter().zip(ys).collect())
}

fn srid(code: u32) -> Result<i32, AppError> {
    i32::try_from(code).map_err(|_| AppError::unsupported_crs(code))
}

fn unavailable() -> AppError {
    AppError::validation(
        "TRANSFORMATION_UNAVAILABLE",
        "The requested operation is unavailable.",
    )
}
