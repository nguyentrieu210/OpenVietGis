"""Independent R004 authorization matrix across actual restricted-runtime HTTP.

Product actions use real cookie/bearer authentication and fresh tenant proof.
Existing fixture helpers only establish synthetic users/catalog/grants.
"""

from __future__ import annotations

from uuid import uuid4

from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.service_identity.test_service_authorization import _service_client
from tests.integration.spatial.test_authorization import (
    _credential,
    _grant,
    _org,
    _principal,
    _root,
    _sibling,
)
from tests.integration.spatial.test_features import _setup
from tests.integration.spatial.test_sources import _client


def _intent(*, expected: int = 0, operation: str | None = None) -> dict:
    return {
        "operation_id": operation or str(uuid4()),
        "expected_revision": expected,
        "layers": [],
        "basemap": "grid",
        "default_viewpoint": {"mode": "preserve"},
    }


def _reads(root: str, layer: str, pins: dict) -> tuple[tuple[str, dict], ...]:
    return (
        (root + "/map", {}),
        (root + "/map/history", {"limit": 1}),
        (
            root + f"/layers/{layer}/viewport",
            {**pins, "west": 106.0, "south": 20.0, "east": 109.0, "north": 22.0, "limit": 1},
        ),
        (root + f"/layers/{layer}/display-extent", pins),
    )


def _assert_reads(client, calls, status: int) -> None:
    for url, params in calls:
        response = client.get(url, params=params)
        assert response.status_code == status, (url, response.text)


def test_all_map_reads_follow_current_grant_and_same_session_revocation(
    migrated_database: None,
) -> None:
    world = seed_access_world()
    root = _root(world)
    with _client(world.owner_user_id) as owner, _client(world.member_user_id) as member:
        _, _, layer, pins = _setup(owner, world, "reference_points_5899")
        calls = _reads(root, layer, pins)
        before = owner.get(root + "/map").json()
        _assert_reads(member, calls, 403)
        grant = _grant(owner, world, "membership", str(world.member_membership_id), "viewer")
        _assert_reads(member, calls, 200)
        denied = member.put(root + "/map", json=_intent())
        assert denied.status_code == 403, denied.text
        assert owner.get(root + "/map").json() == before
        assert owner.post(f"{_org(world)}/grants/{grant}/revoke").status_code == 204
        _assert_reads(member, calls, 403)
        assert owner.get(root + "/map").json() == before


def test_edit_only_service_can_save_and_reconcile_without_read_disclosure(
    migrated_database: None,
) -> None:
    world = seed_access_world()
    root = _root(world)
    with _client(world.owner_user_id) as owner:
        _, _, layer, pins = _setup(owner, world, "reference_points_5899")
        principal = _principal(owner, world)
        grant = _grant(owner, world, "service_principal", principal, "manager")
        credential, secret = _credential(owner, world, principal, ["project.edit"])
        with _service_client(secret) as actor:
            _assert_reads(actor, _reads(root, layer, pins), 403)
            intent = _intent()
            saved = actor.put(root + "/map", json=intent)
            assert saved.status_code == 200, saved.text
            receipt = saved.json()
            assert set(receipt) == {"map_id", "operation_id", "configuration_revision"}
            assert receipt["configuration_revision"] == 1
            op_url = root + "/map/operations/" + intent["operation_id"]
            assert actor.get(op_url).json() == receipt
            assert actor.put(root + "/map", json=intent).json() == receipt
            _assert_reads(actor, _reads(root, layer, pins), 403)
            history = owner.get(root + "/map/history").json()["items"]
            assert len(history) == 1
            # Same actor through a separately issued permitted credential can reconcile.
            _, second = _credential(owner, world, principal, ["project.edit", "project.read"])
            with _service_client(second) as same_actor:
                assert same_actor.get(op_url).json() == receipt
            # A human cannot retrieve the service actor's receipt.
            foreign = owner.get(op_url)
            absent = owner.get(root + "/map/operations/" + str(uuid4()))
            assert foreign.status_code == absent.status_code == 404
            assert foreign.json() == absent.json()
            assert owner.post(f"{_org(world)}/grants/{grant}/revoke").status_code == 204
            assert actor.get(op_url).status_code == 403
            assert actor.put(root + "/map", json=intent).status_code == 403
            revoked = owner.post(
                f"{_org(world)}/service-principals/{principal}/credentials/{credential}/revoke"
            )
            assert revoked.status_code == 204
            assert actor.get(op_url).status_code == 401
        assert len(owner.get(root + "/map/history").json()["items"]) == 1


