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
