"""Cross-software interoperability catalog and read-only adapter profiles."""

from openvietgis_api.domains.integration_hub.catalog import (
    ACTION_PERMISSIONS,
    ADAPTERS,
    AdapterLifecycle,
    AdapterManifest,
    ConnectorAction,
    evaluate_action,
    get_adapter,
    list_adapters,
    serialize_adapter,
)
from openvietgis_api.domains.integration_hub.profiles import (
    PROFILE_ENABLED_ADAPTER_KEYS,
    LayerAccessSnapshot,
    build_adapter_profile,
    execution_status_for,
)

__all__ = [
    "ACTION_PERMISSIONS",
    "ADAPTERS",
    "AdapterLifecycle",
    "AdapterManifest",
    "ConnectorAction",
    "LayerAccessSnapshot",
    "PROFILE_ENABLED_ADAPTER_KEYS",
    "build_adapter_profile",
    "evaluate_action",
    "execution_status_for",
    "get_adapter",
    "list_adapters",
    "serialize_adapter",
]
