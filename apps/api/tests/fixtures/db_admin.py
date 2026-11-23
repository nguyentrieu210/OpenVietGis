"""Dedicated test-database setup/assertion access, never application runtime."""

from functools import lru_cache

from sqlalchemy import create_engine
from sqlalchemy.engine import Engine
from sqlalchemy.orm import Session, sessionmaker
from tools.test_database import assert_safe_test_target

from openvietgis_api.config import get_settings


@lru_cache
def get_admin_engine() -> Engine:
    url = get_settings().migration_db_url
    assert_safe_test_target(url)
    return create_engine(url, pool_pre_ping=True)


@lru_cache
def get_admin_session_factory() -> sessionmaker[Session]:
    return sessionmaker(bind=get_admin_engine(), expire_on_commit=False)


def clear_admin_caches() -> None:
    if get_admin_engine.cache_info().currsize:
        get_admin_engine().dispose()
    get_admin_session_factory.cache_clear()
    get_admin_engine.cache_clear()
