"""R014 ADR-008 verification: two materially distinct vertical modules validated via Core API."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.spatial.test_sources import _client

from modules.house_numbering.workflow import (
    assign_even_odd_sequence,
)
from modules.house_numbering.workflow import (
    load_manifest as load_house_manifest,
)
from modules.land_asset.workflow import (
    build_inspection_payload,
)
from modules.land_asset.workflow import (
    load_manifest as load_land_manifest,
)


def test_two_independent_reference_modules_validate_core_extension_model() -> None:
    world = seed_access_world()
    org_base = f"/api/v1/organizations/{world.organization_id}"
    proj_base = f"{org_base}/projects/{world.project_id}"

    house_manifest = load_house_manifest()
    land_manifest = load_land_manifest()

    with _client(world.owner_user_id) as client:
        # Register & activate Reference Module 1: House Numbering
        rejected = client.post(f"{org_base}/extensions/manifests", json=house_manifest)
        assert rejected.status_code == 409
        assert rejected.json()["code"] == "OFFICIAL_MANIFEST_REQUIRED"
        m1 = client.post(f"{org_base}/extensions/official/house_numbering/register")
        assert m1.status_code == 201, m1.text
        a1 = client.post(
            f"{proj_base}/extensions/activations",
            json={"manifest_id": m1.json()["id"]},
        )
        assert a1.status_code == 201, a1.text

        plates = assign_even_odd_sequence(
            street_name="Tran Hung Dao", side="left", start_number=1, count=3
        )
        for plate in plates:
            r = client.post(
                f"{proj_base}/extensions/house_numbering/records",
                json={"entity_name": "address_plate", "payload": plate},
            )
            assert r.status_code == 201, r.text

        # Register & activate Reference Module 2: Land & Asset Registry
        m2 = client.post(f"{org_base}/extensions/manifests", json=land_manifest)
        assert m2.status_code == 201, m2.text
        a2 = client.post(
            f"{proj_base}/extensions/activations",
            json={"manifest_id": m2.json()["id"]},
        )
        assert a2.status_code == 201, a2.text

        insp = build_inspection_payload(
            asset_code="LND-QN-0042",
            land_use_code="ODT",
            area_m2=185.5,
            condition_score=5,
            inspector_note="Verified boundary markers",
        )
        r2 = client.post(
            f"{proj_base}/extensions/land_asset/records",
            json={"entity_name": "asset_inspection", "payload": insp},
        )
        assert r2.status_code == 201, r2.text

        # Verify both modules operate independently in the same project
        h_recs = client.get(f"{proj_base}/extensions/house_numbering/records").json()[
            "items"
        ]
        l_recs = client.get(f"{proj_base}/extensions/land_asset/records").json()[
            "items"
        ]
        assert len(h_recs) == 3
        assert len(l_recs) == 1
        assert l_recs[0]["payload"]["asset_code"] == "LND-QN-0042"
