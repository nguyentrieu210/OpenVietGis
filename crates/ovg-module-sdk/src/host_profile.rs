//! One host capability contract for SPEC075. A claimed platform is not proof of
//! authentication, device consent, an offline lease, or runtime qualification.
use crate::declarative::{CommandPolicy, Consent, Document, FieldType};
use ovg_core::AppError;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    Web,
    Android,
    Ios,
    Windows,
    Macos,
    Linux,
    Server,
}
impl Platform {
    pub fn parse(value: &str) -> Result<Self, AppError> {
        match value {
            "web" => Ok(Self::Web),
            "android" => Ok(Self::Android),
            "ios" => Ok(Self::Ios),
            "windows" => Ok(Self::Windows),
            "macos" => Ok(Self::Macos),
            "linux" => Ok(Self::Linux),
            "server" => Ok(Self::Server),
            _ => Err(AppError::validation(
                "INVALID_PLUGIN_PLATFORM",
                "unknown platform",
            )),
        }
    }
}
#[derive(Debug, Serialize)]
pub struct HostAvailability {
    pub platform: Platform,
    pub profile: &'static str,
    pub online_execution: bool,
    pub offline_execution: bool,
    pub reason: &'static str,
}
pub fn availability(platform: Platform, api: &str, web_enabled: bool) -> HostAvailability {
    let (enabled, reason) = match (platform, api, web_enabled) {
        (_, api, _) if api != "1.0.0" => (false, "HOST_API_NOT_QUALIFIED"),
        (Platform::Web, _, true) => (true, "PREVIEW_ONLY"),
        (Platform::Web, _, false) => (false, "RUNTIME_DISABLED"),
        (Platform::Android | Platform::Ios, _, _) => {
            (false, "NATIVE_HUMAN_TRANSPORT_NOT_QUALIFIED")
        }
        (Platform::Windows | Platform::Macos | Platform::Linux, _, _) => {
            (false, "DESKTOP_CLOUD_TRANSPORT_NOT_QUALIFIED")
        }
        (Platform::Server, _, _) => (false, "SERVER_PROFILE_NOT_QUALIFIED"),
    };
    HostAvailability {
        platform,
        profile: if enabled {
            "web_read_only"
        } else {
            "metadata_only"
        },
        online_execution: enabled,
        offline_execution: false,
        reason,
    }
}
pub const READ_COMMANDS: [(&str, &str); 5] = [
    ("host_context", "host_context"),
    ("host_notice", "host_notice"),
    ("gis_layers", "map_view"),
    ("gis_layer", "map_view"),
    ("gis_processing_plan", "processing_plan"),
];
/// Same reviewed capability mapping for all hosts. This validates the document,
/// but only a currently qualified host can obtain authorization to execute.
pub fn read_only_policies(key: &str, caps: &BTreeSet<String>) -> BTreeMap<String, CommandPolicy> {
    READ_COMMANDS
        .iter()
        .filter(|(_, cap)| {
            caps.contains(*cap) || (*cap == "processing_plan" && caps.contains("map_view"))
        })
        .map(|(command, cap)| {
            (
                (*command).to_owned(),
                CommandPolicy {
                    permission: format!("ext.{key}.{cap}"),
                    consent: Consent::Read,
                },
            )
        })
        .collect()
}
pub fn validate_read_only(
    doc: &Document,
    permissions: &BTreeSet<String>,
    policies: &BTreeMap<String, CommandPolicy>,
) -> Result<(), AppError> {
    let invalid = || {
        AppError::validation(
            "INVALID_PLUGIN_RUNTIME",
            "unsupported read-only command or input contract",
        )
    };
    if permissions
        .iter()
        .any(|p| !policies.values().any(|policy| policy.permission == *p))
    {
        return Err(invalid());
    }
    doc.validate(permissions, policies)?;
    for action in &doc.actions {
        let input = &action.input;
        let field = |name: &str, required: bool| {
            input.len() == 1
                && input
                    .get(name)
                    .is_some_and(|f| f.required == required && f.kind == FieldType::Text)
        };
        let valid = match action.command_id.as_str() {
            "host_context" => input.is_empty(),
            "host_notice" => field("text", true),
            "gis_layers" => input.is_empty() || field("after_id", false),
            "gis_layer" => field("layer_id", true),
            "gis_processing_plan" => field("request", true),
            _ => false,
        };
        if !valid {
            return Err(invalid());
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn platform_gate_fails_closed() {
        assert!(availability(Platform::Web, "1.0.0", true).online_execution);
        assert!(!availability(Platform::Web, "1.0.0", false).online_execution);
        assert!(!availability(Platform::Web, "2.0.0", true).online_execution);
        for p in [
            Platform::Android,
            Platform::Ios,
            Platform::Windows,
            Platform::Macos,
            Platform::Linux,
            Platform::Server,
        ] {
            let a = availability(p, "1.0.0", true);
            assert!(!a.online_execution && !a.offline_execution);
        }
    }
}
