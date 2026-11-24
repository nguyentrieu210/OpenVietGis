"""R048 existing CAD identity and ChangeSet acceptance on isolated PostGIS."""

import hashlib
import io
from uuid import uuid4

import ezdxf

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
        current = check(owner.get(root + "/layers/" + first["layer_id"]))["source"]
        assert by_source[first["source_id"]]["conversion_id"] == first["id"]
        assert by_source[first["source_id"]]["content_revision"] == current["content_revision"]
        assert by_source[first["source_id"]]["schema_revision"] == current["schema_revision"]
        draft_path = path + "/export-georef-draft"
        draft_request = {
            "conversion_id": first["id"],
            "expected_document_sha256": document["sha256"],
            "expected_content_revision": current["content_revision"],
            "expected_schema_revision": current["schema_revision"],
        }
        preview = check(owner.post(draft_path, json=draft_request))
        assert preview["feature_count"] == 1 and preview["published"] is False
        assert preview["loss_report"]["native_handles_not_preserved"]
        assert (
            owner.post(draft_path, json=draft_request | {"acknowledge_loss": True}).status_code
            == 422
        )
        confirm = draft_request | {
            "expected_preview_fingerprint": preview["preview_fingerprint"],
            "acknowledge_loss": True,
        }
        assert (
            owner.post(
                draft_path, json=confirm | {"expected_preview_fingerprint": "0" * 64}
            ).status_code
            == 409
        )
        assert (
            owner.post(
                draft_path, json=confirm | {"expected_content_revision": layer["content_revision"]}
            ).status_code
            == 409
        )
        assert owner.post(draft_path, json=confirm | {"acknowledge_loss": False}).status_code == 422
        assert (
            owner.post(
                draft_path, json=confirm | {"expected_document_sha256": "0" * 64}
            ).status_code
            == 409
        )
        assert (
            owner.post(
                draft_path, json=confirm | {"expected_schema_revision": "0" * 64}
            ).status_code
            == 409
        )
        assert (
            owner.post(draft_path, json=confirm | {"conversion_id": str(uuid4())}).status_code
            == 404
        )
        exported = owner.post(draft_path, json=confirm)
        assert exported.status_code == 200, exported.text
        assert exported.headers["X-Content-SHA256"] == hashlib.sha256(exported.content).hexdigest()
        doc = ezdxf.read(io.StringIO(exported.content.decode()))
        assert doc.header["$INSUNITS"] == 4
        poly = list(doc.modelspace())[0]
        assert poly.closed
        vertices = [tuple(map(float, p)) for p in poly.get_points("xy")]
        assert abs(max(p[0] for p in vertices) - 11000) < 0.001
        assert abs(max(p[1] for p in vertices) - 5000) < 0.001
        native_request = draft_request | {"profile": "native-xy-dxf-v1"}
        native_preview = check(owner.post(draft_path, json=native_request))
        assert native_preview["changed_handles"] == [handle]
        assert native_preview["loss_report"]["native_handles_not_preserved"] is False
        native_confirm = native_request | {
            "expected_preview_fingerprint": native_preview["preview_fingerprint"],
            "acknowledge_loss": True,
        }
        assert (
            owner.post(
                draft_path, json=native_confirm | {"profile": "derived-local-dxf-v1"}
            ).status_code
            == 409
        )
        native_export = owner.post(draft_path, json=native_confirm)
        assert native_export.status_code == 200, native_export.text
        native_doc = ezdxf.read(io.StringIO(native_export.content.decode()))
        assert native_doc.entitydb[handle].closed
        points = list(native_doc.entitydb[handle].get_points("xy"))
        assert abs(max(p[0] for p in points) - 11000) < 0.001
        assert abs(max(p[1] for p in points) - 5000) < 0.001
        assert (
            native_export.headers["X-Content-SHA256"]
            == hashlib.sha256(native_export.content).hexdigest()
        )
        assert owner.get(path + "/download").headers["X-Content-SHA256"] == document["sha256"]
        with _client(world.wrong_user_id) as other:
            assert other.post(draft_path, json=confirm).status_code in (403, 404)
        version = check(
            owner.post(
                base + "/" + document["id"] + "/versions",
                files={"file": ("r048.dxf", _drawing())},
                data={"expected_version": 1},
            ),
            201,
        )
        assert version["version"] == 2
        assert owner.post(draft_path, json=confirm).status_code == 409
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


