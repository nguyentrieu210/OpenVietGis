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

def compare_scoped_editing(suite, owner_url, fixture, surveyor, difference, member_id, user_id):
    """Exercise positive and negative ACL edits against the same PostGIS state."""
    from openvietgis_api.security.csrf import make_csrf_token

    parity = EditingParity(suite, difference)
    admin = _headers(fixture["session_a_token"], fixture["csrf_a_token"])
    editor = _headers(surveyor, make_csrf_token(surveyor, baseline.DEFAULT_CSRF_SECRET))
    org = f"{API}/organizations/{fixture['org_a_id']}"
    root = f"{org}/projects/{fixture['project_a_id']}"
    layer = fixture["layers"]["edit_4326"]
    sid = layer["source_id"]
    path = f"{root}/sources/{sid}/changesets"
    grant = suite.oracle.post(
        f"{org}/grants", headers=admin,
        json={"subject_kind": "membership", "subject_id": str(member_id),
              "scope_kind": "project", "project_id": fixture["project_a_id"], "role": "editor"},
    )
    assert grant.status_code == 201, grant.text
    grant_id = grant.json()["id"]
    rule_ids = []

    def rule(ids, actions):
        response = suite.oracle.post(
            f"{root}/access/rules", headers=admin,
            json={"user_id": str(user_id), "layer_id": layer["layer_id"],
                  "actions": actions, "feature_ids": ids},
        )
        assert response.status_code == 201, response.text
        rule_id = response.json()["id"]
        rule_ids.append(rule_id)
        return rule_id

    def revoke(rule_id):
        response = suite.oracle.delete(f"{root}/access/rules/{rule_id}", headers=admin)
        assert response.status_code == 204, response.text
        rule_ids.remove(rule_id)

    def snapshot():
        with psycopg.connect(owner_url) as conn:
            revision = conn.execute(
                "SELECT content_revision FROM spatial.data_sources WHERE id=%s", (sid,)
            ).fetchone()[0]
            rows = conn.execute(
                """SELECT feature_id, attributes FROM spatial.reference_features
                   WHERE source_id=%s ORDER BY feature_id LIMIT 2""", (sid,)
            ).fetchall()
            return revision, rows

    def payload(op_id, rev, operations):
        return {"operation_id": op_id, "base_content_revision": rev,
                "summary": "E03 scoped ACL parity", "operations": operations}

    try:
        rev, rows = snapshot()
        (allowed, props), (hidden, _) = rows
        r = rule([allowed], ["read", "geometry"])
        point = {"type": "Point", "coordinates": [105.98, 21.12]}
        update = {"op_type": "update", "feature_id": allowed, "geometry": point}
        forbidden = {"op_type": "update", "feature_id": hidden, "geometry": point}
        parity.same("E03 ACL hidden feature denied", "POST", path, 403,
                    headers=editor, body=payload("e03-acl-hidden", rev, [forbidden]))
        parity.same("E03 ACL mixed batch denied atomically", "POST", path, 403,
                    headers=editor, body=payload("e03-acl-batch", rev, [update, forbidden]))
        parity.same("E03 ACL attributes denied", "POST", path, 403, headers=editor,
                    body=payload("e03-acl-attrs", rev, [
                        {"op_type": "update", "feature_id": allowed,
                         "properties": props | {"code": "ACL-FORBIDDEN"}}
                    ]))
        parity.record("E03 ACL rejected writes preserve revision",
                      lambda: _assert_revision(snapshot()[0], rev))
        body = payload("e03-acl-update", rev, [update])
        receipt = parity.cross("E03 ACL allowed Rust edit replayed by FastAPI",
                               "rust", path, body, editor)
        if receipt:
            parity.same("E03 ACL history still denied", "GET", path, 403, headers=editor)
            parity.same("E03 ACL revert denied", "POST",
                        f"{root}/changesets/{receipt['id']}/revert", 403,
                        headers=editor, body={"operation_id": "e03-acl-revert"})
        revoke(r)
        rule([hidden], ["read", "geometry"])
        parity.same("E03 ACL replay rechecks revoked feature scope", "POST", path, 403,
                    headers=editor, body=body)
    finally:
        for rule_id in list(rule_ids):
            revoke(rule_id)
        response = suite.oracle.post(f"{org}/grants/{grant_id}/revoke", headers=admin)
        assert response.status_code == 204, response.text


def _assert_revision(actual, expected):
    assert actual == expected, "Rejected ACL operation changed content revision"


