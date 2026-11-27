"""E11 project collaboration REST differential parity (FastAPI oracle): chat, comments,
tasks, notifications, edit leases, change reviews and the conflict-rebase report."""

import argparse
import os
import sys
from pathlib import Path
from tempfile import TemporaryDirectory
from uuid import uuid4

import psycopg

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(HERE.parent))
import run as parity
from check_managed import blank, same
from check_realtime import (
    Pair,
    fastapi_server,
    first_page,
    service_bearers,
    validation_shape,
)
from run import (
    baseline,
    configure_python,
    database,
    project_path,
    require,
    rust_client,
    scoped_member,
    session_headers,
    setup_gis_scope,
)

MINTED = ("id", "created_at", "updated_at", "expires_at", "decided_at")


def editor_member(owner, oracle, fixture, csrf, name="Biên tập viên"):
    """An unrestricted project editor with its own session and CSRF token."""
    uid, lid, sid, mid = uuid4(), uuid4(), uuid4(), uuid4()
    token, digest, csrf_token = baseline.make_session_and_csrf(sid, baseline.DEFAULT_CSRF_SECRET)
    with psycopg.connect(owner) as conn:
        conn.execute(
            """INSERT INTO identity.users (id, display_name, email_snapshot, status,
               created_at, updated_at)
               VALUES (%s, %s, 'editor@example.test', 'active', now(), now())""",
            (uid, name),
        )
        conn.execute(
            """INSERT INTO identity.identity_links (id, user_id, issuer, subject,
               email_snapshot, email_verified, display_name_snapshot, created_at)
               VALUES (%s, %s, 'https://parity-issuer.example', %s, 'editor@example.test',
                       true, %s, now())""",
            (lid, uid, f"editor-{uid}", name),
        )
        conn.execute(
            """INSERT INTO identity.web_sessions (id, user_id, identity_link_id, secret_hash,
               issued_at, expires_at)
               VALUES (%s, %s, %s, %s, now(), now() + interval '1 hour')""",
            (sid, uid, lid, digest),
        )
        conn.execute(
            """INSERT INTO org.organization_memberships (id, organization_id, user_id, role,
               status, created_at, activated_at)
               VALUES (%s, %s, %s, 'member', 'active', now(), now())""",
            (mid, fixture["org_a_id"], uid),
        )
    grant = oracle.post(
        f"/api/v1/organizations/{fixture['org_a_id']}/grants",
        headers=csrf,
        json={"subject_kind": "membership", "subject_id": str(mid), "scope_kind": "project",
              "project_id": fixture["project_a_id"], "role": "editor"},
    )
    require(grant.status_code == 201, f"editor grant: {grant.text}")
    return str(uid), session_headers(token), session_headers(
        token, **{"x-openvietgis-csrf": csrf_token}
    )


def first_feature(owner, source):
    with psycopg.connect(owner) as conn:
        return conn.execute(
            "SELECT feature_id FROM spatial.reference_features WHERE source_id=%s "
            "ORDER BY feature_id LIMIT 1",
            (source,),
        ).fetchone()[0]


def revision(owner, source):
    with psycopg.connect(owner) as conn:
        return conn.execute(
            "SELECT content_revision FROM spatial.data_sources WHERE id=%s", (source,)
        ).fetchone()[0]


def commit(oracle, owner, base, source, csrf):
    response = oracle.post(
        f"{base}/sources/{source}/changesets",
        headers=csrf,
        json={
            "operation_id": f"team-{uuid4()}",
            "base_content_revision": revision(owner, source),
            "summary": "Team parity edit",
            "operations": [{
                "op_type": "create",
                "feature_id": f"team-{uuid4().hex[:12]}",
                "geometry": {"type": "Point", "coordinates": [105.7, 21.2]},
                "properties": {"code": "T", "ward": "W1", "metric_val": 2, "active": True},
            }],
        },
    )
    require(response.status_code == 201, f"fixture changeset: {response.text}")
    return response.json()


def lease_items(body):
    return {"items": [blank("expires_at")(item) for item in body["items"]]}


