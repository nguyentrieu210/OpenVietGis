"""CRS consumers must not silently choose an unverified PostGIS operation."""

from __future__ import annotations

from uuid import uuid4

from openvietgis_api.adapters.formats.geopackage import write_geopackage
from openvietgis_api.domains.spatial.domain import FieldType, NativeGeometry
from openvietgis_api.domains.transfers.domain import ExportFieldMapping
from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.m2.test_processing_integrity import _base, _derived_counts, _job, _point
from tests.integration.m2.test_r006_r009_real import _import_geojson
from tests.integration.spatial.test_sources import _client


def _native_import(client, base, crs, geometry):
    member = "native"
    payload, _, _ = write_geopackage(
        layer_name=member,
        crs_code=crs,
        geometry_family=geometry.type,
        field_mappings=[ExportFieldMapping("code", "Code", "code", FieldType.TEXT)],
        features=[("one", {"code": "one"}, geometry)],
    )
    upload = client.post(
        f"{base}/imports/upload",
        files={"file": ("native.gpkg", payload, "application/geopackage+sqlite3")},
    )
    assert upload.status_code == 201, upload.text
    asset_id = upload.json()["asset"]["id"]
    options = {
        "source_asset_id": asset_id,
        "selected_member": member,
        "declared_source_crs": crs,
        "target_crs": crs,
        "declared_encoding": "utf-8",
    }
    check = client.post(f"{base}/imports/inspect", json=options)
    assert check.status_code == 200, check.text
    assert check.json()["inspection"]["confirmable"]
    done = client.post(
        f"{base}/imports/confirm",
        json={
            **options,
            "operation_id": str(uuid4()),
            "source_name": f"Native {crs}",
            "layer_name": f"Native {crs}",
            "inspection_fingerprint": check.json()["inspection"]["inspection_fingerprint"],
            "approved_conversions": [],
        },
    )
    assert done.status_code == 200 and done.json()["status"] == "succeeded", done.text
    return done.json()["target_layer_id"]


def _print(client, base, layer_id, crs):
    layout = client.post(
        f"{base}/cartography/layouts",
        json={
            "name": "CRS control",
            "title": "CRS control",
            "crs_code": crs,
            "layer_ids": [layer_id],
        },
    )
    assert layout.status_code == 201, layout.text
    path = f"{base}/cartography/layouts/{layout.json()['id']}/exports"
    settings = {"output_format": "png", "dpi": 150}
    preview = client.post(path + "/preview", json=settings)
    if preview.status_code != 200:
        return preview
    return client.post(
        path,
        json={
            **settings,
            "expected_revision_fingerprint": preview.json()["revision_fingerprint"],
        },
    )


def test_processing_and_print_fail_closed_for_unapproved_crs(migrated_database):
    world = seed_access_world()
    base = _base(world)
    with _client(world.owner_user_id) as client:
        vn = _native_import(client, base, 4756, NativeGeometry("Point", (107.16, 20.71), 4756))
        overlay = _native_import(
            client,
            base,
            4756,
            NativeGeometry(
                "Polygon",
                (
                    (
                        (107.15, 20.70),
                        (107.17, 20.70),
                        (107.17, 20.72),
                        (107.15, 20.72),
                        (107.15, 20.70),
                    ),
                ),
                4756,
            ),
        )
        wgs = _import_geojson(
            client, world, "point.geojson", [_point("p", 107.16, 20.71)], "WGS", "wgs"
        )["target_layer_id"]
        before = _derived_counts(world)
        failed = _job(client, base, "buffer", vn, {"distance_meters": 100})
        assert failed["status"] == "failed" and failed["error_code"] == "UNSUPPORTED_TRANSFORMATION"
        # The 4326↔4756 edge is approved only for trusted Quang Ninh fixture
        # provenance; this GeoJSON import has no such evidence, so it fails closed
        # as out of area rather than being treated as an unavailable CRS edge.
        assert (
            _job(client, base, "clip", wgs, {}, overlay_layer_id=overlay)["error_code"]
            == "TRANSFORMATION_OUT_OF_AREA"
        )
        assert _derived_counts(world) == before
        print_rejected = _print(client, base, vn, 4326)
        assert (
            print_rejected.status_code == 422
            and print_rejected.json()["code"] == "UNSUPPORTED_TRANSFORMATION"
        )
        assert client.get(f"{base}/cartography/exports").json()["items"] == []


def test_web_mercator_consumers_use_verified_profile_and_bounds(migrated_database):
    world = seed_access_world()
    base = _base(world)
    with _client(world.owner_user_id) as client:
        wgs = _import_geojson(
            client, world, "good.geojson", [_point("p", 105.8, 21)], "Good", "good"
        )["target_layer_id"]
        polar = _import_geojson(
            client, world, "polar.geojson", [_point("p", 105.8, 86)], "Polar", "polar"
        )["target_layer_id"]
        before = _derived_counts(world)
        failed = _job(client, base, "buffer", polar, {"distance_meters": 100})
        assert failed["status"] == "failed" and failed["error_code"] == "TRANSFORMATION_OUT_OF_AREA"
        assert _derived_counts(world) == before
        ok = _job(client, base, "buffer", wgs, {"distance_meters": 100})
        assert ok["status"] == "succeeded" and ok["feature_count"] == 1
        mask = _native_import(
            client,
            base,
            3857,
            NativeGeometry(
                "Polygon",
                (
                    (
                        (11700000, 2300000),
                        (11900000, 2300000),
                        (11900000, 2500000),
                        (11700000, 2500000),
                        (11700000, 2300000),
                    ),
                ),
                3857,
            ),
        )
        clipped = _job(client, base, "clip", wgs, {}, overlay_layer_id=mask)
        assert clipped["status"] == "succeeded" and clipped["feature_count"] == 1
        rejected = _print(client, base, polar, 3857)
        assert (
            rejected.status_code == 422 and rejected.json()["code"] == "TRANSFORMATION_OUT_OF_AREA"
        )
        assert client.get(f"{base}/cartography/exports").json()["items"] == []
        rendered = _print(client, base, wgs, 3857)
        assert rendered.status_code == 201, rendered.text
        layer = rendered.json()["revision_manifest"]["layers"][0]
        assert (
            layer["source_crs"] == 4326 and layer["transformation_operation"] == "epsg-1024-forward"
        )
