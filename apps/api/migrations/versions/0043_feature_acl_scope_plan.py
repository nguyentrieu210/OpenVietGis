"""Resolve the active project's feature-scope membership once per statement."""

from alembic import op

revision = "0043_feature_acl_scope_plan"
down_revision = "0042_qgis_pairing_scope"
branch_labels = None
depends_on = None

_INTERNAL = "COALESCE(current_setting('openvietgis.acl_internal',true),'')='on'"
_FEATURE_ALLOWED = (
    "gis_access.feature_allowed(organization_id,project_id,source_id,"
    "feature_id,attributes,geometry)"
)
_ACTIVE_ORG = "NULLIF(current_setting('openvietgis.organization_id',true),'')::uuid"
_ACTIVE_PROJECT = "NULLIF(current_setting('openvietgis.project_id',true),'')::uuid"


def upgrade() -> None:
    # This shortcut represents the first branch of feature_allowed(). It applies
    # only when the row exactly matches the active scope. Other contexts still
    # use the row's original policy, as do all WITH CHECK evaluations.
    op.execute(f"""
        ALTER POLICY reference_features_gis_acl ON spatial.reference_features
        USING (
            CASE
                WHEN (SELECT {_INTERNAL}) THEN TRUE
                WHEN organization_id = (SELECT {_ACTIVE_ORG})
                    AND project_id = (SELECT {_ACTIVE_PROJECT})
                    AND NOT (SELECT gis_access.scoped({_ACTIVE_ORG}, {_ACTIVE_PROJECT}))
                    THEN TRUE
                ELSE {_FEATURE_ALLOWED}
            END
        )
    """)


def downgrade() -> None:
    op.execute(f"""
        ALTER POLICY reference_features_gis_acl ON spatial.reference_features
        USING (({_INTERNAL}) OR ({_FEATURE_ALLOWED}))
    """)