def compare_chat(pair, owner, fixture, a, csrf, editor, scoped, bearers):
    base = f"{project_path(fixture['org_a_id'], fixture['project_a_id'])}/collaboration"
    editor_id, editor_read, _ = editor
    reader = bearers[0]
    members = f"{base}/members"
    same("R011 members", pair, "GET", members, 200, headers=a)
    same("R011 members 403 service principal", pair, "GET", members, 403, headers=reader)
    same("R011 members 403 GIS-scoped member", pair, "GET", members, 403, headers=scoped)
    same("R011 members 401 anonymous", pair, "GET", members, 401, headers={})

    messages = f"{base}/messages"
    same("R011 message post", pair, "POST", messages, 201, headers=csrf,
         body={"body": "  Xin chào nhóm  "}, normalize=blank(*MINTED))
    same("R011 direct message post", pair, "POST", messages, 201, headers=csrf,
         body={"body": "Riêng tư", "recipient_id": editor_id}, normalize=blank(*MINTED))
    same("R011 message 422 recipient is self", pair, "POST", messages, 422, headers=csrf,
         body={"body": "x", "recipient_id": fixture["user_a_id"]})
    same("R011 message 422 recipient not an editor", pair, "POST", messages, 422,
         headers=csrf, body={"body": "x", "recipient_id": str(uuid4())})
    same("R011 message 422 blank body", pair, "POST", messages, 422, headers=csrf,
         body={"body": "   "}, normalize=validation_shape)
    same("R011 message 403 missing CSRF", pair, "POST", messages, 403, headers=a,
         body={"body": "x"})
    same("R011 message 403 service principal", pair, "POST", messages, 403, headers=reader,
         body={"body": "x"})
    same("R011 messages public", pair, "GET", messages, 200, headers=a)
    same("R011 messages direct with peer", pair, "GET", messages, 200, headers=a,
         params={"peer": editor_id})
    same("R011 messages direct seen by peer", pair, "GET", messages, 200, headers=editor_read,
         params={"peer": fixture["user_a_id"]})
    same("R011 messages 422 limit=0", pair, "GET", messages, 422, headers=a,
         params={"limit": 0}, normalize=validation_shape)
    same("R011 messages 422 malformed peer", pair, "GET", messages, 422, headers=a,
         params={"peer": "nope"}, normalize=validation_shape)

    source = fixture["layers"]["epsg4326_1k"]["source_id"]
    feature = first_feature(owner, source)
    comments = f"{base}/comments"
    same("R011 feature comment with mention", pair, "POST", comments, 201, headers=csrf,
         body={"source_id": source, "feature_id": f" {feature} ", "body": "Kiểm tra",
               "mention_ids": [editor_id, editor_id]}, normalize=blank(*MINTED))
    same("R011 feature comment 422 mention self", pair, "POST", comments, 422, headers=csrf,
         body={"source_id": source, "feature_id": feature, "body": "x",
               "mention_ids": [fixture["user_a_id"]]})
    same("R011 feature comment 404 unknown feature", pair, "POST", comments, 404,
         headers=csrf, body={"source_id": source, "feature_id": "missing", "body": "x"})
    same("R011 feature comment 422 INVALID_FEATURE_ID", pair, "POST", comments, 422,
         headers=csrf, body={"source_id": source, "feature_id": "bad id!", "body": "x"})
    same("R011 feature comments", pair, "GET", comments, 200, headers=a,
         params={"source_id": source, "feature_id": feature})
    same("R011 feature comments 422 missing feature", pair, "GET", comments, 422, headers=a,
         params={"source_id": source}, normalize=validation_shape)
    same("R011 comment pins", pair, "GET", f"{base}/comment-pins", 200, headers=a)
    return source, feature


