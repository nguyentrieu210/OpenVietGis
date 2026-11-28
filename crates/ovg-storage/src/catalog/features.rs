//! Pinned feature reads (`pageSpatialFeatures`, `getSpatialFeature`).
//!
//! Response CRS copies run the registry's fixed `ST_TransformPipeline` steps in
//! PostGIS, as the Python reference does; never a bare `ST_Transform`.

use std::collections::HashSet;

use ovg_core::crs::{
    native_storage_coordinate_wire, reference_transformation, CoordinateBounds, FixtureRequirement,
    TransformationProfile,
};
use ovg_core::AppError;
use ovg_spatial::{parse_wkb, to_geojson, ValidationLimits};
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

use super::{db_error, resource_not_found, ProjectRead};

/// One validated feature read request.
pub struct FeatureRequest<'r> {
    pub layer_id: Uuid,
    pub content_revision: &'r str,
    pub schema_revision: &'r str,
    pub include_geometry: bool,
    pub response_crs: Option<u32>,
}

enum Selection<'r> {
    Page {
        after: Option<&'r str>,
        limit: usize,
    },
    One(&'r str),
}

struct Snapshot {
    context: Value,
    source_id: Uuid,
    native: u32,
    family: String,
    fields: HashSet<String>,
    target: Option<(u32, Value)>,
    edge: Option<&'static TransformationProfile>,
    native_projection: Option<crate::native_projection::NativeProjection>,
    datum_read: Option<super::regional_crs::VerifiedDatumRead>,
}

