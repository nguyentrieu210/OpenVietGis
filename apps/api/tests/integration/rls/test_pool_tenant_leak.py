from __future__ import annotations

from sqlalchemy import text

from openvietgis_api.adapters.persistence.db import get_engine, get_session_factory
from openvietgis_api.adapters.persistence.rls import set_request_context
from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.rls.test_cross_tenant_repository import TABLES, _seed_protected_rows


def test_repeated_pool_checkout_isolates_a_b_and_no_context(
    migrated_database: None,
) -> None:
    a, b = seed_access_world(), seed_access_world()
    rows_a, rows_b = _seed_protected_rows(a), _seed_protected_rows(b)
    engine = get_engine()
    factory = get_session_factory()
    backend_pid = None
    # A new Session per checkout. Commit + rollback both release the connection;
    # deliberately omit set_request_context for no-context requests to test cleanup.
    for _ in range(4):
        for actor, allowed, commit in (
            (a, rows_a, True),
            (b, rows_b, False),
            (None, {}, True),
            (b, rows_b, True),
            (a, rows_a, False),
            (None, {}, False),
        ):
            with factory() as session:
                role, pid = session.execute(text("SELECT current_user, pg_backend_pid()")).one()
                assert role == "openvietgis_runtime"
                if backend_pid is None:
                    backend_pid = pid
                assert pid == backend_pid, "pool did not reuse the same physical connection"
                # Every checkout must start with no retained context before binding a new one.
                previous = session.execute(
                    text("""
                    SELECT nullif(current_setting('openvietgis.organization_id', true), ''),
                           nullif(current_setting('openvietgis.actor_user_id', true), '')
                """)
                ).one()
                assert tuple(previous) == (None, None)
                if actor is not None:
                    set_request_context(
                        session,
                        organization_id=actor.organization_id,
                        actor_user_id=actor.owner_user_id,
                    )
                for table in TABLES:
                    ids = set(
                        session.execute(
                            text(f"SELECT id FROM {table} WHERE id IN (:a, :b)"),
                            {"a": rows_a[table], "b": rows_b[table]},
                        ).scalars()
                    )
                    expected = {allowed[table]} if actor is not None else set()
                    assert ids == expected, table
                if commit:
                    session.commit()
                else:
                    session.rollback()
            assert engine.pool.checkedout() == 0
