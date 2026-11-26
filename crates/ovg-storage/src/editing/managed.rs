//! R029 managed realtime GIS client (`managed_sync`): sessions, the durable
//! commit-ordered delta feed, commit receipts and the desktop command bridge.

use std::collections::BTreeSet;

use chrono::{DateTime, SubsecRound, TimeDelta, Utc};
use serde_json::{json, Map, Value};
use sqlx::postgres::PgRow;
use uuid::Uuid;

use super::{db_error, get, iso, CommitRequest, EditError, EditScope, Permission};

const SESSION_NOT_FOUND: &str = "Managed session not found.";
const ALLOWED_DESKTOP_TOOLS: [&str; 6] = [
    "inspect_project",
    "inspect_layer",
    "select_features",
    "zoom_to_extent",
    "preview_topology",
    "stage_edits",
];

/// Validated `CreateManagedSessionRequest`.
pub struct NewSession {
    pub binding_id: Option<Uuid>,
    pub device_label: String,
    pub client_version: String,
    pub mode: String,
    pub expires_in_hours: i64,
}

fn now() -> DateTime<Utc> {
    Utc::now().trunc_subsecs(6)
}

fn reset(reason: &str, last_cursor: i64, epoch: i64) -> Value {
    json!({
        "reset_required": true,
        "reason": reason,
        "next_cursor": last_cursor,
        "permission_epoch": epoch,
        "has_more": false,
        "changes": [],
    })
}

fn quiet(cursor: i64, epoch: i64) -> Value {
    json!({
        "reset_required": false,
        "next_cursor": cursor,
        "permission_epoch": epoch,
        "has_more": false,
        "changes": [],
    })
}

fn object(value: Option<Value>) -> Value {
    match value {
        Some(Value::Object(map)) => Value::Object(map),
        _ => Value::Object(Map::new()),
    }
}

fn geometry(text: Option<String>) -> Result<Value, EditError> {
    text.map_or(Ok(Value::Null), |text| {
        serde_json::from_str(&text).map_err(|e| EditError::Internal(e.to_string()))
    })
}

fn session_wire(row: &PgRow, current_cursor: Option<i64>) -> Result<Value, EditError> {
    let revoked_at: Option<DateTime<Utc>> = get(row, "revoked_at")?;
    let mut wire = json!({
        "id": get::<Uuid>(row, "id")?,
        "organization_id": get::<Uuid>(row, "organization_id")?,
        "project_id": get::<Uuid>(row, "project_id")?,
        "binding_id": get::<Option<Uuid>>(row, "binding_id")?,
        "actor_type": get::<String>(row, "actor_type")?,
        "actor_id": get::<Uuid>(row, "actor_id")?,
        "actor_display": get::<String>(row, "actor_display")?,
        "device_label": get::<String>(row, "device_label")?,
        "client_version": get::<String>(row, "client_version")?,
        "mode": get::<String>(row, "mode")?,
        "capabilities": object(get(row, "capabilities")?),
        "permission_epoch": get::<i64>(row, "permission_epoch")?,
        "last_ack_cursor": get::<i64>(row, "last_ack_cursor")?,
        "state": get::<String>(row, "state")?,
        "expires_at": iso(get(row, "expires_at")?),
        "revoked_at": revoked_at.map(iso),
        "last_seen_at": iso(get(row, "last_seen_at")?),
        "created_at": iso(get(row, "created_at")?),
    });
    if let Some(cursor) = current_cursor {
        wire["current_cursor"] = json!(cursor);
    }
    Ok(wire)
}

fn command_wire(row: &PgRow) -> Result<Value, EditError> {
    let result: Option<Value> = get(row, "result")?;
    let resolved_at: Option<DateTime<Utc>> = get(row, "resolved_at")?;
    Ok(json!({
        "id": get::<Uuid>(row, "id")?,
        "session_id": get::<Uuid>(row, "session_id")?,
        "tool_name": get::<String>(row, "tool_name")?,
        "arguments": object(get(row, "arguments")?),
        "requires_consent": get::<bool>(row, "requires_consent")?,
        "status": get::<String>(row, "status")?,
        "result": result.map(|value| object(Some(value))),
        "expires_at": iso(get(row, "expires_at")?),
        "created_at": iso(get(row, "created_at")?),
        "resolved_at": resolved_at.map(iso),
    }))
}

