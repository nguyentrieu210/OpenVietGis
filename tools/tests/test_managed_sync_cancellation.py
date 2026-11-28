"""Cancellation races must discard results and bound a multi-layer load."""

from dataclasses import asdict
from threading import Event
from types import SimpleNamespace

import pytest

from integrations.qgis_plugin.openvietgis_connector import managed_client as client


def page(layer="l", count=1, **overrides):
    return client.ManagedBootstrapPage(
        layer_id=layer, source_id="src", name="Fixture", geometry_family="Point",
        dimensions="XY", crs_code=5899, content_revision="a" * 64,
        schema_revision="b" * 64, cursor=3, permission_epoch=1, fields=(),
        features=tuple({"feature_id": f"p{i}"} for i in range(count)),
        has_more=False, next_offset=None, **overrides,
    )


def test_cancellation_during_final_http_page_discards_snapshot():
    """Exercise urllib on loopback, including a cancel arriving during the read."""
    import json
    from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
    from threading import Thread

    from integrations.qgis_plugin.openvietgis_connector.client import OpenVietGisConfig

    cancelled = Event()
    paths = []

    class Handler(BaseHTTPRequestHandler):
        def do_GET(self):
            paths.append(self.path)
            payload = json.dumps(asdict(page())).encode()
            self.send_response(200)
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            cancelled.set()
            self.wfile.write(payload)

        def log_message(self, *args):
            pass

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    worker = Thread(target=server.serve_forever, daemon=True)
    worker.start()
    try:
        cfg = OpenVietGisConfig(
            base_url=f"http://127.0.0.1:{server.server_port}",
            allow_insecure_localhost=True,
            bearer_token="fixture-token-1234567890",
            organization_id="00000000-0000-4000-8000-000000000001",
            project_id="00000000-0000-4000-8000-000000000002",
        )
        with pytest.raises(client.ApiError, match="hủy"):
            client.load_managed_snapshot(
                cfg, layer_id="l", session_id="s", permission_epoch=1,
                cancelled=cancelled.is_set,
            )
        assert len(paths) == 1
    finally:
        server.shutdown()
        server.server_close()
        worker.join(timeout=2)


@pytest.mark.parametrize("cancel_at", ["session", "snapshot"])
def test_cancelled_load_returns_no_layers_or_cache(monkeypatch, tmp_path, cancel_at):
    cancelled = Event()
    calls = []
    writes = []

    def session(*args, **kwargs):
        if cancel_at == "session":
            cancelled.set()
        return SimpleNamespace(id="s", permission_epoch=1)

    def bootstrap(*args, **kwargs):
        calls.append(kwargs["layer_id"])
        cancelled.set()
        return page()

    monkeypatch.setattr(client, "create_managed_session", session)
    monkeypatch.setattr(client, "bootstrap_managed_layer", bootstrap)
    monkeypatch.setattr(client, "cache_path", lambda *a: tmp_path / "missing.sqlite3")
    monkeypatch.setattr(client, "write_snapshot", lambda *a: writes.append(a))
    assert client.load_managed_layers(
        None, ["l", "later"], None, cancelled.is_set, cache_root=tmp_path,
    ) is None
    assert calls == ([] if cancel_at == "session" else ["l"])
    assert writes == []


def test_cancel_during_cache_probe_does_not_read_or_reload_cache(monkeypatch, tmp_path):
    cancelled = Event()
    path = tmp_path / "cache.sqlite3"
    path.touch()
    monkeypatch.setattr(client, "create_managed_session",
                        lambda *a, **k: SimpleNamespace(id="s", permission_epoch=1))
    monkeypatch.setattr(client, "cache_path", lambda *a: path)

    def probe(*args, **kwargs):
        cancelled.set()
        return page(cacheable=True, feature_count=1)

    monkeypatch.setattr(client, "bootstrap_managed_layer", probe)
    monkeypatch.setattr(client, "read_cached_record",
                        lambda *a, **k: pytest.fail("cache read after cancellation"))
    assert client.load_managed_layers(
        None, ["l"], None, cancelled.is_set, cache_root=tmp_path,
    ) is None


@pytest.mark.parametrize("cancel_at", ["delta", "final_probe"])
def test_cancelled_cached_catchup_stops_before_return_or_extra_request(monkeypatch, cancel_at):
    cancelled = Event()
    requests = []
    probe = page(cacheable=True, feature_count=1)
    record = {"cursor": 2, "revision": "old", "features": [{"feature_id": "p0"}]}
    delta = client.ManagedDeltaPage(False, 3, 1, False, ({
        "source_id": "src", "layer_id": "l", "content_revision": "a" * 64,
        "upserts": [], "removals": [],
    },))

    def fetch(*args, **kwargs):
        requests.append("delta")
        if cancel_at == "delta":
            cancelled.set()
        return delta

    def bootstrap(*args, **kwargs):
        requests.append("probe")
        cancelled.set()
        return probe

    monkeypatch.setattr(client, "fetch_managed_delta", fetch)
    monkeypatch.setattr(client, "bootstrap_managed_layer", bootstrap)
    assert client.catch_up_cached_layer(
        None, SimpleNamespace(id="s"), probe, record, cancelled.is_set,
    ) is None
    assert requests == (["delta"] if cancel_at == "delta" else ["delta", "probe"])


@pytest.mark.parametrize("cached", [False, True])
def test_feature_budget_is_shared_across_layers(monkeypatch, tmp_path, cached):
    monkeypatch.setattr(client, "MAX_MANAGED_FEATURES", 3)
    monkeypatch.setattr(client, "create_managed_session",
                        lambda *a, **k: SimpleNamespace(id="s", permission_epoch=1))
    calls = []
    writes = []

    def bootstrap(*args, **kwargs):
        calls.append(kwargs["layer_id"])
        return page(kwargs["layer_id"], 2, cacheable=cached, feature_count=2)

    monkeypatch.setattr(client, "bootstrap_managed_layer", bootstrap)
    monkeypatch.setattr(client, "write_snapshot", lambda *a: writes.append(a))
    monkeypatch.setattr(client, "cache_path", lambda *a: tmp_path / "missing.sqlite3")
    if cached:
        path = tmp_path / "cache.sqlite3"
        path.touch()
        monkeypatch.setattr(client, "cache_path", lambda *a: path)
        monkeypatch.setattr(client, "read_cached_record", lambda *a, **k: {
            "revision": "a" * 64, "features": [{"feature_id": "p0"}, {"feature_id": "p1"}],
        })
    with pytest.raises(client.ApiError, match="ngân sách"):
        client.load_managed_layers(None, ["one", "two"], None, cache_root=tmp_path)
    assert calls == ["one", "two"]
    assert len(writes) == (0 if cached else 1)


def test_delta_growth_cannot_exceed_remaining_feature_budget(monkeypatch):
    probe = page(cacheable=True, feature_count=2)
    record = {"cursor": 2, "revision": "old", "features": [{"feature_id": "p0"}]}
    delta = client.ManagedDeltaPage(False, 3, 1, False, ({
        "source_id": "src", "layer_id": "l", "content_revision": "a" * 64,
        "upserts": [{"feature_id": "p1"}], "removals": [],
    },))
    monkeypatch.setattr(client, "fetch_managed_delta", lambda *a, **k: delta)
    monkeypatch.setattr(client, "bootstrap_managed_layer",
                        lambda *a, **k: pytest.fail("probe after budget exceeded"))
    assert client.catch_up_cached_layer(
        None, SimpleNamespace(id="s"), probe, record, lambda: False, max_features=1,
    ) is None