def compare_service_editing(suite, owner_url, fixture, difference):
    """Machine credentials have no human fallback, obey ceiling and live grants."""
    parity = EditingParity(suite, difference)
    admin = _headers(fixture["session_a_token"], fixture["csrf_a_token"])
    org = f"{API}/organizations/{fixture['org_a_id']}"
    root = f"{org}/projects/{fixture['project_a_id']}"
    source_id = fixture["layers"]["edit_4326"]["source_id"]
    path = f"{root}/sources/{source_id}/changesets"
    snap = f"{root}/editing/snap"
    principal = suite.oracle.post(
        f"{org}/service-principals", headers=admin, json={"name": "E03 parity service"},
    )
    assert principal.status_code == 201, principal.text
    pid = principal.json()["id"]
    credential_path = f"{org}/service-principals/{pid}/credentials"
    grant = suite.oracle.post(
        f"{org}/grants", headers=admin,
        json={"subject_kind": "service_principal", "subject_id": pid,
              "scope_kind": "project", "project_id": fixture["project_a_id"], "role": "editor"},
    )
    assert grant.status_code == 201, grant.text

    def credential(name, ceiling=None):
        response = suite.oracle.post(
            credential_path, headers=admin, json={"name": name, "permission_ceiling": ceiling},
        )
        assert response.status_code == 201, f"Issue credential: {response.status_code}"
        data = response.json()
        return data["id"], {"authorization": f"Bearer {data['secret']}"}

    full_id, full = credential("full")
    _, read_only = credential("read only", ["project.read"])
    with psycopg.connect(owner_url) as conn:
        revision = conn.execute(
            "SELECT content_revision FROM spatial.data_sources WHERE id=%s", (source_id,)
        ).fetchone()[0]
    body = {
        "operation_id": "e03-service-create", "base_content_revision": revision,
        "summary": "Tạo đối tượng 🗺",
        "operations": [{"op_type": "create", "feature_id": "e03-service",
                        "geometry": {"type": "Point", "coordinates": [105.84, 21.02]},
                        "properties": {"code": "Mã địa chính", "ward": "W1",
                                       "metric_val": 7, "active": True}}],
    }
    parity.same("E03 bearer read-only ceiling denies writes", "POST", path, 403,
                headers=read_only, body=body)
    parity.same("E03 bearer read-only ceiling permits list", "GET", path, 200, headers=read_only)
    parity.same("E03 invalid bearer does not fall back to owner cookie", "POST", path, 401,
                headers=admin | {"authorization": "Bearer invalid"}, body=body)
    foreign = f"{API}/organizations/{fixture['org_b_id']}/projects/{fixture['project_b_id']}/editing/snap"
    parity.same("E03 bearer cross-tenant denied", "POST", foreign, 401, headers=full,
                body={"target": [0, 0], "tolerance": 1})
    parity.same("E03 bearer snap without CSRF", "POST", snap, 200, headers=full,
                body={"target": [0, 0], "vertices": [[1, 0]], "tolerance": 1})
    receipt = parity.cross("E03 bearer Rust edit replayed by FastAPI with Unicode digest",
                           "rust", path, body, full)
    if receipt:
        def actor_matches():
            assert receipt["actor_type"] == "service_principal"
            assert receipt["actor_id"] == pid
            with psycopg.connect(owner_url) as conn:
                assert conn.execute(
                    "SELECT actor_type, actor_id FROM audit.audit_events WHERE id=%s",
                    (receipt["audit_event_id"],),
                ).fetchone() == ("service_principal", UUID(pid))
                assert conn.execute(
                    "SELECT last_used_at IS NOT NULL FROM org.service_credentials WHERE id=%s",
                    (full_id,),
                ).fetchone()[0]
        parity.record("E03 bearer audit actor and credential use", actor_matches)
        detail = f"{root}/changesets/{receipt['id']}"
        parity.same("E03 bearer detail", "GET", detail, 200, headers=full)
        parity.same("E03 bearer replay cannot impersonate service as human", "POST", path, 409,
                    headers=admin, body=body)
        parity.cross("E03 bearer Rust revert replayed by FastAPI",
                     "rust", f"{detail}/revert", {"operation_id": "e03-service-revert"}, full)
    response = suite.oracle.post(f"{credential_path}/{full_id}/revoke", headers=admin)
    assert response.status_code == 204, response.status_code
    parity.same("E03 revoked bearer denied before replay", "POST", path, 401, headers=full, body=body)
    response = suite.oracle.post(f"{org}/grants/{grant.json()['id']}/revoke", headers=admin)
    assert response.status_code == 204, response.status_code
    parity.same("E03 bearer live grant revocation denies history", "GET", path, 403, headers=read_only)


