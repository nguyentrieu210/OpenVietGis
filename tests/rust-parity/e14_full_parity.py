#!/usr/bin/env python3
"""Full E14 differential parity across manifests, official modules, records, portability,
marketplace catalog/install, developer portal/review, and partner platform on isolated PostGIS."""

from __future__ import annotations

import argparse
import base64
import json
import os
import sys
import tempfile
from datetime import UTC, datetime
from pathlib import Path
from uuid import uuid4

import psycopg
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from cryptography.hazmat.primitives.serialization import Encoding, PublicFormat

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tests" / "rust-parity"))
sys.path.insert(0, str(ROOT / "tools" / "benchmark" / "rust-platform"))

import benchmark_runner as baseline
import run as parity
from e12_catalog import set_project_role

API = "/api/v1"


def req(cond: bool, msg: str) -> None:
    if not cond:
        raise AssertionError(msg)


def pub_b64(key: Ed25519PrivateKey) -> str:
    return base64.b64encode(
        key.public_key().public_bytes(Encoding.Raw, PublicFormat.Raw)
    ).decode()


def sign_b64(key: Ed25519PrivateKey, value: object) -> str:
    from openvietgis_api.domains.extensions.marketplace import canonical

    return base64.b64encode(key.sign(canonical(value))).decode()


def partner_manifest(ext_key: str, version: str = "1.0.0") -> dict:
    return {
        "extension_key": ext_key,
        "version": version,
        "api_compatibility": "1.x",
        "display_name": f"Partner {ext_key}",
        "description": "Sandboxed partner UI",
        "permissions": [],
        "commands": [],
        "capabilities": [
            "host_context",
            "map_view",
            "map_overlay",
            "tool_register",
        ],
        "entity_schemas": {},
        "ui_contributions": [
            {
                "id": "partner_panel",
                "slot": "project_panel",
                "label": "Partner Panel",
                "route_suffix": "/partner",
                "runtime": "sandboxed_iframe",
                "sdk_version": "1.0",
                "url": "https://plugins.example.com/v1/index.html",
            }
        ],
    }


def generic_manifest(ext_key: str, version: str = "1.0.0") -> dict:
    return {
        "extension_key": ext_key,
        "version": version,
        "api_compatibility": "1.x",
        "display_name": f"Demo {ext_key}",
        "description": "Differential test extension",
        "permissions": [f"ext.{ext_key}.read", f"ext.{ext_key}.write"],
        "commands": [
            {
                "id": "inspect",
                "label": "Inspect",
                "required_permission": f"ext.{ext_key}.read",
            }
        ],
        "capabilities": ["records"],
        "entity_schemas": {
            "note": {
                "fields": {
                    "text": {"type": "text", "required": True},
                    "score": {"type": "number", "required": False},
                    "count": {"type": "integer", "required": False},
                    "flag": {"type": "boolean", "required": False},
                }
            }
        },
        "ui_contributions": [],
    }


def norm_fields(body: object, *keys: str) -> object:
    out = json.loads(json.dumps(body))

    def walk(node: object) -> None:
        if isinstance(node, dict):
            for k in list(node.keys()):
                if k in keys:
                    node[k] = f"<{k}>"
                else:
                    walk(node[k])
        elif isinstance(node, list):
            for item in node:
                walk(item)

    walk(out)
    return out


def compare(label: str, e, a, norm=lambda x: x, *, check_cache: bool = False):
    req(
        e.status_code == a.status_code,
        f"{label}: status {e.status_code} != {a.status_code}; {e.text} vs {a.text}",
    )
    if check_cache:
        req(
            e.headers.get("cache-control") == a.headers.get("cache-control"),
            f"{label}: cache-control {e.headers.get('cache-control')!r} != {a.headers.get('cache-control')!r}",
        )
    eb = norm(e.json())
    ab = norm(a.json())
    diff = parity.difference(eb, ab)
    req(diff is None, f"{label}: {diff}\nexpected={eb}\nactual={ab}")
    print(f"PASS {label}", flush=True)
    return e.json(), a.json()


def create_user_session(owner_url: str, org_id: str, role: str, name: str):
    uid = uuid4()
    lid = uuid4()
    sid = uuid4()
    mid = uuid4()
    token, digest, csrf = baseline.make_session_and_csrf(sid, baseline.DEFAULT_CSRF_SECRET)
    with psycopg.connect(owner_url) as conn:
        conn.execute(
            """INSERT INTO identity.users (id, display_name, email_snapshot, status, created_at, updated_at)
               VALUES (%s, %s, %s, 'active', now(), now())""",
            (uid, name, f"{uid.hex[:8]}@example.test"),
        )
        conn.execute(
            """INSERT INTO identity.identity_links
               (id, user_id, issuer, subject, email_snapshot, email_verified, display_name_snapshot, created_at)
               VALUES (%s, %s, 'https://parity.example', %s, %s, true, %s, now())""",
            (lid, uid, str(uid), f"{uid.hex[:8]}@example.test", name),
        )
        conn.execute(
            """INSERT INTO identity.web_sessions
               (id, user_id, identity_link_id, secret_hash, issued_at, expires_at)
               VALUES (%s, %s, %s, %s, now(), now() + interval '1 hour')""",
            (sid, uid, lid, digest),
        )
        conn.execute(
            """INSERT INTO org.organization_memberships
               (id, organization_id, user_id, role, status, created_at, activated_at)
               VALUES (%s, %s, %s, %s, 'active', now(), now())""",
            (mid, org_id, uid, role),
        )
    return str(uid), str(mid), token, csrf


