use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{DateTime, TimeDelta, Utc};
use ovg_core::{ActorContext, ActorType, Permission, TenantScope};
use ovg_sync::activity_cursor::{
    ActivityCursor, ActivityCursorScope, ActivityStream, CURSOR_MAX_LENGTH,
};
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

#[derive(Deserialize)]
struct Fixture {
    organization_id: Uuid,
    project_id: Uuid,
    actor_id: Uuid,
    cutoff: DateTime<Utc>,
    created_at: DateTime<Utc>,
    row_id: Uuid,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    actor_type: ActorType,
    credential_id: Option<Uuid>,
    stream: FixtureStream,
    token: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum FixtureStream {
    Review,
    Edit,
}

impl Fixture {
    fn read() -> Self {
        serde_json::from_str(include_str!("fixtures/activity_cursor.json")).unwrap()
    }

    fn tenant(&self, case: &Case) -> TenantScope {
        TenantScope {
            organization_id: self.organization_id,
            workspace_id: None,
            project_id: Some(self.project_id),
            actor: ActorContext {
                actor_type: case.actor_type,
                actor_id: self.actor_id,
                credential_id: case.credential_id,
                email: None,
            },
            permissions: vec![Permission::ProjectRead],
        }
    }
}

#[test]
fn reads_and_writes_python_reference_tokens() {
    let fixture = Fixture::read();
    for case in &fixture.cases {
        let scope = ActivityCursorScope::from(&fixture.tenant(case));
        let cursor = ActivityCursor {
            cutoff: fixture.cutoff,
            created_at: fixture.created_at,
            stream: match case.stream {
                FixtureStream::Review => ActivityStream::Review,
                FixtureStream::Edit => ActivityStream::Edit,
            },
            id: fixture.row_id,
        };
        assert_eq!(cursor.encode(&scope).unwrap(), case.token);
        assert_eq!(
            ActivityCursor::decode(&case.token, &scope, fixture.cutoff).unwrap(),
            cursor
        );
    }
}

fn assert_invalid(token: &str, scope: &ActivityCursorScope, now: DateTime<Utc>) {
    let error = ActivityCursor::decode(token, scope, now).unwrap_err();
    assert_eq!(error.status_code(), 422);
    assert_eq!(error.code(), "INVALID_ACTIVITY_CURSOR");
    assert_eq!(
        error.to_string(),
        "activity continuation unavailable; refresh the activity list"
    );
}

#[test]
fn validates_actor_credential_tenant_and_time_window() {
    let fixture = Fixture::read();
    let case = &fixture.cases[1];
    let tenant = fixture.tenant(case);
    let scope = ActivityCursorScope::from(&tenant);
    let cursor = ActivityCursor::decode(&case.token, &scope, fixture.cutoff).unwrap();

    let expiry = fixture.cutoff + TimeDelta::minutes(15);
    assert_eq!(
        ActivityCursor::decode(&case.token, &scope, expiry).unwrap(),
        cursor
    );
    assert_invalid(&case.token, &scope, expiry + TimeDelta::microseconds(1));
    assert_invalid(
        &case.token,
        &scope,
        fixture.cutoff - TimeDelta::microseconds(1),
    );

    for foreign in [
        TenantScope {
            organization_id: Uuid::new_v4(),
            ..tenant.clone()
        },
        TenantScope {
            project_id: Some(Uuid::new_v4()),
            ..tenant.clone()
        },
        TenantScope {
            actor: ActorContext {
                actor_id: Uuid::new_v4(),
                ..tenant.actor.clone()
            },
            ..tenant.clone()
        },
        TenantScope {
            actor: ActorContext {
                credential_id: Some(Uuid::new_v4()),
                ..tenant.actor.clone()
            },
            ..tenant.clone()
        },
    ] {
        assert_invalid(
            &case.token,
            &ActivityCursorScope::from(&foreign),
            fixture.cutoff,
        );
    }

    let after_cutoff = ActivityCursor {
        created_at: fixture.cutoff + TimeDelta::microseconds(1),
        ..cursor
    };
    assert_invalid(
        &after_cutoff.encode(&scope).unwrap(),
        &scope,
        fixture.cutoff,
    );
}

#[test]
fn rejects_invalid_and_noncanonical_wire_formats() {
    let fixture = Fixture::read();
    let case = &fixture.cases[0];
    let scope = ActivityCursorScope::from(&fixture.tenant(case));

    for token in ["", "?", "A", "e30", "W10", "W10="] {
        assert_invalid(token, &scope, fixture.cutoff);
    }
    assert_invalid(&"A".repeat(CURSOR_MAX_LENGTH + 1), &scope, fixture.cutoff);
    assert_invalid(&format!("{}=", case.token), &scope, fixture.cutoff);

    let raw = URL_SAFE_NO_PAD.decode(&case.token).unwrap();
    let wire: Value = serde_json::from_slice(&raw).unwrap();
    let with_spaces = URL_SAFE_NO_PAD.encode(serde_json::to_vec_pretty(&wire).unwrap());
    assert_invalid(&with_spaces, &scope, fixture.cutoff);

    for (field, value) in [
        (0, Value::Bool(true)),
        (0, Value::from(2)),
        (2, Value::from("2026-01-02T03:11:12.123456Z")),
        (2, Value::from("2026-01-02T10:11:12.123456+07:00")),
        (4, Value::Bool(false)),
        (4, Value::from(2)),
        (5, Value::from(fixture.row_id.simple().to_string())),
    ] {
        let mut noncanonical = wire.clone();
        noncanonical[field] = value;
        let token = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&noncanonical).unwrap());
        assert_invalid(&token, &scope, fixture.cutoff);
    }
}
