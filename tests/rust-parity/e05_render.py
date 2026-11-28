"""E05 map presentation parity: viewport and display extent against FastAPI,
plus a Rust-only smoke test for the new Mapbox vector tile route."""

from __future__ import annotations

from uuid import uuid4

WORLD = {"west": -180, "south": -85, "east": 180, "north": 85}
# Seeded 1k polygons lie in lon [105.65, 106.05], lat [20.85, 21.15].
HANOI = {"west": 105.75, "south": 20.92, "east": 105.95, "north": 21.08}
TILE = (8, 203, 112)  # covers the whole seeded polygon area
STALE = "0" * 64


def _pins(layer):
    return {key: layer[key] for key in ("content_revision", "schema_revision")}


def _outcome(suite, label, check):
    try:
        check()
    except AssertionError as exc:
        suite.failures.append(label)
        print(f"FAIL {label}: {exc}", flush=True)
    else:
        suite.passed += 1
        print(f"PASS {label}", flush=True)


def compare_render_reads(suite, fixture, layer_path, owner, outsider, surveyor):
    """Run while the surveyor's single-feature GIS rule on epsg4326_1k is active."""
    layers = fixture["layers"]
    polygons = layers["epsg4326_1k"]
    pins = _pins(polygons)
    viewport = f"{layer_path(polygons['layer_id'])}/viewport"
    extent = f"{layer_path(polygons['layer_id'])}/display-extent"

    suite.get("E05 viewport bbox", viewport, token=owner, params=pins | HANOI)
    suite.get("E05 viewport world", viewport, token=owner, params=pins | WORLD)
    suite.get(
        "E05 viewport feature limit prefix",
        viewport,
        token=owner,
        params=pins | WORLD | {"limit": 3},
        check=lambda body: _require(
            body["limitation"] == "feature_limit", "limit must be reported"
        ),
    )
    suite.get(
        "E05 empty viewport is complete",
        viewport,
        token=owner,
        params=pins | {"west": 0, "south": 0, "east": 1, "north": 1},
    )
    suite.get("E05 display extent", extent, token=owner, params=pins)
    suite.get(
        "E05 stale viewport pin",
        viewport,
        409,
        token=owner,
        params=pins | HANOI | {"content_revision": STALE},
    )
    suite.get(
        "E05 stale extent pin",
        extent,
        409,
        token=owner,
        params=pins | {"schema_revision": STALE},
    )
    for label, params in (
        ("wrapped bbox", pins | HANOI | {"west": 106.0, "east": 105.0}),
        ("latitude outside display", pins | HANOI | {"north": 89}),
        ("non-finite bbox", pins | HANOI | {"east": "inf"}),
        ("limit above 1000", pins | HANOI | {"limit": 1001}),
        ("malformed pin", pins | HANOI | {"content_revision": "XYZ"}),
        ("missing bbox", pins),
    ):
        suite.get(f"E05 invalid viewport: {label}", viewport, 422, token=owner, params=params)
    suite.get("E05 unauthenticated viewport", viewport, 401, params=pins | HANOI)
    suite.get(
        "E05 tenant B cannot read tenant A viewport",
        viewport,
        404,
        token=outsider,
        params=pins | HANOI,
    )
    suite.get(
        "E05 tenant B cannot read tenant A extent",
        extent,
        404,
        token=outsider,
        params=pins,
    )
    suite.get(
        "E05 foreign layer under own tenant",
        f"{layer_path(layers['tenant_b_1k']['layer_id'])}/viewport",
        404,
        token=owner,
        params=pins | HANOI,
    )
    suite.get(
        "E05 missing layer",
        f"{layer_path(uuid4())}/display-extent",
        404,
        token=owner,
        params=pins,
    )
    manual_path = layer_path(layers["edit_4326"]["layer_id"])
    # E03 runs before E05 and advances the editable layer's content pin.
    manual = suite.get("E05 current manual layer pins", manual_path, token=owner)
    manual_pins = _pins(manual["source"]) if manual else _pins(layers["edit_4326"])
    suite.get(
        "E05 manual source has no render profile",
        f"{manual_path}/viewport",
        422,
        token=owner,
        params=manual_pins | HANOI,
    )
    regional = layers["vn2000_5899_1k"]
    suite.get(
        "E05 VN-2000 source without trusted provenance",
        f"{layer_path(regional['layer_id'])}/display-extent",
        422,
        token=owner,
        params=_pins(regional),
    )

    def only_allowed(body):
        ids = [item["feature_id"] for item in body["items"]]
        _require(ids == ["poly-000001"], f"GIS rule leaked features: {ids[:5]}")

    suite.get(
        "E05 GIS-scoped viewport shows only the permitted feature",
        viewport,
        token=surveyor,
        params=pins | WORLD,
        check=only_allowed,
    )
    suite.get("E05 GIS-scoped display extent", extent, token=surveyor, params=pins)
    suite.get(
        "E05 GIS-scoped hidden layer",
        f"{manual_path}/viewport",
        404,
        token=surveyor,
        params=manual_pins | HANOI,
    )
    compare_tiles(suite, layer_path, polygons, owner, outsider, surveyor)


def _require(condition, detail):
    if not condition:
        raise AssertionError(detail)


