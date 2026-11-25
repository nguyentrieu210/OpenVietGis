//! Candidate pure policy for the E16 MCP relay. No credentials, networking,
//! authorization grants or GIS writes. The Python wire adapter stays canonical.
#![forbid(unsafe_code)]

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Queued,
    Claimed,
    Done,
    Denied,
    Expired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmitError {
    SessionExpired,
    PreviousUnread,
    RateLimited,
}

/// Mirror the bridge's one-command-at-a-time and two-second submit guard.
/// Transport authentication and method-specific payload validation happen
/// outside this policy layer.
pub fn can_submit(
    current: Option<State>,
    now_ms: u64,
    last_command_ms: Option<u64>,
    session_deadline_ms: u64,
) -> Result<(), SubmitError> {
    if now_ms >= session_deadline_ms {
        return Err(SubmitError::SessionExpired);
    }
    if current.is_some_and(|state| {
        matches!(
            state,
            State::Queued | State::Claimed | State::Done | State::Denied
        )
    }) {
        return Err(SubmitError::PreviousUnread);
    }
    if last_command_ms.is_some_and(|last| now_ms.saturating_sub(last) < 2_000) {
        return Err(SubmitError::RateLimited);
    }
    Ok(())
}

/// Terminal results are consumed once. Expired is terminal; active states are
/// visible but must not be removed by a polling client.
#[must_use]
pub fn result_is_consumable(state: State) -> bool {
    matches!(state, State::Done | State::Denied | State::Expired)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyError {
    InvalidCoordinate,
    CommandUnavailable,
}

pub fn navigation(longitude: f64, latitude: f64, zoom: f64) -> Result<[f64; 3], PolicyError> {
    if !(-180.0..=180.0).contains(&longitude)
        || !(-85.0..=85.0).contains(&latitude)
        || !(0.0..=22.0).contains(&zoom)
    {
        return Err(PolicyError::InvalidCoordinate);
    }
    Ok([longitude, latitude, zoom])
}

/// Only an active command may expire; terminal results survive until read.
pub fn expire(state: State, now_ms: u64, deadline_ms: u64) -> State {
    if now_ms >= deadline_ms && matches!(state, State::Queued | State::Claimed) {
        State::Expired
    } else {
        state
    }
}

/// Call only after the transport authenticates the browser capability.
pub fn claim(state: State, now_ms: u64, deadline_ms: u64) -> Result<State, PolicyError> {
    match expire(state, now_ms, deadline_ms) {
        State::Queued => Ok(State::Claimed),
        _ => Err(PolicyError::CommandUnavailable),
    }
}

/// The caller must validate consent, identity, revision pins and the result
/// against the existing wire contract. This function cannot grant permission.
pub fn acknowledge(
    state: State,
    approved: bool,
    now_ms: u64,
    deadline_ms: u64,
) -> Result<State, PolicyError> {
    match expire(state, now_ms, deadline_ms) {
        State::Claimed if approved => Ok(State::Done),
        State::Claimed => Ok(State::Denied),
        _ => Err(PolicyError::CommandUnavailable),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigation_fixture() {
        for row in include_str!("../../../apps/mcp/fixtures/e16-navigation.tsv").lines() {
            let fields: Vec<_> = row.split('\t').collect();
            let n = |i: usize| fields[i].parse::<f64>().unwrap();
            assert_eq!(
                navigation(n(0), n(1), n(2)).is_ok(),
                fields[3] == "ok",
                "{row}"
            );
        }
    }

    #[test]
    fn submission_and_result_consumption_match_bridge_rules() {
        assert_eq!(can_submit(None, 1_000, None, 10_000), Ok(()));
        assert_eq!(
            can_submit(Some(State::Done), 5_000, None, 10_000),
            Err(SubmitError::PreviousUnread)
        );
        assert_eq!(
            can_submit(None, 2_500, Some(1_000), 10_000),
            Err(SubmitError::RateLimited)
        );
        assert_eq!(
            can_submit(None, 10_000, None, 10_000),
            Err(SubmitError::SessionExpired)
        );
        assert!(result_is_consumable(State::Done));
        assert!(result_is_consumable(State::Denied));
        assert!(result_is_consumable(State::Expired));
        assert!(!result_is_consumable(State::Queued));
        assert!(!result_is_consumable(State::Claimed));
    }

    #[test]
    fn lifecycle_expiry_and_denial() {
        assert_eq!(claim(State::Queued, 9, 10), Ok(State::Claimed));
        assert_eq!(
            claim(State::Queued, 10, 10),
            Err(PolicyError::CommandUnavailable)
        );
        assert_eq!(
            claim(State::Claimed, 9, 10),
            Err(PolicyError::CommandUnavailable)
        );
        assert_eq!(
            acknowledge(State::Queued, true, 9, 10),
            Err(PolicyError::CommandUnavailable)
        );
        assert_eq!(acknowledge(State::Claimed, false, 9, 10), Ok(State::Denied));
        assert_eq!(acknowledge(State::Claimed, true, 9, 10), Ok(State::Done));
        assert_eq!(
            acknowledge(State::Claimed, true, 10, 10),
            Err(PolicyError::CommandUnavailable)
        );
        for state in [State::Done, State::Denied, State::Expired] {
            assert_eq!(expire(state, 99, 10), state);
            assert_eq!(
                acknowledge(state, true, 9, 10),
                Err(PolicyError::CommandUnavailable)
            );
        }
    }
}
