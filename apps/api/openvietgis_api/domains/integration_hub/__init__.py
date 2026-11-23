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
from openvietgis_api.domains.integration_hub.sync_diff import (
    MAX_SYNC_OPERATIONS,
    ServerFeatureSnapshot,
    SyncDiffPlan,
    SyncOperationPreview,
    UploadedFeatureSnapshot,
    compute_sync_diff,
)

__all__ = [
    "ACTION_PERMISSIONS",
    "ADAPTERS",
    "AdapterLifecycle",
    "AdapterManifest",
    "ConnectorAction",
    "LayerAccessSnapshot",
    "MAX_SYNC_OPERATIONS",
    "PROFILE_ENABLED_ADAPTER_KEYS",
    "ServerFeatureSnapshot",
    "SyncDiffPlan",
    "SyncOperationPreview",
    "UploadedFeatureSnapshot",
    "build_adapter_profile",
    "compute_sync_diff",
    "evaluate_action",
    "execution_status_for",
    "get_adapter",
    "list_adapters",
    "serialize_adapter",
]
