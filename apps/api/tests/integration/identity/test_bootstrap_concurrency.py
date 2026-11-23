from __future__ import annotations

from concurrent.futures import ThreadPoolExecutor
from threading import Barrier

from sqlalchemy import text

from tests.fixtures.db_admin import get_admin_engine
from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.identity.test_bootstrap import (
    ROOT,
    _authority,
    _payload,
)
from tests.integration.identity.test_bootstrap import (
    uninitialized as uninitialized,
)
from tests.integration.rls.test_cross_tenant_api import client_for_user


def test_bootstrap_two_authenticated_requests_have_one_winner(uninitialized: None) -> None:
    world = seed_access_world()
    barrier = Barrier(2)
    payloads = [_payload(), _payload()]

    def attempt(index: int) -> int:
        user_id = (world.wrong_user_id, world.member_user_id)[index]
        with client_for_user(user_id) as client:
            barrier.wait(timeout=15)
            return client.post(
                f"{ROOT}/initialize", json=payloads[index], headers=_authority()
            ).status_code

    with ThreadPoolExecutor(max_workers=2) as pool:
        assert sorted(pool.map(attempt, range(2))) == [201, 409]
    with get_admin_engine().connect() as connection:
        count = connection.execute(
            text("""
            SELECT count(*) FROM org.organizations WHERE slug IN (:a, :b)
        """),
            {"a": payloads[0]["organization_slug"], "b": payloads[1]["organization_slug"]},
        )
        assert count.scalar_one() == 1
