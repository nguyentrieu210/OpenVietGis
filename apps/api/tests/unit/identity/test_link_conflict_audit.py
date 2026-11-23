from __future__ import annotations

from datetime import UTC, datetime
from unittest.mock import Mock
from uuid import uuid4

import pytest

from openvietgis_api.domains.audit.domain import AuditOutcome
from openvietgis_api.domains.identity.domain import IdentityLink, User, UserStatus
from openvietgis_api.domains.identity.service import (
    ExternalIdentityClaims,
    IdentityLinkConflictError,
    IdentityService,
)


def _user(display_name: str) -> User:
    now = datetime.now(UTC)
    return User(
        id=uuid4(),
        display_name=display_name,
        email_snapshot=None,
        status=UserStatus.ACTIVE,
        created_at=now,
        updated_at=now,
    )


def _link(user: User, claims: ExternalIdentityClaims) -> IdentityLink:
    return IdentityLink(
        id=uuid4(),
        user_id=user.id,
        issuer=claims.issuer,
        subject=claims.subject,
        email_snapshot=claims.email,
        email_verified=claims.email_verified,
        display_name_snapshot=claims.display_name,
        created_at=user.created_at,
        last_login_at=user.created_at,
    )


@pytest.fixture
def conflict_case():
    actor = _user("Actor")
    foreign = _user("Foreign")
    claims = ExternalIdentityClaims(
        issuer="https://issuer.example",
        subject="sensitive-provider-subject",
        display_name="Sensitive Claim Name",
        email="sensitive@example.test",
        email_verified=True,
    )
    foreign_link = _link(foreign, claims)
    repository = Mock()
    audit = Mock()
    repository.get_user.return_value = actor
    return actor, foreign, foreign_link, claims, repository, audit


@pytest.mark.parametrize("race_winner", ["known", "concurrent_foreign", "concurrent_missing"])
def test_conflict_denial_is_audited_without_foreign_identity_data(
    conflict_case,
    race_winner: str,
) -> None:
    actor, foreign, foreign_link, claims, repository, audit = conflict_case
    if race_winner == "known":
        repository.find_by_external_identity.return_value = (foreign, foreign_link)
    else:
        winner = (foreign, foreign_link) if race_winner == "concurrent_foreign" else None
        repository.find_by_external_identity.side_effect = [None, winner]
        repository.try_add_identity_link.return_value = False

    service = IdentityService(repository, audit)
    with pytest.raises(IdentityLinkConflictError, match="external identity cannot be linked"):
        service.link_identity(user_id=actor.id, claims=claims)

    if race_winner == "known":
        repository.try_add_identity_link.assert_not_called()
    else:
        repository.try_add_identity_link.assert_called_once()

    audit.append.assert_called_once()
    event = audit.append.call_args.args[0]
    assert event.actor_id == actor.id
    assert event.actor_display_snapshot == actor.display_name
    assert event.action == "identity.link"
    assert event.outcome == AuditOutcome.DENIED
    assert event.organization_id is None
    assert event.resource_id is None
    assert event.metadata == {"reason": "identity_conflict"}


def test_concurrent_same_owner_link_retry_is_idempotent(conflict_case) -> None:
    actor, _foreign, _foreign_link, claims, repository, audit = conflict_case
    actor_link = _link(actor, claims)
    repository.find_by_external_identity.side_effect = [None, (actor, actor_link)]
    repository.try_add_identity_link.return_value = False

    result = IdentityService(repository, audit).link_identity(user_id=actor.id, claims=claims)
    assert result == actor_link
    audit.append.assert_not_called()


def test_unrelated_insert_failure_does_not_become_audited_conflict(conflict_case) -> None:
    actor, _foreign, _foreign_link, claims, repository, audit = conflict_case
    repository.find_by_external_identity.return_value = None
    repository.try_add_identity_link.side_effect = RuntimeError("database unavailable")
    with pytest.raises(RuntimeError, match="database unavailable"):
        IdentityService(repository, audit).link_identity(user_id=actor.id, claims=claims)
    audit.append.assert_not_called()