impl ProjectRead<'_> {
    pub async fn feature_page(
        &mut self,
        request: &FeatureRequest<'_>,
        after: Option<&str>,
        limit: usize,
    ) -> Result<Value, AppError> {
        let snapshot = self.feature_snapshot(request).await?;
        if !(1..=1000).contains(&limit) {
            return Err(invalid_request("feature page limit out of bounds"));
        }
        if let Some(after) = after {
            let pins: Option<(String, String)> = sqlx::query_as(
                "SELECT content_revision, schema_revision FROM spatial.reference_features
                 WHERE organization_id = $1 AND project_id = $2 AND source_id = $3
                   AND feature_id = $4",
            )
            .bind(self.tx.organization_id())
            .bind(self.project_id)
            .bind(snapshot.source_id)
            .bind(after)
            .fetch_optional(&mut **self.tx.tx_mut())
            .await
            .map_err(db_error)?;
            // A cursor hidden by object RLS is indistinguishable from a missing one.
            let (content, schema) = pins.ok_or_else(AppError::invalid_cursor)?;
            if (content.as_str(), schema.as_str())
                != (request.content_revision, request.schema_revision)
            {
                return Err(AppError::internal("stored cursor pins disagree"));
            }
        }
        let mut items = self
            .feature_rows(&snapshot, request, Selection::Page { after, limit })
            .await?;
        let next = (items.len() > limit).then(|| {
            items.truncate(limit);
            items[limit - 1]["feature_id"].clone()
        });
        Ok(json!({
            "context": snapshot.context,
            "items": items,
            "next_after_feature_id": next,
        }))
    }

    pub async fn feature_detail(
        &mut self,
        request: &FeatureRequest<'_>,
        feature_id: &str,
    ) -> Result<Value, AppError> {
        let snapshot = self.feature_snapshot(request).await?;
        let feature = self
            .feature_rows(&snapshot, request, Selection::One(feature_id))
            .await?
            .pop()
            .ok_or_else(resource_not_found)?;
        Ok(json!({ "context": snapshot.context, "feature": feature }))
    }

    async fn feature_snapshot(
        &mut self,
        request: &FeatureRequest<'_>,
    ) -> Result<Snapshot, AppError> {
        let layer = self
            .layer_rows(Some(request.layer_id), None, 1)
            .await?
            .pop()
            .ok_or_else(resource_not_found)?;
        let source_id: Uuid = layer.try_get("source_id").map_err(db_error)?;
        let record = self.get_source_record(source_id).await?;
        if !record.ready {
            return Err(AppError::conflict(
                "SOURCE_NOT_READY",
                "The source is not ready.",
            ));
        }
        let wire = record.wire;
        let content = wire["content_revision"].as_str().unwrap_or_default();
        let schema = wire["schema_revision"].as_str().unwrap_or_default();
        if (content, schema) != (request.content_revision, request.schema_revision) {
            return Err(AppError::conflict(
                "STALE_READ_PIN",
                "Read pins changed; reload the source and start a new traversal.",
            ));
        }
        let native = wire["stored_crs"]["code"]
            .as_u64()
            .and_then(|code| u32::try_from(code).ok())
            .ok_or_else(|| AppError::internal("ready source requires native CRS"))?;
        let mut target = None;
        let mut edge = None;
        let mut native_projection = None;
        let mut datum_read = None;
        if request.include_geometry {
            let code = request.response_crs.unwrap_or(native);
            let reference = native_storage_coordinate_wire(code)
                .ok_or_else(|| AppError::unsupported_crs(code))?;
            if code == 4326 && ovg_core::crs::is_extended_native_vn2000(native) {
                if self.gis_scoped {
                    return Err(out_of_area());
                }
                let org = self.tx.organization_id();
                datum_read = Some(
                    super::regional_crs::verify_extended_datum_read(
                        self.tx.tx_mut(),
                        org,
                        self.project_id,
                        source_id,
                        native,
                        "f.geometry",
                    )
                    .await?,
                );
            } else if code != native && crate::native_projection::is_native_pair(native, code) {
                if self.gis_scoped {
                    return Err(out_of_area());
                }
                let projection = crate::native_projection::NativeProjection::new(native, code)?;
                let org = self.tx.organization_id();
                projection
                    .verify_source(self.tx.tx_mut(), org, self.project_id, source_id)
                    .await?;
                native_projection = Some(projection);
            } else if code != native {
                edge = Some(
                    reference_transformation(native, code)
                        .ok_or_else(|| AppError::unsupported_transformation(native, code))?,
                );
            }
            target = Some((code, reference));
        } else if request.response_crs.is_some() {
            return Err(invalid_request("response CRS requires geometry"));
        }
        if let Some(candidate) = edge.filter(|e| e.fixture_requirement != FixtureRequirement::None)
        {
            if self.gis_scoped {
                return Err(out_of_area());
            }
            let org = self.tx.organization_id();
            edge = Some(
                super::regional_crs::verify_regional_source(
                    self.tx.tx_mut(),
                    org,
                    self.project_id,
                    source_id,
                    candidate,
                )
                .await?,
            );
        }
        let fields = wire["schema"]["fields"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|field| field["field_id"].as_str().map(str::to_owned))
            .collect();
        let context = json!({
            "organization_id": self.tx.organization_id(),
            "project_id": self.project_id,
            "layer_id": request.layer_id,
            "source_id": source_id,
            "content_revision": content,
            "schema_revision": schema,
            "geometry_included": request.include_geometry,
            "response_crs": target.as_ref().map(|(_, reference)| reference.clone()),
            "transformation": edge.map(TransformationProfile::to_wire).or_else(||native_projection.as_ref().map(|p|p.wire())).or_else(||datum_read.as_ref().map(|r|r.wire.clone())),
        });
        Ok(Snapshot {
            context,
            source_id,
            native,
            family: wire["schema"]["geometry_family"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            fields,
            target,
            edge,
            native_projection,
            datum_read,
        })
    }

    async fn feature_rows(
        &mut self,
        snapshot: &Snapshot,
        request: &FeatureRequest<'_>,
        selection: Selection<'_>,
    ) -> Result<Vec<Value>, AppError> {
        let steps = snapshot.edge.map_or(&[][..], |edge| edge.steps);
        let mut response = if steps.is_empty() {
            "NULL::geometry".to_owned()
        } else {
            "f.geometry".to_owned()
        };
        for (index, step) in steps.iter().enumerate() {
            // Function and SRID come from the static registry; pipelines are bound.
            let function = if step.inverse {
                "ST_InverseTransformPipeline"
            } else {
                "ST_TransformPipeline"
            };
            response = format!(
                "{function}({response}, ${}, {})",
                index + 6,
                step.target_code
            );
        }
        if let Some(projection) = &snapshot.native_projection {
            response = projection.expression("f.geometry");
        }
        if let Some(datum) = &snapshot.datum_read {
            response = datum.expression.clone();
        }
        let (filter, key, maximum) = match selection {
            Selection::Page { after, limit } => (
                r#"($4::text IS NULL OR f.feature_id COLLATE "C" > $4)"#,
                after,
                limit as i64 + 1,
            ),
            Selection::One(feature_id) => ("f.feature_id = $4", Some(feature_id), 1),
        };
        let sql = format!(
            r#"SELECT f.feature_id, f.attributes, f.geometry_state, f.content_revision,
                      f.schema_revision, ST_SRID(f.geometry) AS native_srid,
                      CASE WHEN ST_Zmflag(f.geometry) = 0
                                AND ST_NPoints(f.geometry) BETWEEN 1 AND 4096
                                AND NOT ST_IsEmpty(f.geometry)
                           THEN ST_IsValid(f.geometry) ELSE false END AS valid_geometry,
                      CASE WHEN ST_Zmflag(f.geometry) = 0
                                AND ST_NPoints(f.geometry) BETWEEN 1 AND 4096
                                AND NOT ST_IsEmpty(f.geometry)
                           THEN ST_AsBinary(f.geometry, 'NDR') END AS native_wkb,
                      ST_SRID(t.geometry) AS response_srid,
                      COALESCE(ST_IsValid(t.geometry), false) AS response_valid,
                      ST_AsBinary(t.geometry, 'NDR') AS response_wkb
               FROM spatial.reference_features AS f
               CROSS JOIN LATERAL (SELECT {response} AS geometry) AS t
               WHERE f.organization_id = $1 AND f.project_id = $2 AND f.source_id = $3
                 AND {filter}
               ORDER BY f.feature_id COLLATE "C" LIMIT $5"#
        );
        let mut query = sqlx::query(&sql)
            .bind(self.tx.organization_id())
            .bind(self.project_id)
            .bind(snapshot.source_id)
            .bind(key)
            .bind(maximum);
        for step in steps {
            query = query.bind(step.pipeline);
        }
        let rows = query
            .fetch_all(&mut **self.tx.tx_mut())
            .await
            .map_err(|error| {
                if steps.is_empty()
                    && snapshot.native_projection.is_none()
                    && snapshot.datum_read.is_none()
                {
                    db_error(error)
                } else {
                    transformation_unavailable()
                }
            })?;

        let mut items = Vec::with_capacity(rows.len());
        for row in &rows {
            let feature_id: String = row.try_get("feature_id").map_err(db_error)?;
            let attributes: Value = row.try_get("attributes").map_err(db_error)?;
            let state: String = row.try_get("geometry_state").map_err(db_error)?;
            let content: String = row.try_get("content_revision").map_err(db_error)?;
            let schema: String = row.try_get("schema_revision").map_err(db_error)?;
            let srid: Option<i32> = row.try_get("native_srid").map_err(db_error)?;
            let valid: bool = row.try_get("valid_geometry").map_err(db_error)?;
            let wkb: Option<Vec<u8>> = row.try_get("native_wkb").map_err(db_error)?;
            let schema_matches = attributes.as_object().is_some_and(|object| {
                object.len() == snapshot.fields.len()
                    && object.keys().all(|key| snapshot.fields.contains(key))
            });
            if state != "present"
                || !valid
                || (content.as_str(), schema.as_str())
                    != (request.content_revision, request.schema_revision)
                || srid != i32::try_from(snapshot.native).ok()
                || !schema_matches
            {
                return Err(AppError::internal(
                    "stored feature disagrees with its pinned source",
                ));
            }
            let mut item = json!({
                "feature_id": feature_id,
                "attributes": attributes,
                "geometry_state": "present",
            });
            let native = geometry(wkb.as_deref(), &snapshot.family)
                .ok_or_else(|| AppError::internal("ready feature has invalid geometry"))?;
            if let Some((code, reference)) = &snapshot.target {
                let mut output = native;
                if let Some(edge) = snapshot.edge {
                    let bounds = &edge.geographic_bounds;
                    if matches!(snapshot.native, 4326 | 4756)
                        && !within(&output["coordinates"], bounds)
                    {
                        return Err(out_of_area());
                    }
                    let response_srid: Option<i32> =
                        row.try_get("response_srid").map_err(db_error)?;
                    let response_valid: bool = row.try_get("response_valid").map_err(db_error)?;
                    let response: Option<Vec<u8>> =
                        row.try_get("response_wkb").map_err(db_error)?;
                    if !response_valid || response_srid != i32::try_from(*code).ok() {
                        return Err(transformation_unavailable());
                    }
                    output = geometry(response.as_deref(), &snapshot.family)
                        .ok_or_else(transformation_unavailable)?;
                    if matches!(*code, 4326 | 4756) && !within(&output["coordinates"], bounds) {
                        return Err(out_of_area());
                    }
                }
                if snapshot.native_projection.is_some() || snapshot.datum_read.is_some() {
                    let response_srid: Option<i32> =
                        row.try_get("response_srid").map_err(db_error)?;
                    let valid: bool = row.try_get("response_valid").map_err(db_error)?;
                    let wkb: Option<Vec<u8>> = row.try_get("response_wkb").map_err(db_error)?;
                    if !valid || response_srid != i32::try_from(*code).ok() {
                        return Err(transformation_unavailable());
                    }
                    output = geometry(wkb.as_deref(), &snapshot.family)
                        .ok_or_else(transformation_unavailable)?;
                    if let Some(datum) = &snapshot.datum_read {
                        if !within(&output["coordinates"], &datum.bounds) {
                            return Err(out_of_area());
                        }
                    }
                }
                output["coordinate_reference"] = reference.clone();
                item["geometry"] = output;
            }
            items.push(item);
        }
        Ok(items)
    }
}

