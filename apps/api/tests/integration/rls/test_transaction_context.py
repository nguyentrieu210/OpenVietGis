from __future__ import annotations

from uuid import uuid4

from sqlalchemy import text
from sqlalchemy.orm import Session

from openvietgis_api.adapters.persistence.db import get_engine
from openvietgis_api.adapters.persistence.rls import set_request_context


def test_transaction_local_context_does_not_leak_across_reused_connection(
    migrated_database: None,
) -> None:
    org_id = uuid4()
    actor_id = uuid4()

    with get_engine().connect() as connection:
        first = connection.begin()
        session = Session(bind=connection)
        set_request_context(session, organization_id=org_id, actor_user_id=actor_id)
        assert connection.execute(
            text("SELECT current_setting('openvietgis.organization_id', true)")
        ).scalar_one() == str(org_id)
        first.commit()
        session.close()

        second = connection.begin()
        try:
            assert (
                connection.execute(
                    text("SELECT current_setting('openvietgis.organization_id', true)")
                ).scalar_one()
                == ""
            )
            assert (
                connection.execute(
                    text("SELECT current_setting('openvietgis.actor_user_id', true)")
                ).scalar_one()
                == ""
            )
        finally:
            second.rollback()
