"""Real PostGIS/runtime-RLS proof for the self-service marketplace."""

from __future__ import annotations

import base64
import json
import subprocess
import sys
from pathlib import Path

from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from cryptography.hazmat.primitives.serialization import Encoding, PublicFormat
from sqlalchemy import text

from openvietgis_api.adapters.persistence.db import get_engine
from openvietgis_api.config import get_settings
from openvietgis_api.domains.extensions.marketplace import canonical
from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.m3.test_partner_plugin import partner_manifest
from tests.integration.spatial.test_sources import _client

BASE = "/api/v1/developer/marketplace"


def public(key):
    return base64.b64encode(key.public_key().public_bytes(Encoding.Raw, PublicFormat.Raw)).decode()


def sign(key, body):
    return base64.b64encode(key.sign(canonical(body))).decode()


def registration(key, uid, name):
    body = {"name": name, "public_key": public(key)}
    return {
        **body,
        "proof": sign(key, {"purpose": "openvietgis.publisher.v1", "user_id": str(uid), **body}),
    }


def release(key, pid, version="1.0.0"):
    manifest = {
        **partner_manifest(),
        "extension_key": f"partner_{pid}_inspector",
        "version": version,
    }
    body = {"publisher_id": pid, "manifest": manifest}
    return {**body, "signature": sign(key, body)}


def test_developer_self_service_and_review(migrated_database, tmp_path, monkeypatch):
    world = seed_access_world()
    other = seed_access_world()
    publisher, reviewer = Ed25519PrivateKey.generate(), Ed25519PrivateKey.generate()
    operator = Ed25519PrivateKey.generate()
    settings = get_settings()
    monkeypatch.setattr(settings, "marketplace_reviewer_user_ids", [other.owner_user_id])
    monkeypatch.setattr(settings, "marketplace_catalog_path", None)
    monkeypatch.setattr(settings, "marketplace_operator_public_key", None)
    with (
        _client(world.owner_user_id) as dev,
        _client(world.member_user_id) as outsider,
        _client(other.owner_user_id) as admin,
    ):
        state = dev.get(BASE + "/me")
        assert state.status_code == 200 and state.json()["publisher"] is None
        assert state.headers["cache-control"] == "private, no-store"
        assert not state.json()["can_review"]
        assert admin.get(BASE + "/me").json()["can_review"]
        body = registration(publisher, world.owner_user_id, "Independent publisher")
        assert outsider.post(BASE + "/publisher", json=body).status_code == 422
        result = dev.post(BASE + "/publisher", json=body)
        assert result.status_code == 201, result.text
        pid = result.json()["id"]
        assert pid.startswith("dev_") and len(pid) == 28
        assert dev.post(BASE + "/publisher", json=body).status_code == 409
        good = release(publisher, pid)
        assert outsider.post(BASE + "/submissions", json=good).status_code == 403
        assert (
            dev.post(BASE + "/submissions", json={**good, "signature": "A" * 88}).status_code == 422
        )
        signed_other_namespace = release(publisher, "reference")
        assert dev.post(BASE + "/submissions", json=signed_other_namespace).status_code == 403
        created = dev.post(BASE + "/submissions", json=good)
        assert created.status_code == 201, created.text
        mid = created.json()["id"]
        assert created.json()["status"] == "pending"
        assert dev.post(BASE + "/submissions", json=good).status_code == 409
        assert outsider.get(BASE + "/me").json()["submissions"] == []
        assert outsider.post(f"{BASE}/submissions/{mid}/withdraw").status_code == 404
        assert dev.get(BASE + "/review-queue").status_code == 403
        assert (
            dev.post(
                f"{BASE}/review-queue/{mid}/decision",
                json={
                    "decision": "approved",
                    "note": "unauthorized",
                },
            ).status_code
            == 403
        )
        assert dev.get("/api/v1/marketplace/catalog").json()["items"] == []
        with get_engine().begin() as conn:
            conn.execute(
                text("SELECT set_config('openvietgis.actor_user_id',:uid,true)"),
                {"uid": str(world.member_user_id)},
            )
            assert (
                conn.execute(
                    text("SELECT count(*) FROM extensions.marketplace_submissions")
                ).scalar()
                == 0
            )
        queue = admin.get(BASE + "/review-queue")
        assert queue.status_code == 200, queue.text
        assert any(r["id"] == mid for r in queue.json()["items"])
        result = admin.post(
            f"{BASE}/review-queue/{mid}/decision",
            json={
                "decision": "approved",
                "note": "Reviewed sandbox and publisher proof.",
            },
        )
        assert result.status_code == 200, result.text
        assert result.json()["status"] == "approved" and not result.json()["published"]
        assert dev.get(BASE + "/me").json()["submissions"][0]["review_note"]
        assert dev.post(f"{BASE}/submissions/{mid}/withdraw").status_code == 409
        assert (
            admin.post(
                f"{BASE}/review-queue/{mid}/decision",
                json={
                    "decision": "rejected",
                    "note": "duplicate decision",
                },
            ).status_code
            == 409
        )
        assert dev.get("/api/v1/marketplace/catalog").json()["items"] == []
        payload = {
            "schema": "openvietgis.marketplace.v1",
            "publishers": [{"id": pid, "name": body["name"], "public_key": body["public_key"]}],
            "releases": [good],
        }
        catalog = tmp_path / "catalog.json"
        catalog.write_text(json.dumps({"payload": payload, "signature": sign(operator, payload)}))
        monkeypatch.setattr(settings, "marketplace_catalog_path", str(catalog))
        monkeypatch.setattr(settings, "marketplace_operator_public_key", public(operator))
        assert dev.get(BASE + "/me").json()["submissions"][0]["published"]
        assert len(dev.get("/api/v1/marketplace/catalog").json()["items"]) == 1
        install = dev.post(
            f"/api/v1/organizations/{world.organization_id}/marketplace/install/"
            f"{pid}/{good['manifest']['extension_key']}/1.0.0"
        )
        assert install.status_code == 201, install.text
        second = dev.post(BASE + "/submissions", json=release(publisher, pid, "1.0.1"))
        assert second.status_code == 201, second.text
        sid = second.json()["id"]
        assert dev.post(f"{BASE}/submissions/{sid}/withdraw").json()["status"] == "withdrawn"
        assert (
            admin.post(
                f"{BASE}/review-queue/{sid}/decision",
                json={
                    "decision": "approved",
                    "note": "late",
                },
            ).status_code
            == 409
        )
        third = dev.post(BASE + "/submissions", json=release(publisher, pid, "1.0.2"))
        tid = third.json()["id"]
        assert (
            admin.post(
                f"{BASE}/review-queue/{tid}/decision",
                json={
                    "decision": "rejected",
                    "note": "Needs correction",
                },
            ).json()["status"]
            == "rejected"
        )
        self_pub = admin.post(
            BASE + "/publisher",
            json=registration(reviewer, other.owner_user_id, "Reviewer own publisher"),
        )
        assert self_pub.status_code == 201, self_pub.text
        own = admin.post(BASE + "/submissions", json=release(reviewer, self_pub.json()["id"]))
        assert own.status_code == 201, own.text
        assert (
            admin.post(
                f"{BASE}/review-queue/{own.json()['id']}/decision",
                json={
                    "decision": "approved",
                    "note": "self",
                },
            ).status_code
            == 403
        )
        monkeypatch.setattr(settings, "marketplace_reviewer_user_ids", [])
        assert admin.get(BASE + "/review-queue").status_code == 403
        # A malformed signed catalog must not falsely report published.
        catalog.write_text("{}")
        state = dev.get(BASE + "/me").json()
        assert not state["catalog_available"]
        assert not any(r["published"] for r in state["submissions"])


