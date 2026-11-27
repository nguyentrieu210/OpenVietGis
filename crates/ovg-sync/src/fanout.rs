//! Bounded broadcast fanout channel for SSE/WebSocket streams with slow-consumer lag detection.

use tokio::sync::broadcast;
use uuid::Uuid;

use crate::outbox::OutboxEvent;

/// Events delivered to an SSE/WebSocket subscriber.
#[derive(Debug, Clone, PartialEq)]
pub enum SubscriptionEvent {
    Event(OutboxEvent),
    Lagged {
        skipped: u64,
        snapshot_required: bool,
    },
    Closed,
}

/// Bounded fanout hub for tenant-scoped real-time outbox events.
#[derive(Debug, Clone)]
pub struct SyncBroadcaster {
    sender: broadcast::Sender<OutboxEvent>,
    capacity: usize,
}

impl SyncBroadcaster {
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        let cap = capacity.max(1);
        let (sender, _) = broadcast::channel(cap);
        Self {
            sender,
            capacity: cap,
        }
    }

    #[must_use]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Publish an event; returns number of active subscribers that received it.
    pub fn publish(&self, event: OutboxEvent) -> usize {
        self.sender.send(event).unwrap_or(0)
    }

    /// Subscribe to events scoped strictly to one `(organization_id, project_id)`.
    #[must_use]
    pub fn subscribe(&self, organization_id: Uuid, project_id: Uuid) -> SyncSubscription {
        SyncSubscription {
            organization_id,
            project_id,
            receiver: self.sender.subscribe(),
        }
    }
}

/// Tenant-filtered subscription that surfaces slow-consumer lag instead of blocking producers.
pub struct SyncSubscription {
    organization_id: Uuid,
    project_id: Uuid,
    receiver: broadcast::Receiver<OutboxEvent>,
}

impl SyncSubscription {
    pub async fn next_event(&mut self) -> SubscriptionEvent {
        loop {
            match self.receiver.recv().await {
                Ok(ev) => {
                    if ev.organization_id == self.organization_id
                        && ev.project_id == self.project_id
                    {
                        return SubscriptionEvent::Event(ev);
                    }
                }
                Err(broadcast::error::RecvError::Lagged(skipped)) => {
                    return SubscriptionEvent::Lagged {
                        skipped,
                        snapshot_required: true,
                    };
                }
                Err(broadcast::error::RecvError::Closed) => return SubscriptionEvent::Closed,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use ovg_core::RevisionId;
    use serde_json::json;

    fn sample_event(seq: u64, org: Uuid, proj: Uuid) -> OutboxEvent {
        OutboxEvent {
            sequence: seq,
            event_id: Uuid::new_v4(),
            organization_id: org,
            project_id: proj,
            aggregate_type: "feature".to_string(),
            aggregate_id: format!("f{seq}"),
            revision: RevisionId(seq as i64),
            event_type: "feature.updated".to_string(),
            payload: json!({}),
            idempotency_key: None,
            occurred_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn detects_slow_consumer_lag_and_isolates_tenants() {
        let org_a = Uuid::new_v4();
        let org_b = Uuid::new_v4();
        let proj = Uuid::new_v4();

        let hub = SyncBroadcaster::new(2);
        let mut sub_a = hub.subscribe(org_a, proj);

        // Tenant B event must be filtered out; Tenant A event delivered.
        hub.publish(sample_event(1, org_b, proj));
        hub.publish(sample_event(2, org_a, proj));

        match sub_a.next_event().await {
            SubscriptionEvent::Event(ev) => assert_eq!(ev.sequence, 2),
            other => panic!("expected event 2, got {other:?}"),
        }

        // Flood beyond capacity=2 to trigger slow-consumer lag detection.
        hub.publish(sample_event(3, org_a, proj));
        hub.publish(sample_event(4, org_a, proj));
        hub.publish(sample_event(5, org_a, proj));

        match sub_a.next_event().await {
            SubscriptionEvent::Lagged {
                skipped,
                snapshot_required,
            } => {
                assert!(skipped >= 1);
                assert!(snapshot_required);
            }
            other => panic!("expected lag notification, got {other:?}"),
        }
    }
}
