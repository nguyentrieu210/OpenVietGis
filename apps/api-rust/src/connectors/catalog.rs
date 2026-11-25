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
    pub fn wire(self) -> &'static str {
        match self {
            Self::Read => "project.read",
            Self::Edit => "project.edit",
            Self::Manage => "project.manage",
        }
    }

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

impl Lifecycle {
    pub fn wire(self) -> &'static str {
        match self {
            Self::ExistingReadOnly => "existing_read_only",
            Self::Candidate => "candidate",
            Self::Planned => "planned",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct AdapterManifest {
    pub key: &'static str,
    pub name: &'static str,
    pub category: &'static str,
    pub mode: &'static str,
    pub lifecycle: Lifecycle,
    pub protocols: &'static [&'static str],
    pub note: &'static str,
    pub proposed_actions: &'static [ConnectorAction],
    pub existing_api: Option<&'static str>,
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
        note: "R010 provides allowlisted read-only external sources; no hub sync.",
        proposed_actions: OGC_PROPOSED,
        existing_api: Some("external-sources"),
        executable_actions: DISCOVER,
    },
    AdapterManifest {
        key: "qgis",
        name: "QGIS",
        category: "gis",
        mode: "open_protocols",
        lifecycle: Lifecycle::ExistingReadOnly,
        protocols: &["GeoPackage", "PostGIS", "WMS", "WFS"],
        note: "Read-only GeoPackage/GeoJSON/PostGIS SQL export and two-step revision-pinned GeoPackage sync push via R006 changesets (deletions blocked by default).",
        proposed_actions: QGIS_PROPOSED,
        existing_api: Some("integration-hub"),
        executable_actions: QGIS_EXECUTABLE,
    },
    AdapterManifest {
        key: "arcgis",
        name: "ArcGIS Pro / Online",
        category: "gis",
        mode: "remote_service",
        lifecycle: Lifecycle::Planned,
        protocols: &["ArcGIS Feature Service", "FileGDB", "SHP"],
        note: "Vendor API authentication, feature ACL and versioning require tests.",
        proposed_actions: INSPECT_SYNC_PULL,
        existing_api: None,
        executable_actions: DISCOVER,
    },
    AdapterManifest {
        key: "autocad",
        name: "AutoCAD / Map 3D",
        category: "cad",
        mode: "file_or_desktop_bridge",
        lifecycle: Lifecycle::ExistingReadOnly,
        protocols: &["DXF", "DWG"],
        note: "Read-only 2D DXF export (R2010, projected metre CRS) verified in Hub; DWG native licensing and CAD writeback remain denied.",
        proposed_actions: AUTOCAD_PROPOSED,
        existing_api: Some("integration-hub"),
        executable_actions: AUTOCAD_EXECUTABLE,
    },
    AdapterManifest {
        key: "microstation_v8i",
        name: "MicroStation V8i",
        category: "cad",
        mode: "windows_bridge",
        lifecycle: Lifecycle::Candidate,
        protocols: &["DGN V8", "COM"],
        note: "R025 separate branch: licensed Windows instance, read-only preview first.",
        proposed_actions: &[ConnectorAction::Inspect, ConnectorAction::Import],
        existing_api: None,
        executable_actions: DISCOVER,
    },
    AdapterManifest {
        key: "mapinfo",
        name: "MapInfo Pro",
        category: "gis",
        mode: "file_exchange",
        lifecycle: Lifecycle::Planned,
        protocols: &["TAB", "MIF", "MID"],
        note: "Text, CRS and styling preservation not yet accepted.",
        proposed_actions: IMPORT_EXPORT,
        existing_api: None,
        executable_actions: DISCOVER,
    },
    AdapterManifest {
        key: "cadastral_vn",
        name: "gCadas / FAMIS / TMV.Map",
        category: "cadastral",
        mode: "format_adapter",
        lifecycle: Lifecycle::Planned,
        protocols: &["DGN", "cadastral_attributes"],
        note: "Requires lawful schema access and tested parcel/attribute binding.",
        proposed_actions: IMPORT_EXPORT,
        existing_api: None,
        executable_actions: DISCOVER,
    },
    AdapterManifest {
        key: "postgis",
        name: "PostgreSQL / PostGIS",
        category: "database",
        mode: "database_bridge",
        lifecycle: Lifecycle::ExistingReadOnly,
        protocols: &["PostGIS"],
        note: "Read-only PostGIS SQL and GeoPackage export for external PostGIS import; internal DB credentials are never exposed and writeback remains denied.",
        proposed_actions: POSTGIS_PROPOSED,
        existing_api: Some("integration-hub"),
        executable_actions: POSTGIS_EXECUTABLE,
    },
    AdapterManifest {
        key: "ogc_publisher",
        name: "OGC publishing servers",
        category: "gis",
        mode: "remote_service",
        lifecycle: Lifecycle::Planned,
        protocols: &["WMS", "WFS", "OGC API"],
        note: "Generic WMS/WFS may be registered in R010; no dedicated adapter.",
        proposed_actions: INSPECT_SYNC_PULL,
        existing_api: None,
        executable_actions: DISCOVER,
    },
    AdapterManifest {
        key: "google_earth",
        name: "Google Earth Pro",
        category: "gis",
        mode: "file_exchange",
        lifecycle: Lifecycle::Planned,
        protocols: &["KML", "KMZ"],
        note: "Dedicated exchange adapter not verified.",
        proposed_actions: IMPORT_EXPORT,
        existing_api: None,
        executable_actions: DISCOVER,
    },
    AdapterManifest {
        key: "excel",
        name: "Excel / CSV",
        category: "tabular",
        mode: "file_exchange",
        lifecycle: Lifecycle::Planned,
        protocols: &["XLSX", "CSV"],
        note: "Dedicated coordinate/field mapping profile not verified.",
        proposed_actions: IMPORT_EXPORT,
        existing_api: None,
        executable_actions: DISCOVER,
    },
    AdapterManifest {
        key: "survey_3d",
        name: "RTK / UAV / BIM",
        category: "survey_3d",
        mode: "file_exchange",
        lifecycle: Lifecycle::Planned,
        protocols: &["LandXML", "GeoTIFF", "LAS", "3D Tiles"],
        note: "Acquisition device and model adapters require separate acceptance.",
        proposed_actions: IMPORT_EXPORT,
        existing_api: None,
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

pub const ACTIONS: [ConnectorAction; 8] = [
    ConnectorAction::Discover,
    ConnectorAction::Inspect,
    ConnectorAction::Configure,
    ConnectorAction::Import,
    ConnectorAction::Export,
    ConnectorAction::SyncPull,
    ConnectorAction::SyncPush,
    ConnectorAction::Approve,
];

pub fn serialize_adapter(
    adapter: &AdapterManifest,
    permission: PermissionLevel,
) -> serde_json::Value {
    let actions = ACTIONS
        .iter()
        .copied()
        .map(|action| {
            let decision = evaluate_action(adapter, action, permission);
            serde_json::json!({
                "action": action.wire(),
                "required_permission": action.required_permission().wire(),
                "eligible_by_role": decision.eligible_by_role,
                "executable_in_hub": decision.executable_in_hub,
                "allowed": decision.allowed,
                "reason": decision.reason,
            })
        })
        .collect::<Vec<_>>();
    serde_json::json!({
        "key": adapter.key,
        "name": adapter.name,
        "category": adapter.category,
        "mode": adapter.mode,
        "lifecycle": adapter.lifecycle.wire(),
        "protocols": adapter.protocols,
        "note": adapter.note,
        "existing_api": adapter.existing_api,
        "actions": actions,
    })
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
    fn serialized_reader_qgis_matches_python_policy_shape() {
        let value = serialize_adapter(get_adapter("qgis").unwrap(), PermissionLevel::Read);
        assert_eq!(value["lifecycle"], "existing_read_only");
        assert_eq!(value["existing_api"], "integration-hub");
        let actions = value["actions"].as_array().unwrap();
        assert_eq!(actions.len(), 8);
        assert_eq!(actions[0]["action"], "catalog.discover");
        assert_eq!(actions[0]["required_permission"], "project.read");
        assert_eq!(actions[0]["allowed"], true);
        assert_eq!(actions[4]["action"], "data.export");
        assert_eq!(actions[4]["allowed"], true);
        assert_eq!(actions[4]["reason"], "read_only_export");
        assert_eq!(actions[6]["action"], "sync.push");
        assert_eq!(actions[6]["allowed"], false);
        assert_eq!(actions[6]["reason"], "permission_denied");
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
