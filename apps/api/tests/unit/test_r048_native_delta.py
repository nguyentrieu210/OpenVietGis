"""Native geometry proposal qualification; no file vault or database writes."""

from __future__ import annotations

import hashlib
import io
from copy import deepcopy
from uuid import UUID

import ezdxf
import pytest

from openvietgis_api.adapters.cad.dxf_codec import parse_dxf_2d
from openvietgis_api.adapters.cad.native_delta import cad_to_gis_preview, gis_to_cad_preview
from openvietgis_api.domains.cad.domain import CadUnit
from openvietgis_api.domains.cad.georeference import fit_placement, place_features
from openvietgis_api.domains.editing.domain import EditOpType
from openvietgis_api.domains.platform_errors import PlatformConflictError, PlatformValidationError

DOC = UUID(int=48)


def encode(doc):
    buf = io.StringIO()
    doc.write(buf)
    return buf.getvalue().encode()


def digest(data):
    return hashlib.sha256(data).hexdigest()


def fixture(kind):
    doc = ezdxf.new("R2010")
    doc.header["$INSUNITS"] = 4
    doc.layers.new("Control", dxfattribs={"color": 3})
    m = doc.modelspace()
    attrs = {"layer": "Control", "color": 2, "lineweight": 25}
    if kind == "Point":
        a = m.add_point((1000, 2000), dxfattribs=attrs)
        b = m.add_point((3000, 4000), dxfattribs=attrs)
    elif kind == "LineString":
        a = m.add_line((0, 0), (8000, 0), dxfattribs=attrs)
        b = m.add_line((0, 3000), (8000, 3000), dxfattribs=attrs)
    else:
        a = m.add_lwpolyline(
            [(0, 0), (8000, 0), (8000, 3000), (0, 3000)], close=True, dxfattribs=attrs
        )
        b = m.add_lwpolyline(
            [(10000, 0), (18000, 0), (18000, 3000), (10000, 3000)], close=True, dxfattribs=attrs
        )
    base = encode(doc)
    placement = fit_placement(
        document_id=DOC,
        version=1,
        crs_code=3857,
        units=CadUnit.MILLIMETER,
        controls=(
            ((0, 0), (11800000, 1200000)),
            ((8000, 0), (11800000, 1200008)),
            ((0, 3000), (11799997, 1200000)),
        ),
        tolerance_m=0.001,
        max_scale_deviation=0.001,
    )
    _, features, summary, _ = parse_dxf_2d(base, units=CadUnit.MILLIMETER)
    place_features(features, placement)
    return doc, a.dxf.handle, b.dxf.handle, base, placement, summary["entity_mapping"], features


def args(base, candidate, p, mapping):
    return dict(
        base_sha256=digest(base),
        candidate_sha256=digest(candidate),
        placement=p,
        mapping=mapping,
        imported_content_revision="c",
        current_content_revision="c",
        imported_schema_revision="s",
        current_schema_revision="s",
    )


def shift(geometry):
    coords = geometry["coordinates"]
    if geometry["type"] == "Point":
        coords[1] += 1
    elif geometry["type"] == "LineString":
        coords[1][1] += 1
    else:
        coords[0][1][1] += 1
        coords[0][2][1] += 1


@pytest.mark.parametrize("kind", ["Point", "LineString", "Polygon"])
def test_bidirectional_native_proposal_preserves_handle_and_unrelated_entity(kind):
    doc, h1, h2, base, p, mapping, features = fixture(kind)
    edited = deepcopy(features)
    shift(edited[0]["geometry"])
    original = deepcopy(edited)
    out, changed = gis_to_cad_preview(
        base, base_sha256=digest(base), placement=p, mapping=mapping, features=edited
    )
    assert edited == original
    assert changed == (h1,)
    reopened = ezdxf.read(io.StringIO(out.decode()))
    assert [e.dxf.handle for e in reopened.modelspace()] == [h1, h2]
    for handle in [h1, h2]:
        ent = reopened.entitydb[handle]
        assert ent.dxf.layer == "Control" and ent.dxf.color == 2 and ent.dxf.lineweight == 25
    ops = cad_to_gis_preview(base, out, **args(base, out, p, mapping))
    assert len(ops) == 1 and ops[0].op_type == EditOpType.UPDATE
    assert ops[0].feature_id == features[0]["feature_id"] and ops[0].properties is None
    assert ops[0].geometry == edited[0]["geometry"]
    same, changed = gis_to_cad_preview(
        base, base_sha256=digest(base), placement=p, mapping=mapping, features=features
    )
    assert same == base and changed == ()


