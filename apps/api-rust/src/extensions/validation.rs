//! E14 Ed25519 signature verification and signed marketplace catalog loading.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::File;
use std::io::Read;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use ovg_core::AppError;
use ovg_storage::extensions::validation::validate_manifest_fields;
use serde_json::{json, Map, Value};

use crate::config::AppConfig;

pub const MARKETPLACE_SCHEMA: &str = "openvietgis.marketplace.v1";

#[derive(Debug, Clone)]
pub struct CatalogRelease {
    pub publisher_id: String,
    pub publisher_name: String,
    pub manifest: Value,
    pub signature: String,
}

impl CatalogRelease {
    #[must_use]
    pub fn wire(&self) -> Value {
        json!({
            "publisher_id": self.publisher_id,
            "publisher_name": self.publisher_name,
            "manifest": self.manifest,
            "signature": self.signature,
        })
    }
}

fn sort_json_keys(value: &Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(items.iter().map(sort_json_keys).collect()),
        Value::Object(map) => {
            let mut sorted = BTreeMap::new();
            for (k, v) in map {
                sorted.insert(k.clone(), sort_json_keys(v));
            }
            let mut out = Map::new();
            for (k, v) in sorted {
                out.insert(k, v);
            }
            Value::Object(out)
        }
        other => other.clone(),
    }
}

/// Canonical JSON bytes matching Python `marketplace.canonical`.
pub fn marketplace_canonical(value: &Value) -> Result<Vec<u8>, ()> {
    serde_json::to_vec(&sort_json_keys(value)).map_err(|_| ())
}

pub fn decode_ed25519_public_key(public_b64: &str) -> Result<VerifyingKey, ()> {
    let bytes = STANDARD.decode(public_b64).map_err(|_| ())?;
    if bytes.len() != 32 || STANDARD.encode(&bytes) != public_b64 {
        return Err(());
    }
    let arr: [u8; 32] = bytes.try_into().map_err(|_| ())?;
    VerifyingKey::from_bytes(&arr).map_err(|_| ())
}

pub fn verify_ed25519(public_b64: &str, value: &Value, sig_b64: &str) -> Result<(), ()> {
    let vk = decode_ed25519_public_key(public_b64)?;
    let sig_bytes = STANDARD.decode(sig_b64).map_err(|_| ())?;
    if sig_bytes.len() != 64 || STANDARD.encode(&sig_bytes) != sig_b64 {
        return Err(());
    }
    let sig_arr: [u8; 64] = sig_bytes.try_into().map_err(|_| ())?;
    let sig = Signature::from_bytes(&sig_arr);
    let msg = marketplace_canonical(value)?;
    vk.verify(&msg, &sig).map_err(|_| ())
}

fn valid_publisher_id(pid: &str) -> bool {
    let bytes = pid.as_bytes();
    (2..=32).contains(&bytes.len())
        && bytes[0].is_ascii_lowercase()
        && bytes[1..]
            .iter()
            .all(|b| matches!(*b, b'a'..=b'z' | b'0'..=b'9' | b'_'))
}

pub fn validate_catalog(payload: &Value) -> Result<Vec<CatalogRelease>, AppError> {
    let invalid = || AppError::validation("VALIDATION_ERROR", "invalid marketplace catalog");
    let obj = payload.as_object().ok_or_else(invalid)?;
    if obj.len() != 3
        || obj.get("schema").and_then(Value::as_str) != Some(MARKETPLACE_SCHEMA)
        || !obj.contains_key("publishers")
        || !obj.contains_key("releases")
    {
        return Err(invalid());
    }
    let publishers_raw = obj["publishers"].as_array().ok_or_else(invalid)?;
    let releases_raw = obj["releases"].as_array().ok_or_else(invalid)?;
    if publishers_raw.len() > 100 || releases_raw.len() > 500 {
        return Err(invalid());
    }

    let mut publishers: HashMap<String, (String, String)> = HashMap::new();
    for pub_val in publishers_raw {
        let p = pub_val.as_object().ok_or_else(invalid)?;
        if p.len() != 3
            || !p.contains_key("id")
            || !p.contains_key("name")
            || !p.contains_key("public_key")
        {
            return Err(invalid());
        }
        let pid = p["id"].as_str().ok_or_else(invalid)?;
        if !valid_publisher_id(pid) || publishers.contains_key(pid) {
            return Err(invalid());
        }
        let name = p["name"].as_str().ok_or_else(invalid)?;
        if name.trim().is_empty() || name.chars().count() > 120 {
            return Err(invalid());
        }
        let public_key = p["public_key"].as_str().ok_or_else(invalid)?;
        decode_ed25519_public_key(public_key).map_err(|()| invalid())?;
        publishers.insert(pid.to_owned(), (name.to_owned(), public_key.to_owned()));
    }

    const MANIFEST_FIELDS: [&str; 10] = [
        "extension_key",
        "version",
        "api_compatibility",
        "display_name",
        "description",
        "permissions",
        "commands",
        "capabilities",
        "entity_schemas",
        "ui_contributions",
    ];
    let mut seen = HashSet::new();
    let mut releases = Vec::with_capacity(releases_raw.len());
    for rel_val in releases_raw {
        let r = rel_val.as_object().ok_or_else(invalid)?;
        if r.len() != 3
            || !r.contains_key("publisher_id")
            || !r.contains_key("manifest")
            || !r.contains_key("signature")
        {
            return Err(invalid());
        }
        let pid = r["publisher_id"].as_str().ok_or_else(invalid)?;
        let Some((pub_name, pub_key)) = publishers.get(pid) else {
            return Err(invalid());
        };
        let manifest = r["manifest"].as_object().ok_or_else(invalid)?;
        if manifest.len() != MANIFEST_FIELDS.len()
            || MANIFEST_FIELDS.iter().any(|k| !manifest.contains_key(*k))
        {
            return Err(invalid());
        }
        let key = manifest["extension_key"].as_str().ok_or_else(invalid)?;
        let version = manifest["version"].as_str().ok_or_else(invalid)?;
        let api_compat = manifest["api_compatibility"].as_str().ok_or_else(invalid)?;
        let display_name = manifest["display_name"].as_str().ok_or_else(invalid)?;
        let description = manifest["description"].as_str().ok_or_else(invalid)?;
        let required_prefix = format!("partner_{pid}_");
        if !key.starts_with(&required_prefix)
            || !seen.insert((key.to_owned(), version.to_owned()))
            || display_name.trim().is_empty()
            || display_name.chars().count() > 200
            || description.chars().count() > 500
        {
            return Err(invalid());
        }
        validate_manifest_fields(
            key,
            version,
            api_compat,
            &manifest["permissions"],
            &manifest["commands"],
            &manifest["capabilities"],
            &manifest["entity_schemas"],
            &manifest["ui_contributions"],
        )?;
        let signature = r["signature"].as_str().ok_or_else(invalid)?;
        let signed_payload = json!({
            "publisher_id": pid,
            "manifest": r["manifest"],
        });
        verify_ed25519(pub_key, &signed_payload, signature).map_err(|()| invalid())?;
        releases.push(CatalogRelease {
            publisher_id: pid.to_owned(),
            publisher_name: pub_name.clone(),
            manifest: r["manifest"].clone(),
            signature: signature.to_owned(),
        });
    }
    Ok(releases)
}

