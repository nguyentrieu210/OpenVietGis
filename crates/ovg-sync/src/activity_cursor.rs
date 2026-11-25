//! Wire-compatible cursors for collaboration activity keyset paging.
//!
//! A cursor identifies a position within an active authorization scope. The caller
//! must reauthorize each page and check that the boundary is still visible in the
//! scoped activity query before using its paging position.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{DateTime, Datelike, SecondsFormat, TimeDelta, Timelike, Utc};
use ovg_core::{ActorType, AppError, TenantScope};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub const CURSOR_MAX_LENGTH: usize = 512;
const CURSOR_LIFETIME: TimeDelta = TimeDelta::minutes(15);
const WIRE_VERSION: u8 = 1;

/// Digest of the actor, credential, organization, and project for one activity request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityCursorScope(String);

impl From<&TenantScope> for ActivityCursorScope {
    fn from(scope: &TenantScope) -> Self {
        let actor_type = match scope.actor.actor_type {
            ActorType::Human => "human",
            ActorType::ServicePrincipal => "service_principal",
        };
        let raw = format!(
            "{actor_type}|{}|{}|{}|{}",
            scope.actor.actor_id,
            uuid_or_none(scope.actor.credential_id),
            scope.organization_id,
            uuid_or_none(scope.project_id),
        );
        Self(format!("{:x}", Sha256::digest(raw.as_bytes())))
    }
}

fn uuid_or_none(value: Option<Uuid>) -> String {
    value.map_or_else(|| "None".to_owned(), |id| id.to_string())
}

/// The stream breaks ties between review and edit records with equal timestamps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ActivityStream {
    Review = 0,
    Edit = 1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActivityCursor {
    pub cutoff: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub stream: ActivityStream,
    pub id: Uuid,
}

impl ActivityCursor {
    /// Encode the same ordered JSON tuple as the existing Python API.
    pub fn encode(&self, scope: &ActivityCursorScope) -> Result<String, AppError> {
        let cutoff = wire_time(self.cutoff).ok_or_else(invalid_cursor)?;
        let created_at = wire_time(self.created_at).ok_or_else(invalid_cursor)?;
        let raw = serde_json::to_vec(&(
            WIRE_VERSION,
            &scope.0,
            cutoff,
            created_at,
            self.stream as u8,
            self.id,
        ))
        .map_err(|_| invalid_cursor())?;
        Ok(URL_SAFE_NO_PAD.encode(raw))
    }

    /// Decode a canonical version-1 token and validate scope and paging timestamps.
    pub fn decode(token: &str, scope: &ActivityCursorScope, now: DateTime<Utc>) -> Result<Self, AppError> {
        if token.is_empty() || token.len() > CURSOR_MAX_LENGTH {
            return Err(invalid_cursor());
        }

        let raw = URL_SAFE_NO_PAD.decode(token).map_err(|_| invalid_cursor())?;
        let (version, supplied_scope, cutoff, created_at, stream, id): (u8, &str, &str, &str, u8, Uuid) =
            serde_json::from_slice(&raw).map_err(|_| invalid_cursor())?;
        if version != WIRE_VERSION || supplied_scope != scope.0 {
            return Err(invalid_cursor());
        }
        let cursor = Self {
            cutoff: parse_time(cutoff)?,
            created_at: parse_time(created_at)?,
            stream: match stream {
                0 => ActivityStream::Review,
                1 => ActivityStream::Edit,
                _ => return Err(invalid_cursor()),
            },
            id,
        };
        let now = now
            .with_nanosecond(now.nanosecond() / 1_000 * 1_000)
            .ok_or_else(invalid_cursor)?;
        if cursor.cutoff > now
            || now.signed_duration_since(cursor.cutoff) > CURSOR_LIFETIME
            || cursor.created_at > cursor.cutoff
            || cursor.encode(scope)? != token
        {
            return Err(invalid_cursor());
        }
        Ok(cursor)
    }
}

fn wire_time(time: DateTime<Utc>) -> Option<String> {
    ((1..=9999).contains(&time.year()) && time.nanosecond() < 1_000_000_000)
        .then(|| time.to_rfc3339_opts(SecondsFormat::Micros, false))
}

fn parse_time(time: &str) -> Result<DateTime<Utc>, AppError> {
    DateTime::parse_from_rfc3339(time)
        .map(|time| time.with_timezone(&Utc))
        .map_err(|_| invalid_cursor())
}

fn invalid_cursor() -> AppError {
    AppError::validation(
        "INVALID_ACTIVITY_CURSOR",
        "activity continuation unavailable; refresh the activity list",
    )
}
