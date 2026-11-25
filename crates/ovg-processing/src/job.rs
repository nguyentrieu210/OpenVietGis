//! Job state machine (`queued` -> `running` -> `succeeded` | `failed` | `cancelled`).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct JobId(pub Uuid);

impl JobId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for JobId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

impl JobStatus {
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }

    #[must_use]
    pub const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Queued, Self::Running | Self::Cancelled | Self::Failed)
                | (
                    Self::Running,
                    Self::Succeeded | Self::Failed | Self::Cancelled
                )
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobRecord {
    pub id: JobId,
    pub organization_id: Uuid,
    pub job_type: String,
    pub status: JobStatus,
    pub result: Option<Value>,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl JobRecord {
    #[must_use]
    pub fn new_queued(organization_id: Uuid, job_type: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: JobId::new(),
            organization_id,
            job_type: job_type.into(),
            status: JobStatus::Queued,
            result: None,
            error: None,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn transition(&mut self, next: JobStatus) -> bool {
        if !self.status.can_transition_to(next) {
            return false;
        }
        self.status = next;
        self.updated_at = Utc::now();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enforces_valid_job_state_transitions() {
        assert!(JobStatus::Queued.can_transition_to(JobStatus::Running));
        assert!(JobStatus::Queued.can_transition_to(JobStatus::Cancelled));
        assert!(JobStatus::Running.can_transition_to(JobStatus::Succeeded));
        assert!(JobStatus::Running.can_transition_to(JobStatus::Failed));
        assert!(JobStatus::Running.can_transition_to(JobStatus::Cancelled));
        assert!(!JobStatus::Succeeded.can_transition_to(JobStatus::Running));
        assert!(!JobStatus::Cancelled.can_transition_to(JobStatus::Succeeded));
    }
}