def test_native_cad_delta_uses_existing_changeset_transaction(migrated_database):
    from uuid import UUID

    from openvietgis_api.adapters.cad.native_delta import cad_to_gis_preview
    from openvietgis_api.domains.cad.domain import CadUnit
    from openvietgis_api.domains.cad.georeference import fit_placement

    world = seed_access_world()
    root = f"/api/v1/organizations/{world.organization_id}/projects/{world.project_id}"
    base = root + "/cad/documents"
    controls = [
        [[0, 0], [11800000, 1200000]],
        [[10000, 0], [11800000, 1200010]],
        [[0, 5000], [11799995, 1200000]],
    ]
    profile = {
        "crs_code": 3857,
        "units": "millimeter",
        "tolerance_m": 0.001,
        "max_scale_deviation": 0.001,
        "controls": controls,
    }
    original = _drawing()

    with _client(world.owner_user_id) as owner:

        def check(r, expected=200):
            assert r.status_code == expected, r.text
            return r.json()

        saved = check(owner.post(base, files={"file": ("native.dxf", original)}), 201)
        path = base + "/" + saved["id"] + "/versions/1"
        inspected = check(owner.post(path + "/inspect-georef", json=profile))
        source = check(
            owner.post(
                path + "/import-georef",
                json=profile
                | {
                    "layer_name": "Native proposal",
                    "expected_sha256": saved["sha256"],
                    "expected_inspection_fingerprint": inspected["inspection_fingerprint"],
                    "acknowledge_loss": True,
                },
            ),
            201,
        )
        layer = check(owner.get(root + "/layers/" + source["layer_id"]))["source"]
        mapping = inspected["entity_summary"]["entity_mapping"]
        native = ezdxf.read(io.StringIO(original.decode()))
        handle = mapping[0]["handle"]
        native.entitydb[handle].set_points(
            [(0, 0), (12000, 0), (12000, 5000), (0, 5000)], format="xy"
        )
        buffer = io.StringIO()
        native.write(buffer)
        candidate = buffer.getvalue().encode()
        version = check(
            owner.post(
                base + "/" + saved["id"] + "/versions",
                files={"file": ("native.dxf", candidate)},
                data={"expected_version": 1},
            ),
            201,
        )
        assert version["version"] == 2
        placement = fit_placement(
            document_id=UUID(saved["id"]),
            version=1,
            crs_code=3857,
            units=CadUnit.MILLIMETER,
            controls=tuple((tuple(a), tuple(b)) for a, b in controls),
            tolerance_m=0.001,
            max_scale_deviation=0.001,
        )
        # Orchestrate the internal pure planner with trusted snapshot values.
        # This deliberately is NOT a public preview route.
        ops = cad_to_gis_preview(
            original,
            candidate,
            base_sha256=saved["sha256"],
            candidate_sha256=version["sha256"],
            placement=placement,
            mapping=mapping,
            imported_content_revision=layer["content_revision"],
            current_content_revision=layer["content_revision"],
            imported_schema_revision=layer["schema_revision"],
            current_schema_revision=layer["schema_revision"],
        )
        assert len(ops) == 1 and ops[0].feature_id == mapping[0]["feature_id"]
        request = {
            "operation_id": str(uuid4()),
            "summary": "Reviewed CAD native XY proposal",
            "base_content_revision": layer["content_revision"],
            "base_schema_revision": layer["schema_revision"],
            "operations": [
                {"op_type": op.op_type.value, "feature_id": op.feature_id, "geometry": op.geometry}
                for op in ops
            ],
        }
        changes = root + "/sources/" + source["source_id"] + "/changesets"
        with _client(world.wrong_user_id) as outsider:
            assert outsider.post(changes, json=request).status_code in (403, 404)
        accepted = check(owner.post(changes, json=request), 201)
        repeated = owner.post(changes, json=request)
        assert repeated.status_code in (200, 201), repeated.text
        assert repeated.json()["id"] == accepted["id"]
        assert owner.post(changes, json=request | {"operation_id": str(uuid4())}).status_code == 409
        # R006 geometry and revision authority stay on PostGIS, not in CAD.
        from sqlalchemy import text

        from tests.fixtures.db_admin import get_admin_engine

        with get_admin_engine().begin() as conn:
            actual = conn.execute(
                text(
                    "SELECT ST_Area(geometry),ST_YMax(geometry),attributes "
                    "FROM spatial.reference_features WHERE source_id=:src"
                ),
                {"src": source["source_id"]},
            ).one()
            assert abs(actual[0] - 60) < 1e-6 and abs(actual[1] - 1200012) < 1e-6
            assert actual[2]["cad_entity"] == "LWPOLYLINE_CLOSED"
        assert owner.get(path + "/download").headers["X-Content-SHA256"] == saved["sha256"]
