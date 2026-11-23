from __future__ import annotations

from collections.abc import Mapping

from sqlalchemy.engine import make_url

from tools.reset_dev import assert_safe_target

DEFAULT_TEST_DATABASE_URL = (
    "postgresql+psycopg://openvietgis:openvietgis@localhost:54330/openvietgis_test"
)


def assert_safe_test_target(database_url: str) -> None:
    assert_safe_target("test", database_url)
    if make_url(database_url).database != "openvietgis_test":
        raise RuntimeError(
            "Refusing tests outside the dedicated openvietgis_test database"
        )


def build_test_environment(environ: Mapping[str, str]) -> dict[str, str]:
    database_url = environ.get(
        "OPENVIETGIS_TEST_DATABASE_URL", DEFAULT_TEST_DATABASE_URL
    )
    assert_safe_test_target(database_url)
    owner_url = make_url(database_url)
    runtime_url = owner_url.set(
        username="openvietgis_runtime", password="openvietgis-runtime-dev"
    ).render_as_string(hide_password=False)
    assert_safe_test_target(runtime_url)
    return {
        **environ,
        "OPENVIETGIS_ENVIRONMENT": "test",
        "OPENVIETGIS_DATABASE_URL": database_url,
        "OPENVIETGIS_TEST_DATABASE_URL": database_url,
        "OPENVIETGIS_MIGRATION_DATABASE_URL": database_url,
        "OPENVIETGIS_RUNTIME_DATABASE_URL": runtime_url,
        "OPENVIETGIS_APPLICATION_SESSION_SECRET": "openvietgis-test-session-secret",
        "OPENVIETGIS_CSRF_SECRET": "openvietgis-test-csrf-secret",
        "OPENVIETGIS_BOOTSTRAP_SECRET": "openvietgis-test-bootstrap-secret",
    }
