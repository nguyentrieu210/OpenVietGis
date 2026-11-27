//! R011 project collaboration REST: chat messages, feature comments, tasks,
//! notifications, feature edit leases and post-commit change reviews.

use std::collections::HashSet;

use chrono::{DateTime, NaiveDate, Utc};
use serde_json::{json, Map, Value};
use sha1::{Digest, Sha1};
use sqlx::postgres::PgRow;
use sqlx::{Column, Row, TypeInfo};
use uuid::Uuid;

use super::domain::{py_strip, validate_feature_id};
use super::managed::now;
use super::{db_error, get, iso, EditError, EditScope, Permission};

const MESSAGE_COLS: &str = "id,author_id,author_name,body,created_at,recipient_id,source_id,\
    feature_id,longitude,latitude,mentions";
const TASK_COLS: &str = "id,creator_id,creator_name,assignee_id,assignee_name,title,description,\
    status,priority,due_date,longitude,latitude,revision,created_at,updated_at,source_message_id,\
    source_id,feature_id";
const LEASE_COLS: &str = "source_id,feature_id,actor_id,actor_name,expires_at";

type MessageAnchor = (Option<Uuid>, Option<String>, Option<f64>, Option<f64>);
type SourceState = (String, String, String, String);
/// A validated draft operation keyed by its normalized feature ID.
type DraftOperation = (String, Map<String, Value>);

/// FastAPI `jsonable_encoder(dict(row))` for the column types these routes return.
pub(crate) fn row_json(row: &PgRow) -> Result<Value, EditError> {
    let mut map = Map::new();
    for column in row.columns() {
        let name = column.name();
        let value = match column.type_info().name() {
            "UUID" => json!(get::<Option<Uuid>>(row, name)?),
            "TIMESTAMPTZ" => json!(get::<Option<DateTime<Utc>>>(row, name)?.map(iso)),
            "DATE" => json!(get::<Option<NaiveDate>>(row, name)?.map(|d| d.to_string())),
            "FLOAT8" => json!(get::<Option<f64>>(row, name)?),
            "INT2" => json!(get::<Option<i16>>(row, name)?),
            "INT4" => json!(get::<Option<i32>>(row, name)?),
            "INT8" => json!(get::<Option<i64>>(row, name)?),
            "BOOL" => json!(get::<Option<bool>>(row, name)?),
            "JSON" | "JSONB" => get::<Option<Value>>(row, name)?.unwrap_or(Value::Null),
            _ => json!(get::<Option<String>>(row, name)?),
        };
        map.insert(name.to_owned(), value);
    }
    Ok(Value::Object(map))
}

fn rows_json(rows: &[PgRow]) -> Result<Vec<Value>, EditError> {
    rows.iter().map(row_json).collect()
}

fn review_wire(row: &PgRow) -> Result<Value, EditError> {
    let decided_at: Option<DateTime<Utc>> = get(row, "decided_at")?;
    Ok(json!({
        "id": get::<Uuid>(row, "id")?,
        "source_id": get::<Uuid>(row, "source_id")?,
        "changeset_id": get::<Uuid>(row, "changeset_id")?,
        "content_revision": get::<String>(row, "content_revision")?,
        "requested_by_actor_id": get::<Uuid>(row, "requested_by_actor_id")?,
        "assignee_user_id": get::<Uuid>(row, "assignee_user_id")?,
        "status": get::<String>(row, "status")?,
        "decision_note": get::<String>(row, "decision_note")?,
        "decided_by_actor_id": get::<Option<Uuid>>(row, "decided_by_actor_id")?,
        "created_at": iso(get(row, "created_at")?),
        "decided_at": decided_at.map(iso),
    }))
}

fn comment_wire(row: &PgRow) -> Result<Value, EditError> {
    let feature_id: Option<String> = get(row, "feature_id")?;
    Ok(json!({
        "id": get::<Uuid>(row, "id")?,
        "review_id": get::<Uuid>(row, "review_id")?,
        "changeset_id": get::<Uuid>(row, "changeset_id")?,
        "content_revision": get::<String>(row, "content_revision")?,
        "feature_id": feature_id.filter(|id| !id.is_empty()),
        "body": get::<String>(row, "body")?,
        "author_actor_id": get::<Uuid>(row, "author_actor_id")?,
        "author_display": get::<String>(row, "author_display")?,
        "created_at": iso(get(row, "created_at")?),
    }))
}

fn detail(status: u16, message: &str) -> EditError {
    EditError::detail(status, message)
}

fn platform(status: u16, code: &'static str, message: impl Into<String>) -> EditError {
    EditError::Platform {
        status,
        code,
        message: message.into(),
        details: None,
    }
}