def test_order_is_not_identity():
    doc, h1, h2, base, p, mapping, features = fixture("LineString")
    m = doc.modelspace()
    ent = doc.entitydb[h1]
    m.unlink_entity(ent)
    ent.dxf.end = (9000, 0, 0)
    m.add_entity(ent)
    candidate = encode(doc)
    assert [e.dxf.handle for e in m] == [h2, h1]
    ops = cad_to_gis_preview(base, candidate, **args(base, candidate, p, mapping))
    assert len(ops) == 1 and ops[0].feature_id == features[0]["feature_id"]


@pytest.mark.parametrize("change", ["create", "delete", "layer", "type", "unsupported", "z"])
def test_unsafe_native_changes_rejected(change):
    doc, h1, _, base, p, mapping, _ = fixture("LineString")
    m = doc.modelspace()
    ent = doc.entitydb[h1]
    if change == "create":
        m.add_line((1, 1), (2, 2))
    elif change == "delete":
        m.delete_entity(ent)
    elif change == "layer":
        ent.dxf.layer = "0"
    elif change == "type":
        m.delete_entity(ent)
        m.add_point((1, 1))
    elif change == "unsupported":
        m.add_text("Not mapped")
    else:
        ent.dxf.end = (9000, 0, 1)
    candidate = encode(doc)
    with pytest.raises(PlatformValidationError):
        cad_to_gis_preview(base, candidate, **args(base, candidate, p, mapping))


def test_revision_and_mapping_checks():
    _, _, _, base, p, mapping, features = fixture("LineString")
    for override in (
        {"base_sha256": "0" * 64},
        {"candidate_sha256": "0" * 64},
        {"current_content_revision": "stale"},
        {"current_schema_revision": "stale"},
    ):
        with pytest.raises(PlatformConflictError):
            cad_to_gis_preview(base, base, **(args(base, base, p, mapping) | override))
    for bad in (
        mapping[:1],
        [mapping[0], mapping[0]],
        [mapping[0], mapping[1] | {"feature_id": mapping[0]["feature_id"]}],
    ):
        with pytest.raises(PlatformValidationError):
            cad_to_gis_preview(base, base, **args(base, base, p, bad))
    for bad in (features[:1], features + [deepcopy(features[0])]):
        with pytest.raises(PlatformValidationError):
            gis_to_cad_preview(
                base, base_sha256=digest(base), placement=p, mapping=mapping, features=bad
            )


def test_no_line_to_polyline_or_polygon_hole_conversion():
    _, _, _, base, p, mapping, features = fixture("LineString")
    features[0]["geometry"]["coordinates"].insert(1, [11800000, 1200004])
    with pytest.raises(PlatformValidationError):
        gis_to_cad_preview(
            base, base_sha256=digest(base), placement=p, mapping=mapping, features=features
        )
    _, _, _, base, p, mapping, features = fixture("Polygon")
    features[0]["geometry"]["coordinates"].append(
        deepcopy(features[0]["geometry"]["coordinates"][0])
    )
    with pytest.raises(PlatformValidationError):
        gis_to_cad_preview(
            base, base_sha256=digest(base), placement=p, mapping=mapping, features=features
        )


@pytest.mark.parametrize("case", range(100))
def test_geometry_proposals_converge_under_replay(case):
    kind = ("Point", "LineString", "Polygon")[case % 3]
    _, h1, _, base, p, mapping, features = fixture(kind)
    edited = deepcopy(features)
    # deterministic millimetre increments, with a projected coordinate offset
    # that is not restricted to integer metres.
    amount = (case + 1) / 1000
    coords = edited[0]["geometry"]["coordinates"]
    if kind == "Point":
        coords[1] += amount
    elif kind == "LineString":
        coords[1][1] += amount
    else:
        coords[0][1][1] += amount
        coords[0][2][1] += amount
    proposed, handles = gis_to_cad_preview(
        base, base_sha256=digest(base), placement=p, mapping=mapping, features=edited
    )
    assert handles == (h1,)
    ops = cad_to_gis_preview(base, proposed, **args(base, proposed, p, mapping))
    assert len(ops) == 1 and ops[0].feature_id == features[0]["feature_id"]
    assert ops == cad_to_gis_preview(base, proposed, **args(base, proposed, p, mapping))
    # Proposal convergence is not network/transaction idempotency.
    _, roundtrip, summary, _ = parse_dxf_2d(proposed, units=CadUnit.MILLIMETER)
    place_features(roundtrip, p)
    assert {row["handle"] for row in summary["entity_mapping"]} == {
        row["handle"] for row in mapping
    }
    assert roundtrip[1]["geometry"] == features[1]["geometry"]
    again, changed = gis_to_cad_preview(
        proposed, base_sha256=digest(proposed), placement=p, mapping=mapping, features=roundtrip
    )
    assert again == proposed and changed == ()
