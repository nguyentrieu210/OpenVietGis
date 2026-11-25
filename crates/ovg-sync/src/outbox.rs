//! Durable outbox event envelope, stream cursor encoding, idempotency keys, and replay.

use std::collections::{HashMap, VecDeque};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{DateTime, Utc};
use ovg_core::{AppError, RevisionId};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// Validated client-supplied idempotency key preventing duplicate mutations on retries.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct IdempotencyKey(String);

impl IdempotencyKey {
    pub fn parse(raw: &str) -> Result<Self, AppError> {
        let trimmed = raw.trim();
        if trimmed.is_empty()
            || trimmed.len() > 128
            || !trimmed
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':'))
        {
            return Err(AppError::validation(
                "INVALID_IDEMPOTENCY_KEY",
                "Idempotency key must be 1..=128 ASCII characters [A-Za-z0-9._:-].",
            ));
        }
        Ok(Self(trimmed.to_string()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Durable change event emitted atomically with a domain write transaction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutboxEvent {
    pub sequence: u64,
    pub event_id: Uuid,
    pub organization_id: Uuid,
    pub project_id: Uuid,
    pub aggregate_type: String,
    pub aggregate_id: String,
    pub revision: RevisionId,
    pub event_type: String,
    pub payload: Value,
    pub idempotency_key: Option<IdempotencyKey>,
    pub occurred_at: DateTime<Utc>,
}

/// Opaque stream cursor capturing stream epoch and last acknowledged sequence number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamCursor {
    pub epoch: u64,
    pub sequence: u64,
}

impl StreamCursor {
    #[must_use]
    pub const fn new(epoch: u64, sequence: u64) -> Self {
        Self { epoch, sequence }
    }

    #[must_use]
    pub fn encode(&self) -> String {
        let raw = format!("{}:{}", self.epoch, self.sequence);
        URL_SAFE_NO_PAD.encode(raw.as_bytes())
    }

    pub fn decode(token: &str) -> Result<Self, AppError> {
        let bytes = URL_SAFE_NO_PAD
            .decode(token)
            .map_err(|_| AppError::invalid_cursor())?;
        let text = std::str::from_utf8(&bytes).map_err(|_| AppError::invalid_cursor())?;
        let (epoch_s, seq_s) = text.split_once(':').ok_or_else(AppError::invalid_cursor)?;
        let epoch = epoch_s
            .parse::<u64>()
            .map_err(|_| AppError::invalid_cursor())?;
        let sequence = seq_s
            .parse::<u64>()
            .map_err(|_| AppError::invalid_cursor())?;
        Ok(Self { epoch, sequence })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplayRequest {
    pub organization_id: Uuid,
    pub project_id: Uuid,
    pub cursor: Option<StreamCursor>,
    pub limit: usize,
}

/// Replay result: either ordered delta events or an explicit `snapshot_required` fallback
/// when the client's cursor fell behind the retained outbox window or crossed an epoch boundary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum ReplayResponse {
    Events {
        events: Vec<OutboxEvent>,
        next_cursor: StreamCursor,
        has_more: bool,
    },
    SnapshotRequired {
        reason: String,
        latest_cursor: StreamCursor,
    },
}

/// Bounded in-memory outbox log supporting idempotency deduplication and cursor-gap detection.
#[derive(Debug)]
pub struct InMemoryOutboxLog {
    epoch: u64,
    max_retained: usize,
    next_sequence: u64,
    oldest_retained_sequence: Option<u64>,
    events: VecDeque<OutboxEvent>,
    idempotency_index: HashMap<(Uuid, String), u64>,
}

impl InMemoryOutboxLog {
    #[must_use]
    pub fn new(epoch: u64, max_retained: usize) -> Self {
        Self {
            epoch,
            max_retained: max_retained.max(1),
            next_sequence: 1,
            oldest_retained_sequence: None,
            events: VecDeque::new(),
            idempotency_index: HashMap::new(),
        }
    }

    #[must_use]
    pub fn latest_cursor(&self) -> StreamCursor {
        StreamCursor::new(self.epoch, self.next_sequence.saturating_sub(1))
    }

    /// Append an event (deduplicating by `(project_id, idempotency_key)` when provided).
    #[allow(clippy::too_many_arguments)]
    pub fn append(
        &mut self,
        organization_id: Uuid,
        project_id: Uuid,
        aggregate_type: impl Into<String>,
        aggregate_id: impl Into<String>,
        revision: RevisionId,
        event_type: impl Into<String>,
        payload: Value,
        idempotency_key: Option<IdempotencyKey>,
    ) -> OutboxEvent {
        if let Some(ref key) = idempotency_key {
            if let Some(&existing_seq) = self
                .idempotency_index
                .get(&(project_id, key.as_str().to_string()))
            {
                if let Some(existing) = self.events.iter().find(|e| e.sequence == existing_seq) {
                    return existing.clone();
                }
            }
        }

        let seq = self.next_sequence;
        self.next_sequence += 1;
        let event = OutboxEvent {
            sequence: seq,
            event_id: Uuid::new_v4(),
            organization_id,
            project_id,
            aggregate_type: aggregate_type.into(),
            aggregate_id: aggregate_id.into(),
            revision,
            event_type: event_type.into(),
            payload,
            idempotency_key: idempotency_key.clone(),
            occurred_at: Utc::now(),
        };

        if let Some(key) = idempotency_key {
            self.idempotency_index
                .insert((project_id, key.as_str().to_string()), seq);
        }

        self.events.push_back(event.clone());
        while self.events.len() > self.max_retained {
            self.events.pop_front();
        }
        self.oldest_retained_sequence = self.events.front().map(|e| e.sequence);
        event
    }

    /// Replay events for a tenant/project starting strictly after `request.cursor`.
    pub fn replay(&self, request: &ReplayRequest) -> ReplayResponse {
        let limit = request.limit.clamp(1, 500);
        let latest = self.latest_cursor();

        if let Some(cursor) = request.cursor {
            if cursor.epoch != self.epoch {
                return ReplayResponse::SnapshotRequired {
                    reason: "stream_epoch_changed".to_string(),
                    latest_cursor: latest,
                };
            }
            if cursor.sequence > latest.sequence {
                return ReplayResponse::SnapshotRequired {
                    reason: "cursor_ahead_of_stream".to_string(),
                    latest_cursor: latest,
                };
            }
            if let Some(oldest) = self.oldest_retained_sequence {
                // If cursor < oldest - 1, at least one event between cursor and oldest was evicted.
                if cursor.sequence + 1 < oldest {
                    return ReplayResponse::SnapshotRequired {
                        reason: "cursor_gap_evicted".to_string(),
                        latest_cursor: latest,
                    };
                }
            }
        }

        let after_seq = request.cursor.map_or(0, |c| c.sequence);
        let matching: Vec<OutboxEvent> = self
            .events
            .iter()
            .filter(|e| {
                e.sequence > after_seq
                    && e.organization_id == request.organization_id
                    && e.project_id == request.project_id
            })
            .cloned()
            .collect();

        let has_more = matching.len() > limit;
        let page: Vec<OutboxEvent> = matching.into_iter().take(limit).collect();
        let next_seq = page.last().map_or(after_seq, |e| e.sequence);

        ReplayResponse::Events {
            events: page,
            next_cursor: StreamCursor::new(self.epoch, next_seq),
            has_more,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn cursor_roundtrip_and_invalid_rejection() {
        let c = StreamCursor::new(3, 42);
        let tok = c.encode();
        assert_eq!(StreamCursor::decode(&tok).unwrap(), c);
        assert_eq!(
            StreamCursor::decode("!!!not-valid").unwrap_err().code(),
            "INVALID_CURSOR"
        );
    }

    #[test]
    fn replay_returns_snapshot_required_on_gap_and_deduplicates_idempotency() {
        let org = Uuid::new_v4();
        let proj = Uuid::new_v4();
        let mut log = InMemoryOutboxLog::new(1, 3);

        let key = IdempotencyKey::parse("edit-op-001").unwrap();
        let first = log.append(
            org,
            proj,
            "feature",
            "f1",
            RevisionId(1),
            "feature.updated",
            json!({"name": "A"}),
            Some(key.clone()),
        );
        let dup = log.append(
            org,
            proj,
            "feature",
            "f1",
            RevisionId(1),
            "feature.updated",
            json!({"name": "A"}),
            Some(key),
        );
        assert_eq!(first.sequence, dup.sequence);

        for i in 2..=5 {
            log.append(
                org,
                proj,
                "feature",
                format!("f{i}"),
                RevisionId(i),
                "feature.updated",
                json!({}),
                None,
            );
        }

        // Retained window is now sequences [3, 4, 5]. Cursor at sequence 1 has a gap!
        let gap_resp = log.replay(&ReplayRequest {
            organization_id: org,
            project_id: proj,
            cursor: Some(StreamCursor::new(1, 1)),
            limit: 10,
        });
        assert!(matches!(
            gap_resp,
            ReplayResponse::SnapshotRequired { ref reason, .. } if reason == "cursor_gap_evicted"
        ));

        // Cursor at sequence 2 is contiguous with oldest=3 -> returns [3, 4, 5]
        let ok_resp = log.replay(&ReplayRequest {
            organization_id: org,
            project_id: proj,
            cursor: Some(StreamCursor::new(1, 2)),
            limit: 10,
        });
        match ok_resp {
            ReplayResponse::Events {
                events,
                next_cursor,
                has_more,
            } => {
                assert_eq!(events.len(), 3);
                assert_eq!(next_cursor.sequence, 5);
                assert!(!has_more);
            }
            ReplayResponse::SnapshotRequired { .. } => panic!("unexpected snapshot fallback"),
        }
    }
}