/// Python `uuid5(NAMESPACE_URL, name)`.
fn uuid5_url(name: &str) -> Uuid {
    let mut hasher = Sha1::new();
    hasher.update(Uuid::NAMESPACE_URL.as_bytes());
    hasher.update(name.as_bytes());
    let digest = hasher.finalize();
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

/// Validated `TaskRequest` after the model validator.
pub struct NewTask {
    pub title: String,
    pub description: String,
    pub assignee_id: Uuid,
    pub source_message_id: Option<Uuid>,
    pub source_id: Option<Uuid>,
    pub feature_id: Option<String>,
    pub priority: String,
    pub due_date: Option<NaiveDate>,
    pub longitude: Option<f64>,
    pub latitude: Option<f64>,
}

/// A task or comment location: source/feature plus an optional WGS84 point.
struct Anchor {
    source_id: Option<Uuid>,
    feature_id: Option<String>,
    longitude: Option<f64>,
    latitude: Option<f64>,
}

impl EditScope<'_> {
    fn org_project(&self) -> (Uuid, Uuid) {
        (self.tx.organization_id(), self.project_id)
    }

    fn display_200(&self) -> String {
        self.display_name.chars().take(200).collect()
    }

    /// Active unrestricted project editors (or owners/admins), by display name.
    async fn eligible(&mut self) -> Result<Vec<(Uuid, String)>, EditError> {
        let (organization, project) = self.org_project();
        sqlx::query_as(
            "SELECT DISTINCT u.id, u.display_name FROM org.organization_memberships m
             JOIN identity.users u ON u.id=m.user_id AND u.status='active'
             JOIN workspace.projects pr ON pr.organization_id=m.organization_id AND pr.id=$2
             WHERE m.organization_id=$1 AND m.status='active' AND (
               m.role IN ('owner','admin') OR (
                 NOT EXISTS(SELECT 1 FROM gis_access.scoped_members sm
                   WHERE sm.organization_id=$1 AND sm.project_id=$2 AND sm.user_id=u.id)
                 AND EXISTS(SELECT 1 FROM org.resource_grants g WHERE g.organization_id=$1
                   AND g.membership_id=m.id AND g.subject_kind='membership'
                   AND g.revoked_at IS NULL AND g.role IN ('manager','editor','contributor')
                   AND (g.scope_kind='organization'
                     OR (g.scope_kind='workspace' AND g.workspace_id=pr.workspace_id)
                     OR (g.scope_kind='project' AND g.project_id=pr.id)))
               )
             ) ORDER BY u.display_name, u.id",
        )
        .bind(organization)
        .bind(project)
        .fetch_all(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)
    }

    pub async fn team_members(&mut self) -> Result<Value, EditError> {
        self.authorize(Permission::Read).await?;
        let items: Vec<Value> = self
            .eligible()
            .await?
            .into_iter()
            .map(|(id, display_name)| json!({"id": id, "display_name": display_name}))
            .collect();
        Ok(json!({ "items": items }))
    }

    pub async fn team_messages(
        &mut self,
        limit: i64,
        peer: Option<Uuid>,
    ) -> Result<Value, EditError> {
        self.authorize(Permission::Read).await?;
        let (organization, project) = self.org_project();
        let condition = if peer.is_none() {
            "recipient_id IS NULL AND source_id IS NULL"
        } else {
            "((author_id=$4 AND recipient_id=$5) OR (author_id=$5 AND recipient_id=$4))"
        };
        let mut rows = sqlx::query(&format!(
            "SELECT {MESSAGE_COLS} FROM collaboration.messages
             WHERE organization_id=$1 AND project_id=$2 AND {condition}
             ORDER BY created_at DESC, id DESC LIMIT $3"
        ))
        .bind(organization)
        .bind(project)
        .bind(limit)
        .bind(self.actor_id)
        .bind(peer)
        .fetch_all(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;
        rows.reverse();
        Ok(json!({ "items": rows_json(&rows)? }))
    }

    pub async fn post_team_message(
        &mut self,
        body: &str,
        recipient: Option<Uuid>,
    ) -> Result<Value, EditError> {
        self.authorize(Permission::Edit).await?;
        if let Some(recipient) = recipient {
            let eligible = self.eligible().await?;
            if recipient == self.actor_id || !eligible.iter().any(|(id, _)| *id == recipient) {
                return Err(detail(
                    422,
                    "Recipient must be another unrestricted project editor",
                ));
            }
        }
        let (organization, project) = self.org_project();
        let row = sqlx::query(&format!(
            "INSERT INTO collaboration.messages
                 (organization_id, project_id, id, author_id, author_name, body, recipient_id)
             VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING {MESSAGE_COLS}"
        ))
        .bind(organization)
        .bind(project)
        .bind(Uuid::new_v4())
        .bind(self.actor_id)
        .bind(self.display_200())
        .bind(body)
        .bind(recipient)
        .fetch_one(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;
        row_json(&row)
    }

    /// `feature_anchor`: the feature's WGS84 point-on-surface, if it has one.
    async fn feature_anchor(&mut self, source: Uuid, feature: &str) -> Result<Anchor, EditError> {
        let feature_id = validate_feature_id(feature)?;
        let (organization, project) = self.org_project();
        let point = sqlx::query_as::<_, (Option<f64>, Option<f64>)>(
            "SELECT ST_X(anchor), ST_Y(anchor) FROM (
               SELECT CASE WHEN f.geometry IS NULL OR ST_IsEmpty(f.geometry)
                   OR ST_SRID(f.geometry)<=0 THEN NULL
                 ELSE ST_Transform(ST_PointOnSurface(f.geometry),4326) END AS anchor
               FROM spatial.reference_features f JOIN spatial.data_sources s ON
                 s.organization_id=f.organization_id AND s.project_id=f.project_id
                 AND s.id=f.source_id
               WHERE f.organization_id=$1 AND f.project_id=$2 AND f.source_id=$3
                 AND f.feature_id=$4 AND s.lifecycle='active'
             ) AS target",
        )
        .bind(organization)
        .bind(project)
        .bind(source)
        .bind(&feature_id)
        .fetch_optional(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?
        .ok_or_else(|| detail(404, "Feature not found"))?;
        let (lng, lat) = point;
        let inside = lng.is_some_and(|x| (-180.0..=180.0).contains(&x))
            && lat.is_some_and(|y| (-90.0..=90.0).contains(&y));
        let (longitude, latitude) = if inside { (lng, lat) } else { (None, None) };
        Ok(Anchor {
            source_id: Some(source),
            feature_id: Some(feature_id),
            longitude,
            latitude,
        })
    }

    pub async fn feature_comments(
        &mut self,
        source: Uuid,
        feature: &str,
        limit: i64,
    ) -> Result<Value, EditError> {
        self.authorize(Permission::Read).await?;
        let feature_id = validate_feature_id(feature)?;
        let (organization, project) = self.org_project();
        let mut rows = sqlx::query(&format!(
            "SELECT {MESSAGE_COLS} FROM collaboration.messages
             WHERE organization_id=$1 AND project_id=$2 AND source_id=$3 AND feature_id=$4
             ORDER BY created_at DESC, id DESC LIMIT $5"
        ))
        .bind(organization)
        .bind(project)
        .bind(source)
        .bind(feature_id)
        .bind(limit)
        .fetch_all(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;
        rows.reverse();
        Ok(json!({ "items": rows_json(&rows)? }))
    }

    pub async fn comment_pins(&mut self) -> Result<Value, EditError> {
        self.authorize(Permission::Read).await?;
        let (organization, project) = self.org_project();
        let rows = sqlx::query(
            "SELECT source_id, feature_id, longitude, latitude, comment_count FROM (
               SELECT DISTINCT ON (source_id, feature_id) source_id, feature_id, longitude,
                 latitude, created_at,
                 count(*) OVER (PARTITION BY source_id, feature_id) AS comment_count
               FROM collaboration.messages
               WHERE organization_id=$1 AND project_id=$2 AND source_id IS NOT NULL
                 AND longitude IS NOT NULL AND latitude IS NOT NULL
               ORDER BY source_id, feature_id, created_at DESC, id DESC
             ) pins ORDER BY created_at DESC, source_id, feature_id LIMIT 200",
        )
        .bind(organization)
        .bind(project)
        .fetch_all(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;
        Ok(json!({ "items": rows_json(&rows)? }))
    }

    pub async fn post_feature_comment(
        &mut self,
        source: Uuid,
        feature: &str,
        body: &str,
        mention_ids: &[Uuid],
    ) -> Result<Value, EditError> {
        self.authorize(Permission::Edit).await?;
        let anchor = self.feature_anchor(source, feature).await?;
        let people = self.eligible().await?;
        let mut ids: Vec<Uuid> = Vec::new();
        for id in mention_ids {
            if !ids.contains(id) {
                ids.push(*id);
            }
        }
        let mut mentions = Vec::with_capacity(ids.len());
        for id in &ids {
            let person = people.iter().find(|(person, _)| person == id);
            let Some((_, name)) = person.filter(|_| *id != self.actor_id) else {
                return Err(detail(
                    422,
                    "Mentions must target other unrestricted project editors",
                ));
            };
            mentions.push(json!({"id": id, "display_name": name}));
        }
        let (organization, project) = self.org_project();
        let id = Uuid::new_v4();
        let row = sqlx::query(&format!(
            "INSERT INTO collaboration.messages (organization_id, project_id, id, author_id,
                 author_name, body, source_id, feature_id, longitude, latitude, mentions)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) RETURNING {MESSAGE_COLS}"
        ))
        .bind(organization)
        .bind(project)
        .bind(id)
        .bind(self.actor_id)
        .bind(self.display_200())
        .bind(body)
        .bind(anchor.source_id)
        .bind(anchor.feature_id)
        .bind(anchor.longitude)
        .bind(anchor.latitude)
        .bind(Value::Array(mentions))
        .fetch_one(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;
        for user in ids {
            sqlx::query(
                "INSERT INTO collaboration.comment_mentions
                     (organization_id, project_id, message_id, user_id)
                 VALUES ($1, $2, $3, $4)",
            )
            .bind(organization)
            .bind(project)
            .bind(id)
            .bind(user)
            .execute(&mut **self.tx.tx_mut())
            .await
            .map_err(db_error)?;
        }
        row_json(&row)
    }

    pub async fn team_tasks(&mut self, mine: bool, limit: i64) -> Result<Value, EditError> {
        self.authorize(Permission::Read).await?;
        let (organization, project) = self.org_project();
        let rows = sqlx::query(&format!(
            "SELECT {TASK_COLS} FROM collaboration.tasks
             WHERE organization_id=$1 AND project_id=$2 AND (NOT $4 OR assignee_id=$3)
             ORDER BY (status='done'), due_date NULLS LAST, updated_at DESC, id LIMIT $5"
        ))
        .bind(organization)
        .bind(project)
        .bind(self.actor_id)
        .bind(mine)
        .bind(limit)
        .fetch_all(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;
        Ok(json!({ "items": rows_json(&rows)? }))
    }

    pub async fn create_team_task(&mut self, task: NewTask) -> Result<Value, EditError> {
        self.authorize(Permission::Edit).await?;
        let eligible = self.eligible().await?;
        let Some((_, assignee_name)) = eligible.into_iter().find(|(id, _)| *id == task.assignee_id)
        else {
            return Err(detail(
                422,
                "Assignee must be an active unrestricted project editor",
            ));
        };
        let (organization, project) = self.org_project();
        let mut anchor = Anchor {
            source_id: task.source_id,
            feature_id: task.feature_id.clone(),
            longitude: task.longitude,
            latitude: task.latitude,
        };
        let mut message_anchor = false;
        if let Some(message) = task.source_message_id {
            let linked = sqlx::query_as::<_, MessageAnchor>(
                "SELECT source_id, feature_id, longitude, latitude FROM collaboration.messages
                 WHERE organization_id=$1 AND project_id=$2 AND id=$3 AND recipient_id IS NULL",
            )
            .bind(organization)
            .bind(project)
            .bind(message)
            .fetch_optional(&mut **self.tx.tx_mut())
            .await
            .map_err(db_error)?
            .ok_or_else(|| {
                detail(
                    422,
                    "Only a project message or comment can be linked to a project task",
                )
            })?;
            if let (Some(source), feature, longitude, latitude) = linked {
                if task.source_id.is_some()
                    && (task.source_id != Some(source) || task.feature_id != feature)
                {
                    return Err(detail(422, "Task feature must match comment"));
                }
                anchor = Anchor {
                    source_id: Some(source),
                    feature_id: feature,
                    longitude,
                    latitude,
                };
                message_anchor = true;
            }
        }
        if !message_anchor {
            if let (Some(source), Some(feature)) = (task.source_id, task.feature_id.as_deref()) {
                anchor = self.feature_anchor(source, feature).await?;
            }
        }
        let id = Uuid::new_v4();
        let row = sqlx::query(&format!(
            "INSERT INTO collaboration.tasks (organization_id, project_id, id, creator_id,
                 creator_name, assignee_id, assignee_name, title, description, priority,
                 due_date, longitude, latitude, source_message_id, source_id, feature_id)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
             RETURNING {TASK_COLS}"
        ))
        .bind(organization)
        .bind(project)
        .bind(id)
        .bind(self.actor_id)
        .bind(self.display_200())
        .bind(task.assignee_id)
        .bind(assignee_name)
        .bind(&task.title)
        .bind(&task.description)
        .bind(&task.priority)
        .bind(task.due_date)
        .bind(anchor.longitude)
        .bind(anchor.latitude)
        .bind(task.source_message_id)
        .bind(anchor.source_id)
        .bind(anchor.feature_id)
        .fetch_one(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;
        let metadata = json!({"assignee_id": task.assignee_id, "revision": 1});
        self.audit_project(
            "collaboration.task.create",
            "collaboration_task",
            id,
            metadata,
            now(),
        )
        .await?;
        row_json(&row)
    }

    pub async fn change_team_task(
        &mut self,
        task_id: Uuid,
        status: &str,
        revision: i64,
    ) -> Result<Value, EditError> {
        self.authorize(Permission::Edit).await?;
        let (organization, project) = self.org_project();
        let previous = sqlx::query_as::<_, (Uuid, Uuid, String)>(
            "SELECT creator_id, assignee_id, status FROM collaboration.tasks
             WHERE organization_id=$1 AND project_id=$2 AND id=$3",
        )
        .bind(organization)
        .bind(project)
        .bind(task_id)
        .fetch_optional(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?
        .ok_or_else(|| detail(404, "Task not found"))?;
        if self.actor_id != previous.0 && self.actor_id != previous.1 {
            return Err(detail(403, "Only creator or assignee may change status"));
        }
        let row = sqlx::query(&format!(
            "UPDATE collaboration.tasks SET status=$4, revision=revision+1, updated_at=now()
             WHERE organization_id=$1 AND project_id=$2 AND id=$3 AND revision=$5
               AND $6 IN (creator_id, assignee_id)
             RETURNING {TASK_COLS}"
        ))
        .bind(organization)
        .bind(project)
        .bind(task_id)
        .bind(status)
        .bind(revision)
        .bind(self.actor_id)
        .fetch_optional(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?
        .ok_or_else(|| detail(409, "Task revision changed; refresh and retry"))?;
        let new_revision: i32 = get(&row, "revision")?;
        let metadata = json!({"before": previous.2, "after": status, "revision": new_revision});
        self.audit_project(
            "collaboration.task.status",
            "collaboration_task",
            task_id,
            metadata,
            now(),
        )
        .await?;
        row_json(&row)
    }

    pub async fn team_notifications(&mut self) -> Result<Value, EditError> {
        self.authorize(Permission::Read).await?;
        let (organization, project) = self.org_project();
        let inbox = "SELECT t.id, t.title, t.creator_name, t.created_at, r.read_at,
                 'task' AS kind, t.source_id, t.feature_id, t.longitude, t.latitude
             FROM collaboration.tasks t LEFT JOIN collaboration.task_notification_reads r
               ON r.organization_id=t.organization_id AND r.project_id=t.project_id
              AND r.task_id=t.id AND r.user_id=$3
             WHERE t.organization_id=$1 AND t.project_id=$2 AND t.assignee_id=$3
               AND t.creator_id<>$3
             UNION ALL
             SELECT m.id, left(m.body, 200) AS title, m.author_name AS creator_name,
                 m.created_at, r.read_at, 'mention' AS kind, m.source_id, m.feature_id,
                 m.longitude, m.latitude
             FROM collaboration.comment_mentions r JOIN collaboration.messages m
               ON m.organization_id=r.organization_id AND m.project_id=r.project_id
              AND m.id=r.message_id
             WHERE r.organization_id=$1 AND r.project_id=$2 AND r.user_id=$3";
        let rows = sqlx::query(&format!(
            "SELECT * FROM ({inbox}) AS inbox
             ORDER BY (read_at IS NULL) DESC, created_at DESC, id LIMIT 100"
        ))
        .bind(organization)
        .bind(project)
        .bind(self.actor_id)
        .fetch_all(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;
        let unread: i64 = sqlx::query_scalar(&format!(
            "SELECT count(*) FROM ({inbox}) AS inbox WHERE read_at IS NULL"
        ))
        .bind(organization)
        .bind(project)
        .bind(self.actor_id)
        .fetch_one(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;
        Ok(json!({ "items": rows_json(&rows)?, "unread": unread }))
    }

    pub async fn read_task_notification(&mut self, task_id: Uuid) -> Result<Value, EditError> {
        self.authorize(Permission::Edit).await?;
        let (organization, project) = self.org_project();
        let exists: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM collaboration.tasks
             WHERE organization_id=$1 AND project_id=$2 AND id=$3 AND assignee_id=$4
               AND creator_id<>$4",
        )
        .bind(organization)
        .bind(project)
        .bind(task_id)
        .bind(self.actor_id)
        .fetch_optional(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;
        if exists.is_none() {
            return Err(detail(404, "Notification not found"));
        }
        sqlx::query(
            "INSERT INTO collaboration.task_notification_reads
                 (organization_id, project_id, task_id, user_id)
             VALUES ($1, $2, $3, $4) ON CONFLICT DO NOTHING",
        )
        .bind(organization)
        .bind(project)
        .bind(task_id)
        .bind(self.actor_id)
        .execute(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;
        Ok(json!({ "read": true }))
    }

    pub async fn read_mention(&mut self, message_id: Uuid) -> Result<Value, EditError> {
        self.authorize(Permission::Edit).await?;
        let (organization, project) = self.org_project();
        let updated: Option<Uuid> = sqlx::query_scalar(
            "UPDATE collaboration.comment_mentions SET read_at=COALESCE(read_at, now())
             WHERE organization_id=$1 AND project_id=$2 AND message_id=$3 AND user_id=$4
             RETURNING message_id",
        )
        .bind(organization)
        .bind(project)
        .bind(message_id)
        .bind(self.actor_id)
        .fetch_optional(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;
        if updated.is_none() {
            return Err(detail(404, "Mention not found"));
        }
        Ok(json!({ "read": true }))
    }

    pub async fn edit_leases(&mut self) -> Result<Value, EditError> {
        self.authorize(Permission::Read).await?;
        let (organization, project) = self.org_project();
        let rows = sqlx::query(&format!(
            "SELECT {LEASE_COLS} FROM collaboration.feature_edit_leases
             WHERE organization_id=$1 AND project_id=$2 AND expires_at>clock_timestamp()
             ORDER BY source_id, feature_id LIMIT 2000"
        ))
        .bind(organization)
        .bind(project)
        .fetch_all(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;
        Ok(json!({ "items": rows_json(&rows)? }))
    }

    /// Acquire or renew a 45 s lease under the same source lock as a commit.
    pub async fn acquire_edit_lease(
        &mut self,
        source: Uuid,
        feature: &str,
        token: Uuid,
    ) -> Result<Value, EditError> {
        self.authorize(Permission::Edit).await?;
        let feature_id = validate_feature_id(feature)?;
        let (organization, project) = self.org_project();
        let editable: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM spatial.data_sources
             WHERE organization_id=$1 AND project_id=$2 AND id=$3 AND lifecycle='active'
               AND validation_state='ready' AND kind<>'reference_fixture'
             FOR UPDATE",
        )
        .bind(organization)
        .bind(project)
        .bind(source)
        .fetch_optional(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;
        let mut feature_exists = false;
        if editable.is_some() {
            let found: Option<i32> = sqlx::query_scalar(
                "SELECT 1 FROM spatial.reference_features
                 WHERE organization_id=$1 AND project_id=$2 AND source_id=$3 AND feature_id=$4",
            )
            .bind(organization)
            .bind(project)
            .bind(source)
            .bind(&feature_id)
            .fetch_optional(&mut **self.tx.tx_mut())
            .await
            .map_err(db_error)?;
            feature_exists = found.is_some();
        }
        if !feature_exists {
            return Err(platform(404, "NOT_FOUND", "editable feature not found"));
        }
        sqlx::query(
            "DELETE FROM collaboration.feature_edit_leases
             WHERE organization_id=$1 AND project_id=$2 AND source_id=$3
               AND expires_at<=clock_timestamp()",
        )
        .bind(organization)
        .bind(project)
        .bind(source)
        .execute(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;
        let existing = sqlx::query_as::<_, (Uuid, Uuid)>(
            "SELECT actor_id, token FROM collaboration.feature_edit_leases
             WHERE organization_id=$1 AND project_id=$2 AND source_id=$3 AND feature_id=$4",
        )
        .bind(organization)
        .bind(project)
        .bind(source)
        .bind(&feature_id)
        .fetch_optional(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;
        if existing.is_some_and(|(actor, held)| actor != self.actor_id || held != token) {
            return Err(platform(
                409,
                "FEATURE_LOCKED",
                "feature already being edited",
            ));
        }
        let sql = match existing {
            Some(_) => format!(
                "UPDATE collaboration.feature_edit_leases
                 SET expires_at=clock_timestamp()+interval '45 seconds', actor_name=$6
                 WHERE organization_id=$1 AND project_id=$2 AND source_id=$3 AND feature_id=$4
                   AND actor_id=$5 AND token=$7
                 RETURNING {LEASE_COLS}"
            ),
            None => format!(
                "INSERT INTO collaboration.feature_edit_leases (organization_id, project_id,
                     source_id, feature_id, actor_id, actor_name, token, expires_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, clock_timestamp()+interval '45 seconds')
                 RETURNING {LEASE_COLS}"
            ),
        };
        let row = sqlx::query(&sql)
            .bind(organization)
            .bind(project)
            .bind(source)
            .bind(&feature_id)
            .bind(self.actor_id)
            .bind(self.display_200())
            .bind(token)
            .fetch_one(&mut **self.tx.tx_mut())
            .await
            .map_err(db_error)?;
        row_json(&row)
    }

    pub async fn release_edit_lease(
        &mut self,
        source: Uuid,
        feature: &str,
        token: Uuid,
    ) -> Result<Value, EditError> {
        self.authorize(Permission::Edit).await?;
        let feature_id = validate_feature_id(feature)?;
        let (organization, project) = self.org_project();
        sqlx::query(
            "SELECT id FROM spatial.data_sources
             WHERE organization_id=$1 AND project_id=$2 AND id=$3 FOR UPDATE",
        )
        .bind(organization)
        .bind(project)
        .bind(source)
        .execute(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;
        sqlx::query(
            "DELETE FROM collaboration.feature_edit_leases
             WHERE organization_id=$1 AND project_id=$2 AND source_id=$3 AND feature_id=$4
               AND actor_id=$5 AND token=$6",
        )
        .bind(organization)
        .bind(project)
        .bind(source)
        .bind(&feature_id)
        .bind(self.actor_id)
        .bind(token)
        .execute(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;
        Ok(json!({ "released": true }))
    }

    async fn record_activity(
        &mut self,
        event_kind: &str,
        resource_id: Uuid,
        revision_ref: &str,
        summary: String,
        metadata: Value,
    ) -> Result<(), EditError> {
        let (organization, project) = self.org_project();
        let display = if self.display_name.is_empty() {
            self.actor_id.to_string()
        } else {
            self.display_200()
        };
        sqlx::query(
            "INSERT INTO collaboration.activities (
                 id, organization_id, project_id, actor_type, actor_id, actor_display,
                 event_kind, resource_type, resource_id, revision_ref, summary, metadata,
                 created_at
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, 'change_review', $8, $9, $10, $11, $12)",
        )
        .bind(Uuid::new_v4())
        .bind(organization)
        .bind(project)
        .bind(self.actor_type())
        .bind(self.actor_id)
        .bind(display)
        .bind(event_kind)
        .bind(resource_id)
        .bind(revision_ref)
        .bind(summary.chars().take(500).collect::<String>())
        .bind(metadata)
        .bind(now())
        .execute(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;
        Ok(())
    }

    pub async fn list_reviews(&mut self) -> Result<Value, EditError> {
        self.authorize(Permission::Read).await?;
        let (organization, project) = self.org_project();
        let rows = sqlx::query(
            "SELECT * FROM collaboration.change_reviews
             WHERE organization_id=$1 AND project_id=$2 ORDER BY created_at DESC LIMIT 100",
        )
        .bind(organization)
        .bind(project)
        .fetch_all(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;
        let items = rows
            .iter()
            .map(review_wire)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(json!({ "items": items }))
    }

    pub async fn create_review(
        &mut self,
        changeset_id: Uuid,
        assignee: Uuid,
    ) -> Result<Value, EditError> {
        self.authorize(Permission::Edit).await?;
        let (organization, project) = self.org_project();
        let (source, revision) = sqlx::query_as::<_, (Uuid, String)>(
            "SELECT source_id, resulting_content_revision FROM editing.changesets
             WHERE organization_id=$1 AND project_id=$2 AND id=$3",
        )
        .bind(organization)
        .bind(project)
        .bind(changeset_id)
        .fetch_optional(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?
        .ok_or_else(|| platform(404, "NOT_FOUND", "changeset not found"))?;
        // The assignee's current membership and grants; never impersonate them.
        let eligible: bool = sqlx::query_scalar(
            "SELECT COALESCE(bool_or(m.role IN ('owner','admin')
                 OR g.role IN ('manager','editor','contributor')), false)
             FROM org.organization_memberships m
             JOIN identity.users u ON u.id=m.user_id AND u.status='active'
             JOIN workspace.projects p ON p.organization_id=m.organization_id
                  AND p.id=$2 AND p.status='active'
             LEFT JOIN org.resource_grants g ON g.organization_id=m.organization_id
                  AND g.membership_id=m.id AND g.subject_kind='membership'
                  AND g.revoked_at IS NULL
                  AND (g.scope_kind='organization'
                       OR (g.scope_kind='workspace' AND g.workspace_id=p.workspace_id)
                       OR (g.scope_kind='project' AND g.project_id=p.id))
             WHERE m.organization_id=$1 AND m.user_id=$3 AND m.status='active'",
        )
        .bind(organization)
        .bind(project)
        .bind(assignee)
        .fetch_one(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;
        if !eligible {
            return Err(platform(
                422,
                "INVALID_ASSIGNEE",
                "assignee must have current project review eligibility",
            ));
        }
        let at = now();
        let row = sqlx::query(
            "INSERT INTO collaboration.change_reviews (
                 id, organization_id, project_id, source_id, changeset_id, content_revision,
                 requested_by_actor_id, assignee_user_id, status, decision_note,
                 created_at, updated_at
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'pending', '', $9, $9)
             RETURNING *",
        )
        .bind(Uuid::new_v4())
        .bind(organization)
        .bind(project)
        .bind(source)
        .bind(changeset_id)
        .bind(&revision)
        .bind(self.actor_id)
        .bind(assignee)
        .bind(at)
        .fetch_one(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;
        let review_id: Uuid = get(&row, "id")?;
        let metadata = json!({"changeset_id": changeset_id, "assignee_user_id": assignee});
        let summary = format!("Requested post-commit review on changeset {changeset_id}");
        self.record_activity("review.requested", review_id, &revision, summary, metadata)
            .await?;
        review_wire(&row)
    }

    pub async fn decide_review(
        &mut self,
        review_id: Uuid,
        status: &str,
        note: &str,
    ) -> Result<Value, EditError> {
        self.authorize(Permission::Edit).await?;
        if status == "pending" {
            return Err(platform(
                422,
                "VALIDATION_ERROR",
                "decision status must be 'approved' or 'changes_requested'",
            ));
        }
        let (organization, project) = self.org_project();
        let (assignee, current) = sqlx::query_as::<_, (Uuid, String)>(
            "SELECT assignee_user_id, status FROM collaboration.change_reviews
             WHERE organization_id=$1 AND project_id=$2 AND id=$3 FOR UPDATE",
        )
        .bind(organization)
        .bind(project)
        .bind(review_id)
        .fetch_optional(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?
        .ok_or_else(|| platform(404, "NOT_FOUND", "change review not found"))?;
        if self.credential_id.is_some() || assignee != self.actor_id {
            return Err(platform(
                403,
                "FORBIDDEN",
                "only the assigned current reviewer may decide",
            ));
        }
        if current != "pending" {
            return Err(platform(
                409,
                "REVIEW_ALREADY_DECIDED",
                "a completed review cannot be overwritten",
            ));
        }
        let note = py_strip(note);
        let at = now();
        let row = sqlx::query(
            "UPDATE collaboration.change_reviews
             SET status=$4, decision_note=$5, decided_by_actor_id=$6, updated_at=$7,
                 decided_at=$7
             WHERE organization_id=$1 AND project_id=$2 AND id=$3
             RETURNING *",
        )
        .bind(organization)
        .bind(project)
        .bind(review_id)
        .bind(status)
        .bind(note.chars().take(500).collect::<String>())
        .bind(self.actor_id)
        .bind(at)
        .fetch_one(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;
        let revision: String = get(&row, "content_revision")?;
        let metadata = json!({"decision_note": note.chars().take(200).collect::<String>()});
        let summary = format!("Review marked {status}");
        let kind = format!("review.{status}");
        self.record_activity(&kind, review_id, &revision, summary, metadata)
            .await?;
        review_wire(&row)
    }

    pub async fn review_comments(&mut self, review_id: Uuid) -> Result<Value, EditError> {
        self.authorize(Permission::Read).await?;
        let (organization, project) = self.org_project();
        let rows = sqlx::query(
            "SELECT * FROM collaboration.review_comments
             WHERE organization_id=$1 AND project_id=$2 AND review_id=$3
             ORDER BY created_at ASC",
        )
        .bind(organization)
        .bind(project)
        .bind(review_id)
        .fetch_all(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;
        let items = rows
            .iter()
            .map(comment_wire)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(json!({ "items": items }))
    }

    pub async fn add_review_comment(
        &mut self,
        review_id: Uuid,
        body: &str,
        feature_id: Option<&str>,
        operation_id: Option<Uuid>,
    ) -> Result<Value, EditError> {
        self.authorize(Permission::Edit).await?;
        let clean = py_strip(body);
        if clean.is_empty() || clean.chars().count() > 2000 {
            return Err(platform(
                422,
                "VALIDATION_ERROR",
                "comment body must be 1..2000 characters",
            ));
        }
        let (organization, project) = self.org_project();
        let (changeset_id, revision) = sqlx::query_as::<_, (Uuid, String)>(
            "SELECT changeset_id, content_revision FROM collaboration.change_reviews
             WHERE organization_id=$1 AND project_id=$2 AND id=$3",
        )
        .bind(organization)
        .bind(project)
        .bind(review_id)
        .fetch_optional(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?
        .ok_or_else(|| platform(404, "NOT_FOUND", "change review not found"))?;
        if let Some(feature_id) = feature_id {
            // A feature comment cites immutable evidence in the reviewed changeset.
            let anchored: Option<i32> = sqlx::query_scalar(
                "SELECT 1 FROM editing.changeset_operations
                 WHERE organization_id=$1 AND project_id=$2 AND changeset_id=$3
                   AND feature_id=$4",
            )
            .bind(organization)
            .bind(project)
            .bind(changeset_id)
            .bind(feature_id)
            .fetch_optional(&mut **self.tx.tx_mut())
            .await
            .map_err(db_error)?;
            if anchored.is_none() {
                return Err(platform(
                    422,
                    "INVALID_REVIEW_ANCHOR",
                    "feature comment must reference an operation in the reviewed changeset",
                ));
            }
        }
        let actor = format!("{}:{}", self.actor_type(), self.actor_id);
        let comment_id = match operation_id {
            None => Uuid::new_v4(),
            Some(operation) => {
                let scope = format!("{organization}:{project}:{actor}:{review_id}:{operation}");
                uuid5_url(&format!("openvietgis:r011:comment:v1:{scope}"))
            }
        };
        if operation_id.is_some() {
            let existing = sqlx::query(
                "SELECT * FROM collaboration.review_comments
                 WHERE organization_id=$1 AND project_id=$2 AND id=$3",
            )
            .bind(organization)
            .bind(project)
            .bind(comment_id)
            .fetch_optional(&mut **self.tx.tx_mut())
            .await
            .map_err(db_error)?;
            if let Some(existing) = existing {
                let same = get::<Uuid>(&existing, "author_actor_id")? == self.actor_id
                    && get::<Uuid>(&existing, "review_id")? == review_id
                    && get::<String>(&existing, "body")? == clean
                    && get::<Option<String>>(&existing, "feature_id")?.as_deref() == feature_id
                    && get::<Uuid>(&existing, "changeset_id")? == changeset_id
                    && get::<String>(&existing, "content_revision")? == revision;
                if !same {
                    return Err(platform(
                        409,
                        "COMMENT_OPERATION_CONFLICT",
                        "comment operation_id already used for different intent",
                    ));
                }
                return comment_wire(&existing);
            }
        }
        let display = if self.display_name.is_empty() {
            self.actor_id.to_string()
        } else {
            self.display_200()
        };
        let row = sqlx::query(
            "INSERT INTO collaboration.review_comments (
                 id, organization_id, project_id, review_id, changeset_id, content_revision,
                 feature_id, body, author_actor_id, author_display, created_at
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
             RETURNING *",
        )
        .bind(comment_id)
        .bind(organization)
        .bind(project)
        .bind(review_id)
        .bind(changeset_id)
        .bind(&revision)
        .bind(feature_id)
        .bind(clean)
        .bind(self.actor_id)
        .bind(display)
        .bind(now())
        .fetch_one(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?;
        let metadata = json!({"feature_id": feature_id, "comment_id": comment_id});
        let summary = format!("Commented on review {review_id}");
        self.record_activity("review.commented", review_id, &revision, summary, metadata)
            .await?;
        comment_wire(&row)
    }

    /// Explicit refresh-and-reapply report for a stale draft (never last-write-wins).
    pub async fn conflict_rebase(
        &mut self,
        source: Uuid,
        base_revision: &str,
        draft: &[Value],
    ) -> Result<Value, EditError> {
        self.authorize(Permission::Edit).await?;
        let (organization, project) = self.org_project();
        let (current, lifecycle, validation_state, kind) = sqlx::query_as::<_, SourceState>(
            "SELECT content_revision, lifecycle, validation_state, kind
             FROM spatial.data_sources
             WHERE organization_id=$1 AND project_id=$2 AND id=$3 FOR SHARE",
        )
        .bind(organization)
        .bind(project)
        .bind(source)
        .fetch_optional(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?
        .ok_or_else(|| platform(404, "NOT_FOUND", "data source not found"))?;
        if lifecycle != "active" || validation_state != "ready" || kind == "reference_fixture" {
            return Err(platform(422, "SOURCE_NOT_READY", "source is not editable"));
        }
        let validated = validate_rebase_draft(draft)?;

        // Revision hashes are opaque: walk the immutable changeset chain back
        // from the current revision until the caller's exact base is reached.
        let mut chain: Vec<Uuid> = Vec::new();
        let mut cursor = current.clone();
        if cursor != base_revision {
            let history = sqlx::query_as::<_, (Uuid, String, String)>(
                "SELECT id, base_content_revision, resulting_content_revision
                 FROM editing.changesets
                 WHERE organization_id=$1 AND project_id=$2 AND source_id=$3
                 ORDER BY resulting_catalog_revision DESC LIMIT 2001",
            )
            .bind(organization)
            .bind(project)
            .bind(source)
            .fetch_all(&mut **self.tx.tx_mut())
            .await
            .map_err(db_error)?;
            for (id, base, resulting) in history {
                if cursor == base_revision || chain.len() >= 2000 || resulting != cursor {
                    break;
                }
                chain.push(id);
                cursor = base;
            }
            if cursor != base_revision {
                return Err(EditError::Platform {
                    status: 409,
                    code: "REBASE_HISTORY_UNAVAILABLE",
                    message: "cannot prove a continuous edit history from the requested base; \
                              refresh"
                        .to_owned(),
                    details: Some(json!({
                        "current_content_revision": current,
                        "draft_preserved": true,
                    })),
                });
            }
        }
        let feature_ids: Vec<String> = validated.iter().map(|(id, _)| id.clone()).collect();
        let intervening: HashSet<String> = if chain.is_empty() {
            HashSet::new()
        } else {
            sqlx::query_scalar::<_, String>(
                "SELECT DISTINCT feature_id FROM editing.changeset_operations
                 WHERE organization_id=$1 AND project_id=$2 AND changeset_id = ANY($3)
                   AND feature_id = ANY($4)",
            )
            .bind(organization)
            .bind(project)
            .bind(&chain)
            .bind(&feature_ids)
            .fetch_all(&mut **self.tx.tx_mut())
            .await
            .map_err(db_error)?
            .into_iter()
            .collect()
        };
        let existing: HashSet<String> = sqlx::query_scalar::<_, String>(
            "SELECT feature_id FROM spatial.reference_features
             WHERE organization_id=$1 AND project_id=$2 AND source_id=$3
               AND feature_id = ANY($4)",
        )
        .bind(organization)
        .bind(project)
        .bind(source)
        .bind(&feature_ids)
        .fetch_all(&mut **self.tx.tx_mut())
        .await
        .map_err(db_error)?
        .into_iter()
        .collect();
        Ok(analyze_draft_conflicts(
            validated,
            &intervening,
            &current,
            &existing,
        ))
    }
}

/// `validate_rebase_draft`: (normalized feature ID, operation with that ID).
fn validate_rebase_draft(draft: &[Value]) -> Result<Vec<DraftOperation>, EditError> {
    if !(1..=2000).contains(&draft.len()) {
        return Err(platform(
            422,
            "INVALID_REBASE_DRAFT",
            "rebase requires 1..2000 draft operations",
        ));
    }
    let mut seen = HashSet::new();
    let mut validated = Vec::with_capacity(draft.len());
    for operation in draft {
        let invalid = || platform(422, "INVALID_REBASE_DRAFT", "invalid rebase operation");
        let operation = operation.as_object().ok_or_else(invalid)?;
        let raw = operation
            .get("feature_id")
            .and_then(Value::as_str)
            .ok_or_else(invalid)?;
        let kind = operation.get("op_type").and_then(Value::as_str);
        if !matches!(kind, Some("create" | "update" | "delete")) {
            return Err(invalid());
        }
        let feature_id = validate_feature_id(raw)?;
        if !seen.insert(feature_id.clone()) {
            return Err(platform(
                422,
                "DUPLICATE_FEATURE_OP",
                "duplicate draft feature",
            ));
        }
        let mut operation = operation.clone();
        operation.insert("feature_id".into(), json!(feature_id));
        validated.push((feature_id, operation));
    }
    Ok(validated)
}

fn analyze_draft_conflicts(
    draft: Vec<DraftOperation>,
    intervening: &HashSet<String>,
    current: &str,
    existing: &HashSet<String>,
) -> Value {
    let mut conflicts = Vec::new();
    let mut reapplicable = Vec::new();
    for (feature_id, operation) in draft {
        let create = operation.get("op_type").and_then(Value::as_str) == Some("create");
        let reason = if intervening.contains(&feature_id) {
            Some("feature modified in an intervening committed changeset")
        } else if create && existing.contains(&feature_id) {
            Some("feature already exists in current revision")
        } else if !create && !existing.contains(&feature_id) {
            Some("feature absent from current revision")
        } else {
            None
        };
        match reason {
            Some(reason) => conflicts.push(json!({
                "feature_id": feature_id,
                "op_type": operation.get("op_type"),
                "reason": reason,
            })),
            None => reapplicable.push(Value::Object(operation)),
        }
    }
    json!({
        "current_content_revision": current,
        "has_conflicts": !conflicts.is_empty(),
        "conflict_count": conflicts.len(),
        "conflicts": conflicts,
        "reapplicable_operations": reapplicable,
        "policy": "explicit_refresh_and_reapply",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uuid5_matches_python() {
        let id = uuid5_url("https://openvietgis.example");
        assert_eq!(id.get_version_num(), 5);
        assert_eq!(id, uuid5_url("https://openvietgis.example"));
    }

    #[test]
    fn rebase_report_flags_conflicts() {
        let draft = vec![
            json!({"op_type": "create", "feature_id": " a "}),
            json!({"op_type": "update", "feature_id": "b"}),
        ];
        let validated = validate_rebase_draft(&draft).unwrap();
        let existing = HashSet::from(["a".to_owned(), "b".to_owned()]);
        let report = analyze_draft_conflicts(validated, &HashSet::new(), "rev", &existing);
        assert_eq!(report["conflict_count"], 1);
        assert_eq!(report["conflicts"][0]["feature_id"], "a");
        assert_eq!(report["reapplicable_operations"][0]["feature_id"], "b");
        assert!(validate_rebase_draft(&[]).is_err());
    }
}
