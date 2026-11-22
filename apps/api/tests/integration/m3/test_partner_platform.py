"""Persistent contribution authorization and signed marketplace install."""

from __future__ import annotations

import base64
import json
import subprocess
import sys
from pathlib import Path

from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from cryptography.hazmat.primitives.serialization import Encoding, PublicFormat

from openvietgis_api.config import get_settings
from openvietgis_api.domains.extensions.marketplace import canonical, load_catalog
from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.m3.test_partner_plugin import partner_manifest
from tests.integration.spatial.test_authorization import _grant
from tests.integration.spatial.test_sources import _client


def public(key):
    return base64.b64encode(key.public_key().public_bytes(Encoding.Raw, PublicFormat.Raw)).decode()


def sign(key, obj):
    return base64.b64encode(key.sign(canonical(obj))).decode()


def test_partner_persistent_and_marketplace(migrated_database, tmp_path, monkeypatch):
    world = seed_access_world()
    other = seed_access_world()
    org = f"/api/v1/organizations/{world.organization_id}"
    project = f"{org}/projects/{world.project_id}"
    publisher, operator = Ed25519PrivateKey.generate(), Ed25519PrivateKey.generate()
    manifest = partner_manifest()
    manifest["extension_key"] = "partner_acme_example"
    release = {"publisher_id": "acme", "manifest": manifest}
    release["signature"] = sign(publisher, release)
    payload = {
        "schema": "openvietgis.marketplace.v1",
        "publishers": [{"id": "acme", "name": "Acme", "public_key": public(publisher)}],
        "releases": [release],
    }
    path = tmp_path / "catalog.json"
    path.write_text(json.dumps({"payload": payload, "signature": sign(operator, payload)}))
    monkeypatch.setattr(get_settings(), "marketplace_catalog_path", str(path))
    monkeypatch.setattr(get_settings(), "marketplace_operator_public_key", public(operator))
    with _client(world.owner_user_id) as owner, _client(world.member_user_id) as member:
        catalog = owner.get("/api/v1/marketplace/catalog")
        assert catalog.status_code == 200, catalog.text
        assert catalog.json()["items"][0]["publisher_name"] == "Acme"
        install = f"{org}/marketplace/install/acme/partner_acme_example/1.0.0"
        assert member.post(install).status_code == 403
        saved = owner.post(install)
        assert saved.status_code == 201, saved.text
        mid = saved.json()["id"]
        assert (
            owner.post(f"{project}/extensions/activations", json={"manifest_id": mid}).status_code
            == 201
        )
        url = f"{project}/extensions/partner-contributions"
        document = {
            "tools": [{"id": "inspect", "label": "Inspect"}],
            "overlay": {
                "title": "Persistent",
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
        body = {"expected_revision": 0, "document": document}
        _grant(owner, world, "membership", str(world.member_membership_id), "editor")
        assert member.put(f"{url}/{mid}", json=body).status_code == 403
        result = owner.put(f"{url}/{mid}", json=body)
        assert result.status_code == 200, result.text
        assert result.json()["revision"] == 1
        assert owner.put(f"{url}/{mid}", json=body).status_code == 409
        assert member.get(url).json()["items"][0]["document"] == document
        with _client(other.owner_user_id) as outsider:
            assert outsider.get(url).status_code in (403, 404)
        assert owner.post(f"{project}/extensions/partner_acme_example/disable").status_code == 200
        assert owner.get(url).json()["items"] == []
        assert owner.put(f"{url}/{mid}", json=body).status_code == 404
        assert (
            owner.post(f"{project}/extensions/activations", json={"manifest_id": mid}).status_code
            == 201
        )
        assert owner.get(url).json()["items"][0]["revision"] == 1
        payload["releases"][0]["manifest"]["display_name"] = "tampered"
        path.write_text(json.dumps({"payload": payload, "signature": sign(operator, payload)}))
        assert owner.get("/api/v1/marketplace/catalog").status_code == 422
        assert owner.post(install).status_code == 422


def test_marketplace_cli_roundtrip(tmp_path):
    cli = Path(__file__).resolve().parents[5] / "tools" / "marketplace_release.py"

    def run(*args):
        subprocess.run([sys.executable, str(cli), *args], cwd=tmp_path, check=True)

    run("keygen", "publisher.pem", "publisher.txt")
    run("keygen", "operator.pem", "operator.txt")
    assert (tmp_path / "publisher.pem").stat().st_mode & 0o777 == 0o600
    manifest = {**partner_manifest(), "extension_key": "partner_acme_example"}
    (tmp_path / "manifest.json").write_text(json.dumps(manifest))
    run("sign-release", "acme", "manifest.json", "publisher.pem", "release.json")
    payload = {
        "schema": "openvietgis.marketplace.v1",
        "publishers": [
            {
                "id": "acme",
                "name": "Acme",
                "public_key": (tmp_path / "publisher.txt").read_text().strip(),
            }
        ],
        "releases": [json.loads((tmp_path / "release.json").read_text())],
    }
    (tmp_path / "reviewed.json").write_text(json.dumps(payload))
    run("publish-reviewed", "reviewed.json", "operator.pem", "catalog.json", "--reviewed")
    items = load_catalog(
        str(tmp_path / "catalog.json"), (tmp_path / "operator.txt").read_text().strip()
    )
    assert items[0]["manifest"] == manifest
