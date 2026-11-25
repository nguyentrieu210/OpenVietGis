"""E15: keep the mounted FastAPI module contract when Rust hot paths are enabled."""
import json
from pathlib import Path

from fastapi.routing import APIRoute, iter_route_contexts

from openvietgis_api.main import create_app

ROOT = Path(__file__).resolve().parents[3]


def test_retained_house_numbering_routes_match_frozen_inventory():
    inventory = json.loads(
        (ROOT / "specs/064-rust-platform/contracts/engines/e15.json").read_text()
    )
    expected = {
        (method, route["path"], route["handler"])
        for route in inventory["http_routes"]
        for method in route["methods"]
    }
    routes = [
        route
        for route in iter_route_contexts(create_app().routes)
        if isinstance(route.original_route, APIRoute)
        and route.path
        and "/modules/house-numbering/" in route.path
    ]
    observed = {
        (method, route.path, route.endpoint.__name__)
        for route in routes
        for method in route.methods
    }
    assert len(expected) == 72
    assert observed == expected
    for route in routes:
        dependency_names = {
            dependency.call.__name__
            for dependency in route.dependant.dependencies
            if dependency.call is not None and hasattr(dependency.call, "__name__")
        }
        assert "project_access_guard" in dependency_names, route.path
