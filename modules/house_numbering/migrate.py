"""Module-owned migration entry point with an independent Alembic version table."""

from __future__ import annotations

import argparse
from pathlib import Path

from alembic import command
from alembic.config import Config
from openvietgis_api.config import get_settings
from sqlalchemy import create_engine, text

MIGRATIONS = Path(__file__).resolve().parent / "migrations"


def alembic_config() -> Config:
    config = Config()
    config.set_main_option("script_location", str(MIGRATIONS))
    return config


def _require_core_contract() -> None:
    with create_engine(get_settings().migration_db_url).connect() as connection:
        ready = connection.execute(
            text("""
                SELECT to_regclass('extensions.project_activations') IS NOT NULL
                   AND EXISTS (
                       SELECT 1 FROM information_schema.columns
                       WHERE table_schema='extensions' AND table_name='project_activations'
                         AND column_name='revision'
                   )
            """)
        ).scalar_one()
        if ready is not True:
            raise RuntimeError(
                "House Numbering requires the promoted R014 extension lifecycle contract"
            )


def upgrade() -> None:
    _require_core_contract()
    command.upgrade(alembic_config(), "head")


def downgrade() -> None:
    command.downgrade(alembic_config(), "base")


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Migrate the House Numbering module schema."
    )
    parser.add_argument(
        "action", choices=("upgrade", "downgrade"), nargs="?", default="upgrade"
    )
    args = parser.parse_args()
    if args.action == "downgrade":
        downgrade()
    else:
        upgrade()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