def compare_tasks(pair, owner, fixture, a, csrf, editor, source, feature):
    base = f"{project_path(fixture['org_a_id'], fixture['project_a_id'])}/collaboration"
    editor_id, editor_read, editor_csrf = editor
    tasks = f"{base}/tasks"
    public = pair.oracle.get(f"{base}/messages", headers=a).json()["items"][0]["id"]
    private = pair.oracle.get(f"{base}/messages", headers=a,
                              params={"peer": editor_id}).json()["items"][0]["id"]
    comment = pair.oracle.get(f"{base}/comments", headers=a,
                              params={"source_id": source, "feature_id": feature}).json()
    comment_id = comment["items"][0]["id"]
    task = {"title": " Đo đạc ", "assignee_id": editor_id, "priority": "high",
            "due_date": "2026-12-01"}
    same("R011 task create", pair, "POST", tasks, 201, headers=csrf, body=task,
         normalize=blank(*MINTED))
    same("R011 task create linked to message", pair, "POST", tasks, 201, headers=csrf,
         body=task | {"source_message_id": public}, normalize=blank(*MINTED))
    same("R011 task create linked to comment anchor", pair, "POST", tasks, 201, headers=csrf,
         body=task | {"source_message_id": comment_id}, normalize=blank(*MINTED))
    same("R011 task create anchored to feature", pair, "POST", tasks, 201, headers=csrf,
         body=task | {"source_id": source, "feature_id": feature}, normalize=blank(*MINTED))
    same("R011 task create at a point", pair, "POST", tasks, 201, headers=csrf,
         body=task | {"longitude": 105.85, "latitude": 21.03}, normalize=blank(*MINTED))
    same("R011 task 422 private message link", pair, "POST", tasks, 422, headers=csrf,
         body=task | {"source_message_id": private})
    same("R011 task 422 feature differs from comment", pair, "POST", tasks, 422,
         headers=csrf, body=task | {"source_message_id": comment_id, "source_id": source,
                                    "feature_id": "other"})
    same("R011 task 422 assignee not an editor", pair, "POST", tasks, 422, headers=csrf,
         body=task | {"assignee_id": str(uuid4())})
    for name, extra in {
        "longitude without latitude": {"longitude": 105.0},
        "source without feature": {"source_id": source},
        "bad priority": {"priority": "urgent"},
        "bad due date": {"due_date": "2026-13-01"},
    }.items():
        same(f"R011 task 422 {name}", pair, "POST", tasks, 422, headers=csrf,
             body=task | extra, normalize=validation_shape)
    same("R011 tasks", pair, "GET", tasks, 200, headers=a)
    same("R011 tasks mine", pair, "GET", tasks, 200, headers=editor_read,
         params={"mine": "true"})

    created = [pair.oracle.post(tasks, headers=csrf, json=task).json() for _ in range(2)]
    status = {"status": "in_progress", "revision": 1}
    changed = [
        client.patch(f"{tasks}/{item['id']}", headers=editor_csrf, json=status)
        for client, item in zip((pair.oracle, pair.rust), created, strict=True)
    ]
    same_bodies("R011 task status change by assignee", changed, blank(*MINTED))
    same("R011 task 409 stale revision", pair, "PATCH", f"{tasks}/{created[0]['id']}", 409,
         headers=editor_csrf, body=status)
    same("R011 task 404 unknown", pair, "PATCH", f"{tasks}/{uuid4()}", 404,
         headers=editor_csrf, body=status)
    same("R011 task 422 bad status", pair, "PATCH", f"{tasks}/{created[0]['id']}", 422,
         headers=editor_csrf, body={"status": "blocked", "revision": 2},
         normalize=validation_shape)

    inbox = f"{base}/notifications"
    same("R011 notifications for assignee", pair, "GET", inbox, 200, headers=editor_read)
    same("R011 notification read", pair, "POST", f"{inbox}/{created[0]['id']}/read", 200,
         headers=editor_csrf)
    same("R011 notification 404 creator's own task", pair, "POST",
         f"{inbox}/{created[0]['id']}/read", 404, headers=csrf)
    mention = next(
        item for item in pair.oracle.get(inbox, headers=editor_read).json()["items"]
        if item["kind"] == "mention"
    )
    same("R011 mention read", pair, "POST", f"{base}/mentions/{mention['id']}/read", 200,
         headers=editor_csrf)
    same("R011 mention 404 not mentioned", pair, "POST",
         f"{base}/mentions/{mention['id']}/read", 404, headers=csrf)
    same("R011 notifications after reads", pair, "GET", inbox, 200, headers=editor_read)


def compare_leases(pair, owner, fixture, a, csrf, editor):
    base = f"{project_path(fixture['org_a_id'], fixture['project_a_id'])}/collaboration"
    _, _, editor_csrf = editor
    source = fixture["layers"]["edit_4326"]["source_id"]
    feature = first_feature(owner, source)
    locks = f"{base}/edit-locks"
    lease = {"source_id": source, "feature_id": feature, "token": str(uuid4())}
    same("R011 lease acquire then renew", pair, "POST", locks, 200, headers=csrf, body=lease,
         normalize=blank("expires_at"))
    same("R011 lease 409 FEATURE_LOCKED for another editor", pair, "POST", locks, 409,
         headers=editor_csrf, body=lease | {"token": str(uuid4())})
    same("R011 lease 409 FEATURE_LOCKED for another token", pair, "POST", locks, 409,
         headers=csrf, body=lease | {"token": str(uuid4())})
    same("R011 leases", pair, "GET", locks, 200, headers=a, normalize=lease_items)
    same("R011 lease 404 unknown feature", pair, "POST", locks, 404, headers=csrf,
         body=lease | {"feature_id": "missing"})
    same("R011 lease 422 INVALID_FEATURE_ID", pair, "POST", locks, 422, headers=csrf,
         body=lease | {"feature_id": "bad id!"})
    same("R011 lease 422 missing token", pair, "POST", locks, 422, headers=csrf,
         body={"source_id": source, "feature_id": feature}, normalize=validation_shape)
    same("R011 lease release", pair, "POST", f"{locks}/release", 200, headers=csrf,
         body=lease)
    same("R011 leases after release", pair, "GET", locks, 200, headers=a)


