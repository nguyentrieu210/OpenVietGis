from uuid import uuid4

from sqlalchemy import text

from tests.fixtures.db_admin import get_admin_engine
from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.spatial.test_sources import _client


def test_feature_leases_enforce_core_writes_and_expire(migrated_database):
    world = seed_access_world()
    org = f"/api/v1/organizations/{world.organization_id}"
    root = f"{org}/projects/{world.project_id}"
    locks = root + "/collaboration/edit-locks"
    with (
        _client(world.owner_user_id) as owner,
        _client(world.member_user_id) as member,
        _client(world.wrong_user_id) as outsider,
    ):

        def check(r, status=200):
            assert r.status_code == status, r.text
            return r.json()

        check(
            owner.post(
                org + "/grants",
                json={
                    "subject_kind": "membership",
                    "subject_id": str(world.member_membership_id),
                    "scope_kind": "project",
                    "project_id": str(world.project_id),
                    "role": "editor",
                },
            ),
            201,
        )
        layer = check(
            owner.post(
                root + "/layers/manual",
                json={
                    "operation_id": str(uuid4()),
                    "name": "Lease test",
                    "geometry_family": "Point",
                    "fields": [
                        {"field_id": "nm", "name": "Name", "type": "text", "nullable": False}
                    ],
                },
            ),
            201,
        )
        src = layer["source_id"]

        def commit(client, operations):
            source = check(client.get(root + "/layers/" + layer["layer_id"]))["source"]
            return client.post(
                root + "/sources/" + src + "/changesets",
                json={
                    "operation_id": str(uuid4()),
                    "base_content_revision": source["content_revision"],
                    "base_schema_revision": source["schema_revision"],
                    "summary": "Edit lease test",
                    "operations": operations,
                },
            )

        check(
            commit(
                owner,
                [
                    {
                        "op_type": "create",
                        "feature_id": f,
                        "geometry": {"type": "Point", "coordinates": [106.5, 10.7]},
                        "properties": {"nm": f},
                    }
                    for f in ("A", "B")
                ],
            ),
            201,
        )
        a = {"source_id": src, "feature_id": "A", "token": str(uuid4())}
        b = a | {"token": str(uuid4())}
        assert outsider.get(locks).status_code in (403, 404)
        assert outsider.post(locks, json=a).status_code in (403, 404)
        acquired = check(owner.post(locks, json=a))
        assert acquired["actor_id"] == str(world.owner_user_id)
        assert "token" not in acquired
        assert check(member.get(locks))["items"][0]["actor_id"] == str(world.owner_user_id)
        assert check(owner.post(locks, json=a))["expires_at"] >= acquired["expires_at"]
        assert owner.post(locks, json=b).status_code == 409
        assert member.post(locks, json=b).status_code == 409
        check(member.post(locks + "/release", json=b))
        check(owner.post(locks + "/release", json=b))
        assert len(check(owner.get(locks))["items"]) == 1
        change = {
            "op_type": "update",
            "feature_id": "A",
            "geometry": {"type": "Point", "coordinates": [106.6, 10.7]},
        }
        result = commit(member, [change])
        assert result.status_code == 409 and "FEATURE_LOCKED" in result.text, result.text
        check(commit(member, [change | {"feature_id": "B"}]), 201)
        check(commit(owner, [change]), 201)
        assert check(owner.get(locks))["items"] == []
        check(owner.post(locks, json=a))
        check(owner.post(locks + "/release", json=a))
        check(member.post(locks, json=b))
        with get_admin_engine().begin() as db:
            db.execute(
                text(
                    "UPDATE collaboration.feature_edit_leases SET "
                    "expires_at=clock_timestamp()-interval '1 second' WHERE project_id=:p"
                ),
                {"p": world.project_id},
            )
        assert check(owner.get(locks))["items"] == []
        check(owner.post(locks, json=a))
        assert check(member.get(locks))["items"][0]["actor_id"] == str(world.owner_user_id)
        # Scoped users cannot enumerate leases, but Core cannot bypass a lock.
        check(
            owner.post(
                root + "/access/rules",
                json={
                    "user_id": str(world.member_user_id),
                    "layer_id": layer["layer_id"],
                    "actions": ["read", "geometry"],
                    "attribute_filter": {"nm": "A"},
                },
            ),
            201,
        )
        assert member.get(locks).status_code == 403
        result = commit(
            member, [change | {"geometry": {"type": "Point", "coordinates": [106.7, 10.7]}}]
        )
        assert result.status_code == 409 and "FEATURE_LOCKED" in result.text, result.text
        check(owner.post(locks + "/release", json=a))
        check(
            commit(
                member, [change | {"geometry": {"type": "Point", "coordinates": [106.7, 10.7]}}]
            ),
            201,
        )
