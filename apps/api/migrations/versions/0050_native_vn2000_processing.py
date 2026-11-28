"""Extend storage only for bounded same-datum VN-2000 processing outputs."""
import re

import sqlalchemy as sa
from alembic import op

revision = "0050_native_vn2000_processing"
down_revision = "0049_relation_constraints"
branch_labels = None
depends_on = None

OLD = (4326,3857,4756,5899)
DEFINITION = (*OLD,3405,3406)
NEW = (
    *OLD,
    3405, 3406, 5896, 5897, 5898,
    9205, 9206, 9207, 9208, 9209, 9210, 9211,
    9212, 9213, 9214, 9215, 9216, 9217, 9218,
)
EXTRA = (
    3405, 3406, 5896, 5897, 5898,
    9205, 9206, 9207, 9208, 9209, 9210, 9211,
    9212, 9213, 9214, 9215, 9216, 9217, 9218,
)

def _codes(codes):
    return ", ".join(map(str,codes))

def _constraint(table, name, before, after):
    connection = op.get_bind()
    definition = connection.execute(sa.text(
        "SELECT pg_get_constraintdef(oid) FROM pg_constraint "
        "WHERE conrelid=CAST(:table AS regclass) AND conname=:name"
    ), {"table":table,"name":name}).scalar_one()
    pattern = r"ARRAY\[\s*" + r"\s*,\s*".join(map(str,before)) + r"\s*\]"
    updated, count = re.subn(pattern, "ARRAY["+_codes(after)+"]", definition)
    if not count:
        raise RuntimeError("Unexpected CRS constraint: "+name)
    op.execute(f"ALTER TABLE {table} DROP CONSTRAINT {name}")
    op.execute(f"ALTER TABLE {table} ADD CONSTRAINT {name} {updated}")

def _extent(before, after):
    definition=op.get_bind().execute(sa.text(
        "SELECT pg_get_functiondef('spatial.valid_extent(jsonb,integer)'::regprocedure)"
    )).scalar_one()
    pattern=r"native_code\s+IN\s*\(\s*"+r"\s*,\s*".join(map(str,before))+r"\s*\)"
    updated,count=re.subn(pattern,"native_code IN ("+_codes(after)+")",definition,flags=re.I)
    if count!=1:
        raise RuntimeError("Unexpected spatial.valid_extent definition")
    op.execute(updated)

def upgrade():
    _extent(OLD,NEW)
    _constraint("spatial.data_sources","ck_source_crs",DEFINITION,NEW)
    _constraint("spatial.data_sources","ck_stored_crs",DEFINITION,NEW)
    _constraint("spatial.data_sources","ck_source_ready_metadata",OLD,NEW)
    _constraint("processing.jobs","ck_processing_crs",OLD,NEW)
    extra=_codes(EXTRA)
    op.execute(f"""ALTER TABLE spatial.data_sources ADD CONSTRAINT ck_native_vn2000_processing_only
        CHECK (validation_state<>'ready' OR (COALESCE(source_crs NOT IN ({extra}),false)
        AND COALESCE(stored_crs NOT IN ({extra}),false))
        OR (kind='derived_processing' AND source_crs=stored_crs))""")

def downgrade():
    extra=_codes(EXTRA)
    if op.get_bind().execute(sa.text(f"""SELECT EXISTS(SELECT 1 FROM spatial.data_sources
        WHERE source_crs IN ({extra}) OR stored_crs IN ({extra}))
        OR EXISTS(SELECT 1 FROM processing.jobs WHERE output_crs IN ({extra}))""")).scalar_one():
        raise RuntimeError("Remove extended VN-2000 outputs and receipts before downgrade")
    op.execute("ALTER TABLE spatial.data_sources DROP CONSTRAINT ck_native_vn2000_processing_only")
    _constraint("processing.jobs","ck_processing_crs",NEW,OLD)
    _constraint("spatial.data_sources","ck_source_ready_metadata",NEW,OLD)
    _constraint("spatial.data_sources","ck_stored_crs",NEW,DEFINITION)
    _constraint("spatial.data_sources","ck_source_crs",NEW,DEFINITION)
    _extent(NEW,OLD)
