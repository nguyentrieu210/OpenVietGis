from __future__ import annotations

from concurrent.futures import ThreadPoolExecutor
from threading import Barrier
from uuid import UUID, uuid4

from sqlalchemy import create_engine, text
from sqlalchemy.orm import Session
from tools.test_database import assert_safe_test_target

from openvietgis_api.adapters.persistence.audit_repository import SqlAuditRepository
from openvietgis_api.adapters.persistence.identity_repository import SqlIdentityRepository
from openvietgis_api.adapters.persistence.rls import set_request_context
from openvietgis_api.config import get_settings
from openvietgis_api.domains.identity.domain import UnlinkIdentityResult
from openvietgis_api.domains.identity.service import (
    CannotUnlinkLastIdentityError,
    ExternalIdentityClaims,
    IdentityService,
)


def _claims() -> ExternalIdentityClaims:
    return ExternalIdentityClaims(
        issuer="https://issuer.example",
        subject=f"subject-{uuid4()}",
        display_name="Unlink Race User",
        email=None,
        email_verified=False,
    )


def _service(session: Session) -> IdentityService:
    return IdentityService(SqlIdentityRepository(session), SqlAuditRepository(session))


def test_runtime_unlink_is_actor_bound_and_atomic(migrated_database: None) -> None:
    migration_url = get_settings().migration_db_url
    assert_safe_test_target(migration_url)
    engine = create_engine(migration_url)
    try:
        # Setup/assertions use the dedicated test DB's migration role; each competing
        # mutation explicitly switches to the restricted role and its own transaction.
        with Session(engine) as setup:
            service = _service(setup)
            login = service.login(_claims(), session_ttl_seconds=3600)
            user_id = login.user.id
            set_request_context(setup, organization_id=None, actor_user_id=user_id)
            second = service.link_identity(user_id=user_id, claims=_claims())
            second_login = service.login(
                ExternalIdentityClaims(
                    issuer=second.issuer,
                    subject=second.subject,
                    display_name="Unlink Race User",
                    email=None,
                    email_verified=False,
                ),
                session_ttl_seconds=3600,
            )
            setup.commit()

        with Session(engine) as session:
            session.execute(text("SET LOCAL ROLE openvietgis_runtime"))
            assert session.execute(text("SELECT current_user")).scalar_one() == (
                "openvietgis_runtime"
            )
            assert not session.execute(
                text(
                    "SELECT has_table_privilege(current_user, 'identity.identity_links', 'DELETE')"
                )
            ).scalar_one()
            set_request_context(session, organization_id=None, actor_user_id=uuid4())
            assert (
                SqlIdentityRepository(session).unlink_identity(
                    user_id=user_id,
                    identity_link_id=second.id,
                )
                == UnlinkIdentityResult.NOT_FOUND
            )
            session.rollback()

        barrier = Barrier(2)

        def unlink(identity_link_id: UUID) -> str:
            with Session(engine) as session:
                session.execute(text("SET LOCAL ROLE openvietgis_runtime"))
                set_request_context(session, organization_id=None, actor_user_id=user_id)
                barrier.wait(timeout=10)
                try:
                    _service(session).unlink_identity(
                        user_id=user_id, identity_link_id=identity_link_id
                    )
                except CannotUnlinkLastIdentityError:
                    session.rollback()
                    return "last_identity"
                session.commit()
                return "unlinked"

        with ThreadPoolExecutor(max_workers=2) as pool:
            first = pool.submit(unlink, login.identity_link.id)
            second_future = pool.submit(unlink, second.id)
            assert sorted([first.result(timeout=30), second_future.result(timeout=30)]) == [
                "last_identity",
                "unlinked",
            ]

        with Session(engine) as check:
            links = SqlIdentityRepository(check).list_identity_links(user_id)
            assert len(links) == 1
            remaining_id = links[0].id
            remaining_sessions = check.execute(
                text("SELECT id, identity_link_id FROM identity.web_sessions WHERE user_id=:id"),
                {"id": user_id},
            ).all()
            assert len(remaining_sessions) == 1
            assert remaining_sessions[0].identity_link_id == remaining_id
            assert remaining_sessions[0].id in {login.web_session.id, second_login.web_session.id}
            assert (
                check.execute(
                    text(
                        "SELECT count(*) FROM audit.audit_events "
                        "WHERE actor_id=:id AND action='identity.unlink'"
                    ),
                    {"id": user_id},
                ).scalar_one()
                == 1
            )
    finally:
        engine.dispose()