def create_second_project(owner_url: str, org_id: str, actor_id: str) -> str:
    pid = str(uuid4())
    with psycopg.connect(owner_url) as conn:
        conn.execute(
            """INSERT INTO workspace.projects
               (id, organization_id, workspace_id, name, slug, status,
                created_by_actor_type, created_by_actor_id, created_at, updated_at)
               VALUES (%s, %s, NULL, 'Target Project', %s, 'active', 'human', %s, now(), now())""",
            (pid, org_id, f"target-{pid[:8]}", actor_id),
        )
    return pid


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--candidate", type=Path, required=True)
    args = ap.parse_args()
    binary = args.candidate.resolve()
    req(binary.is_file() and os.access(binary, os.X_OK), "candidate binary missing")
    admin_url = os.environ.get("OVG_PARITY_PG_ADMIN_URL", baseline.DEFAULT_ADMIN_PG_URL)

    with tempfile.TemporaryDirectory(prefix="ovg-e14-full-") as d:
        work = Path(d)
        with parity.database(admin_url) as (owner_url, runtime_url):
            art = work / "artifacts"
            art.mkdir()
            parity.configure_python(owner_url, runtime_url, art)
            baseline.run_alembic_upgrade(owner_url, runtime_url)
            f = baseline.seed_benchmark_database(
                owner_url,
                seed=20261126,
                scales=["1k"],
                csrf_secret=baseline.DEFAULT_CSRF_SECRET,
            )

            # Create member, developer users, reviewer, and second project before starting apps.
            member_uid, _, member_tok, member_csrf = create_user_session(
                owner_url, f["org_a_id"], "member", "Org Member"
            )
            dev_o_uid, _, dev_o_tok, dev_o_csrf = create_user_session(
                owner_url, f["org_a_id"], "member", "Dev Oracle"
            )
            dev_r_uid, _, dev_r_tok, dev_r_csrf = create_user_session(
                owner_url, f["org_a_id"], "member", "Dev Rust"
            )
            rev_uid, _, rev_tok, rev_csrf = create_user_session(
                owner_url, f["org_b_id"], "member", "Platform Reviewer"
            )
            target_project_id = create_second_project(owner_url, f["org_a_id"], f["user_a_id"])

            # Prepare signed marketplace catalog.
            op_key = Ed25519PrivateKey.generate()
            acme_key = Ed25519PrivateKey.generate()
            acme_manifest_o = partner_manifest("partner_acme_oracle_tool")
            acme_manifest_r = partner_manifest("partner_acme_rust_tool")
            rel_o = {"publisher_id": "acme", "manifest": acme_manifest_o}
            rel_o["signature"] = sign_b64(acme_key, rel_o)
            rel_r = {"publisher_id": "acme", "manifest": acme_manifest_r}
            rel_r["signature"] = sign_b64(acme_key, rel_r)
            catalog_payload = {
                "schema": "openvietgis.marketplace.v1",
                "publishers": [{"id": "acme", "name": "Acme Geo", "public_key": pub_b64(acme_key)}],
                "releases": [rel_o, rel_r],
            }
            catalog_path = work / "catalog.json"
            catalog_bytes = json.dumps(
                {"payload": catalog_payload, "signature": sign_b64(op_key, catalog_payload)}
            )
            catalog_path.write_text(catalog_bytes, encoding="utf-8")

            op_pub = pub_b64(op_key)
            os.environ["OPENVIETGIS_MARKETPLACE_CATALOG_PATH"] = str(catalog_path)
            os.environ["OPENVIETGIS_MARKETPLACE_OPERATOR_PUBLIC_KEY"] = op_pub
            os.environ["OPENVIETGIS_MARKETPLACE_REVIEWER_USER_IDS"] = json.dumps([rev_uid])

            from fastapi.testclient import TestClient
            from openvietgis_api.adapters.persistence.db import get_engine
            from openvietgis_api.config import get_settings
            from openvietgis_api.main import app

            settings = get_settings()
            settings.marketplace_catalog_path = str(catalog_path)
            settings.marketplace_operator_public_key = op_pub
            from uuid import UUID

            settings.marketplace_reviewer_user_ids = [UUID(rev_uid)]

            org = f"{API}/organizations/{f['org_a_id']}"
            proj = f"{org}/projects/{f['project_a_id']}"
            proj2 = f"{org}/projects/{target_project_id}"
            owner_h = parity.session_headers(f["session_a_token"])
            owner_csrf = parity.session_headers(
                f["session_a_token"], **{"x-openvietgis-csrf": f["csrf_a_token"]}
            )
            member_h = parity.session_headers(member_tok)
            member_csrf_h = parity.session_headers(member_tok, **{"x-openvietgis-csrf": member_csrf})
            foreign_h = parity.session_headers(f["session_b_token"])
            foreign_csrf_h = parity.session_headers(
                f["session_b_token"], **{"x-openvietgis-csrf": f["csrf_b_token"]}
            )

            try:
                with (
                    TestClient(app, raise_server_exceptions=False) as oracle,
                    parity.rust_client(binary, runtime_url, work) as rust,
                ):
                    # --- 1. Official Modules & Manifests ---
                    compare(
                        "E14 list official catalog",
                        oracle.get(f"{org}/extensions/official", headers=owner_h),
                        rust.get(f"{org}/extensions/official", headers=owner_h),
                    )
                    compare(
                        "E14 register official unknown 404",
                        oracle.post(f"{org}/extensions/official/unknown_mod/register", headers=owner_csrf),
                        rust.post(f"{org}/extensions/official/unknown_mod/register", headers=owner_csrf),
                    )
                    compare(
                        "E14 register official denied member",
                        oracle.post(f"{org}/extensions/official/house_numbering/register", headers=member_csrf_h),
                        rust.post(f"{org}/extensions/official/house_numbering/register", headers=member_csrf_h),
                    )
                    compare(
                        "E14 register official no CSRF",
                        oracle.post(f"{org}/extensions/official/house_numbering/register", headers=owner_h),
                        rust.post(f"{org}/extensions/official/house_numbering/register", headers=owner_h),
                    )
                    reg_o = oracle.post(f"{org}/extensions/official/house_numbering/register", headers=owner_csrf)
                    reg_r = rust.post(f"{org}/extensions/official/house_numbering/register", headers=owner_csrf)
                    compare("E14 register official idempotent", reg_o, reg_r)

                    # Register custom manifests (paired oracle/rust)
                    compare(
                        "E14 reject official key on generic manifest endpoint",
                        oracle.post(
                            f"{org}/extensions/manifests",
                            json=generic_manifest("house_numbering"),
                            headers=owner_csrf,
                        ),
                        rust.post(
                            f"{org}/extensions/manifests",
                            json=generic_manifest("house_numbering"),
                            headers=owner_csrf,
                        ),
                    )
                    compare(
                        "E14 reject member manifest registration",
                        oracle.post(
                            f"{org}/extensions/manifests",
                            json=generic_manifest("member_nope"),
                            headers=member_csrf_h,
                        ),
                        rust.post(
                            f"{org}/extensions/manifests",
                            json=generic_manifest("member_nope"),
                            headers=member_csrf_h,
                        ),
                    )
                    compare(
                        "E14 reject invalid permission namespace",
                        oracle.post(
                            f"{org}/extensions/manifests",
                            json={**generic_manifest("bad_perm"), "permissions": ["ext.other.read"]},
                            headers=owner_csrf,
                        ),
                        rust.post(
                            f"{org}/extensions/manifests",
                            json={**generic_manifest("bad_perm"), "permissions": ["ext.other.read"]},
                            headers=owner_csrf,
                        ),
                    )
                    mo, mr = compare(
                        "E14 register generic manifest",
                        oracle.post(
                            f"{org}/extensions/manifests",
                            json=generic_manifest("demo_oracle"),
                            headers=owner_csrf,
                        ),
                        rust.post(
                            f"{org}/extensions/manifests",
                            json=generic_manifest("demo_rust"),
                            headers=owner_csrf,
                        ),
                        lambda b: norm_fields(b, "id", "extension_key", "permissions", "commands", "display_name", "created_at"),
                    )
                    compare(
                        "E14 duplicate manifest rejected",
                        oracle.post(
                            f"{org}/extensions/manifests",
                            json=generic_manifest("demo_oracle"),
                            headers=owner_csrf,
                        ),
                        rust.post(
                            f"{org}/extensions/manifests",
                            json=generic_manifest("demo_rust"),
                            headers=owner_csrf,
                        ),
                    )
                    compare(
                        "E14 list org manifests",
                        oracle.get(f"{org}/extensions/manifests", headers=owner_h),
                        rust.get(f"{org}/extensions/manifests", headers=owner_h),
                    )
                    compare(
                        "E14 list org manifests foreign denied",
                        oracle.get(f"{org}/extensions/manifests", headers=foreign_h),
                        rust.get(f"{org}/extensions/manifests", headers=foreign_h),
                    )

                    # Service principal: allowed on list_manifests, forbidden on register_manifest
                    sp_res = oracle.post(
                        f"{org}/service-principals",
                        json={"name": "e14-svc"},
                        headers=owner_csrf,
                    )
                    req(sp_res.status_code == 201, sp_res.text)
                    sp_id = sp_res.json()["id"]
                    grant_res = oracle.post(
                        f"{org}/grants",
                        json={
                            "subject_kind": "service_principal",
                            "subject_id": sp_id,
                            "scope_kind": "organization",
                            "role": "editor",
                        },
                        headers=owner_csrf,
                    )
                    req(grant_res.status_code == 201, grant_res.text)
                    cred_res = oracle.post(
                        f"{org}/service-principals/{sp_id}/credentials",
                        json={"name": "e14-cred", "permission_ceiling": ["project.read", "project.edit"]},
                        headers=owner_csrf,
                    )
                    req(cred_res.status_code == 201, cred_res.text)
                    svc_h = {"authorization": f"Bearer {cred_res.json()['secret']}"}
                    compare(
                        "E14 service principal list manifests",
                        oracle.get(f"{org}/extensions/manifests", headers=svc_h),
                        rust.get(f"{org}/extensions/manifests", headers=svc_h),
                    )
                    compare(
                        "E14 service principal register manifest denied",
                        oracle.post(
                            f"{org}/extensions/manifests",
                            json=generic_manifest("svc_denied"),
                            headers=svc_h,
                        ),
                        rust.post(
                            f"{org}/extensions/manifests",
                            json=generic_manifest("svc_denied"),
                            headers=svc_h,
                        ),
                    )

                    # --- 2. Records, Links, Replay, History ---
                    mid_o, mid_r = mo["id"], mr["id"]
                    req(
                        oracle.post(f"{proj}/extensions/activations", json={"manifest_id": mid_o}, headers=owner_csrf).status_code == 201,
                        "activate demo_oracle",
                    )
                    req(
                        rust.post(f"{proj}/extensions/activations", json={"manifest_id": mid_r}, headers=owner_csrf).status_code == 201,
                        "activate demo_rust",
                    )

                    # Also activate in target project for portable import test
                    req(
                        oracle.post(f"{proj2}/extensions/activations", json={"manifest_id": mid_o}, headers=owner_csrf).status_code == 201,
                        "activate demo_oracle in proj2",
                    )
                    req(
                        rust.post(f"{proj2}/extensions/activations", json={"manifest_id": mid_r}, headers=owner_csrf).status_code == 201,
                        "activate demo_rust in proj2",
                    )

                    rec_o_url = f"{proj}/extensions/demo_oracle/records"
                    rec_r_url = f"{proj}/extensions/demo_rust/records"

                    compare(
                        "E14 create record missing CSRF",
                        oracle.post(rec_o_url, json={"entity_name": "note", "payload": {"text": "x"}}, headers=owner_h),
                        rust.post(rec_r_url, json={"entity_name": "note", "payload": {"text": "x"}}, headers=owner_h),
                    )
                    compare(
                        "E14 create record unknown entity",
                        oracle.post(rec_o_url, json={"entity_name": "bad", "payload": {"text": "x"}}, headers=owner_csrf),
                        rust.post(rec_r_url, json={"entity_name": "bad", "payload": {"text": "x"}}, headers=owner_csrf),
                    )
                    compare(
                        "E14 create record schema mismatch bool as int",
                        oracle.post(rec_o_url, json={"entity_name": "note", "payload": {"text": "x", "count": True}}, headers=owner_csrf),
                        rust.post(rec_r_url, json={"entity_name": "note", "payload": {"text": "x", "count": True}}, headers=owner_csrf),
                    )
                    compare(
                        "E14 create record dangling feature link",
                        oracle.post(rec_o_url, json={"entity_name": "note", "payload": {"text": "x"}, "linked_feature_id": "f1"}, headers=owner_csrf),
                        rust.post(rec_r_url, json={"entity_name": "note", "payload": {"text": "x"}, "linked_feature_id": "f1"}, headers=owner_csrf),
                    )

                    layer_info = f["layers"]["epsg4326_1k"]
                    compare(
                        "E14 create linked record stale link context",
                        oracle.post(
                            rec_o_url,
                            json={"entity_name": "note", "payload": {"text": "x"}, "linked_layer_id": layer_info["layer_id"]},
                            headers=owner_csrf,
                        ),
                        rust.post(
                            rec_r_url,
                            json={"entity_name": "note", "payload": {"text": "x"}, "linked_layer_id": layer_info["layer_id"]},
                            headers=owner_csrf,
                        ),
                    )

                    op_id = str(uuid4())
                    co, cr = compare(
                        "E14 create record with operation_id",
                        oracle.post(
                            rec_o_url,
                            json={"entity_name": "note", "payload": {"text": "Hà Nội", "score": 12.0}, "operation_id": op_id},
                            headers=owner_csrf,
                        ),
                        rust.post(
                            rec_r_url,
                            json={"entity_name": "note", "payload": {"text": "Hà Nội", "score": 12.0}, "operation_id": op_id},
                            headers=owner_csrf,
                        ),
                        lambda b: norm_fields(b, "id", "extension_key", "manifest_id", "created_at", "updated_at"),
                    )
                    compare(
                        "E14 create record replay idempotent",
                        oracle.post(
                            rec_o_url,
                            json={"entity_name": "note", "payload": {"text": "different"}, "operation_id": op_id},
                            headers=owner_csrf,
                        ),
                        rust.post(
                            rec_r_url,
                            json={"entity_name": "note", "payload": {"text": "different"}, "operation_id": op_id},
                            headers=owner_csrf,
                        ),
                        lambda b: norm_fields(b, "id", "extension_key", "manifest_id", "created_at", "updated_at"),
                    )
                    rid_o, rid_r = co["id"], cr["id"]

                    # Service principal create & list records
                    compare(
                        "E14 service principal list records",
                        oracle.get(rec_o_url, headers=svc_h),
                        rust.get(rec_r_url, headers=svc_h),
                        lambda b: norm_fields(b, "id", "extension_key", "manifest_id", "created_at", "updated_at"),
                    )

                    # Update: no-op with semantic 12 == 12.0
                    compare(
                        "E14 update record semantic noop",
                        oracle.put(
                            f"{rec_o_url}/{rid_o}",
                            json={"expected_revision": 1, "expected_manifest_id": mid_o, "payload": {"text": "Hà Nội", "score": 12}},
                            headers=owner_csrf,
                        ),
                        rust.put(
                            f"{rec_r_url}/{rid_r}",
                            json={"expected_revision": 1, "expected_manifest_id": mid_r, "payload": {"text": "Hà Nội", "score": 12}},
                            headers=owner_csrf,
                        ),
                        lambda b: norm_fields(b, "id", "extension_key", "manifest_id", "created_at", "updated_at"),
                    )
                    # Material update
                    upd_op = str(uuid4())
                    compare(
                        "E14 update record material",
                        oracle.put(
                            f"{rec_o_url}/{rid_o}",
                            json={"expected_revision": 1, "expected_manifest_id": mid_o, "payload": {"text": "Updated", "count": 5}, "operation_id": upd_op},
                            headers=owner_csrf,
                        ),
                        rust.put(
                            f"{rec_r_url}/{rid_r}",
                            json={"expected_revision": 1, "expected_manifest_id": mid_r, "payload": {"text": "Updated", "count": 5}, "operation_id": upd_op},
                            headers=owner_csrf,
                        ),
                        lambda b: norm_fields(b, "id", "extension_key", "manifest_id", "created_at", "updated_at"),
                    )
                    compare(
                        "E14 update record stale revision",
                        oracle.put(
                            f"{rec_o_url}/{rid_o}",
                            json={"expected_revision": 1, "payload": {"text": "Stale"}},
                            headers=owner_csrf,
                        ),
                        rust.put(
                            f"{rec_r_url}/{rid_r}",
                            json={"expected_revision": 1, "payload": {"text": "Stale"}},
                            headers=owner_csrf,
                        ),
                    )
                    # Replay conflict on another record
                    other_o = oracle.post(rec_o_url, json={"entity_name": "note", "payload": {"text": "Other"}}, headers=owner_csrf).json()
                    other_r = rust.post(rec_r_url, json={"entity_name": "note", "payload": {"text": "Other"}}, headers=owner_csrf).json()
                    compare(
                        "E14 update operation_id replay conflict",
                        oracle.put(
                            f"{rec_o_url}/{other_o['id']}",
                            json={"expected_revision": 1, "payload": {"text": "X"}, "operation_id": upd_op},
                            headers=owner_csrf,
                        ),
                        rust.put(
                            f"{rec_r_url}/{other_r['id']}",
                            json={"expected_revision": 1, "payload": {"text": "X"}, "operation_id": upd_op},
                            headers=owner_csrf,
                        ),
                    )
                    compare(
                        "E14 list records filtered by entity_name",
                        oracle.get(rec_o_url, params={"entity_name": "note"}, headers=owner_h),
                        rust.get(rec_r_url, params={"entity_name": "note"}, headers=owner_h),
                        lambda b: norm_fields(b, "id", "extension_key", "manifest_id", "created_at", "updated_at", "operation_id"),
                    )
                    compare(
                        "E14 record history",
                        oracle.get(f"{rec_o_url}/{rid_o}/history", headers=owner_h),
                        rust.get(f"{rec_r_url}/{rid_r}/history", headers=owner_h),
                        lambda b: norm_fields(b, "manifest_id", "occurred_at"),
                    )

                    # --- 3. Portable Export & Import ---
                    exp_o, exp_r = compare(
                        "E14 portable export",
                        oracle.get(f"{proj}/extensions/demo_oracle/portable-export", headers=owner_h),
                        rust.get(f"{proj}/extensions/demo_rust/portable-export", headers=owner_h),
                        lambda b: norm_fields(
                            b,
                            "sha256",
                            "extension_key",
                            "active_manifest_id",
                            "manifest_id",
                            "id",
                            "record_id",
                            "occurred_at",
                            "permissions",
                            "commands",
                            "display_name",
                            "activation_history",
                        ),
                        check_cache=True,
                    )
                    compare(
                        "E14 portable import digest mismatch",
                        oracle.post(
                            f"{proj2}/extensions/demo_oracle/portable-import",
                            json={"expected_manifest_id": mid_o, "bundle": exp_o["bundle"], "sha256": "0" * 64},
                            headers=owner_csrf,
                        ),
                        rust.post(
                            f"{proj2}/extensions/demo_rust/portable-import",
                            json={"expected_manifest_id": mid_r, "bundle": exp_r["bundle"], "sha256": "0" * 64},
                            headers=owner_csrf,
                        ),
                    )
                    compare(
                        "E14 portable import",
                        oracle.post(
                            f"{proj2}/extensions/demo_oracle/portable-import",
                            json={"expected_manifest_id": mid_o, **exp_o},
                            headers=owner_csrf,
                        ),
                        rust.post(
                            f"{proj2}/extensions/demo_rust/portable-import",
                            json={"expected_manifest_id": mid_r, **exp_r},
                            headers=owner_csrf,
                        ),
                        lambda b: norm_fields(b, "id", "sha256", "record_id", "source_record_id"),
                    )
                    compare(
                        "E14 portable import replay",
                        oracle.post(
                            f"{proj2}/extensions/demo_oracle/portable-import",
                            json={"expected_manifest_id": mid_o, **exp_o},
                            headers=owner_csrf,
                        ),
                        rust.post(
                            f"{proj2}/extensions/demo_rust/portable-import",
                            json={"expected_manifest_id": mid_r, **exp_r},
                            headers=owner_csrf,
                        ),
                        lambda b: norm_fields(b, "id", "sha256", "record_id", "source_record_id"),
                    )

                    # --- 4. Marketplace Catalog & Install ---
                    compare(
                        "E14 public marketplace catalog",
                        oracle.get(f"{API}/marketplace/catalog"),
                        rust.get(f"{API}/marketplace/catalog"),
                        check_cache=True,
                    )
                    compare(
                        "E14 marketplace install denied member",
                        oracle.post(
                            f"{org}/marketplace/install/acme/partner_acme_oracle_tool/1.0.0",
                            headers=member_csrf_h,
                        ),
                        rust.post(
                            f"{org}/marketplace/install/acme/partner_acme_rust_tool/1.0.0",
                            headers=member_csrf_h,
                        ),
                    )
                    compare(
                        "E14 marketplace install missing release",
                        oracle.post(
                            f"{org}/marketplace/install/acme/partner_acme_missing/1.0.0",
                            headers=owner_csrf,
                        ),
                        rust.post(
                            f"{org}/marketplace/install/acme/partner_acme_missing/1.0.0",
                            headers=owner_csrf,
                        ),
                    )
                    inst_o, inst_r = compare(
                        "E14 marketplace install release",
                        oracle.post(
                            f"{org}/marketplace/install/acme/partner_acme_oracle_tool/1.0.0",
                            headers=owner_csrf,
                        ),
                        rust.post(
                            f"{org}/marketplace/install/acme/partner_acme_rust_tool/1.0.0",
                            headers=owner_csrf,
                        ),
                        lambda b: norm_fields(b, "id", "extension_key", "display_name", "created_at"),
                    )
                    compare(
                        "E14 marketplace install duplicate rejected",
                        oracle.post(
                            f"{org}/marketplace/install/acme/partner_acme_oracle_tool/1.0.0",
                            headers=owner_csrf,
                        ),
                        rust.post(
                            f"{org}/marketplace/install/acme/partner_acme_rust_tool/1.0.0",
                            headers=owner_csrf,
                        ),
                    )

                    # Tampered catalog check
                    catalog_path.write_text("{}", encoding="utf-8")
                    compare(
                        "E14 tampered marketplace catalog rejected",
                        oracle.get(f"{API}/marketplace/catalog"),
                        rust.get(f"{API}/marketplace/catalog"),
                    )
                    compare(
                        "E14 tampered marketplace install rejected",
                        oracle.post(
                            f"{org}/marketplace/install/acme/partner_acme_oracle_tool/1.0.0",
                            headers=owner_csrf,
                        ),
                        rust.post(
                            f"{org}/marketplace/install/acme/partner_acme_rust_tool/1.0.0",
                            headers=owner_csrf,
                        ),
                    )
                    catalog_path.write_text(catalog_bytes, encoding="utf-8")

                    # --- 5. Developer Portal & Review Queue ---
                    dev_o_h = parity.session_headers(dev_o_tok)
                    dev_o_csrf_h = parity.session_headers(dev_o_tok, **{"x-openvietgis-csrf": dev_o_csrf})
                    dev_r_h = parity.session_headers(dev_r_tok)
                    dev_r_csrf_h = parity.session_headers(dev_r_tok, **{"x-openvietgis-csrf": dev_r_csrf})
                    rev_h = parity.session_headers(rev_tok)
                    rev_csrf_h = parity.session_headers(rev_tok, **{"x-openvietgis-csrf": rev_csrf})

                    compare(
                        "E14 developer me unauthenticated",
                        oracle.get(f"{API}/developer/marketplace/me"),
                        rust.get(f"{API}/developer/marketplace/me"),
                    )
                    compare(
                        "E14 developer me bearer rejected",
                        oracle.get(f"{API}/developer/marketplace/me", headers=svc_h),
                        rust.get(f"{API}/developer/marketplace/me", headers=svc_h),
                    )
                    compare(
                        "E14 developer me initial",
                        oracle.get(f"{API}/developer/marketplace/me", headers=dev_o_h),
                        rust.get(f"{API}/developer/marketplace/me", headers=dev_r_h),
                        lambda b: norm_fields(b, "user_id"),
                        check_cache=True,
                    )
                    compare(
                        "E14 reviewer me can_review",
                        oracle.get(f"{API}/developer/marketplace/me", headers=rev_h),
                        rust.get(f"{API}/developer/marketplace/me", headers=rev_h),
                        check_cache=True,
                    )

                    pub_key_o = Ed25519PrivateKey.generate()
                    pub_key_r = Ed25519PrivateKey.generate()
                    reg_body_o = {
                        "name": "Dev Studio",
                        "public_key": pub_b64(pub_key_o),
                        "proof": sign_b64(
                            pub_key_o,
                            {
                                "purpose": "openvietgis.publisher.v1",
                                "user_id": dev_o_uid,
                                "name": "Dev Studio",
                                "public_key": pub_b64(pub_key_o),
                            },
                        ),
                    }
                    reg_body_r = {
                        "name": "Dev Studio",
                        "public_key": pub_b64(pub_key_r),
                        "proof": sign_b64(
                            pub_key_r,
                            {
                                "purpose": "openvietgis.publisher.v1",
                                "user_id": dev_r_uid,
                                "name": "Dev Studio",
                                "public_key": pub_b64(pub_key_r),
                            },
                        ),
                    }
                    compare(
                        "E14 register publisher bad proof",
                        oracle.post(f"{API}/developer/marketplace/publisher", json={**reg_body_o, "name": "Wrong"}, headers=dev_o_csrf_h),
                        rust.post(f"{API}/developer/marketplace/publisher", json={**reg_body_r, "name": "Wrong"}, headers=dev_r_csrf_h),
                    )
                    po, pr = compare(
                        "E14 register publisher",
                        oracle.post(f"{API}/developer/marketplace/publisher", json=reg_body_o, headers=dev_o_csrf_h),
                        rust.post(f"{API}/developer/marketplace/publisher", json=reg_body_r, headers=dev_r_csrf_h),
                        lambda b: norm_fields(b, "id", "public_key", "created_at"),
                        check_cache=True,
                    )
                    pid_o, pid_r = po["id"], pr["id"]
                    compare(
                        "E14 register publisher duplicate",
                        oracle.post(f"{API}/developer/marketplace/publisher", json=reg_body_o, headers=dev_o_csrf_h),
                        rust.post(f"{API}/developer/marketplace/publisher", json=reg_body_r, headers=dev_r_csrf_h),
                    )

                    sub_m_o = partner_manifest(f"partner_{pid_o}_app", "1.0.0")
                    sub_m_r = partner_manifest(f"partner_{pid_r}_app", "1.0.0")
                    sub_o = {"publisher_id": pid_o, "manifest": sub_m_o, "signature": sign_b64(pub_key_o, {"publisher_id": pid_o, "manifest": sub_m_o})}
                    sub_r = {"publisher_id": pid_r, "manifest": sub_m_r, "signature": sign_b64(pub_key_r, {"publisher_id": pid_r, "manifest": sub_m_r})}

                    compare(
                        "E14 submit release unregistered denied",
                        oracle.post(f"{API}/developer/marketplace/submissions", json=sub_o, headers=member_csrf_h),
                        rust.post(f"{API}/developer/marketplace/submissions", json=sub_r, headers=member_csrf_h),
                    )
                    compare(
                        "E14 submit release bad signature",
                        oracle.post(f"{API}/developer/marketplace/submissions", json={**sub_o, "signature": "A" * 88}, headers=dev_o_csrf_h),
                        rust.post(f"{API}/developer/marketplace/submissions", json={**sub_r, "signature": "A" * 88}, headers=dev_r_csrf_h),
                    )
                    so, sr = compare(
                        "E14 submit release",
                        oracle.post(f"{API}/developer/marketplace/submissions", json=sub_o, headers=dev_o_csrf_h),
                        rust.post(f"{API}/developer/marketplace/submissions", json=sub_r, headers=dev_r_csrf_h),
                        lambda b: norm_fields(b, "id", "publisher_id", "manifest", "signature", "public_key", "created_at"),
                        check_cache=True,
                    )
                    sid_o, sid_r = so["id"], sr["id"]

                    # Withdraw flow on v1.0.1
                    sub_m_o2 = partner_manifest(f"partner_{pid_o}_app", "1.0.1")
                    sub_m_r2 = partner_manifest(f"partner_{pid_r}_app", "1.0.1")
                    sub_o2 = {"publisher_id": pid_o, "manifest": sub_m_o2, "signature": sign_b64(pub_key_o, {"publisher_id": pid_o, "manifest": sub_m_o2})}
                    sub_r2 = {"publisher_id": pid_r, "manifest": sub_m_r2, "signature": sign_b64(pub_key_r, {"publisher_id": pid_r, "manifest": sub_m_r2})}
                    wo = oracle.post(f"{API}/developer/marketplace/submissions", json=sub_o2, headers=dev_o_csrf_h).json()["id"]
                    wr = rust.post(f"{API}/developer/marketplace/submissions", json=sub_r2, headers=dev_r_csrf_h).json()["id"]
                    compare(
                        "E14 withdraw submission",
                        oracle.post(f"{API}/developer/marketplace/submissions/{wo}/withdraw", headers=dev_o_csrf_h),
                        rust.post(f"{API}/developer/marketplace/submissions/{wr}/withdraw", headers=dev_r_csrf_h),
                        lambda b: norm_fields(b, "id", "publisher_id", "manifest", "signature", "public_key", "created_at"),
                        check_cache=True,
                    )
                    compare(
                        "E14 withdraw non-pending rejected",
                        oracle.post(f"{API}/developer/marketplace/submissions/{wo}/withdraw", headers=dev_o_csrf_h),
                        rust.post(f"{API}/developer/marketplace/submissions/{wr}/withdraw", headers=dev_r_csrf_h),
                    )

                    # Review queue & decision
                    compare(
                        "E14 review queue non-reviewer denied",
                        oracle.get(f"{API}/developer/marketplace/review-queue", headers=dev_o_h),
                        rust.get(f"{API}/developer/marketplace/review-queue", headers=dev_r_h),
                    )
                    compare(
                        "E14 review queue",
                        oracle.get(f"{API}/developer/marketplace/review-queue", headers=rev_h),
                        rust.get(f"{API}/developer/marketplace/review-queue", headers=rev_h),
                        check_cache=True,
                    )
                    compare(
                        "E14 review decision blank note",
                        oracle.post(
                            f"{API}/developer/marketplace/review-queue/{sid_o}/decision",
                            json={"decision": "approved", "note": "   "},
                            headers=rev_csrf_h,
                        ),
                        rust.post(
                            f"{API}/developer/marketplace/review-queue/{sid_r}/decision",
                            json={"decision": "approved", "note": "   "},
                            headers=rev_csrf_h,
                        ),
                    )
                    compare(
                        "E14 review decision approve",
                        oracle.post(
                            f"{API}/developer/marketplace/review-queue/{sid_o}/decision",
                            json={"decision": "approved", "note": "Verified parity"},
                            headers=rev_csrf_h,
                        ),
                        rust.post(
                            f"{API}/developer/marketplace/review-queue/{sid_r}/decision",
                            json={"decision": "approved", "note": "Verified parity"},
                            headers=rev_csrf_h,
                        ),
                        lambda b: norm_fields(b, "id", "publisher_id", "manifest", "signature", "public_key", "created_at", "reviewed_at"),
                        check_cache=True,
                    )
                    compare(
                        "E14 review duplicate decision rejected",
                        oracle.post(
                            f"{API}/developer/marketplace/review-queue/{sid_o}/decision",
                            json={"decision": "rejected", "note": "Duplicate"},
                            headers=rev_csrf_h,
                        ),
                        rust.post(
                            f"{API}/developer/marketplace/review-queue/{sid_r}/decision",
                            json={"decision": "rejected", "note": "Duplicate"},
                            headers=rev_csrf_h,
                        ),
                    )

                    # --- 6. Partner Platform Contributions & Integration Events ---
                    pmid_o, pmid_r = inst_o["id"], inst_r["id"]
                    req(
                        oracle.post(f"{proj}/extensions/activations", json={"manifest_id": pmid_o}, headers=owner_csrf).status_code == 201,
                        "activate partner_oracle",
                    )
                    req(
                        rust.post(f"{proj}/extensions/activations", json={"manifest_id": pmid_r}, headers=owner_csrf).status_code == 201,
                        "activate partner_rust",
                    )
                    contrib_url = f"{proj}/extensions/partner-contributions"
                    compare(
                        "E14 list partner contributions empty",
                        oracle.get(contrib_url, headers=owner_h),
                        rust.get(contrib_url, headers=owner_h),
                        check_cache=True,
                    )
                    doc = {
                        "tools": [{"id": "inspect", "label": "Inspect"}],
                        "overlay": {
                            "title": "Partner Overlay",
                            "geojson": {
                                "type": "FeatureCollection",
                                "features": [
                                    {
                                        "type": "Feature",
                                        "geometry": {"type": "Point", "coordinates": [106.6, 10.6]},
                                        "properties": {},
                                    }
                                ],
                            },
                        },
                    }
                    compare(
                        "E14 save partner contributions denied member",
                        oracle.put(f"{contrib_url}/{pmid_o}", json={"expected_revision": 0, "document": doc}, headers=member_csrf_h),
                        rust.put(f"{contrib_url}/{pmid_r}", json={"expected_revision": 0, "document": doc}, headers=member_csrf_h),
                    )
                    compare(
                        "E14 save partner contributions invalid geojson",
                        oracle.put(
                            f"{contrib_url}/{pmid_o}",
                            json={"expected_revision": 0, "document": {"tools": [], "overlay": {"title": "Bad", "geojson": {"type": "FeatureCollection", "features": [{"type": "Feature", "geometry": {"type": "Point", "coordinates": [999, 0]}, "properties": {}}]}}}},
                            headers=owner_csrf,
                        ),
                        rust.put(
                            f"{contrib_url}/{pmid_r}",
                            json={"expected_revision": 0, "document": {"tools": [], "overlay": {"title": "Bad", "geojson": {"type": "FeatureCollection", "features": [{"type": "Feature", "geometry": {"type": "Point", "coordinates": [999, 0]}, "properties": {}}]}}}},
                            headers=owner_csrf,
                        ),
                    )
                    compare(
                        "E14 save partner contributions",
                        oracle.put(f"{contrib_url}/{pmid_o}", json={"expected_revision": 0, "document": doc}, headers=owner_csrf),
                        rust.put(f"{contrib_url}/{pmid_r}", json={"expected_revision": 0, "document": doc}, headers=owner_csrf),
                        lambda b: norm_fields(b, "manifest_id", "updated_at", "display_name"),
                    )
                    compare(
                        "E14 save partner contributions stale revision",
                        oracle.put(f"{contrib_url}/{pmid_o}", json={"expected_revision": 0, "document": doc}, headers=owner_csrf),
                        rust.put(f"{contrib_url}/{pmid_r}", json={"expected_revision": 0, "document": doc}, headers=owner_csrf),
                    )
                    compare(
                        "E14 list partner contributions populated",
                        oracle.get(contrib_url, headers=owner_h),
                        rust.get(contrib_url, headers=owner_h),
                        lambda b: norm_fields(b, "manifest_id", "updated_at", "display_name"),
                        check_cache=True,
                    )

                    # Integration events
                    with psycopg.connect(owner_url) as conn:
                        conn.execute(
                            """INSERT INTO extensions.integration_events
                               (id, organization_id, project_id, event_kind, resource_type, resource_id, revision_ref, created_at)
                               VALUES (%s, %s, %s, 'changeset.committed', 'changeset', %s, 'rev-1', %s)""",
                            (
                                uuid4(),
                                f["org_a_id"],
                                f["project_a_id"],
                                uuid4(),
                                datetime(2026, 11, 26, 5, 0, 0, 123456, tzinfo=UTC),
                            ),
                        )
                    events_url = f"{proj}/integrations/events"
                    compare(
                        "E14 list integration events",
                        oracle.get(events_url, headers=owner_h),
                        rust.get(events_url, headers=owner_h),
                        check_cache=True,
                    )
                    compare(
                        "E14 list integration events invalid limit",
                        oracle.get(events_url, params={"limit": 201}, headers=owner_h),
                        rust.get(events_url, params={"limit": 201}, headers=owner_h),
                    )
                    # GIS-scoped access denied on partner & records & portable-export
                    set_project_role(owner_url, f, "viewer", scoped=True)
                    compare(
                        "E14 scoped GIS denied partner contributions",
                        oracle.get(contrib_url, headers=owner_h),
                        rust.get(contrib_url, headers=owner_h),
                    )
                    compare(
                        "E14 scoped GIS denied integration events",
                        oracle.get(events_url, headers=owner_h),
                        rust.get(events_url, headers=owner_h),
                    )
                    compare(
                        "E14 scoped GIS denied extension records",
                        oracle.get(rec_o_url, headers=owner_h),
                        rust.get(rec_r_url, headers=owner_h),
                    )
                    compare(
                        "E14 scoped GIS denied portable export",
                        oracle.get(f"{proj}/extensions/demo_oracle/portable-export", headers=owner_h),
                        rust.get(f"{proj}/extensions/demo_rust/portable-export", headers=owner_h),
                    )
            finally:
                get_engine().dispose()

    print("E14 full parity PASS", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
