"""Focused regression checks for the unified v1.0 connector hardening slice."""

from __future__ import annotations

import ast
from pathlib import Path
from types import SimpleNamespace

import pytest

from integrations.qgis_plugin.openvietgis_connector.client import ApiError
from integrations.qgis_plugin.openvietgis_connector.managed_client import (
    pin_managed_write,
)

ROOT = Path(__file__).resolve().parents[2]
PLUGIN = ROOT / "integrations/qgis_plugin/openvietgis_connector/plugin.py"


def _pin(previous=None, *, value="A", revision="a" * 64, deletes=False):
    return pin_managed_write(
        previous=previous,
        source_id="fixture-source",
        base_content_revision=revision,
        base_schema_revision="b" * 64,
        operations=[{"op_type": "update", "feature_id": "A-001", "properties": {"name": value}}],
        allow_deletes=deletes,
    )


def test_same_payload_retry_reuses_operation_id():
    first = _pin()
    assert _pin(first) is first
    assert _pin().operation_id != first.operation_id


@pytest.mark.parametrize("changes", [{"value": "B"}, {"revision": "c" * 64}, {"deletes": True}])
def test_uncertain_write_rejects_changed_payload(changes):
    first = _pin()
    with pytest.raises(ApiError, match="chưa xác nhận"):
        _pin(first, **changes)


def test_unavailable_secure_store_never_returns_plaintext_or_base64():
    # Execute actual functions without importing native QGIS/PyQt bindings.
    source = ast.parse(PLUGIN.read_text(encoding="utf-8"))
    functions = [
        node for node in source.body
        if isinstance(node, ast.FunctionDef)
        and node.name in {"_dpapi_protect", "_dpapi_unprotect"}
    ]
    namespace = {"sys": SimpleNamespace(platform="linux")}
    exec(compile(ast.Module(body=functions, type_ignores=[]), str(PLUGIN), "exec"), namespace)  # noqa: S102 -- fixed local AST functions, no external input
    assert namespace["_dpapi_protect"]("synthetic-secret") == ""
    assert namespace["_dpapi_unprotect"]("b64:c3ludGhldGljLXNlY3JldA==") == ""


def test_only_delta_page_advances_managed_cursor():
    tree = ast.parse(PLUGIN.read_text(encoding="utf-8"))
    cls = next(
        n for n in tree.body
        if isinstance(n, ast.ClassDef) and n.name == "OpenVietGisConnectorDialog"
    )
    for method_name in ("_on_commit_r029_managed", "_on_open_r029_managed"):
        method = next(
            n for n in cls.body if isinstance(n, ast.FunctionDef) and n.name == method_name
        )
        method_source = ast.unparse(method)
        assert "receipt.event_cursor" not in method_source
        assert "first_page.cursor" not in method_source
    delta = next(
        n for n in cls.body
        if isinstance(n, ast.FunctionDef) and n.name == "_on_managed_fetch_delta"
    )
    assert "self._managed_cursor = delta.next_cursor" in ast.unparse(delta)
    assert "delta.has_more" in ast.unparse(delta)