def test_dual_authorized_sibling_layer_substitution_and_foreign_save_are_concealed(
    migrated_database: None,
) -> None:
    world = seed_access_world()
    with _client(world.owner_user_id) as owner:
        sibling = _sibling(owner, world)
        _, _, own_layer, own_pins = _setup(owner, world, "reference_points_5899")
        _, _, sibling_layer, sibling_pins = _setup(owner, sibling, "reference_points_5899")
        for current, layer, pins in (
            (world, sibling_layer, sibling_pins),
            (sibling, own_layer, own_pins),
        ):
            root = _root(current)
            actual = _reads(root, layer, pins)[2:]
            missing = _reads(root, str(uuid4()), pins)[2:]
            for (url, params), (absent_url, absent_params) in zip(actual, missing, strict=True):
                denied = owner.get(url, params=params)
                absent = owner.get(absent_url, params=absent_params)
                assert denied.status_code == absent.status_code == 404, denied.text
                assert denied.json() == absent.json()
            payload = _intent()
            payload["layers"] = [
                {
                    "layer_id": layer,
                    "visible": True,
                    "style": {"color": "#3B82F6", "opacity": 1.0, "width": 2.0, "radius": 5.0},
                }
            ]
            before = owner.get(root + "/map").json()
            denied = owner.put(root + "/map", json=payload)
            assert denied.status_code == 404, denied.text
            assert owner.get(root + "/map").json() == before
            assert owner.get(root + "/map/history").json()["items"] == []


def test_map_cookie_save_requires_csrf_but_bearer_uses_live_edit_ceiling(
    migrated_database: None,
) -> None:
    world = seed_access_world()
    root = _root(world)
    with _client(world.owner_user_id) as owner:
        intent = _intent()
        header = owner.headers.pop("X-OpenVietGIS-CSRF")
        denied = owner.put(root + "/map", json=intent)
        assert denied.status_code == 403, denied.text
        owner.headers["X-OpenVietGIS-CSRF"] = header
        assert owner.get(root + "/map").json()["configuration_revision"] == 0
        assert owner.put(root + "/map", json=intent).status_code == 200
        principal = _principal(owner, world)
        _grant(owner, world, "service_principal", principal, "manager")
        _, read_secret = _credential(owner, world, principal, ["project.read"])
        with _service_client(read_secret) as reader:
            assert reader.put(root + "/map", json=_intent(expected=1)).status_code == 403
        assert owner.get(root + "/map").json()["configuration_revision"] == 1


def test_archived_project_remains_readable_but_denies_save_and_edit_receipts(
    migrated_database: None,
) -> None:
    world = seed_access_world()
    root = _root(world)
    with _client(world.owner_user_id) as owner:
        _, _, layer, pins = _setup(owner, world, "reference_points_5899")
        intent = _intent()
        saved = owner.put(root + "/map", json=intent)
        assert saved.status_code == 200, saved.text
        archived = owner.patch(root, json={"status": "archived"})
        assert archived.status_code == 200, archived.text
        _assert_reads(owner, _reads(root, layer, pins), 200)
        current = owner.get(root + "/map").json()
        denied = owner.put(root + "/map", json=_intent(expected=1))
        assert denied.status_code == 409, denied.text
        receipt = owner.get(root + "/map/operations/" + intent["operation_id"])
        assert receipt.status_code == 409, receipt.text
        assert owner.get(root + "/map").json() == current
        assert len(owner.get(root + "/map/history").json()["items"]) == 1