def compare_reviews(pair, owner, fixture, a, csrf, editor, bearers):
    root = project_path(fixture["org_a_id"], fixture["project_a_id"])
    base = f"{root}/collaboration"
    editor_id, _, editor_csrf = editor
    reader = bearers[0]
    source = fixture["layers"]["edit_4326"]["source_id"]
    old_revision = revision(owner, source)
    changesets = [commit(pair.oracle, owner, root, source, csrf) for _ in range(3)]
    reviews = f"{base}/reviews"
    same("R011 review create", pair, "POST", reviews, 201, headers=csrf,
         body={"changeset_id": changesets[0]["id"], "assignee_user_id": editor_id},
         normalize=blank(*MINTED))
    same("R011 review 422 INVALID_ASSIGNEE", pair, "POST", reviews, 422, headers=csrf,
         body={"changeset_id": changesets[0]["id"], "assignee_user_id": str(uuid4())})
    same("R011 review 404 unknown changeset", pair, "POST", reviews, 404, headers=csrf,
         body={"changeset_id": str(uuid4()), "assignee_user_id": editor_id})
    same("R011 review 403 service reader lacks project.edit", pair, "POST", reviews, 403,
         headers=reader,
         body={"changeset_id": changesets[0]["id"], "assignee_user_id": editor_id})
    same("R011 reviews", pair, "GET", reviews, 200, headers=a)
    same("R011 reviews service reader", pair, "GET", reviews, 200, headers=reader)

    review = pair.oracle.get(reviews, headers=a).json()["items"][0]
    comments = f"{reviews}/{review['id']}/comments"
    operation = str(uuid4())
    seeded = pair.oracle.post(comments, headers=csrf,
                              json={"body": " Xem lại ", "operation_id": operation})
    require(seeded.status_code == 201, f"review comment: {seeded.text}")
    same("R011 review comment idempotent replay", pair, "POST", comments, 201, headers=csrf,
         body={"body": "Xem lại", "operation_id": operation})
    same("R011 review comment 409 COMMENT_OPERATION_CONFLICT", pair, "POST", comments, 409,
         headers=csrf, body={"body": "khác", "operation_id": operation})
    feature = (changesets[0].get("operations") or [{}])[0].get("feature_id")
    if feature:
        same("R011 review comment on a reviewed feature", pair, "POST", comments, 201,
             headers=csrf, body={"body": "Điểm này", "feature_id": feature},
             normalize=blank(*MINTED))
    same("R011 review comment 422 INVALID_REVIEW_ANCHOR", pair, "POST", comments, 422,
         headers=csrf, body={"body": "x", "feature_id": "not-in-changeset"})
    same("R011 review comment 422 blank body", pair, "POST", comments, 422, headers=csrf,
         body={"body": "   "})
    same("R011 review comment 404 unknown review", pair, "POST",
         f"{reviews}/{uuid4()}/comments", 404, headers=csrf, body={"body": "x"})
    same("R011 review comments", pair, "GET", comments, 200, headers=a)

    for changeset in changesets[1:]:
        created = pair.oracle.post(reviews, headers=csrf, json={
            "changeset_id": changeset["id"], "assignee_user_id": editor_id})
        require(created.status_code == 201, f"review: {created.text}")
    pending = [item for item in pair.oracle.get(reviews, headers=a).json()["items"]
               if item["status"] == "pending"]
    decide = {"status": "approved", "decision_note": "  Đạt  "}
    same("R011 review decide 403 not the assignee", pair, "POST",
         f"{reviews}/{pending[0]['id']}/decide", 403, headers=csrf, body=decide)
    same("R011 review decide 422 pending", pair, "POST",
         f"{reviews}/{pending[0]['id']}/decide", 422, headers=editor_csrf,
         body={"status": "pending"})
    same("R011 review decide 422 unknown status", pair, "POST",
         f"{reviews}/{pending[0]['id']}/decide", 422, headers=editor_csrf,
         body={"status": "maybe"}, normalize=validation_shape)
    decided = [
        client.post(f"{reviews}/{item['id']}/decide", headers=editor_csrf, json=decide)
        for client, item in zip((pair.oracle, pair.rust), pending[:2], strict=True)
    ]
    same_bodies("R011 review decided by the assignee", decided,
                blank(*MINTED, "changeset_id", "content_revision"))
    same("R011 review decide 409 REVIEW_ALREADY_DECIDED", pair, "POST",
         f"{reviews}/{pending[0]['id']}/decide", 409, headers=editor_csrf, body=decide)
    same("R011 activities include review events", pair, "GET", f"{base}/activities", 200,
         headers=a, normalize=first_page)

    rebase = f"{root}/sources/{source}/conflict-rebase"
    current = revision(owner, source)
    touched = (changesets[-1].get("operations") or [{}])[0].get("feature_id", "team-missing")
    draft = [
        {"op_type": "update", "feature_id": f" {touched} "},
        {"op_type": "create", "feature_id": "brand-new"},
        {"op_type": "delete", "feature_id": "gone"},
    ]
    same("R011 conflict-rebase at current revision", pair, "POST", rebase, 200,
         headers=csrf, body={"base_content_revision": current, "draft_operations": draft})
    same("R011 conflict-rebase across intervening commits", pair, "POST", rebase, 200,
         headers=csrf,
         body={"base_content_revision": old_revision, "draft_operations": draft})
    same("R011 conflict-rebase 409 REBASE_HISTORY_UNAVAILABLE", pair, "POST", rebase, 409,
         headers=csrf, body={"base_content_revision": "0" * 64, "draft_operations": draft})
    same("R011 conflict-rebase 422 INVALID_REBASE_DRAFT empty", pair, "POST", rebase, 422,
         headers=csrf, body={"base_content_revision": current, "draft_operations": []})
    same("R011 conflict-rebase 422 DUPLICATE_FEATURE_OP", pair, "POST", rebase, 422,
         headers=csrf, body={"base_content_revision": current,
                             "draft_operations": [draft[1], draft[1]]})
    same("R011 conflict-rebase 422 INVALID_REBASE_DRAFT op", pair, "POST", rebase, 422,
         headers=csrf, body={"base_content_revision": current,
                             "draft_operations": [{"op_type": "move", "feature_id": "x"}]})
    same("R011 conflict-rebase 404 unknown source", pair, "POST",
         f"{root}/sources/{uuid4()}/conflict-rebase", 404, headers=csrf,
         body={"base_content_revision": current, "draft_operations": draft})