/// Decode a 2D Point/LineString/Polygon of the source family into GeoJSON.
fn geometry(wkb: Option<&[u8]>, family: &str) -> Option<Value> {
    let parsed = parse_wkb(wkb?, &ValidationLimits::default()).ok()?;
    let value = to_geojson(&parsed.geometry);
    (value["type"] == family).then_some(value)
}

fn within(coordinates: &Value, bounds: &CoordinateBounds) -> bool {
    let Some(items) = coordinates.as_array() else {
        return false;
    };
    match (
        items.first().and_then(Value::as_f64),
        items.get(1).and_then(Value::as_f64),
    ) {
        (Some(x), Some(y)) => items.len() == 2 && bounds.contains(x, y, 0.0),
        _ => !items.is_empty() && items.iter().all(|item| within(item, bounds)),
    }
}

fn invalid_request(message: &'static str) -> AppError {
    AppError::validation("INVALID_FEATURE_REQUEST", message)
}

fn out_of_area() -> AppError {
    AppError::validation(
        "TRANSFORMATION_OUT_OF_AREA",
        "The source is outside the approved operation area or evidence.",
    )
}

fn transformation_unavailable() -> AppError {
    AppError::validation(
        "TRANSFORMATION_UNAVAILABLE",
        "The requested operation is unavailable.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn area_check_walks_every_vertex() {
        let bounds = CoordinateBounds::new(-180.0, -85.0, 180.0, 85.0);
        assert!(within(&json!([105.0, 21.0]), &bounds));
        assert!(within(
            &json!([[[0.0, 0.0], [1.0, 1.0], [0.0, 0.0]]]),
            &bounds
        ));
        assert!(!within(&json!([[0.0, 0.0], [1.0, 86.0]]), &bounds));
        assert!(!within(&json!([]), &bounds));
        assert!(!within(&json!("x"), &bounds));
    }
}
