"""E10 CAD/DXF differential parity: inspect, import, export, georef inspect/import/draft, and native delta preview."""

from __future__ import annotations

import argparse
import copy
import hashlib
import io
import os
import sys
import tempfile
from pathlib import Path

import ezdxf

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tests" / "rust-parity"))
sys.path.insert(0, str(ROOT / "tools" / "benchmark" / "rust-platform"))

import benchmark_runner as baseline
import e13_field_offline as offline_helpers
import run as parity

API = "/api/v1"


def require(cond: bool, msg: str) -> None:
    if not cond:
        raise AssertionError(msg)


def _make_dxf_bytes(pts: list[tuple[float, float]], *, include_unsupported_text: bool = False) -> bytes:
    doc = ezdxf.new("R2010")
    doc.header["$INSUNITS"] = 6
    msp = doc.modelspace()
    for x, y in pts:
        msp.add_point((x, y), dxfattribs={"layer": "SURVEY"})
    if include_unsupported_text:
        msp.add_text("Note", dxfattribs={"layer": "ANNO", "height": 1.0}).set_placement((pts[0][0], pts[0][1]))
    buf = io.StringIO()
    doc.write(buf)
    return buf.getvalue().encode("utf-8")


def _norm_document_bound(body):
    out = copy.deepcopy(body)

    def walk(value):
        if isinstance(value, dict):
            for key, item in value.items():
                if key in {"cad_document_id", "inspection_fingerprint"} and isinstance(item, str):
                    value[key] = f"<{key}>"
                else:
                    walk(item)
        elif isinstance(value, list):
            for item in value:
                walk(item)

    walk(out)
    return out


def _norm_inspect(body):
    return _norm_document_bound(body)


def _norm_import(body):
    out = _norm_document_bound(body)
    for k in ("id", "source_id", "layer_id", "content_revision", "created_at"):
        if k in out and out[k] is not None:
            out[k] = f"<{k}>"
    return out


def _norm_draft(body):
    out = copy.deepcopy(body)
    for k in ("document_id", "conversion_id", "source_id", "layer_id", "content_revision", "schema_revision", "preview_fingerprint"):
        if k in out and out[k] is not None:
            out[k] = f"<{k}>"
    return out