def test_cli_publisher_proof(tmp_path):
    cli = Path(__file__).resolve().parents[5] / "tools" / "marketplace_release.py"
    from uuid import uuid4

    from openvietgis_api.domains.extensions.marketplace import verify

    uid = uuid4()

    def run(*args):
        subprocess.run([sys.executable, str(cli), *args], cwd=tmp_path, check=True)

    run("keygen", "publisher.pem", "publisher.txt")
    run(
        "publisher-proof", str(uid), "New publisher", "publisher.txt", "publisher.pem", "proof.json"
    )
    body = json.loads((tmp_path / "proof.json").read_text())
    verify(
        body["public_key"],
        {
            "purpose": "openvietgis.publisher.v1",
            "user_id": str(uid),
            "name": body["name"],
            "public_key": body["public_key"],
        },
        body["proof"],
    )


def test_standalone_signing_and_safe_catalog_merge(tmp_path):
    root = Path(__file__).resolve().parents[5]
    cli = root / "tools/marketplace_release.py"
    standalone = root / "packages/plugin-sdk/marketplace_sign.py"
    from uuid import uuid4

    from openvietgis_api.domains.extensions.marketplace import load_catalog, verify

    def run(tool, *args):
        subprocess.run([sys.executable, str(tool), *args], cwd=tmp_path, check=True)

    uid = uuid4()
    pid = "dev_" + uid.hex[:24]
    run(standalone, "keygen", "dev.pem", "dev.txt")
    run(standalone, "publisher-proof", str(uid), "Portable", "dev.txt", "dev.pem", "proof.json")
    body = json.loads((tmp_path / "proof.json").read_text())
    verify(
        body["public_key"],
        {
            "purpose": "openvietgis.publisher.v1",
            "user_id": str(uid),
            "name": body["name"],
            "public_key": body["public_key"],
        },
        body["proof"],
    )
    assert (tmp_path / "dev.pem").stat().st_mode & 0o777 == 0o600
    for version in ("1.0.0", "1.0.1"):
        manifest = {
            **partner_manifest(),
            "extension_key": f"partner_{pid}_test",
            "version": version,
        }
        (tmp_path / f"manifest-{version}.json").write_text(json.dumps(manifest))
        run(
            standalone,
            "sign-release",
            pid,
            f"manifest-{version}.json",
            "dev.pem",
            f"release-{version}.json",
        )
        payload = {
            "schema": "openvietgis.marketplace.v1",
            "publishers": [{"id": pid, "name": body["name"], "public_key": body["public_key"]}],
            "releases": [json.loads((tmp_path / f"release-{version}.json").read_text())],
        }
        (tmp_path / f"entry-{version}.json").write_text(json.dumps(payload))
    run(cli, "keygen", "operator.pem", "operator.txt")
    run(cli, "publish-reviewed", "entry-1.0.0.json", "operator.pem", "base.json", "--reviewed")
    run(
        cli,
        "merge-reviewed",
        "--base",
        "base.json",
        "--operator-public",
        "operator.txt",
        "--output",
        "merged.json",
        "entry-1.0.1.json",
        "entry-1.0.0.json",
    )
    run(cli, "publish-reviewed", "merged.json", "operator.pem", "published.json", "--reviewed")
    items = load_catalog(
        str(tmp_path / "published.json"), (tmp_path / "operator.txt").read_text().strip()
    )
    assert {x["manifest"]["version"] for x in items} == {"1.0.0", "1.0.1"}
