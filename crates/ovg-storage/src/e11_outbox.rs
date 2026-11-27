//! Candidate project-wide invalidation replay. No private message payloads.
//! Only an authorized ProjectRead can invoke this repository.

use ovg_core::AppError;
use serde::Serialize;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SpatialCursor {
    pub epoch: Uuid,
    pub sequence: i64,
}

#[derive(Debug, Serialize)]
pub struct SpatialInvalidation {
    pub sequence: i64,
    pub kind: String,
}

#[derive(Debug, Serialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum SpatialReplay {
    Events {
        events: Vec<SpatialInvalidation>,
        next_cursor: Option<SpatialCursor>,
        has_more: bool,
    },
    SnapshotRequired {
        latest_cursor: Option<SpatialCursor>,
    },
}

pub(crate) async fn replay(
    tx: &mut Transaction<'_, Postgres>,
    organization: Uuid,
    project: Uuid,
    cursor: Option<SpatialCursor>,
    limit: usize,
) -> Result<SpatialReplay, AppError> {
    if !(1..=500).contains(&limit) {
        return Err(AppError::bad_request("invalid replay limit"));
    }
    if cursor.is_some_and(|c| c.sequence < 0 || c.epoch.is_nil()) {
        return Err(AppError::invalid_cursor());
    }

    // One statement, one MVCC snapshot for the watermark and event page.
    // A separate watermark SELECT and event SELECT at READ COMMITTED could
    // race with retention and silently skip a committed invalidation.
    let rows = sqlx::query_as::<_, (Uuid, i64, Option<i64>, Option<String>)>(
        "SELECT s.epoch, s.sequence, e.sequence, e.kind
         FROM ovg_e11.streams s
         LEFT JOIN LATERAL (
             SELECT sequence, kind FROM ovg_e11.events
             WHERE organization_id = s.organization_id
               AND project_id = s.project_id AND sequence > $3
             ORDER BY sequence LIMIT $4
         ) e ON true
         WHERE s.organization_id = $1 AND s.project_id = $2
         ORDER BY e.sequence",
    )
    .bind(organization)
    .bind(project)
    .bind(cursor.map_or(0, |c| c.sequence))
    .bind((limit + 1) as i64)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| {
        let missing_schema = error
            .as_database_error()
            .and_then(sqlx::error::DatabaseError::code)
            .is_some_and(|code| matches!(code.as_ref(), "3F000" | "42P01"));
        if missing_schema {
            AppError::service_unavailable("OUTBOX_UNAVAILABLE", "spatial invalidations unavailable")
        } else {
            AppError::internal(format!("E11 outbox replay failed: {error}"))
        }
    })?;

    let Some((epoch, latest, _, _)) = rows.first() else {
        return Ok(if cursor.is_some() {
            SpatialReplay::SnapshotRequired {
                latest_cursor: None,
            }
        } else {
            SpatialReplay::Events {
                events: Vec::new(),
                next_cursor: None,
                has_more: false,
            }
        });
    };
    let watermark = SpatialCursor {
        epoch: *epoch,
        sequence: *latest,
    };
    let after = cursor.map_or(0, |c| c.sequence);
    if cursor.is_some_and(|c| c.epoch != watermark.epoch || c.sequence > watermark.sequence)
        || after < watermark.sequence.saturating_sub(4096)
    {
        return Ok(SpatialReplay::SnapshotRequired {
            latest_cursor: Some(watermark),
        });
    }

    let mut events = Vec::with_capacity(limit);
    for (_, _, sequence, kind) in rows {
        if let (Some(sequence), Some(kind)) = (sequence, kind) {
            events.push(SpatialInvalidation { sequence, kind });
        }
    }
    // Fail closed on unexpected holes, not just the expected retention boundary.
    if events
        .iter()
        .enumerate()
        .any(|(index, event)| after.checked_add(index as i64 + 1) != Some(event.sequence))
        || (events.len() <= limit
            && events.last().map_or(after, |event| event.sequence) != watermark.sequence)
    {
        return Ok(SpatialReplay::SnapshotRequired {
            latest_cursor: Some(watermark),
        });
    }
    let has_more = events.len() > limit;
    events.truncate(limit);
    let next_cursor = SpatialCursor {
        epoch: watermark.epoch,
        sequence: events.last().map_or(after, |event| event.sequence),
    };
    Ok(SpatialReplay::Events {
        events,
        next_cursor: Some(next_cursor),
        has_more,
    })
}
