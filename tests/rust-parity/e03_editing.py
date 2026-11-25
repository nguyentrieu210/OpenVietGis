"""E03 editing parity: changeset writes, replay, conflicts, revert and snapping.

Error cases do not mutate, so both implementations answer the same state. A
successful write is committed by one implementation and replayed with the same
operation_id on the other: the receipts must be identical, which proves the
request digest, the persisted rows and the wire format agree in both directions.
"""

from __future__ import annotations

from uuid import UUID, uuid4

import benchmark_runner as baseline
import httpx
import psycopg

API = "/api/v1"
COOKIE = "ovg_session"


def _headers(token=None, csrf=None):
    headers = {"cookie": f"{COOKIE}={token}"} if token else {}
    if csrf:
        headers["x-openvietgis-csrf"] = csrf
    return headers


def _send(client, method, path, headers, body, raw):
    if raw is not None:
        return client.request(
            method,
            path,
            headers=headers | {"content-type": "application/json"},
            content=raw,
        )
    if body is None:
        return client.request(method, path, headers=headers)
    return client.request(method, path, headers=headers, json=body)


class EditingParity:
    def __init__(self, suite, difference):
        self.suite = suite
        self.difference = difference

    def record(self, label, check):
        try:
            check()
        except (AssertionError, ValueError, KeyError, httpx.HTTPError) as exc:
            self.suite.failures.append(label)
            print(f"FAIL {label}: {exc}", flush=True)
        else:
            self.suite.passed += 1
            print(f"PASS {label}", flush=True)

    def same(self, label, method, path, status, *, headers, body=None, raw=None):
        """Non-mutating request: both implementations answer identically."""

        def check():
            expected = _send(self.suite.oracle, method, path, headers, body, raw)
            actual = _send(self.suite.rust, method, path, headers, body, raw)
            assert expected.status_code == status, (
                f"FastAPI status {expected.status_code}, wanted {status}"
            )
            assert actual.status_code == status, (
                f"Rust status {actual.status_code}, wanted {status}"
            )
            mismatch = self.difference(expected.json(), actual.json())
            assert mismatch is None, mismatch
            assert actual.headers.get("cache-control") == expected.headers.get(
                "cache-control"
            ), "Cache-Control differs from FastAPI"

        self.record(label, check)

    def cross(self, label, first, path, body, headers):
        """`first` commits; the other implementation must replay the same receipt."""
        second = "oracle" if first == "rust" else "rust"
        receipt = {}

        def check():
            committed = getattr(self.suite, first).post(path, headers=headers, json=body)
            assert committed.status_code == 201, (
                f"{first} commit status {committed.status_code}: {committed.text[:400]}"
            )
            replayed = getattr(self.suite, second).post(path, headers=headers, json=body)
            assert replayed.status_code == 201, (
                f"{second} replay status {replayed.status_code}: {replayed.text[:400]}"
            )
            mismatch = self.difference(committed.json(), replayed.json())
            assert mismatch is None, mismatch
            receipt.update(committed.json())

        self.record(label, check)
        return receipt


