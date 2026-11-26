//! Project activity history: committed edits merged with review events, keyset paged.

use std::time::Duration;

use chrono::{DateTime, Utc};
use ovg_core::AppError;
use serde_json::{json, Value};
use sqlx::postgres::{PgListener, PgPool, PgPoolOptions};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::DbPool;

const EDIT_ACTIVITY: &str = "
    SELECT cs.id, cs.actor_type::text AS actor_type, cs.actor_id,
        cs.actor_display_snapshot::text AS actor_display,
        CASE WHEN cs.status='reverted' THEN 'changeset.reverted'
             ELSE 'changeset.committed' END AS event_kind,
        'changeset' AS resource_type, cs.id AS resource_id,
        cs.resulting_content_revision::text AS revision_ref, cs.summary::text AS summary,
        jsonb_build_object('source_id', cs.source_id,
            'changeset_id', cs.id, 'base_content_revision', cs.base_content_revision,
            'operation_count', cs.operation_count,
            'reverted_from_changeset_id', cs.reverted_from_changeset_id) AS metadata,
        cs.created_at, 1 AS stream
    FROM editing.changesets cs
    JOIN spatial.data_sources src ON src.organization_id=cs.organization_id
        AND src.project_id=cs.project_id AND src.id=cs.source_id
    WHERE cs.organization_id=$1 AND cs.project_id=$2
        AND cs.status IN ('committed','reverted') AND cs.created_at<=$3";

const REVIEW_ACTIVITY: &str = "
    SELECT a.id, a.actor_type::text AS actor_type, a.actor_id,
        a.actor_display::text AS actor_display, a.event_kind::text AS event_kind,
        a.resource_type::text AS resource_type, a.resource_id,
        a.revision_ref::text AS revision_ref, a.summary::text AS summary, a.metadata,
        a.created_at, 0 AS stream
    FROM collaboration.activities a
    JOIN collaboration.change_reviews r ON r.organization_id=a.organization_id
        AND r.project_id=a.project_id AND r.id=a.resource_id
        AND r.content_revision=a.revision_ref
    JOIN editing.changesets cs ON cs.organization_id=r.organization_id
        AND cs.project_id=r.project_id AND cs.id=r.changeset_id
        AND cs.source_id=r.source_id AND cs.resulting_content_revision=r.content_revision
    JOIN spatial.data_sources src ON src.organization_id=cs.organization_id
        AND src.project_id=cs.project_id AND src.id=cs.source_id
    WHERE a.organization_id=$1 AND a.project_id=$2
        AND a.resource_type='change_review'
        AND a.event_kind IN ('review.requested','review.commented',
                             'review.approved','review.changes_requested')
        AND a.created_at<=$3";

/// Keyset position: `(created_at, stream, id)` of the last row already delivered.
#[derive(Debug, Clone, Copy)]
pub struct ActivityPosition {
    pub created_at: DateTime<Utc>,
    pub stream: i32,
    pub id: Uuid,
}

pub struct Activity {
    pub position: ActivityPosition,
    pub wire: Value,
}

