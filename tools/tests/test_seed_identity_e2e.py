from types import SimpleNamespace
from unittest.mock import Mock

import pytest
from openvietgis_api import config

from tools import seed_identity_e2e

OWNER_URL = "postgresql+psycopg://owner:password@localhost:54330/openvietgis_test"
RUNTIME_URL = (
    "postgresql+psycopg://openvietgis_runtime:password@localhost:54330/openvietgis_test"
)


@pytest.mark.parametrize(
    ("environment", "migration_url", "runtime_url"),
    [
        ("development", OWNER_URL, RUNTIME_URL),
        ("test", OWNER_URL.replace("openvietgis_test", "openvietgis"), RUNTIME_URL),
        ("test", OWNER_URL, RUNTIME_URL.replace("openvietgis_test", "openvietgis_dev")),
        ("test", OWNER_URL.replace("localhost", "live.example.com"), RUNTIME_URL),
        ("test", OWNER_URL, RUNTIME_URL.replace("localhost", "live.example.com")),
        ("test", OWNER_URL, RUNTIME_URL.replace("54330", "5432")),
        ("test", OWNER_URL, RUNTIME_URL + "?dbname=customer"),
        ("test", OWNER_URL, OWNER_URL),
        (
            "test",
            OWNER_URL,
            RUNTIME_URL.replace("openvietgis_runtime", "administrator"),
        ),
    ],
)
@pytest.mark.parametrize("uninitialized", [False, True])
def test_unsafe_seed_never_connects(
    monkeypatch, environment, migration_url, runtime_url, uninitialized
) -> None:
    monkeypatch.setattr(
        config,
        "get_settings",
        lambda: SimpleNamespace(
            environment=environment,
            migration_db_url=migration_url,
            runtime_db_url=runtime_url,
        ),
    )
    connect = Mock()
    monkeypatch.setattr(seed_identity_e2e, "create_engine", connect)
    with pytest.raises(RuntimeError, match="Refusing"):
        seed_identity_e2e.main(uninitialized=uninitialized)
    connect.assert_not_called()


def test_dedicated_seed_target_allowed() -> None:
    seed_identity_e2e.assert_safe_seed_target("test", OWNER_URL, RUNTIME_URL)
