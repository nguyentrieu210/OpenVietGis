//! E12 read-only adapter profile serialization.

use ovg_core::lookup_readable_crs;
use ovg_storage::integration_hub::LayerAccessRow;
use serde_json::{json, Value};

use super::catalog::{AdapterManifest, Lifecycle};

pub fn execution_status(adapter: &AdapterManifest) -> &'static str {
    match adapter.key {
        "ogc_services" => "existing_read_only_api",
        "qgis" | "postgis" | "autocad" => "read_only_profile_ready",
        "microstation_v8i" => "candidate_external_branch",
        _ if adapter.lifecycle == Lifecycle::Candidate => "candidate_external_branch",
        _ => "planned_not_implemented",
    }
}

pub fn connection_profile(key: &str, organization_id: &str, project_id: &str) -> Option<Value> {
    let base = format!("/api/v1/organizations/{organization_id}/projects/{project_id}");
    match key {
        "qgis" => Some(json!({
            "adapter_key":"qgis",
            "access_mode":"read_only_exchange",
            "supports_direct_db_credentials":false,
            "requires_per_layer_acl":true,
            "default_write_policy":"deny",
            "sync_push_enabled":true,
            "deletions_blocked_by_default":true,
            "supported_protocols":["GeoPackage","GeoJSON","WMS","WFS","PostGIS_read_only_export"],
            "export_formats":["geopackage","geojson","postgis_sql"],
            "existing_endpoints":[
                {"name":"project_layers","method":"GET","path_template":format!("{base}/layers"),"required_permission":"project.read"},
                {"name":"scoped_geojson_export","method":"GET","path_template":format!("{base}/access/export/{{layer_id}}"),"required_permission":"project.read"},
                {"name":"hub_layer_export","method":"GET","path_template":format!("{base}/integration-hub/adapters/qgis/layers/{{layer_id}}/export"),"required_permission":"project.read"},
                {"name":"sync_preview","method":"POST","path_template":format!("{base}/integration-hub/adapters/qgis/layers/{{layer_id}}/sync-preview"),"required_permission":"project.edit"},
                {"name":"sync_push","method":"POST","path_template":format!("{base}/integration-hub/adapters/qgis/layers/{{layer_id}}/sync-push"),"required_permission":"project.edit"}
            ],
            "crs_policy":"explicit_only_never_inferred",
            "restrictions":[
                "No direct internal PostgreSQL/PostGIS credentials are exposed.",
                "Only active layers visible under RLS and per-layer/feature GIS ACL are listed.",
                "CRS is reported explicitly from stored_crs; missing CRS is never inferred.",
                "Two-step revision-pinned GeoPackage sync push commits through R006 changesets (deletions blocked by default); all other writebacks remain denied."
            ]
        })),
        "autocad" => Some(json!({
            "adapter_key":"autocad",
            "access_mode":"read_only_exchange",
            "supports_direct_db_credentials":false,
            "requires_per_layer_acl":true,
            "default_write_policy":"deny",
            "supported_protocols":["DXF"],
            "export_formats":["dxf"],
            "existing_endpoints":[
                {"name":"project_layers","method":"GET","path_template":format!("{base}/layers"),"required_permission":"project.read"},
                {"name":"hub_layer_export","method":"GET","path_template":format!("{base}/integration-hub/adapters/autocad/layers/{{layer_id}}/export?format=dxf"),"required_permission":"project.read"}
            ],
            "crs_policy":"explicit_only_never_inferred",
            "restrictions":[
                "Projected metre CRS only (EPSG:5899, EPSG:3857); geographic degree CRS is rejected.",
                "2D single-part geometries only (Point, LineString, Polygon without holes).",
                "Tabular attributes are not stored in DXF modelspace entities.",
                "Native DWG binaries are not supported and all CAD writebacks remain denied.",
                "No direct internal PostgreSQL/PostGIS credentials are exposed."
            ]
        })),
        "postgis" => Some(json!({
            "adapter_key":"postgis",
            "access_mode":"read_only_exchange",
            "supports_direct_db_credentials":false,
            "requires_per_layer_acl":true,
            "default_write_policy":"deny",
            "supported_protocols":["PostGIS_read_only_export","GeoPackage"],
            "export_formats":["postgis_sql","geopackage"],
            "existing_endpoints":[
                {"name":"project_layers","method":"GET","path_template":format!("{base}/layers"),"required_permission":"project.read"},
                {"name":"scoped_geojson_export","method":"GET","path_template":format!("{base}/access/export/{{layer_id}}"),"required_permission":"project.read"},
                {"name":"hub_layer_export","method":"GET","path_template":format!("{base}/integration-hub/adapters/postgis/layers/{{layer_id}}/export"),"required_permission":"project.read"}
            ],
            "crs_policy":"explicit_only_never_inferred",
            "restrictions":[
                "Internal OpenVietGIS PostGIS is never exposed directly as an external DB bridge.",
                "No direct internal PostgreSQL/PostGIS credentials are exposed.",
                "External DB connectors require host allowlist, credential vault, and review.",
                "Only active layers visible under RLS and per-layer/feature GIS ACL are listed.",
                "CRS is reported explicitly from stored_crs; missing CRS is never inferred.",
                "All PostGIS writeback and sync.push operations remain denied by default."
            ]
        })),
        "ogc_services" => Some(json!({
            "adapter_key":"ogc_services",
            "access_mode":"read_only_exchange",
            "supports_direct_db_credentials":false,
            "requires_per_layer_acl":true,
            "default_write_policy":"deny",
            "supported_protocols":["WMS","WFS","COG","XYZ"],
            "existing_endpoints":[
                {"name":"external_sources","method":"GET","path_template":format!("{base}/external-sources"),"required_permission":"project.read"}
            ],
            "crs_policy":"explicit_only_never_inferred",
            "restrictions":[
                "R010 SSRF allowlist and read-only enforcement remain unchanged.",
                "No direct internal PostgreSQL/PostGIS credentials are exposed.",
                "CRS is reported explicitly from stored_crs; missing CRS is never inferred.",
                "No hub mutation or remote writeback is permitted."
            ]
        })),
        _ => None,
    }
}

pub fn serialize_layer(row: &LayerAccessRow) -> Value {
    let explicit = row
        .stored_crs_code
        .and_then(|code| u32::try_from(code).ok())
        .is_some_and(|code| row.validation_state == "ready" && lookup_readable_crs(code).is_some());
    json!({
        "layer_id":row.layer_id,
        "source_id":row.source_id,
        "name":row.name,
        "geometry_family":row.geometry_family,
        "dimensions":row.dimensions,
        "validation_state":row.validation_state,
        "source_crs_code":row.source_crs_code,
        "stored_crs_code":row.stored_crs_code,
        "crs_explicit":explicit,
        "layer_revision":row.layer_revision,
        "content_revision":row.content_revision,
        "schema_revision":row.schema_revision,
        "export_permitted":row.export_acl && explicit,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn profile_enablement_matches_python() {
        for key in ["qgis", "postgis", "autocad", "ogc_services"] {
            assert!(connection_profile(key, "o", "p").is_some(), "{key}");
        }
        for key in ["arcgis", "microstation_v8i", "mapinfo", "excel"] {
            assert!(connection_profile(key, "o", "p").is_none(), "{key}");
        }
    }
}
