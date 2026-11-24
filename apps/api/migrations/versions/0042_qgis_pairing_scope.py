"""Permit an authenticated QGIS device to read only its own active pairing binding."""
from alembic import op

revision = "0042_qgis_pairing_scope"
down_revision = "0041_r029_managed_realtime"
branch_labels = None
depends_on = None


def upgrade() -> None:
    scope = """organization_id = NULLIF(
        current_setting('openvietgis.organization_id', true), '')::uuid
        AND project_id = NULLIF(current_setting('openvietgis.project_id', true), '')::uuid
        AND NULLIF(current_setting('openvietgis.actor_user_id', true), '') IS NULL"""
    principal = "NULLIF(current_setting('openvietgis.qgis_pairing_principal', true), '')::uuid"
    credential = "NULLIF(current_setting('openvietgis.qgis_pairing_credential', true), '')::uuid"
    op.execute(f"""
        CREATE POLICY qgis_device_pairing_read ON integration_hub.binding_pairing_tokens
        FOR SELECT TO openvietgis_runtime USING (
            {scope} AND service_principal_id = {principal}
            AND service_credential_id = {credential}
            AND revoked_at IS NULL AND expires_at > now()
        )
    """)
    op.execute(f"""
        CREATE POLICY qgis_device_binding_read ON integration_hub.connector_bindings
        FOR SELECT TO openvietgis_runtime USING (
            {scope} AND state = 'active' AND service_principal_id = {principal}
            AND EXISTS (
                SELECT 1 FROM integration_hub.binding_pairing_tokens AS tok
                WHERE tok.organization_id = connector_bindings.organization_id
                  AND tok.project_id = connector_bindings.project_id
                  AND tok.binding_id = connector_bindings.id
                  AND tok.service_principal_id = {principal}
                  AND tok.service_credential_id = {credential}
                  AND tok.revoked_at IS NULL AND tok.expires_at > now()
            )
        )
    """)


def downgrade() -> None:
    op.execute("DROP POLICY qgis_device_binding_read ON integration_hub.connector_bindings")
    op.execute("DROP POLICY qgis_device_pairing_read ON integration_hub.binding_pairing_tokens")