fn receipt_wire(row: &PgRow) -> Result<Value, EditError> {
    Ok(json!({
        "status": "accepted",
        "changeset_id": get::<Uuid>(row, "id")?,
        "operation_id": get::<String>(row, "operation_id")?,
        "source_id": get::<Uuid>(row, "source_id")?,
        "resulting_content_revision": get::<String>(row, "resulting_content_revision")?,
        "operation_count": get::<i32>(row, "operation_count")?,
        "event_cursor": get::<Option<i64>>(row, "cursor")?.unwrap_or(0),
        "audit_event_id": get::<Uuid>(row, "audit_event_id")?,
        "created_at": iso(get(row, "created_at")?),
    }))
}

impl EditScope<'_> {
    fn ids(&self) -> (Uuid, Uuid) {
        (self.tx.organization_id(), self.project_id)
    }

    /// `_ensure_counter`: (last_cursor, permission_epoch, retention_floor).
    pub async fn event_counter(&mut self) -> Result<(i64, i64, i64), EditError> {
        let (organization, project) = self.ids();
        let row = sqlx::query_as::<_, (i64, i64, i64)>(
            "SELECT last_cursor, permission_epoch, retention_floor
             FROM integration_hub.project_event_counters
             WHERE organization_id=$1 AND project_id=$2",
        )
        .bind(organization)
        .bind(project)
        .fetch_optional(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;
        Ok(row.unwrap_or((0, 1, 0)))
    }

    async fn audit_project(
        &mut self,
        action: &str,
        resource_type: &str,
        resource_id: Uuid,
        metadata: Value,
        at: DateTime<Utc>,
    ) -> Result<(), EditError> {
        let organization = self.tx.organization_id();
        sqlx::query(
            "INSERT INTO audit.audit_events (
                 id, organization_id, actor_type, actor_id, actor_display_snapshot,
                 action, resource_type, resource_id, outcome, metadata, created_at
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'success', $9, $10)",
        )
        .bind(Uuid::new_v4())
        .bind(organization)
        .bind(self.actor_type())
        .bind(self.actor_id)
        .bind(&self.display_name)
        .bind(action)
        .bind(resource_type)
        .bind(resource_id)
        .bind(metadata)
        .bind(at)
        .execute(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;
        Ok(())
    }

    async fn notify_project(&mut self, kind: &str) -> Result<(), EditError> {
        let (organization, project) = self.ids();
        let payload = json!({"o": organization, "p": project, "a": null, "b": null, "kind": kind});
        sqlx::query("SELECT pg_notify('ovg_team_collaboration', $1)")
            .bind(payload.to_string())
            .execute(&mut **self.tx.tx_mut())
            .await
            .map_err(db_error)?;
        Ok(())
    }

    async fn session_row(&mut self, session_id: Uuid) -> Result<PgRow, EditError> {
        let (organization, project) = self.ids();
        sqlx::query(
            "SELECT * FROM integration_hub.managed_sessions
             WHERE organization_id=$1 AND project_id=$2 AND id=$3",
        )
        .bind(organization)
        .bind(project)
        .bind(session_id)
        .fetch_optional(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?
        .ok_or_else(|| EditError::detail(404, SESSION_NOT_FOUND))
    }

    fn owns(&self, row: &PgRow) -> Result<bool, EditError> {
        Ok(get::<Uuid>(row, "actor_id")? == self.actor_id
            && get::<String>(row, "actor_type")? == self.actor_type())
    }

    /// `_load_active_session` with `_require_session_access`.
    pub async fn active_session(
        &mut self,
        session_id: Uuid,
        allow_manager: bool,
        require_current_epoch: bool,
    ) -> Result<PgRow, EditError> {
        let row = self.session_row(session_id).await?;
        // Do not disclose whether another actor's session is active or expired.
        if !self.owns(&row)? && !(allow_manager && self.granted(Permission::Manage).await?) {
            return Err(EditError::detail(404, SESSION_NOT_FOUND));
        }
        let state: String = get(&row, "state")?;
        let revoked_at: Option<DateTime<Utc>> = get(&row, "revoked_at")?;
        if state != "active" || revoked_at.is_some() {
            return Err(EditError::detail(
                403,
                "Managed session is revoked or inactive.",
            ));
        }
        if get::<DateTime<Utc>>(&row, "expires_at")? <= Utc::now() {
            return Err(EditError::detail(401, "Managed session has expired."));
        }
        if require_current_epoch
            && get::<i64>(&row, "permission_epoch")? != self.event_counter().await?.1
        {
            return Err(EditError::detail(
                403,
                "Managed session permission epoch changed; reconnect.",
            ));
        }
        Ok(row)
    }

    pub async fn create_managed_session(&mut self, new: NewSession) -> Result<Value, EditError> {
        self.authorize(Permission::Read).await?;
        let (organization, project) = self.ids();
        if let Some(binding_id) = new.binding_id {
            let service = self.credential_id;
            if let Some(credential) = service {
                // Only the verified request identity may open a pairing view.
                sqlx::query(
                    "SELECT set_config('openvietgis.qgis_pairing_principal', $1, true),
                            set_config('openvietgis.qgis_pairing_credential', $2, true)",
                )
                .bind(self.actor_id.to_string())
                .bind(credential.to_string())
                .execute(&mut **self.tx.tx_mut())
                .await
                .map_err(db_error)?;
            }
            let state: Option<String> = sqlx::query_scalar(
                "SELECT state FROM integration_hub.connector_bindings
                 WHERE organization_id=$1 AND project_id=$2 AND id=$3",
            )
            .bind(organization)
            .bind(project)
            .bind(binding_id)
            .fetch_optional(&mut **self.tx.tx_mut())
            .await
            .map_err(db_error)?;
            if service.is_some() {
                sqlx::query(
                    "SELECT set_config('openvietgis.qgis_pairing_principal', '', true),
                            set_config('openvietgis.qgis_pairing_credential', '', true)",
                )
                .execute(&mut **self.tx.tx_mut())
                .await
                .map_err(db_error)?;
            }
            if state.as_deref() != Some("active") {
                return Err(EditError::detail(
                    403,
                    "Managed binding is not active or authorized.",
                ));
            }
        }

        let can_edit = self.granted(Permission::Edit).await?;
        let (last_cursor, epoch, _) = self.event_counter().await?;
        let at = now();
        let id = Uuid::new_v4();
        let capabilities = json!({
            "can_read": true,
            "can_edit": can_edit,
            "can_delete": false,
            "realtime_delta": true,
            "ai_desktop_bridge": true,
        });
        let display = if self.display_name.is_empty() {
            self.actor_id.to_string()
        } else {
            self.display_name.chars().take(200).collect()
        };
        let row = sqlx::query(
            "INSERT INTO integration_hub.managed_sessions (
                 id, organization_id, project_id, binding_id, actor_type, actor_id,
                 actor_display, device_label, client_version, mode, capabilities,
                 permission_epoch, last_ack_cursor, state, expires_at, last_seen_at, created_at
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, 'active',
                       $14, $15, $15)
             RETURNING *",
        )
        .bind(id)
        .bind(organization)
        .bind(project)
        .bind(new.binding_id)
        .bind(self.actor_type())
        .bind(self.actor_id)
        .bind(display)
        .bind(&new.device_label)
        .bind(&new.client_version)
        .bind(&new.mode)
        .bind(capabilities)
        .bind(epoch)
        .bind(last_cursor)
        .bind(at + TimeDelta::hours(new.expires_in_hours))
        .bind(at)
        .fetch_one(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;
        let metadata = json!({
            "device_label": new.device_label,
            "client_version": new.client_version,
            "can_edit": can_edit,
        });
        self.audit_project(
            "integration.managed_session.created",
            "managed_session",
            id,
            metadata,
            at,
        )
        .await?;
        session_wire(&row, Some(last_cursor))
    }

    pub async fn list_managed_sessions(&mut self) -> Result<Value, EditError> {
        self.authorize(Permission::Read).await?;
        let (organization, project) = self.ids();
        let (last_cursor, epoch, floor) = self.event_counter().await?;
        let manager = self.granted(Permission::Manage).await?;
        let rows = sqlx::query(
            "SELECT * FROM integration_hub.managed_sessions
             WHERE organization_id=$1 AND project_id=$2
               AND ($3 OR (actor_id=$4 AND actor_type=$5))
             ORDER BY created_at DESC LIMIT 100",
        )
        .bind(organization)
        .bind(project)
        .bind(manager)
        .bind(self.actor_id)
        .bind(self.actor_type())
        .fetch_all(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;
        let items = rows
            .iter()
            .map(|row| session_wire(row, None))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(json!({
            "current_cursor": last_cursor,
            "permission_epoch": epoch,
            "retention_floor": floor,
            "items": items,
        }))
    }

    pub async fn get_managed_session(&mut self, session_id: Uuid) -> Result<Value, EditError> {
        self.authorize(Permission::Read).await?;
        let row = self.session_row(session_id).await?;
        if !self.owns(&row)? && !self.granted(Permission::Manage).await? {
            return Err(EditError::detail(404, SESSION_NOT_FOUND));
        }
        let (last_cursor, _, _) = self.event_counter().await?;
        session_wire(&row, Some(last_cursor))
    }

    pub async fn revoke_managed_session(
        &mut self,
        session_id: Uuid,
        bump_epoch: bool,
    ) -> Result<Value, EditError> {
        self.authorize(Permission::Read).await?;
        let (organization, project) = self.ids();
        let row = self.session_row(session_id).await?;
        if !self.owns(&row)? || bump_epoch {
            if !self.granted(Permission::Manage).await? {
                return Err(EditError::detail(403, "Project manage permission required."));
            }
            self.authorize(Permission::Manage).await?;
        }
        let at = now();
        let updated = sqlx::query(
            "UPDATE integration_hub.managed_sessions
             SET state='revoked', revoked_at=$4, last_seen_at=$4
             WHERE organization_id=$1 AND project_id=$2 AND id=$3
             RETURNING *",
        )
        .bind(organization)
        .bind(project)
        .bind(session_id)
        .bind(at)
        .fetch_one(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;
        if bump_epoch {
            sqlx::query(
                "INSERT INTO integration_hub.project_event_counters
                     (organization_id, project_id, last_cursor, permission_epoch,
                      retention_floor, updated_at)
                 VALUES ($1, $2, 0, 2, 0, $3)
                 ON CONFLICT (organization_id, project_id) DO UPDATE SET
                     permission_epoch = integration_hub.project_event_counters.permission_epoch + 1,
                     updated_at = $3",
            )
            .bind(organization)
            .bind(project)
            .bind(at)
            .execute(&mut **self.tx.tx_mut())
            .await
            .map_err(db_error)?;
        }
        self.notify_project("session_revoked").await?;
        let metadata = json!({ "bump_epoch": bump_epoch });
        self.audit_project(
            "integration.managed_session.revoked",
            "managed_session",
            session_id,
            metadata,
            at,
        )
        .await?;
        session_wire(&updated, None)
    }

    /// ACL-filtered layer page pinned to one cursor, epoch and source revision.
    pub async fn managed_bootstrap(
        &mut self,
        layer_id: Uuid,
        session_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Value, EditError> {
        self.authorize(Permission::Read).await?;
        self.active_session(session_id, false, true).await?;
        let (organization, project) = self.ids();
        let layer = sqlx::query(
            "SELECT l.id, l.name, l.source_id, s.geometry_family, s.dimensions,
                    s.stored_crs, s.content_revision, s.schema_revision, s.feature_count
             FROM spatial.layers l
             JOIN spatial.data_sources s ON s.id=l.source_id
                  AND s.organization_id=l.organization_id AND s.project_id=l.project_id
             WHERE l.organization_id=$1 AND l.project_id=$2 AND l.id=$3
               AND l.lifecycle='active' AND s.lifecycle='active'",
        )
        .bind(organization)
        .bind(project)
        .bind(layer_id)
        .fetch_optional(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?
        .ok_or_else(|| EditError::detail(404, "Layer not found."))?;
        let source_id: Uuid = get(&layer, "source_id")?;
        if self.scoped && !self.source_readable(source_id).await? {
            return Err(EditError::detail(
                403,
                "Layer is outside your authorized GIS scope.",
            ));
        }
        let (last_cursor, epoch, _) = self.event_counter().await?;
        let fields = sqlx::query_as::<_, (String, String, String, bool)>(
            "SELECT field_id, name, type, nullable FROM spatial.source_fields
             WHERE organization_id=$1 AND project_id=$2 AND source_id=$3
             ORDER BY ordinal ASC",
        )
        .bind(organization)
        .bind(project)
        .bind(source_id)
        .fetch_all(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;
        let rows = sqlx::query(
            "SELECT f.feature_id, ST_AsGeoJSON(f.geometry, 15, 0) AS geom_json,
                    f.attributes AS properties, f.content_revision
             FROM spatial.reference_features f
             WHERE f.organization_id=$1 AND f.project_id=$2 AND f.source_id=$3
             ORDER BY f.feature_id ASC LIMIT $4 OFFSET $5",
        )
        .bind(organization)
        .bind(project)
        .bind(source_id)
        .bind(limit + 1)
        .bind(offset)
        .fetch_all(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;
        let pin = sqlx::query_as::<_, (String, String)>(
            "SELECT content_revision, schema_revision FROM spatial.data_sources
             WHERE organization_id=$1 AND project_id=$2 AND id=$3 AND lifecycle='active'",
        )
        .bind(organization)
        .bind(project)
        .bind(source_id)
        .fetch_optional(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;
        let content_revision: String = get(&layer, "content_revision")?;
        let schema_revision: String = get(&layer, "schema_revision")?;
        let (cursor_now, epoch_now, _) = self.event_counter().await?;
        if pin != Some((content_revision.clone(), schema_revision.clone()))
            || (cursor_now, epoch_now) != (last_cursor, epoch)
        {
            return Err(EditError::detail(
                409,
                "Snapshot changed during read; reopen layer.",
            ));
        }
        let has_more = rows.len() as i64 > limit;
        let mut features = Vec::new();
        for row in rows.iter().take(limit as usize) {
            features.push(json!({
                "feature_id": get::<String>(row, "feature_id")?,
                "geometry": geometry(get(row, "geom_json")?)?,
                "properties": object(get(row, "properties")?),
                "content_revision": get::<String>(row, "content_revision")?,
            }));
        }
        let feature_count: Option<i32> = get(&layer, "feature_count")?;
        let feature_count = feature_count.filter(|_| !self.scoped);
        let stored_crs: Option<i32> = get(&layer, "stored_crs")?;
        let fields: Vec<Value> = fields
            .into_iter()
            .map(|(name, display_name, data_type, nullable)| {
                json!({
                    "name": name,
                    "display_name": display_name,
                    "data_type": data_type,
                    "nullable": nullable,
                })
            })
            .collect();
        let delivered = features.len() as i64;
        Ok(json!({
            "layer_id": get::<Uuid>(&layer, "id")?,
            "source_id": source_id,
            "name": get::<String>(&layer, "name")?,
            "geometry_family": get::<String>(&layer, "geometry_family")?,
            "dimensions": get::<String>(&layer, "dimensions")?,
            "crs_code": stored_crs.filter(|code| *code != 0),
            "content_revision": content_revision,
            "schema_revision": schema_revision,
            "cursor": last_cursor,
            "permission_epoch": epoch,
            "cacheable": feature_count.is_some(),
            "feature_count": feature_count,
            "fields": fields,
            "features": features,
            "has_more": has_more,
            "next_offset": has_more.then_some(offset + delivered),
        }))
    }

    async fn source_readable(&mut self, source_id: Uuid) -> Result<bool, EditError> {
        let (organization, project) = self.ids();
        sqlx::query_scalar("SELECT gis_access.source_allowed($1, $2, $3, 'read')")
            .bind(organization)
            .bind(project)
            .bind(source_id)
            .fetch_one(&mut **self.tx.tx_mut())
            .await
            .map_err(db_error)
    }

    async fn acknowledge(&mut self, session_id: Uuid, cursor: i64) -> Result<(), EditError> {
        let (organization, project) = self.ids();
        sqlx::query(
            "UPDATE integration_hub.managed_sessions
             SET last_ack_cursor=GREATEST(last_ack_cursor, $4), last_seen_at=now()
             WHERE organization_id=$1 AND project_id=$2 AND id=$3",
        )
        .bind(organization)
        .bind(project)
        .bind(session_id)
        .bind(cursor)
        .execute(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;
        Ok(())
    }

    /// Commit-ordered authorized deltas after `cursor` (`fetch_managed_delta`).
    pub async fn managed_delta(
        &mut self,
        session_id: Uuid,
        cursor: i64,
        limit: i64,
    ) -> Result<Value, EditError> {
        self.authorize(Permission::Read).await?;
        let session = self.active_session(session_id, false, false).await?;
        let (organization, project) = self.ids();
        let (last_cursor, epoch, floor) = self.event_counter().await?;
        let last_ack: i64 = get(&session, "last_ack_cursor")?;
        if get::<i64>(&session, "permission_epoch")? != epoch
            || cursor < floor
            || cursor > last_cursor
        {
            return Ok(reset(
                "permission_epoch_or_retention_changed",
                last_cursor,
                epoch,
            ));
        }
        // Quiet poll: acknowledge only the caller's cursor, never the server's.
        if cursor == last_cursor {
            if cursor > last_ack {
                self.acknowledge(session_id, cursor).await?;
            }
            return Ok(quiet(cursor, epoch));
        }
        let events = sqlx::query(
            "SELECT cursor, kind, source_id, changeset_id, content_revision, committed_at
             FROM integration_hub.project_events
             WHERE organization_id=$1 AND project_id=$2 AND cursor > $3 AND cursor <= $4
             ORDER BY cursor ASC LIMIT $5",
        )
        .bind(organization)
        .bind(project)
        .bind(cursor)
        .bind(last_cursor)
        .bind(limit + 1)
        .fetch_all(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;
        let has_more = events.len() as i64 > limit;
        let page = &events[..events.len().min(limit as usize)];
        let Some(last) = page.last() else {
            if cursor > last_ack {
                self.acknowledge(session_id, cursor).await?;
            }
            return Ok(quiet(cursor, epoch));
        };
        let scanned_through: i64 = get(last, "cursor")?;

        // Groups in first-seen order: (source, events).
        let mut groups: Vec<(Uuid, Vec<&PgRow>)> = Vec::new();
        for event in page {
            let kind: String = get(event, "kind")?;
            let changeset: Option<Uuid> = get(event, "changeset_id")?;
            let source: Option<Uuid> = get(event, "source_id")?;
            let (Some(source), Some(_), "changeset") = (source, changeset, kind.as_str()) else {
                return Ok(reset(&kind, last_cursor, epoch));
            };
            if self.scoped {
                if !self.source_readable(source).await? {
                    continue;
                }
                return Ok(reset("scoped_snapshot_required", last_cursor, epoch));
            }
            match groups.iter().position(|(id, _)| *id == source) {
                Some(index) => groups[index].1.push(event),
                None => groups.push((source, vec![event])),
            }
        }
        let mut keyed = Vec::with_capacity(groups.len());
        for (source, group) in groups {
            let event = group[group.len() - 1];
            keyed.push((get::<i64>(event, "cursor")?, source, group));
        }
        // Python's sort is stable; group-final cursors are unique anyway.
        keyed.sort_by_key(|(last, _, _)| *last);

        let mut changes = Vec::new();
        let mut delivered: usize = 0;
        for (_, source, group) in keyed {
            let event = group[group.len() - 1];
            let layers = sqlx::query_as::<_, (Uuid, String)>(
                "SELECT l.id, s.content_revision
                 FROM spatial.layers l JOIN spatial.data_sources s
                   ON s.id=l.source_id AND s.organization_id=l.organization_id
                  AND s.project_id=l.project_id
                 WHERE l.organization_id=$1 AND l.project_id=$2 AND l.source_id=$3
                   AND l.lifecycle='active' AND s.lifecycle='active'
                 ORDER BY l.id LIMIT 17",
            )
            .bind(organization)
            .bind(project)
            .bind(source)
            .fetch_all(&mut **self.tx.tx_mut())
            .await
            .map_err(db_error)?;
            let Some((_, layer_revision)) = layers.first().cloned() else {
                continue;
            };
            let event_revision: Option<String> = get(event, "content_revision")?;
            if layers.len() > 16 || event_revision.as_deref() != Some(layer_revision.as_str()) {
                return Ok(reset(
                    "source_revision_or_alias_limit",
                    last_cursor,
                    epoch,
                ));
            }
            let changesets = group
                .iter()
                .map(|row| get::<Uuid>(row, "changeset_id"))
                .collect::<Result<Vec<_>, _>>()?;
            let operations = sqlx::query_as::<_, (String, String)>(
                "SELECT DISTINCT feature_id, op_type FROM editing.changeset_operations
                 WHERE organization_id=$1 AND project_id=$2 AND changeset_id = ANY($3)
                 LIMIT 5001",
            )
            .bind(organization)
            .bind(project)
            .bind(&changesets)
            .fetch_all(&mut **self.tx.tx_mut())
            .await
            .map_err(db_error)?;
            if operations.len() > 5000 {
                return Ok(reset("delta_operation_budget", last_cursor, epoch));
            }
            let touched: BTreeSet<String> = operations.into_iter().map(|(id, _)| id).collect();
            if touched.is_empty() {
                continue;
            }
            let touched: Vec<String> = touched.into_iter().collect();
            let live = sqlx::query(
                "SELECT f.feature_id, ST_AsGeoJSON(f.geometry, 15, 0) AS geom_json,
                        f.attributes AS properties, f.content_revision
                 FROM spatial.reference_features f
                 WHERE f.organization_id=$1 AND f.project_id=$2 AND f.source_id=$3
                   AND f.feature_id = ANY($4)",
            )
            .bind(organization)
            .bind(project)
            .bind(source)
            .bind(&touched)
            .fetch_all(&mut **self.tx.tx_mut())
            .await
            .map_err(db_error)?;
            let mut upserts = Vec::with_capacity(live.len());
            let mut present = BTreeSet::new();
            for row in &live {
                let feature_id: String = get(row, "feature_id")?;
                present.insert(feature_id.clone());
                upserts.push(json!({
                    "feature_id": feature_id,
                    "geometry": geometry(get(row, "geom_json")?)?,
                    "properties": object(get(row, "properties")?),
                    "content_revision": get::<String>(row, "content_revision")?,
                }));
            }
            let removals: Vec<&String> =
                touched.iter().filter(|id| !present.contains(*id)).collect();
            delivered += touched.len() * layers.len();
            if delivered > 5000 {
                return Ok(reset("delta_delivery_budget", last_cursor, epoch));
            }
            let committed_at: DateTime<Utc> = get(event, "committed_at")?;
            for (layer_id, revision) in &layers {
                changes.push(json!({
                    "cursor": get::<i64>(event, "cursor")?,
                    "layer_id": layer_id,
                    "source_id": source,
                    "changeset_id": get::<Uuid>(event, "changeset_id")?,
                    "coalesced_changesets": group.len(),
                    "content_revision": event_revision.clone().filter(|r| !r.is_empty())
                        .unwrap_or_else(|| revision.clone()),
                    "committed_at": iso(committed_at),
                    "upserts": upserts,
                    "removals": removals,
                }));
            }
        }
        let (cursor_now, epoch_now, _) = self.event_counter().await?;
        if (cursor_now, epoch_now) != (last_cursor, epoch) {
            return Ok(reset("concurrent_project_change", last_cursor, epoch));
        }
        self.acknowledge(session_id, cursor).await?;
        Ok(json!({
            "reset_required": false,
            "next_cursor": scanned_through,
            "permission_epoch": epoch,
            "has_more": has_more,
            "changes": changes,
        }))
    }

    /// Server-authoritative R006 commit returning a durable receipt and event cursor.
    pub async fn managed_commit(
        &mut self,
        session_id: Uuid,
        source_id: Uuid,
        allow_deletes: bool,
        request: CommitRequest,
    ) -> Result<Value, EditError> {
        self.authorize(Permission::Edit).await?;
        let session = self.active_session(session_id, false, true).await?;
        let capabilities = object(get(&session, "capabilities")?);
        if capabilities.get("can_edit") != Some(&Value::Bool(true)) {
            return Err(EditError::detail(
                403,
                "Managed session does not have edit capability.",
            ));
        }
        let deletes = request
            .operations
            .iter()
            .any(|op| op.op_type.as_str() == "delete");
        if deletes && !allow_deletes {
            return Err(EditError::detail(
                403,
                "Deletions are blocked by default; pass allow_deletes=true with explicit approval.",
            ));
        }
        if request.operation_id.starts_with("field-sync-") {
            return Err(EditError::detail(422, "Reserved internal operation_id prefix."));
        }
        let changeset = self.commit_changeset(source_id, request).await?;
        let (organization, project) = self.ids();
        let changeset_id = changeset["id"]
            .as_str()
            .and_then(|id| Uuid::parse_str(id).ok())
            .ok_or_else(|| EditError::Internal("changeset without id".into()))?;
        let cursor: Option<i64> = sqlx::query_scalar(
            "SELECT cursor FROM integration_hub.project_events
             WHERE organization_id=$1 AND project_id=$2 AND changeset_id=$3
             ORDER BY cursor DESC LIMIT 1",
        )
        .bind(organization)
        .bind(project)
        .bind(changeset_id)
        .fetch_optional(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;
        Ok(json!({
            "status": "accepted",
            "changeset_id": changeset["id"],
            "operation_id": changeset["operation_id"],
            "source_id": changeset["source_id"],
            "resulting_content_revision": changeset["resulting_content_revision"],
            "operation_count": changeset["operation_count"],
            "event_cursor": cursor.unwrap_or(0),
            "audit_event_id": changeset["audit_event_id"],
            "created_at": changeset["created_at"],
        }))
    }

    /// Recover the durable receipt of this actor's commit by operation ID.
    pub async fn managed_receipt(&mut self, operation_id: &str) -> Result<Value, EditError> {
        self.authorize(Permission::Read).await?;
        let (organization, project) = self.ids();
        let row = sqlx::query(
            "SELECT c.id, c.source_id, c.operation_id, c.resulting_content_revision,
                    c.operation_count, c.audit_event_id, c.created_at, e.cursor
             FROM editing.changesets c
             LEFT JOIN integration_hub.project_events e
               ON e.organization_id=c.organization_id AND e.project_id=c.project_id
              AND e.changeset_id=c.id
             WHERE c.organization_id=$1 AND c.project_id=$2
               AND c.operation_id=$3 AND c.actor_id=$4 AND c.actor_type=$5
             ORDER BY c.created_at DESC LIMIT 1",
        )
        .bind(organization)
        .bind(project)
        .bind(super::domain::py_strip(operation_id))
        .bind(self.actor_id)
        .bind(self.actor_type())
        .fetch_optional(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?
        .ok_or_else(|| {
            EditError::detail(404, "Commit receipt not found for this operation_id.")
        })?;
        receipt_wire(&row)
    }

    /// Enqueue an allowlisted typed command for an active opted-in QGIS session.
    pub async fn enqueue_desktop_command(
        &mut self,
        session_id: Uuid,
        tool_name: &str,
        arguments: Value,
        ttl_seconds: i64,
    ) -> Result<Value, EditError> {
        self.authorize(Permission::Read).await?;
        if !ALLOWED_DESKTOP_TOOLS.contains(&tool_name) {
            return Err(EditError::detail(
                403,
                format!("Tool '{tool_name}' is not in the R029 desktop allowlist."),
            ));
        }
        if tool_name == "stage_edits" {
            self.authorize(Permission::Edit).await?;
        }
        self.active_session(session_id, true, true).await?;
        let (organization, project) = self.ids();
        let at = now();
        let id = Uuid::new_v4();
        let row = sqlx::query(
            "INSERT INTO integration_hub.desktop_commands (
                 id, organization_id, project_id, session_id, tool_name, arguments,
                 requires_consent, status, requested_by, expires_at, created_at
             ) VALUES ($1, $2, $3, $4, $5, $6, true, 'pending', $7, $8, $9)
             RETURNING *",
        )
        .bind(id)
        .bind(organization)
        .bind(project)
        .bind(session_id)
        .bind(tool_name)
        .bind(arguments)
        .bind(self.actor_id)
        .bind(at + TimeDelta::seconds(ttl_seconds))
        .bind(at)
        .fetch_one(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;
        self.notify_project("desktop_command").await?;
        let metadata = json!({ "session_id": session_id, "tool_name": tool_name });
        self.audit_project(
            "integration.desktop_command.enqueued",
            "desktop_command",
            id,
            metadata,
            at,
        )
        .await?;
        command_wire(&row)
    }

    pub async fn list_desktop_commands(
        &mut self,
        session_id: Uuid,
        status: Option<&str>,
    ) -> Result<Value, EditError> {
        self.authorize(Permission::Read).await?;
        self.active_session(session_id, false, true).await?;
        let (organization, project) = self.ids();
        let rows = sqlx::query(
            "SELECT * FROM integration_hub.desktop_commands
             WHERE organization_id=$1 AND project_id=$2 AND session_id=$3
               AND ($4::text IS NULL OR status=$4)
             ORDER BY created_at DESC LIMIT 50",
        )
        .bind(organization)
        .bind(project)
        .bind(session_id)
        .bind(status)
        .fetch_all(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;
        let items = rows
            .iter()
            .map(command_wire)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(json!({ "items": items }))
    }

    pub async fn respond_desktop_command(
        &mut self,
        session_id: Uuid,
        command_id: Uuid,
        status: &str,
        result: Option<Value>,
    ) -> Result<Value, EditError> {
        self.authorize(Permission::Read).await?;
        self.active_session(session_id, false, true).await?;
        let (organization, project) = self.ids();
        let row = sqlx::query(
            "UPDATE integration_hub.desktop_commands
             SET status=$5, result=$6, resolved_at=$7
             WHERE organization_id=$1 AND project_id=$2 AND session_id=$3 AND id=$4
               AND status IN ('pending', 'approved') AND expires_at > $7
             RETURNING *",
        )
        .bind(organization)
        .bind(project)
        .bind(session_id)
        .bind(command_id)
        .bind(status)
        .bind(result)
        .bind(now())
        .fetch_optional(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?
        .ok_or_else(|| {
            EditError::detail(409, "Command not found, already resolved, or expired.")
        })?;
        command_wire(&row)
    }
}
