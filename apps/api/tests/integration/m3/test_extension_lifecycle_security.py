"""R014 lifecycle authorization and retention acceptance for official modules."""

from __future__ import annotations

from sqlalchemy import text

from tests.fixtures.db_admin import get_admin_engine
from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.spatial.test_authorization import _grant
from tests.integration.spatial.test_sources import _client


def _manifest(version: str = "1.0.0") -> dict[str, object]:
    return {
        "extension_key": "audit_notes",
        "version": version,
        "api_compatibility": "1.x",
        "display_name": "Audit notes",
        "description": "Synthetic lifecycle acceptance module",
        "permissions": ["ext.audit_notes.read", "ext.audit_notes.write"],
        "commands": [],
        "capabilities": ["notes"],
        "entity_schemas": {
            "note": {
                "fields": {
                    "text": {"type": "text", "required": True},
                }
            }
        },
        "ui_contributions": [],
    }


def test_extension_lifecycle_requires_admin_and_project_manage(
    migrated_database: None,
) -> None:
    world = seed_access_world()
    org = f"/api/v1/organizations/{world.organization_id}"
    project = f"{org}/projects/{world.project_id}"

    with (
        _client(world.owner_user_id) as owner,
        _client(world.member_user_id) as member,
    ):
        denied_register = member.post(f"{org}/extensions/manifests", json=_manifest())
        assert denied_register.status_code == 403, denied_register.text

        registered = owner.post(f"{org}/extensions/manifests", json=_manifest())
        assert registered.status_code == 201, registered.text
        manifest_id = registered.json()["id"]

        editor_grant = _grant(
            owner,
            world,
            "membership",
            str(world.member_membership_id),
            "editor",
        )
        denied_activate = member.post(
            f"{project}/extensions/activations",
            json={"manifest_id": manifest_id},
        )
        assert denied_activate.status_code == 403, denied_activate.text

        revoked = owner.post(f"{org}/grants/{editor_grant}/revoke")
        assert revoked.status_code == 204, revoked.text
        _grant(
            owner,
            world,
            "membership",
            str(world.member_membership_id),
            "manager",
        )

        activated = member.post(
            f"{project}/extensions/activations",
            json={"manifest_id": manifest_id},
        )
        assert activated.status_code == 201, activated.text
        assert activated.json()["status"] == "active"

        records = f"{project}/extensions/audit_notes/records"
        made = owner.post(
            records,
            json={
                "entity_name": "note",
                "payload": {"text": "retained after disable"},
            },
        )
        assert made.status_code == 201, made.text
        record_id = made.json()["id"]

        replacement = owner.post(
            f"{org}/extensions/manifests",
            json=_manifest("2.0.0"),
        )
        assert replacement.status_code == 201, replacement.text
        blocked_upgrade = member.post(
            f"{project}/extensions/activations",
            json={"manifest_id": replacement.json()["id"]},
        )
        assert blocked_upgrade.status_code == 409, blocked_upgrade.text
        assert blocked_upgrade.json()["code"] == "EXTENSION_VERSION_TRANSITION_REQUIRED"

        disabled = member.post(f"{project}/extensions/audit_notes/disable")
        assert disabled.status_code == 200, disabled.text
        assert disabled.json()["status"] == "disabled"

        # Disabling the runtime removes mutation authority, not authorized
        # access to retained Core records (same contract as deactivate).
        inactive_read = owner.get(records)
        assert inactive_read.status_code == 200, inactive_read.text
        assert [row["id"] for row in inactive_read.json()["items"]] == [record_id]
        inactive_write = owner.post(
            records, json={"entity_name": "note", "payload": {"text": "must not be saved"}}
        )
        assert inactive_write.status_code == 404, inactive_write.text
        foreign = seed_access_world()
        with _client(foreign.owner_user_id) as outsider:
            assert outsider.get(records).status_code in (403, 404)

        reactivated = member.post(
            f"{project}/extensions/activations",
            json={"manifest_id": manifest_id},
        )
        assert reactivated.status_code == 201, reactivated.text
        assert reactivated.json()["status"] == "active"
        assert reactivated.json()["revision"] > activated.json()["revision"]

        retained = owner.get(records)
        assert retained.status_code == 200, retained.text
        assert [row["id"] for row in retained.json()["items"]] == [record_id]

    with get_admin_engine().connect() as connection:
        actions = (
            connection.execute(
                text(
                    """
                SELECT action
                FROM audit.audit_events
                WHERE organization_id=:org
                  AND resource_type='extension_activation'
                ORDER BY created_at,id
                """
                ),
                {"org": world.organization_id},
            )
            .scalars()
            .all()
        )
    assert actions.count("extension.activate") == 2
    assert actions.count("extension.disable") == 1