def compare_tiles(suite, layer_path, polygons, owner, outsider, surveyor):
    rust = suite.rust
    pins = _pins(polygons)
    z, x, y = TILE
    tile = f"{layer_path(polygons['layer_id'])}/tiles/{z}/{x}/{y}.mvt"

    def get(token, path=tile, params=pins):
        return rust.get(path, params=params, headers={"cookie": f"ovg_session={token}"})

    def populated():
        response = get(owner)
        _require(response.status_code == 200, f"status {response.status_code}")
        _require(
            response.headers.get("content-type") == "application/vnd.mapbox-vector-tile",
            "content type",
        )
        _require(response.headers.get("cache-control") == "private, no-store", "cache")
        _require(response.headers.get("x-openvietgis-tile-complete") == "true", "complete")
        _require(len(response.content) > 1000, f"tile has {len(response.content)} bytes")
        _require(b"poly-000002" in response.content, "owner tile misses features")

    def stable():
        first = get(owner)
        second = get(owner)
        _require(first.status_code == second.status_code == 200, "tile status")
        _require(first.content == second.content, "same pinned tile must be deterministic")

    def world():
        response = get(owner, path=tile.replace(f"/{z}/{x}/{y}.mvt", "/0/0/0.mvt"))
        _require(response.status_code == 200, f"world tile status {response.status_code}")
        _require(
            response.headers.get("content-type") == "application/vnd.mapbox-vector-tile",
            "world tile type",
        )

    def scoped():
        response = get(surveyor)
        _require(response.status_code == 200, f"status {response.status_code}")
        _require(b"poly-000001" in response.content, "permitted feature missing")
        _require(b"poly-000002" not in response.content, "GIS rule leaked into tile")

    def empty():
        response = get(owner, path=tile.replace(f"/{z}/{x}/{y}.mvt", "/8/0/0.mvt"))
        _require(response.status_code == 200, f"status {response.status_code}")
        _require(response.content == b"", "tile outside the data must be empty")

    def status(expected, token, path=tile, params=pins):
        def check():
            response = get(token, path, params)
            _require(response.status_code == expected, f"status {response.status_code}")

        return check

    _outcome(suite, "E05 tile populated", populated)
    _outcome(suite, "E05 tile deterministic", stable)
    _outcome(suite, "E05 world tile", world)
    _outcome(suite, "E05 tile GIS-scoped", scoped)
    _outcome(suite, "E05 tile empty area", empty)
    _outcome(suite, "E05 tile tenant B denied", status(404, outsider))
    _outcome(suite, "E05 tile stale pin", status(409, owner, params=pins | {"content_revision": STALE}))
    _outcome(
        suite,
        "E05 tile address outside zoom",
        status(422, owner, path=tile.replace(f"/{z}/{x}/{y}.mvt", "/8/256/0.mvt")),
    )
    _outcome(
        suite,
        "E05 tile zoom above 22",
        status(422, owner, path=tile.replace(f"/{z}/{x}/{y}.mvt", "/23/0/0.mvt")),
    )
    _outcome(suite, "E05 tile unauthenticated", lambda: _require(
        rust.get(tile, params=pins).status_code == 401, "anonymous tile"
    ))


def check_cached_tile_revocation(
    suite, layer_path, layer, org_path, project_id, membership_id, owner, member, owner_csrf
):
    """Warm the private Rust tile cache, revoke its reader grant, and retry the same URL."""
    z, x, y = TILE
    tile = f"{layer_path(layer['layer_id'])}/tiles/{z}/{x}/{y}.mvt"
    pins = _pins(layer)
    owner_headers = {"cookie": f"ovg_session={owner}", "x-openvietgis-csrf": owner_csrf}

    def check():
        created = suite.oracle.post(
            f"{org_path}/grants",
            headers=owner_headers,
            json={
                "subject_kind": "membership",
                "subject_id": str(membership_id),
                "scope_kind": "project",
                "project_id": str(project_id),
                "role": "viewer",
            },
        )
        _require(created.status_code == 201, f"reader grant: {created.status_code} {created.text}")
        grant_id = created.json()["id"]
        headers = {"cookie": f"ovg_session={member}"}
        first = suite.rust.get(tile, params=pins, headers=headers)
        _require(first.status_code == 200, f"warm tile status {first.status_code}: {first.text[:200]}")
        _require(
            first.headers.get("content-type") == "application/vnd.mapbox-vector-tile"
            and b"poly-000002" in first.content,
            "authorized reader did not receive the seeded tile",
        )
        warm = suite.rust.get(tile, params=pins, headers=headers)
        _require(warm.status_code == 200 and warm.content == first.content, "repeat tile was not stable")

        revoked = suite.oracle.post(f"{org_path}/grants/{grant_id}/revoke", headers=owner_headers)
        _require(revoked.status_code == 204, f"grant revoke: {revoked.status_code} {revoked.text[:200]}")
        denied = suite.rust.get(tile, params=pins, headers=headers)
        _require(denied.status_code == 403, f"cached tile survived revocation: HTTP {denied.status_code}")
        _require(
            denied.headers.get("content-type") != "application/vnd.mapbox-vector-tile"
            and b"poly-000002" not in denied.content,
            "revoked response disclosed cached tile bytes",
        )

    _outcome(suite, "E05 cached tile rechecks revoked project grant", check)
