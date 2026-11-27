//! E02 write operations over the canonical PostgreSQL spatial catalog schema.

use std::collections::{BTreeMap, HashSet};

use chrono::{DateTime, Utc};
use ovg_core::AppError;
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::Row;
use uuid::Uuid;

use crate::spatial_pins::{schema_revision_digest, validate_ready_geojson, FieldSpec};

use super::fixtures::{
    diff_snapshots, materialize_fixture, source_snapshot, verify_native_postgis, SourceSnapshotArgs,
};
use super::{db_error, resource_not_found, ProjectRead};

const MAX_REVISION: i64 = 9_007_199_254_740_991;

/// Fixed-detail errors matching FastAPI `HTTPException(status, detail)`.
pub const DETAIL_CODE: &str = "CATALOG_HTTP_DETAIL";

#[derive(Debug, Clone, Serialize)]
pub struct ManualFieldInput {
    pub field_id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: String,
    pub nullable: bool,
}

#[derive(Debug, Clone)]
pub struct GeometryPreviewInput {
    pub action: String,
    pub content_revision: String,
    pub schema_revision: String,
    pub feature_ids: Vec<String>,
    pub blade: Vec<Vec<f64>>,
}

fn detail_422(msg: &str) -> AppError {
    AppError::validation(DETAIL_CODE, msg)
}

fn detail_409(msg: &str) -> AppError {
    AppError::conflict(DETAIL_CODE, msg)
}

fn clean_name(raw: &str) -> Result<String, AppError> {
    let cleaned = raw.trim();
    if cleaned.is_empty() || cleaned.chars().count() > 200 || cleaned.contains('\0') {
        return Err(AppError::validation(
            "INVALID_REQUEST",
            "invalid source name",
        ));
    }
    Ok(cleaned.to_owned())
}

fn layer_snapshot(
    name: &str,
    source_id: Uuid,
    lifecycle: &str,
    rev: i64,
) -> BTreeMap<&'static str, Value> {
    let mut m = BTreeMap::new();
    m.insert("name", json!(name));
    m.insert("source_id", json!(source_id.to_string()));
    m.insert("lifecycle", json!(lifecycle));
    m.insert("layer_revision", json!(rev));
    m
}

struct CatalogEvidence<'a> {
    actor_display: &'a str,
    resource_type: &'a str,
    resource_id: Uuid,
    source_id: Option<Uuid>,
    layer_id: Option<Uuid>,
    operation: &'a str,
    prior_revision: Option<Value>,
    new_revision: Value,
    changes: Value,
    now: DateTime<Utc>,
}

