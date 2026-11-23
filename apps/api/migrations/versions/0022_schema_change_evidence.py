"""A separate catalog operation for revision-pinned schema changes."""

from alembic import op

revision = "0022_schema_change_evidence"
down_revision = "0021_professional_tools"
branch_labels = None
depends_on = None


def _constraint(schema):
    extra = ", 'update_schema'" if schema else ""
    op.execute("ALTER TABLE spatial.catalog_changes DROP CONSTRAINT ck_change_resource")
    op.execute(
        (
            "ALTER TABLE spatial.catalog_changes ADD CONSTRAINT "
            "ck_change_resource CHECK (COALESCE(\n        "
            "(resource_type='data_source' AND source_id=resource_id AND "
            "layer_id IS NULL\n        AND operation IN "
            "('register_source','import_source','processing_publish','edi"
            "t_features','update_source','correct_source_crs','retire_sou"
            "rce','refresh_reference_fixture'"
        )
        + extra
        + """))
        OR (resource_type='layer' AND layer_id=resource_id AND source_id IS NULL
        AND operation IN ('create_layer','rename_layer','retire_layer')), false))"""
    )


def upgrade():
    _constraint(True)


def downgrade():
    _constraint(False)
