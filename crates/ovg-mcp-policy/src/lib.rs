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
pub enum MachineError {
    SessionUnavailable,
    PreviousUnread,
    RateLimited,
    CommandUnavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandMachine {
    pub id: u64,
    pub state: State,
    pub deadline_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PollCommand {
    pub id: u64,
    pub expires_in_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionMachine {
    session_deadline_ms: u64,
    heartbeat_ttl_ms: u64,
    last_seen_ms: u64,
    last_command_ms: Option<u64>,
    command: Option<CommandMachine>,
    closed: bool,
}

impl SessionMachine {
    #[must_use]
    pub fn new(now_ms: u64, session_ttl_ms: u64, heartbeat_ttl_ms: u64) -> Self {
        Self {
            session_deadline_ms: now_ms.saturating_add(session_ttl_ms),
            heartbeat_ttl_ms,
            last_seen_ms: now_ms,
            last_command_ms: None,
            command: None,
            closed: false,
        }
    }

    #[must_use]
    pub fn command_state(&self) -> Option<State> {
        self.command.map(|command| command.state)
    }

    fn require_live(&self, now_ms: u64) -> Result<(), MachineError> {
        let heartbeat_age = now_ms.saturating_sub(self.last_seen_ms);
        if self.closed
            || now_ms >= self.session_deadline_ms
            || heartbeat_age > self.heartbeat_ttl_ms
        {
            Err(MachineError::SessionUnavailable)
        } else {
            Ok(())
        }
    }

    fn expire_command(&mut self, now_ms: u64) {
        if let Some(command) = &mut self.command {
            command.state = expire(command.state, now_ms, command.deadline_ms);
        }
    }

    pub fn ping_browser(&mut self, now_ms: u64) -> Result<(), MachineError> {
        self.require_live(now_ms)?;
        self.last_seen_ms = now_ms;
        self.expire_command(now_ms);
        Ok(())
    }

    pub fn submit_ai(
        &mut self,
        command_id: u64,
        now_ms: u64,
        command_ttl_ms: u64,
    ) -> Result<(), MachineError> {
        self.require_live(now_ms)?;
        self.expire_command(now_ms);
        can_submit(
            self.command.map(|command| command.state),
            now_ms,
            self.last_command_ms,
            self.session_deadline_ms,
        )
        .map_err(|error| match error {
            SubmitError::SessionExpired => MachineError::SessionUnavailable,
            SubmitError::PreviousUnread => MachineError::PreviousUnread,
            SubmitError::RateLimited => MachineError::RateLimited,
        })?;
        self.last_command_ms = Some(now_ms);
        self.command = Some(CommandMachine {
            id: command_id,
            state: State::Queued,
            deadline_ms: self
                .session_deadline_ms
                .min(now_ms.saturating_add(command_ttl_ms)),
        });
        Ok(())
    }

    pub fn poll_browser(&mut self, now_ms: u64) -> Result<Option<PollCommand>, MachineError> {
        self.require_live(now_ms)?;
        self.last_seen_ms = now_ms;
        self.expire_command(now_ms);
        let Some(command) = &mut self.command else {
            return Ok(None);
        };
        if command.state != State::Queued {
            return Ok(None);
        }
        command.state = claim(command.state, now_ms, command.deadline_ms)
            .map_err(|_| MachineError::CommandUnavailable)?;
        Ok(Some(PollCommand {
            id: command.id,
            expires_in_ms: command.deadline_ms.saturating_sub(now_ms),
        }))
    }

    pub fn check_claimed_browser(
        &mut self,
        command_id: u64,
        now_ms: u64,
    ) -> Result<u64, MachineError> {
        self.require_live(now_ms)?;
        self.expire_command(now_ms);
        let command = self
            .command
            .filter(|command| command.id == command_id && command.state == State::Claimed)
            .ok_or(MachineError::CommandUnavailable)?;
        Ok(command.deadline_ms.saturating_sub(now_ms))
    }

    pub fn cancel_ai(&mut self, command_id: u64, now_ms: u64) -> bool {
        if self.require_live(now_ms).is_err() {
            return false;
        }
        self.expire_command(now_ms);
        if self.command.is_some_and(|command| command.id == command_id) {
            self.command = None;
            true
        } else {
            false
        }
    }

    pub fn acknowledge_browser(
        &mut self,
        command_id: u64,
        approved: bool,
        now_ms: u64,
    ) -> Result<State, MachineError> {
        self.require_live(now_ms)?;
        self.last_seen_ms = now_ms;
        self.expire_command(now_ms);
        let command = self
            .command
            .as_mut()
            .filter(|command| command.id == command_id)
            .ok_or(MachineError::CommandUnavailable)?;
        command.state = acknowledge(command.state, approved, now_ms, command.deadline_ms)
            .map_err(|_| MachineError::CommandUnavailable)?;
        Ok(command.state)
    }

    pub fn result_ai(&mut self, command_id: u64, now_ms: u64) -> Result<State, MachineError> {
        self.require_live(now_ms)?;
        self.expire_command(now_ms);
        let state = self
            .command
            .filter(|command| command.id == command_id)
            .map(|command| command.state)
            .ok_or(MachineError::CommandUnavailable)?;
        if result_is_consumable(state) {
            self.command = None;
        }
        Ok(state)
    }

    pub fn close_browser(&mut self, now_ms: u64) -> Result<(), MachineError> {
        self.require_live(now_ms)?;
        self.closed = true;
        self.command = None;
        Ok(())
    }
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
    fn session_machine_matches_bridge_store_lifecycle() {
        let mut session = SessionMachine::new(100_000, 900_000, 15_000);
        session.submit_ai(1, 100_000, 40_000).unwrap();
        assert_eq!(session.result_ai(1, 100_000), Ok(State::Queued));

        let polled = session.poll_browser(100_000).unwrap().unwrap();
        assert_eq!(polled.id, 1);
        assert_eq!(polled.expires_in_ms, 40_000);
        assert_eq!(session.poll_browser(100_000), Ok(None));
        assert_eq!(session.check_claimed_browser(1, 100_000), Ok(40_000));

        session.ping_browser(109_000).unwrap();
        assert_eq!(
            session.acknowledge_browser(1, false, 109_000),
            Ok(State::Denied)
        );
        assert_eq!(session.result_ai(1, 109_000), Ok(State::Denied));
        assert_eq!(
            session.result_ai(1, 109_000),
            Err(MachineError::CommandUnavailable)
        );

        session.submit_ai(2, 109_000, 40_000).unwrap();
        assert!(session.cancel_ai(2, 109_000));
        assert_eq!(
            session.submit_ai(3, 110_999, 40_000),
            Err(MachineError::RateLimited)
        );
        session.submit_ai(3, 111_000, 40_000).unwrap();
    }

    #[test]
    fn session_machine_expires_commands_and_heartbeat_like_python() {
        let mut session = SessionMachine::new(100_000, 900_000, 15_000);
        session.submit_ai(7, 100_000, 40_000).unwrap();
        session.poll_browser(100_000).unwrap();
        for now in [109_000, 118_000, 127_000, 136_000, 145_000] {
            session.ping_browser(now).unwrap();
        }
        assert_eq!(session.command_state(), Some(State::Expired));
        assert_eq!(session.result_ai(7, 145_000), Ok(State::Expired));

        assert_eq!(
            session.poll_browser(160_001),
            Err(MachineError::SessionUnavailable)
        );
    }

    #[test]
    fn terminal_unread_blocks_submit_but_expired_command_can_be_replaced() {
        let mut session = SessionMachine::new(0, 900_000, 15_000);
        session.submit_ai(1, 0, 40_000).unwrap();
        session.poll_browser(0).unwrap();
        assert_eq!(session.acknowledge_browser(1, true, 0), Ok(State::Done));
        assert_eq!(
            session.submit_ai(2, 2_000, 40_000),
            Err(MachineError::PreviousUnread)
        );
        assert_eq!(session.result_ai(1, 2_000), Ok(State::Done));
        session.submit_ai(2, 2_000, 1_000).unwrap();
        session.ping_browser(4_000).unwrap();
        assert_eq!(session.command_state(), Some(State::Expired));
        session.submit_ai(3, 4_000, 40_000).unwrap();
        assert_eq!(session.command_state(), Some(State::Queued));
        session.close_browser(4_000).unwrap();
        assert_eq!(
            session.poll_browser(4_000),
            Err(MachineError::SessionUnavailable)
        );
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