def _norm_native(body):
    out = dict(body)
    for k in ("conversion_id", "source_id", "base_content_revision", "base_schema_revision"):
        if k in out and out[k] is not None:
            out[k] = f"<{k}>"
    return out


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--candidate", type=Path, required=True)
    args = ap.parse_args()
    binary = args.candidate.resolve()
    require(binary.is_file(), f"candidate binary not found: {binary}")
    admin_url = os.environ.get("OVG_PARITY_PG_ADMIN_URL", baseline.DEFAULT_ADMIN_PG_URL)

    with tempfile.TemporaryDirectory(prefix="ovg-e10-cad-") as tmp:
        work = Path(tmp)
        with parity.database(admin_url) as (owner_url, runtime_url):
            artifacts = work / "artifacts"
            artifacts.mkdir()
            parity.configure_python(owner_url, runtime_url, artifacts)
            os.environ["OPENVIETGIS_CAD_PYTHON"] = sys.executable
            baseline.run_alembic_upgrade(owner_url, runtime_url)
            f = baseline.seed_benchmark_database(
                owner_url,
                seed=20261127,
                scales=["1k"],
                csrf_secret=baseline.DEFAULT_CSRF_SECRET,
            )

            org_id = str(f["org_a_id"])
            template_proj = str(f["project_a_id"])
            proj_o = offline_helpers.clone_project(owner_url, org_id, template_proj, "oracle")
            proj_r = offline_helpers.clone_project(owner_url, org_id, template_proj, "rust")
            base_o = f"{API}/organizations/{org_id}/projects/{proj_o}"
            base_r = f"{API}/organizations/{org_id}/projects/{proj_r}"
            csrf_a = parity.session_headers(
                f["session_a_token"], **{"x-openvietgis-csrf": f["csrf_a_token"]}
            )

            dxf_lossy = _make_dxf_bytes([(11872000.0, 1204000.0), (11872050.0, 1204050.0)], include_unsupported_text=True)
            dxf_local = _make_dxf_bytes([(10.0, 20.0), (40.0, 60.0)])
            dxf_local_v2 = _make_dxf_bytes([(12.0, 24.0), (40.0, 60.0)])

            from fastapi.testclient import TestClient
            from openvietgis_api.adapters.persistence.db import get_engine
            from openvietgis_api.main import app

            try:
                with (
                    TestClient(app, raise_server_exceptions=False) as oracle,
                    parity.rust_client(binary, runtime_url, work) as rust,
                ):
                    # 1. /cad/inspect negative + valid
                    offline_helpers.compare(
                        "E10 inspect 4326 rejected 422",
                        oracle.post(
                            f"{base_o}/cad/inspect",
                            headers=csrf_a,
                            files={"file": ("a.dxf", dxf_lossy, "application/dxf")},
                            data={"units": "meter", "crs_code": "4326"},
                        ),
                        rust.post(
                            f"{base_r}/cad/inspect",
                            headers=csrf_a,
                            files={"file": ("a.dxf", dxf_lossy, "application/dxf")},
                            data={"units": "meter", "crs_code": "4326"},
                        ),
                        expect_status=422,
                    )
                    ins_o = oracle.post(
                        f"{base_o}/cad/inspect",
                        headers=csrf_a,
                        files={"file": ("survey.dxf", dxf_lossy, "application/dxf")},
                        data={"units": "meter", "crs_code": "3857"},
                    )
                    ins_r = rust.post(
                        f"{base_r}/cad/inspect",
                        headers=csrf_a,
                        files={"file": ("survey.dxf", dxf_lossy, "application/dxf")},
                        data={"units": "meter", "crs_code": "3857"},
                    )
                    offline_helpers.compare("E10 cad/inspect 200", ins_o, ins_r, norm=_norm_inspect, expect_status=200)

                    # 2. /cad/import loss-ack required 422 + valid 201 + dwg 422
                    offline_helpers.compare(
                        "E10 cad/import loss ack required 422",
                        oracle.post(
                            f"{base_o}/cad/import",
                            headers=csrf_a,
                            files={"file": ("survey.dxf", dxf_lossy, "application/dxf")},
                            data={"units": "meter", "crs_code": "3857", "layer_name": "CAD Survey"},
                        ),
                        rust.post(
                            f"{base_r}/cad/import",
                            headers=csrf_a,
                            files={"file": ("survey.dxf", dxf_lossy, "application/dxf")},
                            data={"units": "meter", "crs_code": "3857", "layer_name": "CAD Survey"},
                        ),
                        expect_status=422,
                    )
                    imp_o = oracle.post(
                        f"{base_o}/cad/import",
                        headers=csrf_a,
                        files={"file": ("survey.dxf", dxf_lossy, "application/dxf")},
                        data={
                            "units": "meter",
                            "crs_code": "3857",
                            "layer_name": "CAD Survey",
                            "acknowledge_loss": "true",
                            "expected_sha256": ins_o.json()["sha256"],
                            "expected_inspection_fingerprint": ins_o.json()["inspection_fingerprint"],
                        },
                    )
                    imp_r = rust.post(
                        f"{base_r}/cad/import",
                        headers=csrf_a,
                        files={"file": ("survey.dxf", dxf_lossy, "application/dxf")},
                        data={
                            "units": "meter",
                            "crs_code": "3857",
                            "layer_name": "CAD Survey",
                            "acknowledge_loss": "true",
                            "expected_sha256": ins_r.json()["sha256"],
                            "expected_inspection_fingerprint": ins_r.json()["inspection_fingerprint"],
                        },
                    )
                    offline_helpers.compare("E10 cad/import 201", imp_o, imp_r, norm=_norm_import, expect_status=201)

                    # DWG upload rejected with evidence persisted
                    dwg_o = oracle.post(
                        f"{base_o}/cad/import",
                        headers=csrf_a,
                        files={"file": ("legacy.dwg", b"AC1032abcdef", "application/octet-stream")},
                        data={"units": "meter", "crs_code": "3857"},
                    )
                    dwg_r = rust.post(
                        f"{base_r}/cad/import",
                        headers=csrf_a,
                        files={"file": ("legacy.dwg", b"AC1032abcdef", "application/octet-stream")},
                        data={"units": "meter", "crs_code": "3857"},
                    )
                    require(dwg_o.status_code == dwg_r.status_code == 422, "expected 422 on DWG upload")
                    require(dwg_r.json().get("code") == "DWG_ADAPTER_REQUIRED", f"bad dwg code: {dwg_r.json()}")

                    # 3. /cad/export
                    exp_o = oracle.post(
                        f"{base_o}/cad/export",
                        headers=csrf_a,
                        json={
                            "layer_id": imp_o.json()["layer_id"],
                            "units": "meter",
                            "expected_content_revision": imp_o.json()["content_revision"],
                            "acknowledge_attribute_loss": True,
                        },
                    )
                    exp_r = rust.post(
                        f"{base_r}/cad/export",
                        headers=csrf_a,
                        json={
                            "layer_id": imp_r.json()["layer_id"],
                            "units": "meter",
                            "expected_content_revision": imp_r.json()["content_revision"],
                            "acknowledge_attribute_loss": True,
                        },
                    )
                    require(exp_o.status_code == 200 and exp_r.status_code == 200, f"cad/export failed: {exp_r.status_code} {exp_r.text}")
                    require(
                        exp_r.headers["x-content-sha256"] == hashlib.sha256(exp_r.content).hexdigest(),
                        "cad/export sha mismatch",
                    )
                    exp_doc = ezdxf.read(io.StringIO(exp_r.content.decode("utf-8")))
                    require(len(list(exp_doc.modelspace())) == 2, "expected 2 exported DXF entities")

                    # 4. Upload local CAD document v1 + inspect-georef + import-georef
                    doc_o = oracle.post(
                        f"{base_o}/cad/documents",
                        headers=csrf_a,
                        files={"file": ("local.dxf", dxf_local, "application/dxf")},
                    ).json()
                    doc_r = rust.post(
                        f"{base_r}/cad/documents",
                        headers=csrf_a,
                        files={"file": ("local.dxf", dxf_local, "application/dxf")},
                    ).json()
                    did_o, did_r = doc_o["id"], doc_r["id"]

                    controls = [
                        [[0.0, 0.0], [11872000.0, 1204000.0]],
                        [[100.0, 0.0], [11872100.0, 1204000.0]],
                        [[0.0, 100.0], [11872000.0, 1204100.0]],
                    ]
                    georef_req = {
                        "crs_code": 3857,
                        "units": "meter",
                        "controls": controls,
                        "tolerance_m": 1.0,
                        "max_scale_deviation": 0.05,
                    }
                    ig_o = oracle.post(f"{base_o}/cad/documents/{did_o}/versions/1/inspect-georef", headers=csrf_a, json=georef_req)
                    ig_r = rust.post(f"{base_r}/cad/documents/{did_r}/versions/1/inspect-georef", headers=csrf_a, json=georef_req)
                    offline_helpers.compare("E10 inspect-georef 200", ig_o, ig_r, norm=_norm_inspect, expect_status=200)

                    # import-georef 201
                    imp_g_o = oracle.post(
                        f"{base_o}/cad/documents/{did_o}/versions/1/import-georef",
                        headers=csrf_a,
                        json={
                            **georef_req,
                            "layer_name": "Local Calibrated",
                            "expected_sha256": ig_o.json()["sha256"],
                            "expected_inspection_fingerprint": ig_o.json()["inspection_fingerprint"],
                            "acknowledge_loss": True,
                        },
                    )
                    imp_g_r = rust.post(
                        f"{base_r}/cad/documents/{did_r}/versions/1/import-georef",
                        headers=csrf_a,
                        json={
                            **georef_req,
                            "layer_name": "Local Calibrated",
                            "expected_sha256": ig_r.json()["sha256"],
                            "expected_inspection_fingerprint": ig_r.json()["inspection_fingerprint"],
                            "acknowledge_loss": True,
                        },
                    )
                    offline_helpers.compare("E10 import-georef 201", imp_g_o, imp_g_r, norm=_norm_import, expect_status=201)

                    # 5. export-georef-draft preview + download
                    layer_meta_o = oracle.get(f"{base_o}/layers/{imp_g_o.json()['layer_id']}", headers=csrf_a).json()["source"]
                    layer_meta_r = rust.get(f"{base_r}/layers/{imp_g_r.json()['layer_id']}", headers=csrf_a).json()["source"]
                    draft_req_o = {
                        "conversion_id": imp_g_o.json()["id"],
                        "expected_document_sha256": doc_o["sha256"],
                        "expected_content_revision": layer_meta_o["content_revision"],
                        "expected_schema_revision": layer_meta_o["schema_revision"],
                        "profile": "native-xy-dxf-v1",
                    }
                    draft_req_r = {
                        "conversion_id": imp_g_r.json()["id"],
                        "expected_document_sha256": doc_r["sha256"],
                        "expected_content_revision": layer_meta_r["content_revision"],
                        "expected_schema_revision": layer_meta_r["schema_revision"],
                        "profile": "native-xy-dxf-v1",
                    }
                    dp_o = oracle.post(f"{base_o}/cad/documents/{did_o}/versions/1/export-georef-draft", headers=csrf_a, json=draft_req_o)
                    dp_r = rust.post(f"{base_r}/cad/documents/{did_r}/versions/1/export-georef-draft", headers=csrf_a, json=draft_req_r)
                    offline_helpers.compare(
                        "E10 export-georef-draft preview 200",
                        dp_o,
                        dp_r,
                        norm=_norm_draft,
                        expect_status=200,
                    )
                    for side, response in (("oracle", dp_o), ("rust", dp_r)):
                        fingerprint = response.json()["preview_fingerprint"]
                        require(
                            len(fingerprint) == 64
                            and all(ch in "0123456789abcdef" for ch in fingerprint),
                            f"{side} draft preview fingerprint is not a sha256 hex digest",
                        )

                    dd_o = oracle.post(
                        f"{base_o}/cad/documents/{did_o}/versions/1/export-georef-draft",
                        headers=csrf_a,
                        json={
                            **draft_req_o,
                            "expected_preview_fingerprint": dp_o.json()["preview_fingerprint"],
                            "acknowledge_loss": True,
                        },
                    )
                    dd_r = rust.post(
                        f"{base_r}/cad/documents/{did_r}/versions/1/export-georef-draft",
                        headers=csrf_a,
                        json={
                            **draft_req_r,
                            "expected_preview_fingerprint": dp_r.json()["preview_fingerprint"],
                            "acknowledge_loss": True,
                        },
                    )
                    require(dd_o.status_code == 200 and dd_r.status_code == 200, f"draft download failed: {dd_r.status_code} {dd_r.text}")
                    require(
                        dd_r.headers["x-cad-preview-fingerprint"] == dp_r.json()["preview_fingerprint"],
                        "draft download fingerprint header mismatch",
                    )

                    # 6. Upload candidate v2 + preview-native-import
                    v2_o = oracle.post(
                        f"{base_o}/cad/documents/{did_o}/versions",
                        headers=csrf_a,
                        files={"file": ("local.dxf", dxf_local_v2, "application/dxf")},
                        data={"expected_version": "1"},
                    ).json()
                    v2_r = rust.post(
                        f"{base_r}/cad/documents/{did_r}/versions",
                        headers=csrf_a,
                        files={"file": ("local.dxf", dxf_local_v2, "application/dxf")},
                        data={"expected_version": "1"},
                    ).json()
                    pn_o = oracle.post(
                        f"{base_o}/cad/documents/{did_o}/versions/1/preview-native-import",
                        headers=csrf_a,
                        json={"conversion_id": imp_g_o.json()["id"], "expected_candidate_sha256": v2_o["sha256"]},
                    )
                    pn_r = rust.post(
                        f"{base_r}/cad/documents/{did_r}/versions/1/preview-native-import",
                        headers=csrf_a,
                        json={"conversion_id": imp_g_r.json()["id"], "expected_candidate_sha256": v2_r["sha256"]},
                    )
                    offline_helpers.compare("E10 preview-native-import 200", pn_o, pn_r, norm=_norm_native, expect_status=200)
                    require(len(pn_r.json()["operations"]) == 1, f"expected 1 native delta op, got {pn_r.json()}")
            finally:
                get_engine().dispose()

    print("E10 CAD/DXF parity PASS", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