/// Rows at or before `cutoff`, newest first, at most `limit + 1` (lookahead).
/// `Ok(None)` means the continuation boundary is not visible in the current scope.
pub async fn list_activities(
    tx: &mut Transaction<'_, Postgres>,
    organization: Uuid,
    project: Uuid,
    cutoff: DateTime<Utc>,
    after: Option<ActivityPosition>,
    limit: i64,
) -> Result<Option<Vec<Activity>>, AppError> {
    sqlx::query("SELECT set_config('statement_timeout','2000ms',true)")
        .execute(&mut **tx)
        .await
        .map_err(unavailable)?;

    let (mut edit, mut review) = (EDIT_ACTIVITY.to_owned(), REVIEW_ACTIVITY.to_owned());
    if let Some(after) = after {
        // A forged or stale boundary has no authority independent of scoped history.
        let boundary = if after.stream == 1 {
            format!("{edit} AND cs.id=$6 AND cs.created_at=$4")
        } else {
            format!("{review} AND a.id=$6 AND a.created_at=$4")
        };
        let probe = format!("SELECT 1 FROM ({boundary}) AS boundary LIMIT 1");
        let visible = sqlx::query(&probe)
            .bind(organization)
            .bind(project)
            .bind(cutoff)
            .bind(after.created_at)
            .bind(after.stream)
            .bind(after.id)
            .fetch_optional(&mut **tx)
            .await
            .map_err(unavailable)?;
        if visible.is_none() {
            return Ok(None);
        }
        edit.push_str(" AND (cs.created_at,1,cs.id)<($4,$5,$6)");
        review.push_str(" AND (a.created_at,0,a.id)<($4,$5,$6)");
    }
    // Each branch returns a bounded lookahead; the merge is at most 2*(limit+1) rows.
    let order = " ORDER BY created_at DESC, stream DESC, id DESC LIMIT $7";
    let sql = format!("({edit}{order}) UNION ALL ({review}{order}){order}");
    let after = after.unwrap_or(ActivityPosition {
        created_at: cutoff,
        stream: 0,
        id: Uuid::nil(),
    });
    let rows = sqlx::query(&sql)
        .bind(organization)
        .bind(project)
        .bind(cutoff)
        .bind(after.created_at)
        .bind(after.stream)
        .bind(after.id)
        .bind(limit + 1)
        .fetch_all(&mut **tx)
        .await
        .map_err(unavailable)?;

    rows.iter()
        .map(|row| -> Result<Activity, AppError> {
            let id: Uuid = row.try_get("id").map_err(unavailable)?;
            let created_at: DateTime<Utc> = row.try_get("created_at").map_err(unavailable)?;
            let revision_ref: Option<String> = row.try_get("revision_ref").map_err(unavailable)?;
            let wire = json!({
                "id": id,
                "actor_type": row.try_get::<String, _>("actor_type").map_err(unavailable)?,
                "actor_id": row.try_get::<Uuid, _>("actor_id").map_err(unavailable)?,
                "actor_display": row.try_get::<String, _>("actor_display").map_err(unavailable)?,
                "event_kind": row.try_get::<String, _>("event_kind").map_err(unavailable)?,
                "resource_type": row.try_get::<String, _>("resource_type").map_err(unavailable)?,
                "resource_id": row.try_get::<Uuid, _>("resource_id").map_err(unavailable)?,
                // Python maps an empty revision reference to None.
                "revision_ref": revision_ref.filter(|value| !value.is_empty()),
                "summary": row.try_get::<String, _>("summary").map_err(unavailable)?,
                "metadata": row.try_get::<Value, _>("metadata").map_err(unavailable)?,
                "created_at": isoformat(created_at),
            });
            Ok(Activity {
                position: ActivityPosition {
                    created_at,
                    stream: row.try_get("stream").map_err(unavailable)?,
                    id,
                },
                wire,
            })
        })
        .collect::<Result<Vec<_>, AppError>>()
        .map(Some)
}

/// One dedicated LISTEN connection outside the request pool, like the Python stream.
pub struct ChangeListener {
    listener: PgListener,
    pool: PgPool,
}

impl ChangeListener {
    pub async fn connect(pool: &DbPool, channel: &str) -> Result<Self, AppError> {
        let options = (*pool.inner().connect_options()).clone();
        let dedicated = PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(Duration::from_secs(5))
            .connect_with(options)
            .await
            .map_err(listener_error)?;
        let listener = match PgListener::connect_with(&dedicated).await {
            Ok(listener) => listener,
            Err(error) => {
                dedicated.close().await;
                return Err(listener_error(error));
            }
        };
        let mut this = Self {
            listener,
            pool: dedicated,
        };
        if let Err(error) = this.listener.listen(channel).await {
            this.close().await;
            return Err(listener_error(error));
        }
        Ok(this)
    }

    /// Next notification payload; an error once the connection is lost.
    pub async fn next(&mut self) -> Result<String, AppError> {
        match self.listener.try_recv().await {
            Ok(Some(notice)) => Ok(notice.payload().to_owned()),
            Ok(None) => Err(AppError::service_unavailable(
                "LISTENER_UNAVAILABLE",
                "listener connection lost",
            )),
            Err(error) => Err(listener_error(error)),
        }
    }

    /// Close the dedicated connection now; a dropped pool is only reaped later.
    pub async fn close(self) {
        drop(self.listener);
        self.pool.close().await;
    }
}

fn listener_error(error: sqlx::Error) -> AppError {
    tracing::warn!(%error, "collaboration listener failed");
    AppError::service_unavailable("LISTENER_UNAVAILABLE", "listener unavailable")
}

/// Python `datetime.isoformat()` of a UTC timestamp.
#[must_use]
pub fn isoformat(time: DateTime<Utc>) -> String {
    let format = if time.timestamp_subsec_micros() == 0 {
        "%Y-%m-%dT%H:%M:%S+00:00"
    } else {
        "%Y-%m-%dT%H:%M:%S%.6f+00:00"
    };
    time.format(format).to_string()
}

fn unavailable(error: sqlx::Error) -> AppError {
    tracing::warn!(%error, "activity read failed");
    AppError::service_unavailable(
        "ACTIVITY_READ_UNAVAILABLE",
        "activity read unavailable; refresh or retry safely",
    )
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn isoformat_matches_python() {
        let whole = Utc.with_ymd_and_hms(2026, 11, 26, 1, 2, 3).unwrap();
        assert_eq!(isoformat(whole), "2026-11-26T01:02:03+00:00");
        let micros = whole + chrono::TimeDelta::microseconds(450);
        assert_eq!(isoformat(micros), "2026-11-26T01:02:03.000450+00:00");
    }
}
