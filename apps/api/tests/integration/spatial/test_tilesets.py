import base64
import io
import json
import struct
import zipfile
from uuid import uuid4

from sqlalchemy import text

from tests.fixtures.db_admin import get_admin_engine
from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.spatial.test_model_vault import _model
from tests.integration.spatial.test_sources import _client


def glb():
    doc = json.loads(_model())
    binary = base64.b64decode(doc["buffers"][0].pop("uri").split(",")[1])
    raw = json.dumps(doc).encode()
    raw += b" " * (-len(raw) % 4)
    return (
        struct.pack("<5I", 0x46546C67, 2, 28 + len(raw) + len(binary), len(raw), 0x4E4F534A)
        + raw
        + struct.pack("<2I", len(binary), 0x004E4942)
        + binary
    )


def manifest(uri="mesh/tile.glb"):
    return {
        "asset": {"version": "1.1"},
        "geometricError": 100,
        "root": {
            "boundingVolume": {"region": [-0.001, -0.001, 0.001, 0.001, -1, 100]},
            "geometricError": 0,
            "refine": "REPLACE",
            "transform": [0, 1, 0, 0, 0, 0, 1, 0, 1, 0, 0, 0, 6378137, 0, 0, 1],
            "content": {"uri": uri},
        },
    }


def archive(doc=None, extra=None):
    stream = io.BytesIO()
    with zipfile.ZipFile(stream, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("tileset.json", json.dumps(doc if doc is not None else manifest()))
        z.writestr("mesh/tile.glb", glb())
        if extra:
            z.writestr(extra, b"invalid")
    return stream.getvalue()


def test_tilesets(migrated_database):
    world = seed_access_world()
    prefix = f"/api/v1/organizations/{world.organization_id}/projects/{world.project_id}"
    base = prefix + "/3d/tilesets"
    with _client(world.owner_user_id) as owner:

        def upload(data):
            return owner.post(base, files={"file": ("data.zip", data)}, data={"name": "City"})

        for payload in (
            b"not zip",
            archive(extra="../evil"),
            archive(manifest("https://example.org/a.glb")),
            archive(manifest("../tile.glb")),
        ):
            assert upload(payload).status_code == 422
        bad = manifest()
        bad["root"]["boundingVolume"]["region"][0] = 100
        assert upload(archive(bad)).status_code == 422
        bad = manifest()
        bad["root"]["refine"] = "ADD"
        assert upload(archive(bad)).status_code == 422
        posted = upload(archive())
        assert posted.status_code == 201, posted.text
        record = posted.json()
        assert record["tile_count"] == 1
        path = base + "/" + record["id"]
        assert owner.get(base).json()["items"] == [record]
        assert owner.get(path + "/tileset.json").json() == manifest()
        resource = owner.get(path + "/mesh/tile.glb")
        assert resource.content == glb() and "no-store" in resource.headers["cache-control"]
        assert owner.get(path + "/unknown.glb").status_code == 404
        assert owner.get(base + "/" + str(uuid4()) + "/tileset.json").status_code == 404
        with get_admin_engine().begin() as conn:
            assert (
                conn.execute(
                    text("SELECT count(*) FROM spatial.tileset_contents WHERE tileset_id=:id"),
                    {"id": record["id"]},
                ).scalar_one()
                == 1
            )
        org = prefix.split("/projects/")[0]
        grant = owner.post(
            org + "/grants",
            json={
                "subject_kind": "membership",
                "subject_id": str(world.member_membership_id),
                "scope_kind": "project",
                "project_id": str(world.project_id),
                "role": "viewer",
            },
        )
        assert grant.status_code == 201, grant.text
    with _client(world.member_user_id) as member:
        assert member.get(path + "/mesh/tile.glb").content == glb()
        assert (
            member.post(
                base, files={"file": ("data.zip", archive())}, data={"name": "denied"}
            ).status_code
            == 403
        )
    with _client(world.wrong_user_id) as other:
        for url in (base, path + "/tileset.json", path + "/mesh/tile.glb"):
            assert other.get(url).status_code in (403, 404)
