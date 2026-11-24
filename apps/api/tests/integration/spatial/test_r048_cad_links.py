"""R048 existing CAD identity and ChangeSet acceptance on isolated PostGIS."""

from uuid import uuid4

from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.spatial.test_cad_georef import _drawing
from tests.integration.spatial.test_sources import _client


def test_cad_link_cardinality_and_revision_pinning(migrated_database):
    world = seed_access_world()
    root = f"/api/v1/organizations/{world.organization_id}/projects/{world.project_id}"
    base = root + "/cad/documents"
    control = {
        "crs_code": 3857,
        "units": "millimeter",
        "tolerance_m": 0.001,
        "max_scale_deviation": 0.001,
        "controls": [
            [[0, 0], [11800000, 1200000]],
            [[10000, 0], [11800000, 1200010]],
            [[0, 5000], [11799995, 1200000]],
        ],
    }

    with _client(world.owner_user_id) as owner:

        def check(response, status=200):
            assert response.status_code == status, response.text
            return response.json()

        document = check(owner.post(base, files={"file": ("r048.dxf", _drawing())}), 201)
        path = f"{base}/{document['id']}/versions/1"
        sources = []
        for shift in (0, 20):
            body = control | {
                "controls": [[a, [b[0] + shift, b[1]]] for a, b in control["controls"]]
            }
            inspected = check(owner.post(path + "/inspect-georef", json=body))
            mapping = inspected["entity_summary"]["entity_mapping"][0]
            receipt = check(
                owner.post(
                    path + "/import-georef",
                    json=body
                    | {
                        "layer_name": f"R048 link {shift}",
                        "expected_sha256": document["sha256"],
                        "expected_inspection_fingerprint": inspected["inspection_fingerprint"],
                        "acknowledge_loss": True,
                    },
                ),
                201,
            )
            sources.append(receipt)
        handle, fid = mapping["handle"], mapping["feature_id"]
        links = check(owner.get(path + "/links", params={"handle": handle.lower()}))["items"]
        assert len(links) == 2
        assert {x["source_id"] for x in links} == {x["source_id"] for x in sources}
        assert all(x["version"] == 1 and x["original_revision"] for x in links)
        for source in sources:
            reverse = check(
                owner.get(
                    base + "/links",
                    params={
                        "source_id": source["source_id"],
                        "feature_id": fid,
                    },
                )
            )["items"]
            assert len(reverse) == 1
            assert reverse[0]["handle"] == handle
        first = sources[0]
        layer = check(owner.get(root + "/layers/" + first["layer_id"]))["source"]
        changes = root + "/sources/" + first["source_id"] + "/changesets"
        update = {
            "operation_id": str(uuid4()),
            "base_content_revision": layer["content_revision"],
            "base_schema_revision": layer["schema_revision"],
            "summary": "R048 synthetic geometry edit",
            "operations": [
                {
                    "op_type": "update",
                    "feature_id": fid,
                    "geometry": {
                        "type": "Polygon",
                        "coordinates": [
                            [
                                [11800000, 1200000],
                                [11800000, 1200011],
                                [11799995, 1200011],
                                [11799995, 1200000],
                                [11800000, 1200000],
                            ]
                        ],
                    },
                }
            ],
        }
        accepted = check(owner.post(changes, json=update), 201)
        replay = owner.post(changes, json=update)
        assert replay.status_code in (200, 201), replay.text
        assert replay.json()["id"] == accepted["id"]
        assert (
            owner.post(changes, json=update | {"summary": "different payload"}).status_code == 409
        )
        assert owner.post(changes, json=update | {"operation_id": str(uuid4())}).status_code == 409
        linked = check(owner.get(path + "/links", params={"handle": handle}))["items"]
        by_source = {x["source_id"]: x for x in linked}
        assert by_source[first["source_id"]]["original_revision"] is False
        assert by_source[sources[1]["source_id"]]["original_revision"] is True
        version = check(
            owner.post(
                base + "/" + document["id"] + "/versions",
                files={"file": ("r048.dxf", _drawing())},
                data={"expected_version": 1},
            ),
            201,
        )
        assert version["version"] == 2
        assert (
            check(
                owner.get(
                    path.replace("/versions/1", "/versions/2") + "/links", params={"handle": handle}
                )
            )["items"]
            == []
        )
        assert len(check(owner.get(path + "/links", params={"handle": handle}))["items"]) == 2
        assert (
            owner.post(
                base + "/" + document["id"] + "/versions",
                files={"file": ("r048.dxf", _drawing())},
                data={"expected_version": 1},
            ).status_code
            == 409
        )
        latest = check(owner.get(root + "/layers/" + first["layer_id"]))["source"]
        check(
            owner.post(
                changes,
                json={
                    "operation_id": str(uuid4()),
                    "summary": "Delete synthetic feature",
                    "base_content_revision": latest["content_revision"],
                    "base_schema_revision": latest["schema_revision"],
                    "operations": [{"op_type": "delete", "feature_id": fid}],
                },
            ),
            201,
        )
        after = check(owner.get(path + "/links", params={"handle": handle}))["items"]
        assert len(after) == 1 and after[0]["source_id"] == sources[1]["source_id"]
    with _client(world.wrong_user_id) as outsider:
        assert outsider.get(path + "/links", params={"handle": handle}).status_code in (403, 404)
