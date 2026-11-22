from uuid import uuid4

from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.spatial.test_sources import _client

DXF = b"0\nSECTION\n2\nENTITIES\n0\nENDSEC\n0\nEOF\n"


def test_cad_versions_are_private_and_immutable(migrated_database):
    world = seed_access_world()
    base = (
        f"/api/v1/organizations/{world.organization_id}/projects/{world.project_id}/cad/documents"
    )
    with _client(world.owner_user_id) as owner:
        assert owner.get(base).json() == {"items": []}
        response = owner.post(base, files={"file": ("test.dxf", DXF)})
        assert response.status_code == 201, response.text
        doc = response.json()
        assert doc["version"] == 1
        path = f"{base}/{doc['id']}/versions"
        response = owner.post(
            path, files={"file": ("test.dxf", DXF + b"\n")}, data={"expected_version": 1}
        )
        assert response.status_code == 201, response.text
        assert response.json()["version"] == 2
        assert (
            owner.post(
                path, files={"file": ("test.dxf", DXF)}, data={"expected_version": 1}
            ).status_code
            == 409
        )
        assert owner.post(base, files={"file": ("test.dxf", b"wrong")}).status_code == 422
        assert [x["version"] for x in owner.get(path).json()["items"]] == [2, 1]
        assert owner.get(path + "/1/download").content == DXF
        assert owner.get(path + "/2/download").content == DXF + b"\n"
        assert owner.get(base).json()["items"][0]["version"] == 2
    root = base.rsplit("/cad/documents", 1)[0]
    org = root.split("/projects/")[0]
    with _client(world.owner_user_id) as owner, _client(world.member_user_id) as member:
        grant = owner.post(
            org + "/grants",
            json={
                "subject_kind": "membership",
                "subject_id": str(world.member_membership_id),
                "scope_kind": "project",
                "project_id": str(world.project_id),
                "role": "editor",
            },
        )
        assert grant.status_code == 201, grant.text
        layer = owner.post(
            root + "/layers/manual",
            json={
                "operation_id": str(uuid4()),
                "name": "Scoped GIS",
                "geometry_family": "Point",
                "fields": [{"field_id": "nm", "name": "Name", "type": "text", "nullable": False}],
            },
        )
        assert layer.status_code == 201, layer.text
        rule = owner.post(
            root + "/access/rules",
            json={
                "user_id": str(world.member_user_id),
                "layer_id": layer.json()["layer_id"],
                "actions": ["read", "geometry", "attributes"],
            },
        )
        assert rule.status_code == 201, rule.text
        assert member.get(root + "/access/mine").json()["restricted"]
        for url in (base, path, path + "/1/download"):
            assert member.get(url).status_code == 403
        assert member.post(base, files={"file": ("test.dxf", DXF)}).status_code == 403
    with _client(world.wrong_user_id) as outsider:
        for url in (base, path, path + "/1/download"):
            assert outsider.get(url).status_code in (403, 404)
        assert outsider.post(base, files={"file": ("test.dxf", DXF)}).status_code in (403, 404)