@parity.case
def same_bodies(label, responses, normalize):
    for response in responses:
        require(response.status_code == 200, f"status {response.status_code}: {response.text}")
    left, right = (normalize(response.json()) for response in responses)
    mismatch = parity.difference(left, right)
    require(mismatch is None, mismatch)
    print(f"PASS {label}", flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate", type=Path, required=True)
    binary = parser.parse_args().candidate.resolve()
    if not binary.is_file():
        parser.error("build the Rust API first")
    admin = os.environ.get("OVG_PARITY_PG_ADMIN_URL", baseline.DEFAULT_ADMIN_PG_URL)
    with (
        TemporaryDirectory(prefix="ovg-e11-team-") as directory,
        database(admin) as (owner, runtime),
    ):
        work = Path(directory)
        configure_python(owner, runtime, work)
        baseline.run_alembic_upgrade(owner, runtime)
        fixture = baseline.seed_benchmark_database(
            owner, seed=20261126, scales=["1k"], csrf_secret=baseline.DEFAULT_CSRF_SECRET
        )
        a = session_headers(fixture["session_a_token"])
        csrf = session_headers(
            fixture["session_a_token"], **{"x-openvietgis-csrf": fixture["csrf_a_token"]}
        )
        with fastapi_server(work) as oracle, rust_client(binary, runtime, work) as rust:
            pair = Pair(oracle, rust)
            user, membership, scoped_token = scoped_member(owner, fixture)
            setup_gis_scope(
                oracle,
                fixture,
                project_path(fixture["org_a_id"], fixture["project_a_id"]),
                fixture["layers"]["epsg4326_1k"]["layer_id"],
                membership,
                user,
            )
            editor = editor_member(owner, oracle, fixture, csrf)
            bearers = service_bearers(oracle, fixture, csrf)
            scoped = session_headers(scoped_token)
            source, feature = compare_chat(pair, owner, fixture, a, csrf, editor, scoped,
                                           bearers)
            compare_tasks(pair, owner, fixture, a, csrf, editor, source, feature)
            compare_leases(pair, owner, fixture, a, csrf, editor)
            compare_reviews(pair, owner, fixture, a, csrf, editor, bearers)
    print("E11 team collaboration parity finished", flush=True)


if __name__ == "__main__":
    main()