def compare_topology_and_concurrency(suite, owner_url, fixture, difference):
    """Exercise real GEOS topology, feature leases and concurrent cross-engine replay."""
    from concurrent.futures import ThreadPoolExecutor
    from threading import Barrier

    parity = EditingParity(suite, difference)
    admin = _headers(fixture["session_a_token"], fixture["csrf_a_token"])
    root = f"{API}/organizations/{fixture['org_a_id']}/projects/{fixture['project_a_id']}"

    def manual(family):
        response = suite.oracle.post(
            f"{root}/layers/manual", headers=admin,
            json={"operation_id": str(uuid4()), "name": f"E03 {family} topology fixture",
                  "geometry_family": family, "fields": [
                      {"field_id": "code", "name": "Code", "type": "text", "nullable": False}
                  ]},
        )
        assert response.status_code == 201, response.text
        sid = response.json()["source_id"]
        with psycopg.connect(owner_url) as conn:
            rev = conn.execute("SELECT content_revision FROM spatial.data_sources WHERE id=%s",
                               (sid,)).fetchone()[0]
        return sid, rev

    def payload(op_id, rev, ops):
        return {"operation_id": op_id, "base_content_revision": rev, "operations": ops}

    def create(fid, geometry):
        return {"op_type": "create", "feature_id": fid, "geometry": geometry,
                "properties": {"code": fid}}

    sid, rev = manual("Polygon")
    path = f"{root}/sources/{sid}/changesets"
    outer = [[105.8, 21.0], [105.84, 21.0], [105.84, 21.04],
             [105.8, 21.04], [105.8, 21.0]]
    hole = [[105.81, 21.01], [105.81, 21.02], [105.82, 21.02],
            [105.82, 21.01], [105.81, 21.01]]
    bowtie = [[105.8, 21.0], [105.84, 21.04], [105.8, 21.04],
              [105.84, 21.0], [105.8, 21.0]]
    valid = create("valid", {"type": "Polygon", "coordinates": [outer, hole]})
    invalid = create("bad", {"type": "Polygon", "coordinates": [bowtie]})
    parity.same("E03 GEOS rejects self-intersecting polygon", "POST", path, 422,
                headers=admin, body=payload("e03-bowtie", rev, [invalid]))
    parity.same("E03 invalid topology rolls back whole batch", "POST", path, 422,
                headers=admin, body=payload("e03-bowtie-batch", rev, [valid, invalid]))
    parity.same("E03 unclosed ring rejected", "POST", path, 422, headers=admin,
                body=payload("e03-open-ring", rev, [
                    create("open", {"type": "Polygon", "coordinates": [outer[:-1]]})
                ]))
    body = payload("e03-concurrent-polygon", rev, [valid])
    receipts = []

    def concurrent_replay():
        gate = Barrier(2)
        def submit(client):
            gate.wait(timeout=10)
            return client.post(path, headers=admin, json=body)

        with ThreadPoolExecutor(max_workers=2) as workers:
            a = workers.submit(submit, suite.oracle)
            b = workers.submit(submit, suite.rust)
            first, second = a.result(timeout=30), b.result(timeout=30)
        assert first.status_code == second.status_code == 201, (
            first.status_code, second.status_code, first.text[:250], second.text[:250]
        )
        assert difference(first.json(), second.json()) is None
        with psycopg.connect(owner_url) as conn:
            assert conn.execute("SELECT count(*) FROM editing.changesets WHERE source_id=%s",
                                (sid,)).fetchone()[0] == 1
            assert conn.execute(
                "SELECT feature_id, ST_IsValid(geometry), ST_NumInteriorRings(geometry) "
                "FROM spatial.reference_features WHERE source_id=%s", (sid,),
            ).fetchall() == [("valid", True, 1)]
        receipts.append(first.json())

    parity.record("E03 concurrent Python/Rust replay one valid polygon with hole", concurrent_replay)
    if receipts:
        current = receipts[0]["resulting_content_revision"]
        with psycopg.connect(owner_url) as conn:
            conn.execute(
                """INSERT INTO collaboration.feature_edit_leases
                   (organization_id, project_id, source_id, feature_id, actor_id,
                    actor_name, token, expires_at)
                   VALUES (%s,%s,%s,'valid',%s,'Other actor',%s,now()+interval '5 minutes')""",
                (fixture["org_a_id"], fixture["project_a_id"], sid, fixture["user_b_id"], uuid4()),
            )
        delete = payload("e03-lease-delete", current, [{"op_type": "delete", "feature_id": "valid"}])
        try:
            parity.same("E03 active foreign lease denies delete", "POST", path, 409,
                        headers=admin, body=delete)
            with psycopg.connect(owner_url) as conn:
                conn.execute("UPDATE collaboration.feature_edit_leases "
                             "SET expires_at=now()-interval '1 second' WHERE source_id=%s", (sid,))
            parity.cross("E03 expired foreign lease permits delete", "rust", path, delete, admin)
        finally:
            with psycopg.connect(owner_url) as conn:
                conn.execute("DELETE FROM collaboration.feature_edit_leases WHERE source_id=%s", (sid,))

    sid, rev = manual("LineString")
    path = f"{root}/sources/{sid}/changesets"
    parity.same("E03 line requires two positions", "POST", path, 422, headers=admin,
                body=payload("e03-line-short", rev, [
                    create("short", {"type": "LineString", "coordinates": [[105.8, 21.0]]})
                ]))
    parity.cross("E03 valid line Rust edit replayed by FastAPI", "rust", path,
                 payload("e03-line-valid", rev, [
                     create("line", {"type": "LineString",
                                     "coordinates": [[105.8, 21.0], [105.9, 21.1]]})
                 ]), admin)
