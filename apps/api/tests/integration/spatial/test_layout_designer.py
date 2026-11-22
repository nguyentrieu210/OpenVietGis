import copy
from uuid import uuid4

from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.spatial.test_sources import _client


def test_layout_designer_revision_scope_and_geometry(migrated_database):
    world = seed_access_world()
    base = (
        "/api/v1/organizations/"
        f"{world.organization_id}"
        "/projects/"
        f"{world.project_id}"
        "/cartography/designer"
    )
    camera = {"lng": 106.66, "lat": 10.70, "zoom": 14, "bearing": 10, "reference_width": 900}
    a, b = str(uuid4()), str(uuid4())
    doc = {
        "version": 1,
        "paper": "A4",
        "landscape": True,
        "dpi": 300,
        "items": [
            {
                "id": a,
                "kind": "map",
                "x": 5,
                "y": 20,
                "w": 150,
                "h": 140,
                "camera": camera,
                "grid": True,
            },
            {
                "id": b,
                "kind": "map",
                "x": 160,
                "y": 20,
                "w": 130,
                "h": 140,
                "camera": {**camera, "zoom": 17},
            },
            {"id": str(uuid4()), "kind": "scale", "x": 5, "y": 175, "w": 110, "h": 15, "map_id": a},
            {
                "id": str(uuid4()),
                "kind": "text",
                "x": 5,
                "y": 3,
                "w": 280,
                "h": 15,
                "text": "BẢN ĐỒ BÌNH HƯNG",
                "font_size": 6,
            },
        ],
    }
    payload = {"name": "Layout 2 frames", "document": doc}
    with _client(world.owner_user_id) as c:
        r = c.post(base, json=payload)
        assert r.status_code == 201, r.text
        saved = r.json()
        assert saved["revision"] == 1 and len(saved["document"]["items"]) == 4
        assert c.get(base).json()["items"] == [saved]
        invalid = copy.deepcopy(doc)
        invalid["items"][0]["w"] = 300
        assert c.post(base, json=payload | {"document": invalid}).status_code == 422
        invalid = copy.deepcopy(doc)
        invalid["items"][2]["map_id"] = str(uuid4())
        assert c.post(base, json=payload | {"document": invalid}).status_code == 422
        invalid = copy.deepcopy(doc)
        invalid["items"][0]["camera"]["url"] = "https://example.com"
        assert c.post(base, json=payload | {"document": invalid}).status_code == 422
        p = base + "/" + saved["id"]
        r = c.put(p, json=payload | {"revision": 1, "name": "Revision 2"})
        assert r.status_code == 200, r.text
        assert r.json()["revision"] == 2
        assert c.put(p, json=payload | {"revision": 1}).status_code == 409
        with _client(world.wrong_user_id) as other:
            assert other.get(base).status_code in (403, 404)
            assert other.put(p, json=payload | {"revision": 2}).status_code in (403, 404)
        assert c.delete(p, params={"revision": 1}).status_code == 409
        assert c.delete(p, params={"revision": 2}).status_code == 204
        assert c.get(base).json()["items"] == []
