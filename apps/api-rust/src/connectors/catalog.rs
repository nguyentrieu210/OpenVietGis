//! E12 Rust-native integration hub catalog policy.
//!
//! This module owns OpenVietGIS adapter metadata and fail-closed action
//! eligibility. External engines (QGIS, AutoCAD, PostGIS, OGC services) remain
//! adapters below this control plane.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionLevel {
    Read,
    Edit,
    Manage,
}

impl PermissionLevel {
    fn satisfies(self, required: Self) -> bool {
        matches!(
            (self, required),
            (Self::Manage, _) | (Self::Edit, Self::Read | Self::Edit) | (Self::Read, Self::Read)
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectorAction {
    Discover,
    Inspect,
    Configure,
    Import,
    Export,
    SyncPull,
    SyncPush,
    Approve,
}

impl ConnectorAction {
    pub fn wire(self) -> &'static str {
        match self {
            Self::Discover => "catalog.discover",
            Self::Inspect => "connection.inspect",
            Self::Configure => "connection.configure",
            Self::Import => "data.import",
            Self::Export => "data.export",
            Self::SyncPull => "sync.pull",
            Self::SyncPush => "sync.push",
            Self::Approve => "sync.approve",
        }
    }

    pub fn required_permission(self) -> PermissionLevel {
        match self {
            Self::Discover | Self::Inspect | Self::Export | Self::SyncPull => PermissionLevel::Read,
            Self::Import | Self::SyncPush => PermissionLevel::Edit,
            Self::Configure | Self::Approve => PermissionLevel::Manage,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lifecycle {
    ExistingReadOnly,
    Candidate,
    Planned,
}

#[derive(Debug, Clone, Copy)]
pub struct AdapterManifest {
    pub key: &'static str,
    pub name: &'static str,
    pub category: &'static str,
    pub mode: &'static str,
    pub lifecycle: Lifecycle,
    pub protocols: &'static [&'static str],
    pub proposed_actions: &'static [ConnectorAction],
    pub executable_actions: &'static [ConnectorAction],
}

const DISCOVER: &[ConnectorAction] = &[ConnectorAction::Discover];
const QGIS_PROPOSED: &[ConnectorAction] = &[
    ConnectorAction::Inspect,
    ConnectorAction::Export,
    ConnectorAction::SyncPull,
    ConnectorAction::SyncPush,
];
const QGIS_EXECUTABLE: &[ConnectorAction] = &[
    ConnectorAction::Discover,
    ConnectorAction::Export,
    ConnectorAction::SyncPush,
];
const AUTOCAD_PROPOSED: &[ConnectorAction] = &[ConnectorAction::Import, ConnectorAction::Export];
const AUTOCAD_EXECUTABLE: &[ConnectorAction] =
    &[ConnectorAction::Discover, ConnectorAction::Export];
const POSTGIS_PROPOSED: &[ConnectorAction] = &[
    ConnectorAction::Inspect,
    ConnectorAction::Export,
    ConnectorAction::SyncPull,
];
const POSTGIS_EXECUTABLE: &[ConnectorAction] =
    &[ConnectorAction::Discover, ConnectorAction::Export];
const OGC_PROPOSED: &[ConnectorAction] = &[ConnectorAction::Inspect];
const INSPECT_SYNC_PULL: &[ConnectorAction] =
    &[ConnectorAction::Inspect, ConnectorAction::SyncPull];
const IMPORT_EXPORT: &[ConnectorAction] = &[ConnectorAction::Import, ConnectorAction::Export];

pub const ADAPTERS: &[AdapterManifest] = &[
    AdapterManifest {
        key: "ogc_services",
        name: "OGC WMS/WFS",
        category: "gis",
        mode: "remote_service",
        lifecycle: Lifecycle::ExistingReadOnly,
        protocols: &["WMS", "WFS", "COG", "XYZ"],
        proposed_actions: OGC_PROPOSED,
        executable_actions: DISCOVER,
    },
    AdapterManifest {
        key: "qgis",
        name: "QGIS",
        category: "gis",
        mode: "open_protocols",
        lifecycle: Lifecycle::ExistingReadOnly,
        protocols: &["GeoPackage", "PostGIS", "WMS", "WFS"],
        proposed_actions: QGIS_PROPOSED,
        executable_actions: QGIS_EXECUTABLE,
    },
    AdapterManifest {
        key: "arcgis",
        name: "ArcGIS Pro / Online",
        category: "gis",
        mode: "remote_service",
        lifecycle: Lifecycle::Planned,
        protocols: &["ArcGIS Feature Service", "FileGDB", "SHP"],
        proposed_actions: INSPECT_SYNC_PULL,
        executable_actions: DISCOVER,
    },
    AdapterManifest {
        key: "autocad",
        name: "AutoCAD / Map 3D",
        category: "cad",
        mode: "file_or_desktop_bridge",
        lifecycle: Lifecycle::ExistingReadOnly,
        protocols: &["DXF", "DWG"],
        proposed_actions: AUTOCAD_PROPOSED,
        executable_actions: AUTOCAD_EXECUTABLE,
    },
    AdapterManifest {
        key: "microstation_v8i",
        name: "MicroStation V8i",
        category: "cad",
        mode: "windows_bridge",
        lifecycle: Lifecycle::Candidate,
        protocols: &["DGN V8", "COM"],
        proposed_actions: &[ConnectorAction::Inspect, ConnectorAction::Import],
        executable_actions: DISCOVER,
    },
    AdapterManifest {
        key: "mapinfo",
        name: "MapInfo Pro",
        category: "gis",
        mode: "file_exchange",
        lifecycle: Lifecycle::Planned,
        protocols: &["TAB", "MIF", "MID"],
        proposed_actions: IMPORT_EXPORT,
        executable_actions: DISCOVER,
    },
    AdapterManifest {
        key: "cadastral_vn",
        name: "gCadas / FAMIS / TMV.Map",
        category: "cadastral",
        mode: "format_adapter",
        lifecycle: Lifecycle::Planned,
        protocols: &["DGN", "cadastral_attributes"],
        proposed_actions: IMPORT_EXPORT,
        executable_actions: DISCOVER,
    },
    AdapterManifest {
        key: "postgis",
        name: "PostgreSQL / PostGIS",
        category: "database",
        mode: "database_bridge",
        lifecycle: Lifecycle::ExistingReadOnly,
        protocols: &["PostGIS"],
        proposed_actions: POSTGIS_PROPOSED,
        executable_actions: POSTGIS_EXECUTABLE,
    },
    AdapterManifest {
        key: "ogc_publisher",
        name: "OGC publishing servers",
        category: "gis",
        mode: "remote_service",
        lifecycle: Lifecycle::Planned,
        protocols: &["WMS", "WFS", "OGC API"],
        proposed_actions: INSPECT_SYNC_PULL,
        executable_actions: DISCOVER,
    },
    AdapterManifest {
        key: "google_earth",
        name: "Google Earth Pro",
        category: "gis",
        mode: "file_exchange",
        lifecycle: Lifecycle::Planned,
        protocols: &["KML", "KMZ"],
        proposed_actions: IMPORT_EXPORT,
        executable_actions: DISCOVER,
    },
    AdapterManifest {
        key: "excel",
        name: "Excel / CSV",
        category: "tabular",
        mode: "file_exchange",
        lifecycle: Lifecycle::Planned,
        protocols: &["XLSX", "CSV"],
        proposed_actions: IMPORT_EXPORT,
        executable_actions: DISCOVER,
    },
    AdapterManifest {
        key: "survey_3d",
        name: "RTK / UAV / BIM",
        category: "survey_3d",
        mode: "file_exchange",
        lifecycle: Lifecycle::Planned,
        protocols: &["LandXML", "GeoTIFF", "LAS", "3D Tiles"],
        proposed_actions: IMPORT_EXPORT,
        executable_actions: DISCOVER,
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionDecision {
    pub allowed: bool,
    pub eligible_by_role: bool,
    pub executable_in_hub: bool,
    pub reason: &'static str,
}

pub fn evaluate_action(
    adapter: &AdapterManifest,
    action: ConnectorAction,
    permission: PermissionLevel,
) -> ActionDecision {
    let eligible = permission.satisfies(action.required_permission());
    let declared =
        matches!(action, ConnectorAction::Discover) || adapter.proposed_actions.contains(&action);
    let executable = adapter.executable_actions.contains(&action);
    let allowed = eligible && declared && executable;
    let reason = if !eligible {
        "permission_denied"
    } else if !declared {
        "unsupported_by_adapter"
    } else if !executable {
        "not_implemented"
    } else if matches!(action, ConnectorAction::Export) {
        "read_only_export"
    } else if matches!(action, ConnectorAction::SyncPush) {
        "controlled_changeset_sync"
    } else {
        "catalog_only"
    };
    ActionDecision {
        allowed,
        eligible_by_role: eligible,
        executable_in_hub: executable,
        reason,
    }
}

pub fn get_adapter(key: &str) -> Option<&'static AdapterManifest> {
    ADAPTERS.iter().find(|adapter| adapter.key == key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapter_inventory_matches_the_python_catalog_order() {
        let keys: Vec<_> = ADAPTERS.iter().map(|adapter| adapter.key).collect();
        assert_eq!(
            keys,
            vec![
                "ogc_services",
                "qgis",
                "arcgis",
                "autocad",
                "microstation_v8i",
                "mapinfo",
                "cadastral_vn",
                "postgis",
                "ogc_publisher",
                "google_earth",
                "excel",
                "survey_3d",
            ]
        );
        for key in [
            "arcgis",
            "mapinfo",
            "cadastral_vn",
            "ogc_publisher",
            "google_earth",
            "excel",
            "survey_3d",
        ] {
            let adapter = get_adapter(key).unwrap();
            assert_eq!(adapter.lifecycle, Lifecycle::Planned);
            assert_eq!(adapter.executable_actions, DISCOVER);
        }
    }

    #[test]
    fn qgis_writeback_is_only_the_controlled_sync_path() {
        let qgis = get_adapter("qgis").unwrap();
        let sync = evaluate_action(qgis, ConnectorAction::SyncPush, PermissionLevel::Edit);
        assert!(sync.allowed);
        assert_eq!(sync.reason, "controlled_changeset_sync");

        let import = evaluate_action(qgis, ConnectorAction::Import, PermissionLevel::Manage);
        assert!(!import.allowed);
        assert_eq!(import.reason, "unsupported_by_adapter");
    }

    #[test]
    fn catalog_fails_closed_by_permission_and_implementation() {
        let cad = get_adapter("autocad").unwrap();
        let export = evaluate_action(cad, ConnectorAction::Export, PermissionLevel::Read);
        assert!(export.allowed);
        assert_eq!(export.reason, "read_only_export");

        let import = evaluate_action(cad, ConnectorAction::Import, PermissionLevel::Edit);
        assert!(!import.allowed);
        assert_eq!(import.reason, "not_implemented");

        let qgis = get_adapter("qgis").unwrap();
        let push = evaluate_action(qgis, ConnectorAction::SyncPush, PermissionLevel::Read);
        assert!(!push.allowed);
        assert_eq!(push.reason, "permission_denied");
    }
}
