"""Spec 054: scoped export SQL composition (area / route) stays parameterised."""

from __future__ import annotations

from openvietgis_api.adapters.http.integration_hub import _export_features_sql, _natural_key


def test_unscoped_export_sql_has_no_scope_filters() -> None:
    sql = str(_export_features_sql(scoped=False, area_mode=None, route_mode=None))
    assert "ST_Intersects" not in sql and "ST_DWithin" not in sql and "tuyen_id" not in sql
    assert "feature_allowed" not in sql
    scoped = str(_export_features_sql(scoped=True, area_mode=None, route_mode=None))
    assert "feature_allowed" in scoped


def test_area_and_route_filters_use_bound_values_only() -> None:
    spatial = str(_export_features_sql(scoped=False, area_mode="spatial", route_mode=None))
    assert "ST_Intersects" in spatial and ":area_fids" in spatial and ":native_srid" in spatial
    by_name = str(_export_features_sql(scoped=False, area_mode="attribute", route_mode=None))
    assert "attributes->>'ap' = ANY(:area_names)" in by_name
    own = str(_export_features_sql(scoped=False, area_mode="own_name", route_mode="self"))
    assert "attributes->>'name' = ANY(:area_names)" in own and ":route_ids" in own
    assert "attributes->>'tuyen_id' = ANY(:route_ids)" in str(
        _export_features_sql(scoped=False, area_mode=None, route_mode="attribute")
    )
    near = str(_export_features_sql(scoped=True, area_mode="spatial", route_mode="distance"))
    assert "ST_DWithin" in near and ":route_distance_m" in near and "feature_allowed" in near


def test_hamlets_sort_numerically() -> None:
    names = ["Ấp 10", "Ấp 2", "Ấp 1"]
    assert sorted(names, key=_natural_key) == ["Ấp 1", "Ấp 2", "Ấp 10"]


def test_historic_hamlet_layer_is_not_a_current_hamlet_scope() -> None:
    from types import SimpleNamespace
    from uuid import uuid4

    from openvietgis_api.adapters.http.integration_hub import _scope_layers

    rows = [
        {"layer_id": uuid4(), "name": n, "source_id": n, "geometry_family": f}
        for n, f in [
            ("Ranh xã, phường", "Polygon"),
            ("Ấp hiện hành", "Polygon"),
            ("Ấp cũ", "Polygon"),
            ("Đường và hẻm", "LineString"),
            ("Số nhà cũ (sổ biên mục, dự án)", "Point"),
        ]
    ]
    session = SimpleNamespace(
        execute=lambda *_: SimpleNamespace(mappings=lambda: SimpleNamespace(all=lambda: rows))
    )
    found = _scope_layers(session, uuid4(), uuid4())  # type: ignore[arg-type]
    assert found == {
        "commune": ["Ranh xã, phường"],
        "hamlet": ["Ấp hiện hành"],
        "route": ["Đường và hẻm"],
    }


def test_ordinary_layers_mentioning_xa_or_duong_are_not_scope_layers() -> None:
    from types import SimpleNamespace
    from uuid import uuid4

    from openvietgis_api.adapters.http.integration_hub import _scope_layers

    rows = [
        {"layer_id": uuid4(), "name": n, "source_id": n, "geometry_family": f}
        for n, f in [
            ("Thửa đất xã Bình Hưng", "Polygon"),
            ("Trạm biến áp", "Polygon"),
            ("Đường dây điện", "LineString"),
            ("Ranh giới xã Bình Hưng", "Polygon"),
            ("Tuyến cần đối soát", "LineString"),
        ]
    ]
    session = SimpleNamespace(
        execute=lambda *_: SimpleNamespace(mappings=lambda: SimpleNamespace(all=lambda: rows))
    )
    found = _scope_layers(session, uuid4(), uuid4())  # type: ignore[arg-type]
    assert found == {
        "commune": ["Ranh giới xã Bình Hưng"],
        "hamlet": [],
        "route": ["Tuyến cần đối soát"],
    }


def test_commune_boundary_scope_selects_the_commune_not_its_neighbours() -> None:
    sql = str(_export_features_sql(scoped=False, area_mode="commune", route_mode=None))
    assert "feature_id = ANY(:area_fids)" in sql and ":area_codes" in sql
    assert "ST_Intersects" not in sql