impl ProjectRead<'_> {
    async fn append_catalog_evidence(&mut self, ev: CatalogEvidence<'_>) -> Result<Uuid, AppError> {
        let change_id = Uuid::new_v4();
        let audit_id = Uuid::new_v4();
        let action = format!("spatial.catalog.{}", ev.operation);
        let metadata = json!({
            "project_id": self.project_id.to_string(),
            "catalog_change_id": change_id.to_string(),
        });
        sqlx::query(
            "INSERT INTO audit.audit_events (
                 id, organization_id, actor_type, actor_id, actor_display_snapshot,
                 action, resource_type, resource_id, outcome, metadata, created_at
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'success', $9, $10)",
        )
        .bind(audit_id)
        .bind(self.tx.organization_id())
        .bind(self.actor_type())
        .bind(self.tx.actor_id())
        .bind(ev.actor_display)
        .bind(&action)
        .bind(ev.resource_type)
        .bind(ev.resource_id)
        .bind(&metadata)
        .bind(ev.now)
        .execute(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;

        sqlx::query(
            "INSERT INTO spatial.catalog_changes (
                 id, organization_id, project_id, resource_type, resource_id, source_id,
                 layer_id, actor_type, actor_id, actor_display_snapshot, operation, created_at,
                 audit_event_id, prior_revision, new_revision, changes
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)",
        )
        .bind(change_id)
        .bind(self.tx.organization_id())
        .bind(self.project_id)
        .bind(ev.resource_type)
        .bind(ev.resource_id)
        .bind(ev.source_id)
        .bind(ev.layer_id)
        .bind(self.actor_type())
        .bind(self.tx.actor_id())
        .bind(ev.actor_display)
        .bind(ev.operation)
        .bind(ev.now)
        .bind(audit_id)
        .bind(ev.prior_revision)
        .bind(ev.new_revision)
        .bind(ev.changes)
        .execute(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;

        Ok(change_id)
    }

    pub async fn register_source(
        &mut self,
        reference_key: &str,
        name: &str,
        actor_display: &str,
    ) -> Result<Value, AppError> {
        self.require_unrestricted()?;
        if !self.edit_authorized {
            return Err(AppError::forbidden("missing permission: project.edit"));
        }
        let cleaned = clean_name(name)?;
        let existing: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM spatial.data_sources
             WHERE organization_id = $1 AND project_id = $2 AND reference_key = $3",
        )
        .bind(self.tx.organization_id())
        .bind(self.project_id)
        .bind(reference_key)
        .fetch_optional(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;
        if existing.is_some() {
            return Err(AppError::conflict(
                "SOURCE_ALREADY_REGISTERED",
                "This reference source is already registered.",
            ));
        }

        let mat = materialize_fixture(reference_key, None)?;
        let records_json = serde_json::to_string(
            &mat.records
                .iter()
                .map(|r| {
                    json!({
                        "feature_id": r.feature_id,
                        "attributes": r.attributes,
                        "wkt": r.geometry.as_ref().map(|g| g.wkt.as_str()),
                        "srid": r.geometry.as_ref().and_then(|g| g.srid),
                        "geometry_state": r.geometry_state,
                    })
                })
                .collect::<Vec<_>>(),
        )
        .map_err(|e| AppError::internal(format!("fixture json serialization failed: {e}")))?;

        verify_native_postgis(self.tx.tx_mut(), &mat, &records_json).await?;

        let source_id = Uuid::new_v4();
        let now = Utc::now();
        let reason_wire = mat.validation_reason.map(|r| r.to_wire());
        let inserted: Option<Uuid> = sqlx::query_scalar(
            "INSERT INTO spatial.data_sources (
                 id, organization_id, project_id, name, kind, reference_key,
                 fixture_version, manifest_digest, source_asset_id, lifecycle,
                 validation_state, validation_reason, source_crs, stored_crs, schema_state,
                 geometry_family, dimensions, feature_count_state, feature_count,
                 extent_state, extent, catalog_revision, content_revision, schema_revision,
                 created_at, updated_at, retired_at
             ) VALUES (
                 $1, $2, $3, $4, 'reference_fixture', $5, 'r003-reference-v1',
                 $6, NULL, 'active', $7, $8,
                 $9, $10, 'known', $11, $12, 'known',
                 $13, $14, $15, 1, $16, $17,
                 $18, $18, NULL
             ) ON CONFLICT (organization_id, project_id, reference_key) DO NOTHING
             RETURNING id",
        )
        .bind(source_id)
        .bind(self.tx.organization_id())
        .bind(self.project_id)
        .bind(&cleaned)
        .bind(mat.spec.key)
        .bind(&mat.manifest_digest)
        .bind(mat.validation_state)
        .bind(&reason_wire)
        .bind(mat.spec.declared_crs)
        .bind(mat.spec.stored_crs)
        .bind(mat.spec.expected_family)
        .bind(mat.spec.expected_dimensions)
        .bind(mat.records.len() as i32)
        .bind(mat.extent_state)
        .bind(&mat.extent)
        .bind(&mat.content_revision)
        .bind(&mat.schema_revision)
        .bind(now)
        .fetch_optional(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;
        if inserted.is_none() {
            return Err(AppError::conflict(
                "SOURCE_ALREADY_REGISTERED",
                "This reference source is already registered.",
            ));
        }

        let fields_json = serde_json::to_string(
            &mat.fields
                .iter()
                .map(|f| {
                    json!({
                        "field_id": f.field_id,
                        "name": f.name,
                        "ordinal": f.ordinal,
                        "type": f.field_type,
                        "nullable": f.nullable,
                    })
                })
                .collect::<Vec<_>>(),
        )
        .map_err(|e| AppError::internal(format!("fields json serialization failed: {e}")))?;
        sqlx::query(
            "INSERT INTO spatial.source_fields (
                 organization_id, project_id, source_id,
                 field_id, name, ordinal, type, nullable, schema_revision
             )
             SELECT $1, $2, $3, r.field_id, r.name, r.ordinal, r.type, r.nullable, $4
             FROM jsonb_to_recordset(CAST($5 AS jsonb))
                 AS r(field_id text, name text, ordinal integer, type text, nullable boolean)",
        )
        .bind(self.tx.organization_id())
        .bind(self.project_id)
        .bind(source_id)
        .bind(&mat.schema_revision)
        .bind(&fields_json)
        .execute(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;

        if !mat.records.is_empty() {
            sqlx::query(
                "INSERT INTO spatial.reference_features (
                     organization_id, project_id, source_id,
                     feature_id, attributes, geometry, geometry_state, content_revision,
                     schema_revision
                 )
                 SELECT $1, $2, $3, r.feature_id, r.attributes,
                        CASE WHEN r.wkt IS NULL THEN NULL
                             ELSE ST_GeomFromText(r.wkt, COALESCE(r.srid, 0)) END,
                        r.geometry_state, $4, $5
                 FROM jsonb_to_recordset(CAST($6 AS jsonb))
                     AS r(feature_id text, attributes jsonb, wkt text, srid integer, geometry_state text)",
            )
            .bind(self.tx.organization_id())
            .bind(self.project_id)
            .bind(source_id)
            .bind(&mat.content_revision)
            .bind(&mat.schema_revision)
            .bind(&records_json)
            .execute(&mut **self.tx.tx_mut())
            .await
            .map_err(db_error)?;
        }

        let reason_pair = mat.validation_reason.map(|r| (r.as_str(), r.message()));
        let cur_snap = source_snapshot(&SourceSnapshotArgs {
            name: &cleaned,
            kind: "reference_fixture",
            reference_key: mat.spec.key,
            fixture_version: "r003-reference-v1",
            manifest_digest: &mat.manifest_digest,
            lifecycle: "active",
            validation_state: mat.validation_state,
            reason: reason_pair,
            source_crs: mat.spec.declared_crs,
            stored_crs: mat.spec.stored_crs,
            schema_state: "known",
            geometry_family: mat.spec.expected_family,
            dimensions: mat.spec.expected_dimensions,
            feature_count_state: "known",
            feature_count: mat.records.len() as i64,
            extent_state: mat.extent_state,
            catalog_revision: 1,
            content_revision: &mat.content_revision,
            schema_revision: &mat.schema_revision,
        });
        let changes = diff_snapshots(&BTreeMap::new(), &cur_snap);
        let new_rev = json!({
            "catalog_revision": 1,
            "content_revision": mat.content_revision,
            "schema_revision": mat.schema_revision,
        });
        let change_id = self
            .append_catalog_evidence(CatalogEvidence {
                actor_display,
                resource_type: "data_source",
                resource_id: source_id,
                source_id: Some(source_id),
                layer_id: None,
                operation: "register_source",
                prior_revision: None,
                new_revision: new_rev.clone(),
                changes,
                now,
            })
            .await?;

        Ok(json!({
            "organization_id": self.tx.organization_id(),
            "project_id": self.project_id,
            "resource_type": "data_source",
            "resource_id": source_id,
            "change_id": change_id,
            "prior_revision": Value::Null,
            "new_revision": new_rev,
            "lifecycle": "active",
            "validation_state": mat.validation_state,
            "validation_reason": reason_wire,
        }))
    }

    pub async fn update_source(
        &mut self,
        source_id: Uuid,
        expected_revision: i64,
        name: Option<&str>,
        declared_crs: Option<i32>,
        actor_display: &str,
    ) -> Result<Value, AppError> {
        self.require_unrestricted()?;
        if !self.edit_authorized {
            return Err(AppError::forbidden("missing permission: project.edit"));
        }
        let row = sqlx::query(
            "SELECT name, kind, reference_key, fixture_version, manifest_digest, lifecycle,
                    validation_state, validation_reason, source_crs, stored_crs, schema_state,
                    geometry_family, dimensions, feature_count_state,
                    feature_count::bigint AS feature_count, extent_state, extent,
                    catalog_revision, content_revision, schema_revision
             FROM spatial.data_sources
             WHERE organization_id = $1 AND project_id = $2 AND id = $3
             FOR UPDATE",
        )
        .bind(self.tx.organization_id())
        .bind(self.project_id)
        .bind(source_id)
        .fetch_optional(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?
        .ok_or_else(resource_not_found)?;

        let lifecycle: String = row.try_get("lifecycle").map_err(db_error)?;
        if lifecycle != "active" {
            return Err(resource_not_found());
        }
        let prior_rev: i64 = row.try_get("catalog_revision").map_err(db_error)?;
        if expected_revision != prior_rev {
            return Err(AppError::conflict(
                "STALE_CATALOG_REVISION",
                "Source revision changed; read current metadata and retry.",
            ));
        }
        if !(1..MAX_REVISION).contains(&prior_rev) {
            return Err(AppError::validation(
                "INVALID_REQUEST",
                "catalog revision exhausted",
            ));
        }
        let next_rev = prior_rev + 1;

        let prior_name: String = row.try_get("name").map_err(db_error)?;
        let kind: String = row.try_get("kind").map_err(db_error)?;
        let ref_key: String = row.try_get("reference_key").map_err(db_error)?;
        let fixture_version: String = row.try_get("fixture_version").map_err(db_error)?;
        let manifest_digest: String = row.try_get("manifest_digest").map_err(db_error)?;
        let prior_val_state: String = row.try_get("validation_state").map_err(db_error)?;
        let prior_reason: Option<Value> = row.try_get("validation_reason").map_err(db_error)?;
        let prior_source_crs: Option<i32> = row.try_get("source_crs").map_err(db_error)?;
        let stored_crs: Option<i32> = row.try_get("stored_crs").map_err(db_error)?;
        let schema_state: String = row.try_get("schema_state").map_err(db_error)?;
        let geometry_family: String = row.try_get("geometry_family").map_err(db_error)?;
        let dimensions: String = row.try_get("dimensions").map_err(db_error)?;
        let count_state: String = row.try_get("feature_count_state").map_err(db_error)?;
        let feature_count: i64 = row.try_get("feature_count").map_err(db_error)?;
        let prior_extent_state: String = row.try_get("extent_state").map_err(db_error)?;
        let prior_content_rev: String = row.try_get("content_revision").map_err(db_error)?;
        let schema_rev: String = row.try_get("schema_revision").map_err(db_error)?;

        let clean_new_name = match name {
            Some(n) => clean_name(n)?,
            None => prior_name.clone(),
        };

        let prior_reason_code = prior_reason
            .as_ref()
            .and_then(|v| v.get("code"))
            .and_then(Value::as_str)
            .map(str::to_owned);
        let prior_reason_msg = prior_reason
            .as_ref()
            .and_then(|v| v.get("message"))
            .and_then(Value::as_str)
            .map(str::to_owned);
        let prior_snap = source_snapshot(&SourceSnapshotArgs {
            name: &prior_name,
            kind: &kind,
            reference_key: &ref_key,
            fixture_version: &fixture_version,
            manifest_digest: &manifest_digest,
            lifecycle: "active",
            validation_state: &prior_val_state,
            reason: prior_reason_code
                .as_deref()
                .zip(prior_reason_msg.as_deref()),
            source_crs: prior_source_crs,
            stored_crs,
            schema_state: &schema_state,
            geometry_family: &geometry_family,
            dimensions: &dimensions,
            feature_count_state: &count_state,
            feature_count,
            extent_state: &prior_extent_state,
            catalog_revision: prior_rev,
            content_revision: &prior_content_rev,
            schema_revision: &schema_rev,
        });

        let now = Utc::now();
        let prior_rev_wire = json!({
            "catalog_revision": prior_rev,
            "content_revision": prior_content_rev,
            "schema_revision": schema_rev,
        });

        let needs_crs_correction = declared_crs
            .is_some_and(|crs| Some(crs) != prior_source_crs || prior_val_state != "ready");

        let (
            new_val_state,
            new_reason,
            new_source_crs,
            new_extent_state,
            new_extent,
            new_content_rev,
            operation,
        ): (
            String,
            Option<Value>,
            Option<i32>,
            String,
            Option<Value>,
            String,
            &str,
        ) = if needs_crs_correction {
            let crs = declared_crs.unwrap();
            let valid_correction = kind == "reference_fixture"
                && ref_key == "reference_correctable_crs"
                && crs == 4326
                && stored_crs == Some(4326)
                && prior_val_state == "invalid"
                && prior_reason_code.as_deref() == Some("crs_mismatch")
                && prior_source_crs != Some(4326);
            if !valid_correction {
                return Err(AppError::validation(
                    "CRS_CORRECTION_UNVERIFIED",
                    "The requested CRS correction cannot be verified.",
                ));
            }
            let mat = materialize_fixture(&ref_key, Some(crs))?;
            if mat.manifest_digest != manifest_digest || mat.schema_revision != schema_rev {
                return Err(AppError::validation(
                    "CRS_CORRECTION_UNVERIFIED",
                    "The requested CRS correction cannot be verified.",
                ));
            }
            (
                mat.validation_state.to_owned(),
                None,
                Some(crs),
                mat.extent_state.to_owned(),
                mat.extent,
                mat.content_revision,
                "correct_source_crs",
            )
        } else {
            if clean_new_name == prior_name {
                return Err(AppError::validation(
                    "NO_CHANGES",
                    "The request does not change metadata.",
                ));
            }
            let extent: Option<Value> = row.try_get("extent").map_err(db_error)?;
            (
                prior_val_state,
                prior_reason,
                prior_source_crs,
                prior_extent_state,
                extent,
                prior_content_rev.clone(),
                "update_source",
            )
        };

        let updated: Option<Uuid> = sqlx::query_scalar(
            "UPDATE spatial.data_sources
             SET name = $4, validation_state = $5, validation_reason = $6,
                 source_crs = $7, extent_state = $8, extent = $9,
                 catalog_revision = $10, content_revision = $11, updated_at = $12
             WHERE organization_id = $1 AND project_id = $2 AND id = $3
               AND catalog_revision = $13 AND lifecycle = 'active'
             RETURNING id",
        )
        .bind(self.tx.organization_id())
        .bind(self.project_id)
        .bind(source_id)
        .bind(&clean_new_name)
        .bind(&new_val_state)
        .bind(&new_reason)
        .bind(new_source_crs)
        .bind(&new_extent_state)
        .bind(&new_extent)
        .bind(next_rev)
        .bind(&new_content_rev)
        .bind(now)
        .bind(expected_revision)
        .fetch_optional(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;
        if updated.is_none() {
            return Err(AppError::conflict(
                "STALE_CATALOG_REVISION",
                "Source revision changed; read current metadata and retry.",
            ));
        }

        if new_content_rev != prior_content_rev {
            sqlx::query(
                "UPDATE spatial.reference_features
                 SET content_revision = $4
                 WHERE organization_id = $1 AND project_id = $2 AND source_id = $3",
            )
            .bind(self.tx.organization_id())
            .bind(self.project_id)
            .bind(source_id)
            .bind(&new_content_rev)
            .execute(&mut **self.tx.tx_mut())
            .await
            .map_err(db_error)?;
        }

        let new_reason_code = new_reason
            .as_ref()
            .and_then(|v| v.get("code"))
            .and_then(Value::as_str)
            .map(str::to_owned);
        let new_reason_msg = new_reason
            .as_ref()
            .and_then(|v| v.get("message"))
            .and_then(Value::as_str)
            .map(str::to_owned);
        let cur_snap = source_snapshot(&SourceSnapshotArgs {
            name: &clean_new_name,
            kind: &kind,
            reference_key: &ref_key,
            fixture_version: &fixture_version,
            manifest_digest: &manifest_digest,
            lifecycle: "active",
            validation_state: &new_val_state,
            reason: new_reason_code.as_deref().zip(new_reason_msg.as_deref()),
            source_crs: new_source_crs,
            stored_crs,
            schema_state: &schema_state,
            geometry_family: &geometry_family,
            dimensions: &dimensions,
            feature_count_state: &count_state,
            feature_count,
            extent_state: &new_extent_state,
            catalog_revision: next_rev,
            content_revision: &new_content_rev,
            schema_revision: &schema_rev,
        });
        let changes = diff_snapshots(&prior_snap, &cur_snap);
        let new_rev_wire = json!({
            "catalog_revision": next_rev,
            "content_revision": new_content_rev,
            "schema_revision": schema_rev,
        });
        let change_id = self
            .append_catalog_evidence(CatalogEvidence {
                actor_display,
                resource_type: "data_source",
                resource_id: source_id,
                source_id: Some(source_id),
                layer_id: None,
                operation,
                prior_revision: Some(prior_rev_wire),
                new_revision: new_rev_wire.clone(),
                changes,
                now,
            })
            .await?;

        Ok(json!({
            "organization_id": self.tx.organization_id(),
            "project_id": self.project_id,
            "resource_type": "data_source",
            "resource_id": source_id,
            "change_id": change_id,
            "prior_revision": {
                "catalog_revision": prior_rev,
                "content_revision": prior_content_rev,
                "schema_revision": schema_rev,
            },
            "new_revision": new_rev_wire,
            "lifecycle": "active",
            "validation_state": new_val_state,
            "validation_reason": new_reason,
        }))
    }

    pub async fn retire_source(
        &mut self,
        source_id: Uuid,
        expected_revision: i64,
        actor_display: &str,
    ) -> Result<Value, AppError> {
        self.require_unrestricted()?;
        let row = sqlx::query(
            "SELECT name, kind, reference_key, fixture_version, manifest_digest, lifecycle,
                    validation_state, validation_reason, source_crs, stored_crs, schema_state,
                    geometry_family, dimensions, feature_count_state,
                    feature_count::bigint AS feature_count, extent_state,
                    catalog_revision, content_revision, schema_revision
             FROM spatial.data_sources
             WHERE organization_id = $1 AND project_id = $2 AND id = $3
             FOR UPDATE",
        )
        .bind(self.tx.organization_id())
        .bind(self.project_id)
        .bind(source_id)
        .fetch_optional(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?
        .ok_or_else(resource_not_found)?;

        let lifecycle: String = row.try_get("lifecycle").map_err(db_error)?;
        if lifecycle != "active" {
            return Err(resource_not_found());
        }
        let prior_rev: i64 = row.try_get("catalog_revision").map_err(db_error)?;
        if expected_revision != prior_rev {
            return Err(AppError::conflict(
                "STALE_CATALOG_REVISION",
                "Source revision changed; read current metadata and retry.",
            ));
        }
        if !(1..MAX_REVISION).contains(&prior_rev) {
            return Err(AppError::validation(
                "INVALID_REQUEST",
                "catalog revision exhausted",
            ));
        }
        let has_active: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                 SELECT 1 FROM spatial.layers
                 WHERE organization_id = $1 AND project_id = $2
                   AND source_id = $3 AND lifecycle = 'active'
             )",
        )
        .bind(self.tx.organization_id())
        .bind(self.project_id)
        .bind(source_id)
        .fetch_one(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;
        if has_active {
            return Err(AppError::conflict(
                "ACTIVE_LAYER_DEPENDENCIES",
                "Retire active Layers before retiring the source.",
            ));
        }

        let next_rev = prior_rev + 1;
        let now = Utc::now();
        let updated: Option<Uuid> = sqlx::query_scalar(
            "UPDATE spatial.data_sources
             SET lifecycle = 'retired', catalog_revision = $4, updated_at = $5, retired_at = $5
             WHERE organization_id = $1 AND project_id = $2 AND id = $3
               AND catalog_revision = $6 AND lifecycle = 'active'
             RETURNING id",
        )
        .bind(self.tx.organization_id())
        .bind(self.project_id)
        .bind(source_id)
        .bind(next_rev)
        .bind(now)
        .bind(expected_revision)
        .fetch_optional(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;
        if updated.is_none() {
            return Err(AppError::conflict(
                "STALE_CATALOG_REVISION",
                "Source revision changed; read current metadata and retry.",
            ));
        }

        let name: String = row.try_get("name").map_err(db_error)?;
        let kind: String = row.try_get("kind").map_err(db_error)?;
        let ref_key: String = row.try_get("reference_key").map_err(db_error)?;
        let fixture_version: String = row.try_get("fixture_version").map_err(db_error)?;
        let manifest_digest: String = row.try_get("manifest_digest").map_err(db_error)?;
        let val_state: String = row.try_get("validation_state").map_err(db_error)?;
        let val_reason: Option<Value> = row.try_get("validation_reason").map_err(db_error)?;
        let source_crs: Option<i32> = row.try_get("source_crs").map_err(db_error)?;
        let stored_crs: Option<i32> = row.try_get("stored_crs").map_err(db_error)?;
        let schema_state: String = row.try_get("schema_state").map_err(db_error)?;
        let geometry_family: String = row.try_get("geometry_family").map_err(db_error)?;
        let dimensions: String = row.try_get("dimensions").map_err(db_error)?;
        let count_state: String = row.try_get("feature_count_state").map_err(db_error)?;
        let feature_count: i64 = row.try_get("feature_count").map_err(db_error)?;
        let extent_state: String = row.try_get("extent_state").map_err(db_error)?;
        let content_rev: String = row.try_get("content_revision").map_err(db_error)?;
        let schema_rev: String = row.try_get("schema_revision").map_err(db_error)?;
        let rc = val_reason
            .as_ref()
            .and_then(|v| v.get("code"))
            .and_then(Value::as_str)
            .map(str::to_owned);
        let rm = val_reason
            .as_ref()
            .and_then(|v| v.get("message"))
            .and_then(Value::as_str)
            .map(str::to_owned);
        let prior_snap = source_snapshot(&SourceSnapshotArgs {
            name: &name,
            kind: &kind,
            reference_key: &ref_key,
            fixture_version: &fixture_version,
            manifest_digest: &manifest_digest,
            lifecycle: "active",
            validation_state: &val_state,
            reason: rc.as_deref().zip(rm.as_deref()),
            source_crs,
            stored_crs,
            schema_state: &schema_state,
            geometry_family: &geometry_family,
            dimensions: &dimensions,
            feature_count_state: &count_state,
            feature_count,
            extent_state: &extent_state,
            catalog_revision: prior_rev,
            content_revision: &content_rev,
            schema_revision: &schema_rev,
        });
        let cur_snap = source_snapshot(&SourceSnapshotArgs {
            name: &name,
            kind: &kind,
            reference_key: &ref_key,
            fixture_version: &fixture_version,
            manifest_digest: &manifest_digest,
            lifecycle: "retired",
            validation_state: &val_state,
            reason: rc.as_deref().zip(rm.as_deref()),
            source_crs,
            stored_crs,
            schema_state: &schema_state,
            geometry_family: &geometry_family,
            dimensions: &dimensions,
            feature_count_state: &count_state,
            feature_count,
            extent_state: &extent_state,
            catalog_revision: next_rev,
            content_revision: &content_rev,
            schema_revision: &schema_rev,
        });
        let changes = diff_snapshots(&prior_snap, &cur_snap);
        let prior_rev_wire = json!({
            "catalog_revision": prior_rev,
            "content_revision": content_rev,
            "schema_revision": schema_rev,
        });
        let new_rev_wire = json!({
            "catalog_revision": next_rev,
            "content_revision": content_rev,
            "schema_revision": schema_rev,
        });
        let change_id = self
            .append_catalog_evidence(CatalogEvidence {
                actor_display,
                resource_type: "data_source",
                resource_id: source_id,
                source_id: Some(source_id),
                layer_id: None,
                operation: "retire_source",
                prior_revision: Some(prior_rev_wire.clone()),
                new_revision: new_rev_wire.clone(),
                changes,
                now,
            })
            .await?;

        Ok(json!({
            "organization_id": self.tx.organization_id(),
            "project_id": self.project_id,
            "resource_type": "data_source",
            "resource_id": source_id,
            "change_id": change_id,
            "prior_revision": prior_rev_wire,
            "new_revision": new_rev_wire,
            "lifecycle": "retired",
            "validation_state": val_state,
            "validation_reason": val_reason,
        }))
    }

    pub async fn create_layer(
        &mut self,
        source_id: Uuid,
        name: &str,
        actor_display: &str,
    ) -> Result<Value, AppError> {
        self.require_unrestricted()?;
        if !self.edit_authorized {
            return Err(AppError::forbidden("missing permission: project.edit"));
        }
        let cleaned = clean_name(name)?;
        let row = sqlx::query(
            "SELECT lifecycle, validation_state
             FROM spatial.data_sources
             WHERE organization_id = $1 AND project_id = $2 AND id = $3
             FOR UPDATE",
        )
        .bind(self.tx.organization_id())
        .bind(self.project_id)
        .bind(source_id)
        .fetch_optional(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?
        .ok_or_else(resource_not_found)?;

        let lifecycle: String = row.try_get("lifecycle").map_err(db_error)?;
        if lifecycle != "active" {
            return Err(resource_not_found());
        }
        let val_state: String = row.try_get("validation_state").map_err(db_error)?;
        if val_state != "ready" {
            return Err(AppError::conflict(
                "SOURCE_NOT_READY",
                "The source is not ready.",
            ));
        }

        let layer_id = Uuid::new_v4();
        let now = Utc::now();
        sqlx::query(
            "INSERT INTO spatial.layers (
                 id, organization_id, project_id, source_id, name, lifecycle,
                 layer_revision, created_at, updated_at, retired_at
             ) VALUES ($1, $2, $3, $4, $5, 'active', 1, $6, $6, NULL)",
        )
        .bind(layer_id)
        .bind(self.tx.organization_id())
        .bind(self.project_id)
        .bind(source_id)
        .bind(&cleaned)
        .bind(now)
        .execute(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;

        let cur_snap = layer_snapshot(&cleaned, source_id, "active", 1);
        let changes = diff_snapshots(&BTreeMap::new(), &cur_snap);
        let new_rev_wire = json!({ "layer_revision": 1 });
        let change_id = self
            .append_catalog_evidence(CatalogEvidence {
                actor_display,
                resource_type: "layer",
                resource_id: layer_id,
                source_id: None,
                layer_id: Some(layer_id),
                operation: "create_layer",
                prior_revision: None,
                new_revision: new_rev_wire.clone(),
                changes,
                now,
            })
            .await?;

        Ok(json!({
            "organization_id": self.tx.organization_id(),
            "project_id": self.project_id,
            "resource_type": "layer",
            "resource_id": layer_id,
            "change_id": change_id,
            "prior_revision": Value::Null,
            "new_revision": new_rev_wire,
            "lifecycle": "active",
        }))
    }

    pub async fn create_manual_layer(
        &mut self,
        operation_id: Uuid,
        name: &str,
        geometry_family: &str,
        fields: &[ManualFieldInput],
        actor_display: &str,
    ) -> Result<Value, AppError> {
        self.require_unrestricted()?;
        if !self.edit_authorized {
            return Err(AppError::forbidden("missing permission: project.edit"));
        }
        let cleaned = clean_name(name)?;
        let mut seen_ids = HashSet::with_capacity(fields.len());
        let mut seen_names = HashSet::with_capacity(fields.len());
        for f in fields {
            if !seen_ids.insert(f.field_id.as_str()) || !seen_names.insert(f.name.as_str()) {
                return Err(detail_422("Field identifiers and names must be unique"));
            }
        }

        // Compute the same digest shape as Pydantic `ManualLayerRequest.model_dump_json() + actor_id`.
        #[derive(Serialize)]
        struct DigestBody<'b> {
            operation_id: Uuid,
            name: &'b str,
            geometry_family: &'b str,
            fields: &'b [ManualFieldInput],
        }
        let body_json = serde_json::to_string(&DigestBody {
            operation_id,
            name: &cleaned,
            geometry_family,
            fields,
        })
        .map_err(|e| AppError::internal(format!("manual digest serialize failed: {e}")))?;
        let digest = format!(
            "{:x}",
            Sha256::digest(format!("{body_json}{}", self.tx.actor_id()).as_bytes())
        );

        let key = format!("manual_{}", operation_id.simple());
        let org_id = self.tx.organization_id();
        let proj_id = self.project_id;
        let source_ns = format!("openvietgis/{org_id}/{proj_id}/{key}/source");
        let layer_ns = format!("openvietgis/{org_id}/{proj_id}/{key}/layer");
        let source_id = crate::editing::team::uuid5_url(&source_ns);
        let layer_id = crate::editing::team::uuid5_url(&layer_ns);

        let existing: Option<(String, Option<String>)> = sqlx::query_as(
            "SELECT kind, manifest_digest
             FROM spatial.data_sources
             WHERE organization_id = $1 AND project_id = $2 AND reference_key = $3
             FOR SHARE",
        )
        .bind(org_id)
        .bind(proj_id)
        .bind(&key)
        .fetch_optional(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;

        if let Some((kind, existing_digest)) = existing {
            let layer_lifecycle: Option<String> = sqlx::query_scalar(
                "SELECT lifecycle FROM spatial.layers
                 WHERE organization_id = $1 AND project_id = $2 AND id = $3",
            )
            .bind(org_id)
            .bind(proj_id)
            .bind(layer_id)
            .fetch_optional(&mut **self.tx.tx_mut())
            .await
            .map_err(db_error)?;
            if kind != "manual"
                || existing_digest.as_deref() != Some(digest.as_str())
                || layer_lifecycle.as_deref() != Some("active")
            {
                return Err(detail_409("Operation identifier already used"));
            }
            return Ok(json!({
                "source_id": source_id,
                "layer_id": layer_id,
                "operation_id": operation_id,
            }));
        }

        let field_specs: Vec<FieldSpec> = fields
            .iter()
            .enumerate()
            .map(|(i, f)| FieldSpec {
                field_id: f.field_id.clone(),
                name: f.name.clone(),
                ordinal: i as i32,
                field_type: f.field_type.clone(),
                nullable: f.nullable,
                schema_revision: String::new(),
            })
            .collect();
        let schema_revision = schema_revision_digest(&field_specs, geometry_family, "XY", "known");
        let content_revision = crate::spatial_pins::content_revision_digest(
            &[],
            &field_specs,
            4326,
            4326,
            geometry_family,
            "XY",
        );

        let now = Utc::now();
        let inserted: Option<Uuid> = sqlx::query_scalar(
            "INSERT INTO spatial.data_sources (
                 id, organization_id, project_id, name, kind, reference_key,
                 fixture_version, manifest_digest, source_asset_id, lifecycle,
                 validation_state, validation_reason, source_crs, stored_crs, schema_state,
                 geometry_family, dimensions, feature_count_state, feature_count,
                 extent_state, extent, catalog_revision, content_revision, schema_revision,
                 created_at, updated_at, retired_at
             ) VALUES (
                 $1, $2, $3, $4, 'manual', $5, 'manual-v1', $6, NULL, 'active',
                 'ready', NULL, 4326, 4326, 'known', $7, 'XY', 'known', 0,
                 'empty', NULL, 1, $8, $9, $10, $10, NULL
             ) ON CONFLICT (organization_id, project_id, reference_key) DO NOTHING
             RETURNING id",
        )
        .bind(source_id)
        .bind(org_id)
        .bind(proj_id)
        .bind(&cleaned)
        .bind(&key)
        .bind(&digest)
        .bind(geometry_family)
        .bind(&content_revision)
        .bind(&schema_revision)
        .bind(now)
        .fetch_optional(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;
        if inserted.is_none() {
            return Err(detail_409("Operation identifier already used"));
        }

        if !field_specs.is_empty() {
            let fields_json = serde_json::to_string(
                &field_specs
                    .iter()
                    .map(|f| {
                        json!({
                            "field_id": f.field_id,
                            "name": f.name,
                            "ordinal": f.ordinal,
                            "type": f.field_type,
                            "nullable": f.nullable,
                        })
                    })
                    .collect::<Vec<_>>(),
            )
            .map_err(|e| AppError::internal(format!("manual fields serialize failed: {e}")))?;
            sqlx::query(
                "INSERT INTO spatial.source_fields (
                     organization_id, project_id, source_id,
                     field_id, name, ordinal, type, nullable, schema_revision
                 )
                 SELECT $1, $2, $3, r.field_id, r.name, r.ordinal, r.type, r.nullable, $4
                 FROM jsonb_to_recordset(CAST($5 AS jsonb))
                     AS r(field_id text, name text, ordinal integer, type text, nullable boolean)",
            )
            .bind(org_id)
            .bind(proj_id)
            .bind(source_id)
            .bind(&schema_revision)
            .bind(&fields_json)
            .execute(&mut **self.tx.tx_mut())
            .await
            .map_err(db_error)?;
        }

        let src_snap = source_snapshot(&SourceSnapshotArgs {
            name: &cleaned,
            kind: "manual",
            reference_key: &key,
            fixture_version: "manual-v1",
            manifest_digest: &digest,
            lifecycle: "active",
            validation_state: "ready",
            reason: None,
            source_crs: Some(4326),
            stored_crs: Some(4326),
            schema_state: "known",
            geometry_family,
            dimensions: "XY",
            feature_count_state: "known",
            feature_count: 0,
            extent_state: "empty",
            catalog_revision: 1,
            content_revision: &content_revision,
            schema_revision: &schema_revision,
        });
        self.append_catalog_evidence(CatalogEvidence {
            actor_display,
            resource_type: "data_source",
            resource_id: source_id,
            source_id: Some(source_id),
            layer_id: None,
            operation: "register_source",
            prior_revision: None,
            new_revision: json!({
                "catalog_revision": 1,
                "content_revision": content_revision,
                "schema_revision": schema_revision,
            }),
            changes: diff_snapshots(&BTreeMap::new(), &src_snap),
            now,
        })
        .await?;

        sqlx::query(
            "INSERT INTO spatial.layers (
                 id, organization_id, project_id, source_id, name, lifecycle,
                 layer_revision, created_at, updated_at, retired_at
             ) VALUES ($1, $2, $3, $4, $5, 'active', 1, $6, $6, NULL)",
        )
        .bind(layer_id)
        .bind(org_id)
        .bind(proj_id)
        .bind(source_id)
        .bind(&cleaned)
        .bind(now)
        .execute(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;

        let lyr_snap = layer_snapshot(&cleaned, source_id, "active", 1);
        self.append_catalog_evidence(CatalogEvidence {
            actor_display,
            resource_type: "layer",
            resource_id: layer_id,
            source_id: None,
            layer_id: Some(layer_id),
            operation: "create_layer",
            prior_revision: None,
            new_revision: json!({ "layer_revision": 1 }),
            changes: diff_snapshots(&BTreeMap::new(), &lyr_snap),
            now,
        })
        .await?;

        Ok(json!({
            "source_id": source_id,
            "layer_id": layer_id,
            "operation_id": operation_id,
        }))
    }

    pub async fn rename_layer(
        &mut self,
        layer_id: Uuid,
        expected_revision: i64,
        name: &str,
        actor_display: &str,
    ) -> Result<Value, AppError> {
        self.require_unrestricted()?;
        if !self.edit_authorized {
            return Err(AppError::forbidden("missing permission: project.edit"));
        }
        let cleaned = clean_name(name)?;
        let row = sqlx::query(
            "SELECT source_id, name, lifecycle, layer_revision
             FROM spatial.layers
             WHERE organization_id = $1 AND project_id = $2 AND id = $3
             FOR UPDATE",
        )
        .bind(self.tx.organization_id())
        .bind(self.project_id)
        .bind(layer_id)
        .fetch_optional(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?
        .ok_or_else(resource_not_found)?;

        let lifecycle: String = row.try_get("lifecycle").map_err(db_error)?;
        if lifecycle != "active" {
            return Err(resource_not_found());
        }
        let prior_rev: i64 = row.try_get("layer_revision").map_err(db_error)?;
        if expected_revision != prior_rev {
            return Err(AppError::conflict(
                "STALE_LAYER_REVISION",
                "Layer revision changed; read current metadata and retry.",
            ));
        }
        if !(1..MAX_REVISION).contains(&prior_rev) {
            return Err(AppError::validation(
                "INVALID_REQUEST",
                "layer revision exhausted",
            ));
        }
        let prior_name: String = row.try_get("name").map_err(db_error)?;
        if prior_name == cleaned {
            return Err(AppError::validation(
                "NO_CHANGES",
                "The request does not change metadata.",
            ));
        }

        let source_id: Uuid = row.try_get("source_id").map_err(db_error)?;
        let next_rev = prior_rev + 1;
        let now = Utc::now();
        let updated: Option<Uuid> = sqlx::query_scalar(
            "UPDATE spatial.layers
             SET name = $4, layer_revision = $5, updated_at = $6
             WHERE organization_id = $1 AND project_id = $2 AND id = $3
               AND layer_revision = $7 AND lifecycle = 'active'
             RETURNING id",
        )
        .bind(self.tx.organization_id())
        .bind(self.project_id)
        .bind(layer_id)
        .bind(&cleaned)
        .bind(next_rev)
        .bind(now)
        .bind(expected_revision)
        .fetch_optional(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;
        if updated.is_none() {
            return Err(AppError::conflict(
                "STALE_LAYER_REVISION",
                "Layer revision changed; read current metadata and retry.",
            ));
        }

        let prior_snap = layer_snapshot(&prior_name, source_id, "active", prior_rev);
        let cur_snap = layer_snapshot(&cleaned, source_id, "active", next_rev);
        let prior_rev_wire = json!({ "layer_revision": prior_rev });
        let new_rev_wire = json!({ "layer_revision": next_rev });
        let change_id = self
            .append_catalog_evidence(CatalogEvidence {
                actor_display,
                resource_type: "layer",
                resource_id: layer_id,
                source_id: None,
                layer_id: Some(layer_id),
                operation: "rename_layer",
                prior_revision: Some(prior_rev_wire.clone()),
                new_revision: new_rev_wire.clone(),
                changes: diff_snapshots(&prior_snap, &cur_snap),
                now,
            })
            .await?;

        Ok(json!({
            "organization_id": self.tx.organization_id(),
            "project_id": self.project_id,
            "resource_type": "layer",
            "resource_id": layer_id,
            "change_id": change_id,
            "prior_revision": prior_rev_wire,
            "new_revision": new_rev_wire,
            "lifecycle": "active",
        }))
    }

    pub async fn retire_layer(
        &mut self,
        layer_id: Uuid,
        expected_revision: i64,
        actor_display: &str,
    ) -> Result<Value, AppError> {
        self.require_unrestricted()?;
        if !self.edit_authorized {
            return Err(AppError::forbidden("missing permission: project.edit"));
        }
        let row = sqlx::query(
            "SELECT source_id, name, lifecycle, layer_revision
             FROM spatial.layers
             WHERE organization_id = $1 AND project_id = $2 AND id = $3
             FOR UPDATE",
        )
        .bind(self.tx.organization_id())
        .bind(self.project_id)
        .bind(layer_id)
        .fetch_optional(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?
        .ok_or_else(resource_not_found)?;

        let lifecycle: String = row.try_get("lifecycle").map_err(db_error)?;
        if lifecycle != "active" {
            return Err(resource_not_found());
        }
        let prior_rev: i64 = row.try_get("layer_revision").map_err(db_error)?;
        if expected_revision != prior_rev {
            return Err(AppError::conflict(
                "STALE_LAYER_REVISION",
                "Layer revision changed; read current metadata and retry.",
            ));
        }
        if !(1..MAX_REVISION).contains(&prior_rev) {
            return Err(AppError::validation(
                "INVALID_REQUEST",
                "layer revision exhausted",
            ));
        }

        let name: String = row.try_get("name").map_err(db_error)?;
        let source_id: Uuid = row.try_get("source_id").map_err(db_error)?;
        let next_rev = prior_rev + 1;
        let now = Utc::now();
        let updated: Option<Uuid> = sqlx::query_scalar(
            "UPDATE spatial.layers
             SET lifecycle = 'retired', layer_revision = $4, updated_at = $5, retired_at = $5
             WHERE organization_id = $1 AND project_id = $2 AND id = $3
               AND layer_revision = $6 AND lifecycle = 'active'
             RETURNING id",
        )
        .bind(self.tx.organization_id())
        .bind(self.project_id)
        .bind(layer_id)
        .bind(next_rev)
        .bind(now)
        .bind(expected_revision)
        .fetch_optional(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;
        if updated.is_none() {
            return Err(AppError::conflict(
                "STALE_LAYER_REVISION",
                "Layer revision changed; read current metadata and retry.",
            ));
        }

        let prior_snap = layer_snapshot(&name, source_id, "active", prior_rev);
        let cur_snap = layer_snapshot(&name, source_id, "retired", next_rev);
        let prior_rev_wire = json!({ "layer_revision": prior_rev });
        let new_rev_wire = json!({ "layer_revision": next_rev });
        let change_id = self
            .append_catalog_evidence(CatalogEvidence {
                actor_display,
                resource_type: "layer",
                resource_id: layer_id,
                source_id: None,
                layer_id: Some(layer_id),
                operation: "retire_layer",
                prior_revision: Some(prior_rev_wire.clone()),
                new_revision: new_rev_wire.clone(),
                changes: diff_snapshots(&prior_snap, &cur_snap),
                now,
            })
            .await?;

        Ok(json!({
            "organization_id": self.tx.organization_id(),
            "project_id": self.project_id,
            "resource_type": "layer",
            "resource_id": layer_id,
            "change_id": change_id,
            "prior_revision": prior_rev_wire,
            "new_revision": new_rev_wire,
            "lifecycle": "retired",
        }))
    }

    pub async fn preview_geometry(
        &mut self,
        layer_id: Uuid,
        req: &GeometryPreviewInput,
    ) -> Result<Value, AppError> {
        self.require_unrestricted()?;
        if !self.edit_authorized {
            return Err(AppError::forbidden("missing permission: project.edit"));
        }
        let layer_row = sqlx::query(
            "SELECT l.lifecycle AS layer_lifecycle, l.source_id,
                    s.lifecycle AS source_lifecycle, s.validation_state, s.kind,
                    s.stored_crs, s.geometry_family, s.content_revision, s.schema_revision
             FROM spatial.layers l
             JOIN spatial.data_sources s
               ON s.organization_id = l.organization_id
              AND s.project_id = l.project_id
              AND s.id = l.source_id
             WHERE l.organization_id = $1 AND l.project_id = $2 AND l.id = $3
             FOR UPDATE",
        )
        .bind(self.tx.organization_id())
        .bind(self.project_id)
        .bind(layer_id)
        .fetch_optional(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;

        let Some(row) = layer_row else {
            return Err(detail_422("Layer is not editable in EPSG:4326"));
        };
        let l_life: String = row.try_get("layer_lifecycle").map_err(db_error)?;
        let s_life: String = row.try_get("source_lifecycle").map_err(db_error)?;
        let val_state: String = row.try_get("validation_state").map_err(db_error)?;
        let kind: String = row.try_get("kind").map_err(db_error)?;
        let stored_crs: Option<i32> = row.try_get("stored_crs").map_err(db_error)?;
        if l_life != "active"
            || s_life != "active"
            || val_state != "ready"
            || kind == "reference_fixture"
            || stored_crs != Some(4326)
        {
            return Err(detail_422("Layer is not editable in EPSG:4326"));
        }

        let source_id: Uuid = row.try_get("source_id").map_err(db_error)?;
        let content_rev: String = row.try_get("content_revision").map_err(db_error)?;
        let schema_rev: String = row.try_get("schema_revision").map_err(db_error)?;
        if (content_rev.as_str(), schema_rev.as_str())
            != (req.content_revision.as_str(), req.schema_revision.as_str())
        {
            return Err(detail_409("Source has changed; reload"));
        }

        let family: String = row.try_get("geometry_family").map_err(db_error)?;
        let unique_ids: HashSet<&str> = req.feature_ids.iter().map(String::as_str).collect();
        if !matches!(family.as_str(), "LineString" | "Polygon")
            || unique_ids.len() != req.feature_ids.len()
        {
            return Err(detail_422("Choose distinct line or polygon features"));
        }
        for fid in &req.feature_ids {
            if fid.is_empty()
                || fid.len() > 128
                || !fid
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
            {
                return Err(AppError::validation(
                    "INVALID_REQUEST",
                    "invalid feature identifier",
                ));
            }
        }

        let expression = if req.action == "split" {
            if req.feature_ids.len() != 1 || req.blade.len() < 2 {
                return Err(detail_422("Choose one feature and draw a cutting line"));
            }
            let blade_geojson = json!({
                "type": "LineString",
                "coordinates": req.blade,
            });
            validate_ready_geojson(&blade_geojson, Some("LineString"), 4326)
                .map_err(|_| AppError::validation("INVALID_REQUEST", "invalid blade geometry"))?;
            "ST_Split((array_agg(geometry))[1], ST_SetSRID(ST_GeomFromGeoJSON($5),4326))"
        } else {
            if req.feature_ids.len() < 2 || !req.blade.is_empty() {
                return Err(detail_422("Choose two or more adjacent features"));
            }
            if family == "LineString" {
                "ST_LineMerge(ST_UnaryUnion(ST_Collect(geometry)))"
            } else {
                "ST_UnaryUnion(ST_Collect(geometry))"
            }
        };

        let blade_json = serde_json::to_string(&json!({
            "type": "LineString",
            "coordinates": req.blade,
        }))
        .map_err(|e| AppError::internal(format!("blade json serialize failed: {e}")))?;

        let sql = format!(
            "WITH selected AS (
                 SELECT geometry FROM spatial.reference_features
                 WHERE organization_id = $1 AND project_id = $2
                   AND source_id = $3 AND feature_id = ANY($4) AND geometry_state = 'present'
             ),
             result AS (
                 SELECT {expression} AS g FROM selected HAVING count(*) = $6
             ),
             parts AS (
                 SELECT (ST_Dump(g)).geom AS g FROM result
             )
             SELECT ST_AsGeoJSON(g, 15) AS geo,
                    ST_IsValid(g) AND NOT ST_IsEmpty(g) AS valid
             FROM parts LIMIT 17"
        );

        let rows = sqlx::query(&sql)
            .bind(self.tx.organization_id())
            .bind(self.project_id)
            .bind(source_id)
            .bind(&req.feature_ids)
            .bind(&blade_json)
            .bind(req.feature_ids.len() as i64)
            .fetch_all(&mut **self.tx.tx_mut())
            .await
            .map_err(|_| detail_422("Geometry operation could not be computed"))?;

        if (req.action == "merge" && rows.len() != 1)
            || (req.action == "split" && !(2..=16).contains(&rows.len()))
        {
            return Err(detail_422(
                "Cut must cross the feature; merge must form a single connected geometry",
            ));
        }

        let mut geometries = Vec::with_capacity(rows.len());
        for record in rows {
            let valid: bool = record.try_get("valid").map_err(db_error)?;
            if !valid {
                return Err(detail_422("Result geometry is invalid"));
            }
            let geo_str: String = record.try_get("geo").map_err(db_error)?;
            let geo: Value = serde_json::from_str(&geo_str)
                .map_err(|e| AppError::internal(format!("postgis geojson parse failed: {e}")))?;
            if geo.get("type").and_then(Value::as_str) != Some(family.as_str()) {
                return Err(detail_422("Result geometry family is incompatible"));
            }
            validate_ready_geojson(&geo, Some(&family), 4326)
                .map_err(|_| detail_422("Result geometry is invalid"))?;
            geometries.push(geo);
        }

        Ok(json!({
            "source_id": source_id,
            "content_revision": content_rev,
            "geometries": geometries,
        }))
    }
}
