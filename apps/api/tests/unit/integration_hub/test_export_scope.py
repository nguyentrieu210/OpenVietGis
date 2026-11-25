"""Spec 054: scoped export SQL composition (area / route) stays parameterised."""

from __future__ import annotations

from openvietgis_api.adapters.http.integration_hub import _export_features_sql, _natural_key


def test_unscoped_export_sql_is_unchanged_apart_from_output_crs() -> None:
    sql = str(_export_features_sql(scoped=False, area=False, route_mode=None))
    assert "ST_Intersects" not in sql and "ST_DWithin" not in sql and "tuyen_id" not in sql
    assert "feature_allowed" not in sql
    assert "feature_allowed" in str(_export_features_sql(scoped=True, area=False, route_mode=None))


def test_area_and_route_filters_use_bound_values_only() -> None:
    area = str(_export_features_sql(scoped=False, area=True, route_mode=None))
    assert "ST_Intersects" in area and ":area_fids" in area and ":native_srid" in area
    assert ":route_ids" in str(_export_features_sql(scoped=False, area=False, route_mode="self"))
    assert "attributes->>'tuyen_id' = ANY(:route_ids)" in str(
        _export_features_sql(scoped=False, area=False, route_mode="attribute")
    )
    near = str(_export_features_sql(scoped=True, area=True, route_mode="distance"))
    assert "ST_DWithin" in near and ":route_distance_m" in near and "feature_allowed" in near


def test_hamlets_sort_numerically() -> None:
    names = ["Ấp 10", "Ấp 2", "Ấp 1"]
    assert sorted(names, key=_natural_key) == ["Ấp 1", "Ấp 2", "Ấp 10"]
