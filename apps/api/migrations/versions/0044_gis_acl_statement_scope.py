"""Evaluate the GIS ACL "is this actor scoped?" check once per statement, not per row.

Each *_gis_acl table has exactly one permissive policy (*_project_scope) pinning
organization_id/project_id to the session settings, so every visible or writable
row has (o, p) equal to those settings and scoped(o, p) == scoped(settings).
Wrapping that in a scalar sub-SELECT lets PostgreSQL run it as an InitPlan once,
instead of one membership lookup per feature row (~45 us/row measured on 30k rows).
"""

from alembic import op

revision = "0044_gis_acl_statement_scope"
down_revision = "0043_chat_rooms"
branch_labels = None
depends_on = None

INTERNAL = "COALESCE(current_setting('openvietgis.acl_internal',true),'')='on'"
UNSCOPED = (
    "(SELECT NOT gis_access.scoped("
    "NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid,"
    "NULLIF(current_setting('openvietgis.project_id',true),'')::uuid))"
)
# Same expressions as 0023_gis_access.
CONDS = {
    "layers": "gis_access.layer_allowed(organization_id,project_id,id)",
    "data_sources": "gis_access.source_allowed(organization_id,project_id,id)",
    "source_fields": "gis_access.source_allowed(organization_id,project_id,source_id)",
    "reference_features": (
        "gis_access.feature_allowed(organization_id,project_id,source_id,"
        "feature_id,attributes,geometry)"
    ),
    "catalog_changes": "NOT gis_access.scoped(organization_id,project_id)",
}


def _recreate(with_hoist: bool) -> None:
    for table, expr in CONDS.items():
        hoist = f" OR {UNSCOPED}" if with_hoist else ""
        cond = f"({INTERNAL}){hoist} OR ({expr})"
        op.execute(f"DROP POLICY {table}_gis_acl ON spatial.{table}")
        op.execute(
            f"CREATE POLICY {table}_gis_acl ON spatial.{table} "
            "AS RESTRICTIVE FOR ALL TO openvietgis_runtime "
            f"USING ({cond}) WITH CHECK ({cond})"
        )


def upgrade() -> None:
    _recreate(with_hoist=True)


def downgrade() -> None:
    _recreate(with_hoist=False)
