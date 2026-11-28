"""Dashboard coverage and privacy contract for SPEC 066 aggregate telemetry."""
import json
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parents[2]


def test_dashboard_panels_use_only_aggregate_metrics_and_document_receipts():
    dashboard = json.loads(
        (ROOT / "infra/observability/spec066.dashboard.json").read_text()
    )
    assert dashboard["uid"] == "openvietgis-spec066-ops"
    panels = dashboard["panels"]
    titles = {panel["title"] for panel in panels}
    assert {
        "HTTP request latency (p95 / p99)",
        "HTTP server error ratio",
        "HTTP request rate",
        "Private tile cache",
        "API process memory",
        "Private tile cache observations",
        "Build and deployment receipts",
    } <= titles

    rules = yaml.safe_load(
        (ROOT / "infra/observability/spec066.rules.yml").read_text()
    )["groups"][0]["rules"]
    record_names = {rule["record"] for rule in rules if "record" in rule}
    queries = [
        target["expr"]
        for panel in panels
        for target in panel.get("targets", [])
        if target.get("expr")
    ]
    # Every recording rule displayed by the dashboard is declared in the shipped
    # rules file. Raw cache counters are exported directly by TileCache.
    assert {
        "ovg:http_latency:p95_5m",
        "ovg:http_latency:p99_5m",
        "ovg:http_errors:ratio_5m",
        "ovg:http_requests:rate_5m",
        "ovg:tile_cache:hit_ratio_5m",
        "ovg:tile_cache:capacity_ratio",
        "ovg:api:resident_memory_bytes",
        "ovg:api:max_instance_resident_memory_bytes",
    } <= set(queries)
    assert record_names <= set(queries)

    serialized = json.dumps(dashboard).lower()
    for forbidden in ("organization_id", "project_id", "actor_id", "feature_id", "token=", "route="):
        assert forbidden not in serialized
    receipt_panel = next(panel for panel in panels if panel["title"] == "Build and deployment receipts")
    assert "release-qualification.json" in receipt_panel["options"]["content"]
    assert "deployment-observation.json" in receipt_panel["options"]["content"]
    assert "not deployment approval" in receipt_panel["options"]["content"]
