import copy
from uuid import uuid4

from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.m2.test_processing_integrity import _job, _point, _read_output
from tests.integration.m2.test_r006_r009_real import _import_geojson
from tests.integration.spatial.test_sources import _client


def test_saved_processing_graph_and_real_chain(migrated_database):
    world = seed_access_world()
    project = f"/api/v1/organizations/{world.organization_id}/projects/{world.project_id}"
    base = project + "/processing/models"
    with _client(world.owner_user_id) as c:
        source = _import_geojson(
            c,
            world,
            "modeler.geojson",
            [_point("p1", 106.66, 10.7), _point("p2", 106.6601, 10.7)],
            "Model input",
            "import",
        )

        def ref(kind, id):
            return {"type": kind, "id": str(id)}

        a, b = str(uuid4()), str(uuid4())
        graph = {
            "version": 1,
            "nodes": [
                {
                    "id": b,
                    "algorithm": "dissolve",
                    "name": "Union",
                    "a": ref("step", a),
                    "parameters": {"group_by_field": None},
                    "x": 300,
                    "y": 80,
                },
                {
                    "id": a,
                    "algorithm": "buffer",
                    "name": "Buffer",
                    "a": ref("layer", source["target_layer_id"]),
                    "parameters": {"distance_meters": 50, "quad_segs": 8},
                    "x": 40,
                    "y": 80,
                },
            ],
        }
        payload = {"name": "Buffer to union", "graph": graph}
        created = c.post(base, json=payload)
        assert created.status_code == 201, created.text
        saved = created.json()
        assert saved["revision"] == 1
        assert c.get(base).json()["items"] == [saved]
        wrong = copy.deepcopy(graph)
        wrong["nodes"][1]["a"] = ref("step", b)
        assert c.post(base, json=payload | {"graph": wrong}).status_code == 422
        wrong = copy.deepcopy(graph)
        wrong["nodes"][0]["algorithm"] = "slope"
        wrong["nodes"][0]["parameters"] = {"band_a": 1}
        assert c.post(base, json=payload | {"graph": wrong}).status_code == 422
        wrong = copy.deepcopy(graph)
        wrong["nodes"][1]["parameters"] = {"distance_meters": 0}
        assert c.post(base, json=payload | {"graph": wrong}).status_code == 422
        wrong = copy.deepcopy(graph)
        wrong["nodes"][0]["a"] = ref("step", uuid4())
        assert c.post(base, json=payload | {"graph": wrong}).status_code == 422
        r = c.put(base + "/" + saved["id"], json=payload | {"revision": 1, "name": "Updated model"})
        assert r.status_code == 200, r.text
        assert r.json()["revision"] == 2
        assert c.put(base + "/" + saved["id"], json=payload | {"revision": 1}).status_code == 409
        job = _job(
            c, project, "buffer", source["target_layer_id"], {"distance_meters": 50, "quad_segs": 8}
        )
        _, _, items = _read_output(c, project, job)
        assert len(items) == 2
        result = _job(c, project, "dissolve", job["output_layer_id"], {"group_by_field": None})
        _, _, items = _read_output(c, project, result)
        assert len(items) == 1
        with _client(world.wrong_user_id) as stranger:
            assert stranger.get(base).status_code in (403, 404)
            assert stranger.put(
                base + "/" + saved["id"], json=payload | {"revision": 2}
            ).status_code in (403, 404)
        assert c.delete(base + "/" + saved["id"], params={"revision": 1}).status_code == 409
        assert c.delete(base + "/" + saved["id"], params={"revision": 2}).status_code == 204
        assert c.get(base).json()["items"] == []
        assert c.get(project + "/layers/" + result["output_layer_id"]).status_code == 200
