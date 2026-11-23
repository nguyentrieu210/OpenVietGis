from collections.abc import Iterator
from functools import lru_cache

from sqlalchemy import create_engine, event
from sqlalchemy.engine import Engine
from sqlalchemy.orm import Session, sessionmaker

from openvietgis_api.config import get_settings


@lru_cache
def get_engine() -> Engine:
    engine = create_engine(get_settings().runtime_db_url, pool_pre_ping=True)

    @event.listens_for(engine, "connect")
    def verify_runtime_role(dbapi_connection: object, _: object) -> None:
        with dbapi_connection.cursor() as cursor:  # type: ignore[attr-defined]
            cursor.execute(
                """
                SELECT current_user = 'openvietgis_runtime'
                    AND NOT EXISTS (
                        SELECT 1 FROM pg_roles
                        WHERE pg_has_role(current_user, oid, 'MEMBER')
                          AND (rolsuper OR rolbypassrls OR rolcreaterole OR rolcreatedb)
                    )
                    AND NOT EXISTS (
                        SELECT 1 FROM pg_class c JOIN pg_namespace n
                        ON c.relnamespace = n.oid
                        WHERE n.nspname IN ('identity', 'org', 'workspace', 'audit', 'platform')
                          AND c.relkind IN ('r', 'p')
                          AND pg_has_role(current_user, c.relowner, 'MEMBER')
                    )
                """
            )
            valid = cursor.fetchone()[0]
        dbapi_connection.rollback()  # type: ignore[attr-defined]
        if not valid:
            raise RuntimeError("Unsafe database role: restricted runtime identity required")

    return engine


@lru_cache
def get_session_factory() -> sessionmaker[Session]:
    return sessionmaker(bind=get_engine(), expire_on_commit=False)


def session_scope() -> Iterator[Session]:
    session = get_session_factory()()
    try:
        yield session
        session.commit()
    except Exception:
        session.rollback()
        raise
    finally:
        session.close()