def compare_editing(suite, owner_url, fixture, surveyor, difference):
    from openvietgis_api.domains.editing.domain import (
        compute_content_revision,
        geometry_to_wkt,
    )
    from openvietgis_api.security.csrf import make_csrf_token

    parity = EditingParity(suite, difference)
    owner, owner_csrf = fixture["session_a_token"], fixture["csrf_a_token"]
    foreign = _headers(fixture["session_b_token"], fixture["csrf_b_token"])
    write = _headers(owner, owner_csrf)
    surveyor_write = _headers(
        surveyor, make_csrf_token(surveyor, baseline.DEFAULT_CSRF_SECRET)
    )
    source_id = fixture["layers"]["edit_4326"]["source_id"]
    root = f"{API}/organizations/{fixture['org_a_id']}/projects/{fixture['project_a_id']}"
    changesets = f"{root}/sources/{source_id}/changesets"
    with psycopg.connect(owner_url) as conn:
        revision = conn.execute(
            "SELECT content_revision FROM spatial.data_sources WHERE id=%s", (source_id,)
        ).fetchone()[0]
        existing_id, existing_props = conn.execute(
            """SELECT feature_id, attributes FROM spatial.reference_features
               WHERE source_id=%s ORDER BY feature_id LIMIT 1""",
            (source_id,),
        ).fetchone()

    point = {"type": "Point", "coordinates": [105.84, 21.02]}
    props = {"code": "E03-1", "ward": "W1", "metric_val": 7, "active": True}

    def commit(operation_id, operations, base=None, **extra):
        return {
            "operation_id": operation_id,
            "base_content_revision": base or revision,
            "summary": "E03 parity",
            "operations": operations,
        } | extra

    def create(fid="e03-p1", geometry=point, properties=props):
        return {
            "op_type": "create",
            "feature_id": fid,
            "geometry": geometry,
            "properties": properties,
        }

    valid = commit("e03-probe", [create()])
    parity.same("E03 commit unauthenticated", "POST", changesets, 401, headers={}, body=valid)
    parity.same(
        "E03 malformed JSON precedes authentication",
        "POST",
        changesets,
        422,
        headers={},
        raw="{",
    )
    parity.same(
        "E03 commit without CSRF", "POST", changesets, 403, headers=_headers(owner), body=valid
    )
    parity.same(
        "E03 commit foreign tenant", "POST", changesets, 404, headers=foreign, body=valid
    )
    parity.same(
        "E03 commit member without project grant",
        "POST",
        changesets,
        403,
        headers=surveyor_write,
        body=valid,
    )
    for label, status, body in [
        ("extra request field", 422, valid | {"extra": 1}),
        ("reserved field-sync operation id", 422, commit(" field-sync-1", [create()])),
        ("invalid operation id", 422, commit("bad id", [create()])),
        ("invalid feature id", 422, commit("e03-x1", [create("bad id!")])),
        ("stale content revision", 409, commit("e03-x2", [create()], "0" * 64)),
        (
            "stale schema revision",
            409,
            commit("e03-x3", [create()], base_schema_revision="f" * 64),
        ),
        ("empty operations", 422, commit("e03-x4", [])),
        (
            "geometry family mismatch",
            422,
            commit(
                "e03-x5",
                [create(geometry={"type": "LineString", "coordinates": [[0, 0], [1, 1]]})],
            ),
        ),
        (
            "3D coordinates",
            422,
            commit("e03-x6", [create(geometry={"type": "Point", "coordinates": [1, 2, 3]})]),
        ),
        ("unknown attribute", 422, commit("e03-x7", [create(properties=props | {"zz": 1})])),
        (
            "typed attribute mismatch",
            422,
            commit("e03-x8", [create(properties=props | {"metric_val": "7"})]),
        ),
        (
            "update missing feature",
            404,
            commit("e03-x9", [{"op_type": "update", "feature_id": "e03-none"}]),
        ),
        ("create existing feature", 409, commit("e03-x10", [create(existing_id)])),
        (
            "no material change",
            409,
            commit(
                "e03-x11",
                [{"op_type": "update", "feature_id": existing_id, "properties": existing_props}],
            ),
        ),
        ("duplicate feature", 422, commit("e03-x12", [create(), create()])),
    ]:
        parity.same(f"E03 commit {label}", "POST", changesets, status, headers=write, body=body)
    parity.same(
        "E03 commit unknown source",
        "POST",
        f"{root}/sources/{uuid4()}/changesets",
        404,
        headers=write,
        body=valid,
    )
    parity.same(
        "E03 commit invalid source path",
        "POST",
        f"{root}/sources/not-a-uuid/changesets",
        422,
        headers=write,
        body=valid,
    )

    created = parity.cross(
        "E03 FastAPI create replayed by Rust",
        "oracle",
        changesets,
        commit("e03-py-create", [create()]),
        write,
    )
    rev1 = created.get("resulting_content_revision", revision)
    moved = {"type": "Point", "coordinates": [105.9, 21.1]}
    changed = {"code": "E03-1b", "ward": "W2", "metric_val": -5, "active": False}
    updated = parity.cross(
        "E03 Rust update replayed by FastAPI",
        "rust",
        changesets,
        commit(
            "e03-rs-update",
            [
                {
                    "op_type": "update",
                    "feature_id": "e03-p1",
                    "geometry": moved,
                    "properties": changed,
                }
            ],
            rev1,
        ),
        write,
    )

    def digest_matches():
        expected = compute_content_revision(
            UUID(source_id),
            rev1,
            [
                {
                    "op": "update",
                    "fid": "e03-p1",
                    "props": changed,
                    "wkt": geometry_to_wkt(moved),
                }
            ],
        )
        assert updated.get("resulting_content_revision") == expected, (
            "Rust content revision differs from the Python digest"
        )

    parity.record("E03 Rust content revision equals Python digest", digest_matches)
    rev2 = updated.get("resulting_content_revision", rev1)
    parity.same(
        "E03 operation id reused with a different request",
        "POST",
        changesets,
        409,
        headers=write,
        body=commit("e03-py-create", [create()], rev2, summary="different"),
    )
    second = parity.cross(
        "E03 Rust create replayed by FastAPI",
        "rust",
        changesets,
        commit("e03-rs-create", [create("e03-p2", moved, props)], rev2),
        write,
    )
    rev3 = second.get("resulting_content_revision", rev2)
    deleted = parity.cross(
        "E03 Rust delete replayed by FastAPI",
        "rust",
        changesets,
        commit("e03-rs-delete", [{"op_type": "delete", "feature_id": "e03-p2"}], rev3),
        write,
    )
    rev4 = deleted.get("resulting_content_revision", rev3)

    owner_read = _headers(owner)
    parity.same("E03 changeset list", "GET", changesets, 200, headers=owner_read)
    parity.same(
        "E03 changeset list limit", "GET", f"{changesets}?limit=2", 200, headers=owner_read
    )
    parity.same(
        "E03 changeset list limit out of range",
        "GET",
        f"{changesets}?limit=0",
        422,
        headers=owner_read,
    )
    parity.same("E03 changeset list unauthenticated", "GET", changesets, 401, headers={})
    parity.same(
        "E03 changeset list member without grant",
        "GET",
        changesets,
        403,
        headers=_headers(surveyor),
    )
    detail = f"{root}/changesets/{deleted.get('id', uuid4())}"
    parity.same("E03 changeset detail", "GET", detail, 200, headers=owner_read)
    parity.same(
        "E03 changeset detail foreign tenant",
        "GET",
        detail,
        404,
        headers=_headers(fixture["session_b_token"]),
    )
    parity.same(
        "E03 changeset detail unknown",
        "GET",
        f"{root}/changesets/{uuid4()}",
        404,
        headers=owner_read,
    )
    parity.same(
        "E03 changeset detail invalid id",
        "GET",
        f"{root}/changesets/not-a-uuid",
        422,
        headers=owner_read,
    )

    stale_target = f"{root}/changesets/{updated.get('id', uuid4())}/revert"
    parity.same(
        "E03 revert of an older changeset is stale",
        "POST",
        stale_target,
        409,
        headers=write,
        body={"operation_id": "e03-revert-stale"},
    )
    parity.same(
        "E03 revert unknown changeset",
        "POST",
        f"{root}/changesets/{uuid4()}/revert",
        404,
        headers=write,
        body={"operation_id": "e03-revert-missing"},
    )
    parity.same(
        "E03 revert without CSRF",
        "POST",
        f"{detail}/revert",
        403,
        headers=owner_read,
        body={"operation_id": "e03-revert-csrf"},
    )
    reverted = parity.cross(
        "E03 Rust revert replayed by FastAPI",
        "rust",
        f"{detail}/revert",
        {"operation_id": "e03-rs-revert"},
        write,
    )
    rev5 = reverted.get("resulting_content_revision", rev4)

    # A database guard (SQLSTATE OV409, as the signed survey-point trigger raises)
    # must surface as 409 DATA_LOCKED with the trigger message on both sides.
    with psycopg.connect(owner_url) as conn:
        conn.execute(
            """CREATE FUNCTION spatial.e03_parity_lock() RETURNS trigger
               LANGUAGE plpgsql AS $$ BEGIN
                 RAISE EXCEPTION USING ERRCODE='OV409',
                   MESSAGE='Điểm thuộc thôn parity đã được ký duyệt đối chiếu.';
               END $$"""
        )
        conn.execute(
            """CREATE TRIGGER e03_parity_lock
               BEFORE UPDATE OF attributes, geometry OR DELETE ON spatial.reference_features
               FOR EACH ROW WHEN (OLD.feature_id = 'e03-p1')
               EXECUTE FUNCTION spatial.e03_parity_lock()"""
        )
    try:
        parity.same(
            "E03 OV409 database guard -> DATA_LOCKED",
            "POST",
            changesets,
            409,
            headers=write,
            body=commit("e03-locked", [{"op_type": "delete", "feature_id": "e03-p1"}], rev5),
        )
    finally:
        with psycopg.connect(owner_url) as conn:
            conn.execute("DROP TRIGGER e03_parity_lock ON spatial.reference_features")
            conn.execute("DROP FUNCTION spatial.e03_parity_lock()")

    snap = f"{root}/editing/snap"
    segment = [[[0, 0], [10, 0]]]
    for label, status, headers, body in [
        ("vertex", 200, write, {"target": [1, 1], "vertices": [[1.5, 1.5]], "tolerance": 2}),
        ("segment", 200, write, {"target": [4, 1], "segments": segment, "tolerance": 2}),
        ("miss", 200, write, {"target": [4, 9], "segments": segment, "tolerance": 2}),
        ("tolerance out of range", 422, write, {"target": [0, 0], "tolerance": 0}),
        ("unauthenticated", 401, {}, {"target": [0, 0], "tolerance": 1}),
        ("without CSRF", 403, owner_read, {"target": [0, 0], "tolerance": 1}),
        ("member without grant", 403, surveyor_write, {"target": [0, 0], "tolerance": 1}),
    ]:
        parity.same(f"E03 snap {label}", "POST", snap, status, headers=headers, body=body)
