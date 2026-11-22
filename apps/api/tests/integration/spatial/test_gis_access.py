from uuid import uuid4

from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.spatial.test_layers import _path
from tests.integration.spatial.test_sources import _client


def test_scoped_layer_features_and_editing(migrated_database):
    world = seed_access_world()
    path = _path(world)
    root = path.rsplit("/", 1)[0]
    org = root.split("/projects/")[0]
    with _client(world.owner_user_id) as admin, _client(world.member_user_id) as member:

        def check(r, status=200):
            assert r.status_code == status, r.text
            return r.json()

        def layer(name):
            return check(
                admin.post(
                    path + "/manual",
                    json={
                        "operation_id": str(uuid4()),
                        "name": name,
                        "geometry_family": "Point",
                        "fields": [
                            {"field_id": "nm", "name": "Tên", "type": "text", "nullable": False}
                        ],
                    },
                ),
                201,
            )

        first = layer("Allowed")
        second = layer("Hidden")

        def info(client, layer_record):
            return check(client.get(path + "/" + layer_record["layer_id"]))["source"]

        def commit(client, layer_record, ops, expected=201):
            src = info(client, layer_record)
            return check(
                client.post(
                    root + "/sources/" + layer_record["source_id"] + "/changesets",
                    json={
                        "operation_id": str(uuid4()),
                        "base_content_revision": src["content_revision"],
                        "base_schema_revision": src["schema_revision"],
                        "summary": "ACL test",
                        "operations": ops,
                    },
                ),
                expected,
            )

        for layer_record in (first, second):
            commit(
                admin,
                layer_record,
                [
                    {
                        "op_type": "create",
                        "feature_id": a,
                        "geometry": {"type": "Point", "coordinates": coords},
                        "properties": {"nm": a},
                    }
                    for a, coords in [("A", [106.5, 10.7]), ("B", [106.6, 10.7])]
                ],
            )
        check(
            admin.post(
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
        r = check(
            admin.post(
                root + "/access/rules",
                json={
                    "user_id": str(world.member_user_id),
                    "layer_id": first["layer_id"],
                    "actions": ["read", "geometry", "attributes", "export"],
                    "attribute_filter": {"nm": "A"},
                },
            ),
            201,
        )
        assert check(member.get(root + "/access/mine"))["restricted"]
        assert [layer_record["id"] for layer_record in check(member.get(path))["items"]] == [
            first["layer_id"]
        ]
        assert member.get(path + "/" + second["layer_id"]).status_code == 404
        assert info(member, first)["feature_count"] == 1

        def features(client, layer_record):
            src = info(client, layer_record)
            return check(
                client.get(
                    path + "/" + layer_record["layer_id"] + "/features",
                    params={
                        "content_revision": src["content_revision"],
                        "schema_revision": src["schema_revision"],
                        "include_geometry": "true",
                        "response_crs": 4326,
                    },
                )
            )["items"]

        assert [x["feature_id"] for x in features(member, first)] == ["A"]
        commit(
            member,
            first,
            [
                {
                    "op_type": "update",
                    "feature_id": "B",
                    "geometry": {"type": "Point", "coordinates": [106.7, 10.7]},
                }
            ],
            403,
        )
        commit(
            member,
            first,
            [{"op_type": "update", "feature_id": "A", "properties": {"nm": "B"}}],
            403,
        )
        commit(
            member,
            first,
            [
                {
                    "op_type": "update",
                    "feature_id": "A",
                    "geometry": {"type": "Point", "coordinates": [106.51, 10.7]},
                }
            ],
        )
        assert info(admin, first)["feature_count"] == 2
        assert len(features(admin, first)) == 2
        export = check(member.get(root + "/access/export/" + first["layer_id"]))
        assert [f["id"] for f in export["features"]] == ["A"]
        assert member.get(root + "/access").status_code == 403
        assert member.get(root + "/extensions/activations").json() == {"items": []}
        assert admin.delete(root + "/access/rules/" + r["id"]).status_code == 204
        assert check(member.get(path))["items"] == []
        assert len(check(admin.get(path))["items"]) == 2

        group = check(
            admin.post(
                root + "/access/groups",
                json={"name": "Survey A", "user_ids": [str(world.member_user_id)]},
            ),
            201,
        )
        check(
            admin.post(
                root + "/access/rules",
                json={
                    "group_id": group["id"],
                    "layer_id": first["layer_id"],
                    "actions": ["read", "geometry", "approve"],
                    "region": {
                        "type": "Polygon",
                        "coordinates": [
                            [
                                [106.4, 10.6],
                                [106.55, 10.6],
                                [106.55, 10.8],
                                [106.4, 10.8],
                                [106.4, 10.6],
                            ]
                        ],
                    },
                },
            ),
            201,
        )
        assert [x["feature_id"] for x in features(member, first)] == ["A"]
        commit(
            member,
            first,
            [
                {
                    "op_type": "update",
                    "feature_id": "A",
                    "geometry": {"type": "Point", "coordinates": [106.7, 10.7]},
                }
            ],
            403,
        )
        admin_receipt = commit(
            admin,
            first,
            [
                {
                    "op_type": "update",
                    "feature_id": "A",
                    "geometry": {"type": "Point", "coordinates": [106.52, 10.7]},
                }
            ],
        )
        previews = check(member.get(root + "/access/reviews"))["items"]
        assert admin_receipt["id"] in {r["id"] for r in previews}
        check(
            member.post(
                root + "/access/reviews/" + admin_receipt["id"],
                json={"decision": "approved", "note": "checked"},
            ),
            201,
        )
        assert admin.get(root + "/access").status_code == 200
        assert member.get(root + "/access/export/" + first["layer_id"]).status_code == 403
        check(
            admin.put(
                root + "/access/groups/" + group["id"], json={"name": "Survey A", "user_ids": []}
            )
        )
        assert check(member.get(path))["items"] == []