pub fn load_catalog(config: &AppConfig) -> Result<Vec<CatalogRelease>, AppError> {
    match (
        config.marketplace_catalog_path.as_deref(),
        config.marketplace_operator_public_key.as_deref(),
    ) {
        (None, None) => Ok(Vec::new()),
        (Some(_), None) | (None, Some(_)) => Err(AppError::validation(
            "VALIDATION_ERROR",
            "marketplace trust configuration incomplete",
        )),
        (Some(path), Some(operator_public)) => {
            let invalid = || {
                AppError::validation(
                    "VALIDATION_ERROR",
                    "marketplace signature or catalog invalid",
                )
            };
            let file = File::open(path).map_err(|_| invalid())?;
            let mut data = Vec::new();
            file.take(2_000_001)
                .read_to_end(&mut data)
                .map_err(|_| invalid())?;
            if data.len() > 2_000_000 {
                return Err(invalid());
            }
            let envelope: Value = serde_json::from_slice(&data).map_err(|_| invalid())?;
            let env_obj = envelope.as_object().ok_or_else(invalid)?;
            if env_obj.len() != 2
                || !env_obj.contains_key("payload")
                || !env_obj.contains_key("signature")
            {
                return Err(invalid());
            }
            let sig = env_obj["signature"].as_str().ok_or_else(invalid)?;
            verify_ed25519(operator_public, &env_obj["payload"], sig).map_err(|()| invalid())?;
            validate_catalog(&env_obj["payload"]).map_err(|_| invalid())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ovg_storage::extensions::validation::validate_generic;
    use ovg_storage::extensions::ManifestContractRow;
    use uuid::Uuid;

    fn manifest(key: &str, version: &str) -> ManifestContractRow {
        ManifestContractRow {
            id: Uuid::new_v4(),
            extension_key: key.to_owned(),
            version: version.to_owned(),
            api_compatibility: "1.x".to_owned(),
            display_name: "Demo".to_owned(),
            description: String::new(),
            permissions: json!(["ext.demo.read"]),
            commands: json!([]),
            capabilities: json!([]),
            entity_schemas: json!({"thing":{"fields":{"name":{"type":"text"}}}}),
            ui_contributions: json!([]),
            enabled: true,
        }
    }

    #[test]
    fn generic_manifest_and_partner_contract_isolated() {
        let current = manifest("demo", "1.0.0");
        assert!(validate_generic(&current));

        let mut partner = manifest("partner_demo", "1.0.0");
        partner.permissions = json!([]);
        partner.entity_schemas = json!({});
        partner.capabilities = json!(["host_context", "map_view"]);
        partner.ui_contributions = json!([{
            "id":"partner_panel",
            "slot":"project_panel",
            "label":"Partner",
            "route_suffix":"/partner",
            "runtime":"sandboxed_iframe",
            "url":"https://partner.example.com/app",
            "sdk_version":"1.0"
        }]);
        assert!(validate_generic(&partner));
        partner.ui_contributions[0]["url"] = json!("https://openvietgis.cloud/app");
        assert!(!validate_generic(&partner));
    }
}
