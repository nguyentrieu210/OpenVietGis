//! E14 manifest, partner contribution, and record schema contract validation.

use std::collections::HashSet;

use ovg_core::AppError;
use serde_json::{json, Map, Value};

use super::{canonical_bytes, ManifestContractRow};

#[must_use]
pub fn valid_extension_key(value: &str) -> bool {
    let bytes = value.as_bytes();
    (2..=100).contains(&bytes.len())
        && bytes[0].is_ascii_lowercase()
        && bytes
            .iter()
            .all(|byte| matches!(*byte, b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-'))
}

#[must_use]
pub fn valid_identifier(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 64
        && bytes[0].is_ascii_alphabetic()
        && bytes[1..]
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
}

#[must_use]
pub fn semver(value: &str) -> Option<(u64, u64, u64)> {
    if value.len() > 32 {
        return None;
    }
    let mut parts = value.split('.');
    let parse = |part: &str| {
        (!part.is_empty()
            && part.bytes().all(|byte| byte.is_ascii_digit())
            && (part.len() == 1 || !part.starts_with('0')))
        .then(|| part.parse::<u64>().ok())
        .flatten()
    };
    let major = parse(parts.next()?)?;
    let minor = parse(parts.next()?)?;
    let patch = parse(parts.next()?)?;
    parts.next().is_none().then_some((major, minor, patch))
}

fn string_array(value: &Value) -> Option<Vec<&str>> {
    value
        .as_array()?
        .iter()
        .map(Value::as_str)
        .collect::<Option<Vec<_>>>()
}

fn portable(value: &Value) -> bool {
    match value {
        Value::Null | Value::Bool(_) | Value::String(_) => true,
        Value::Number(number) => {
            const LIMIT: i128 = 9_007_199_254_740_991;
            if let Some(value) = number.as_i64() {
                i128::from(value).abs() <= LIMIT
            } else if let Some(value) = number.as_u64() {
                i128::from(value) <= LIMIT
            } else {
                number
                    .as_f64()
                    .is_some_and(|value| value.is_finite() && value.abs() <= LIMIT as f64)
            }
        }
        Value::Array(items) => items.iter().all(portable),
        Value::Object(object) => object.values().all(portable),
    }
}

#[must_use]
pub fn valid_partner_url(url: &str) -> bool {
    if url.len() > 2048
        || url
            .bytes()
            .any(|byte| byte <= 0x20 || byte >= 0x7f || byte == 0x5c)
        || url.contains('?')
        || url.contains('#')
    {
        return false;
    }
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let Some((host, path)) = rest.split_once('/') else {
        return false;
    };
    if host.is_empty()
        || host.contains(':')
        || host.contains('@')
        || host == "openvietgis.cloud"
        || host.ends_with(".openvietgis.cloud")
        || host.ends_with(".local")
        || host.ends_with(".localhost")
        || host.ends_with(".internal")
        || host.ends_with(".test")
        || !path.is_empty() && !url.contains('/')
    {
        return false;
    }
    let labels = host.split('.').collect::<Vec<_>>();
    labels.len() >= 2
        && labels.last().is_some_and(|label| {
            (2..=63).contains(&label.len()) && label.bytes().all(|byte| byte.is_ascii_lowercase())
        })
        && labels.iter().all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        })
}

fn validate_partner(
    extension_key: &str,
    permissions: &Value,
    commands: &Value,
    capabilities: &Value,
    entity_schemas: &Value,
    ui_contributions: &Value,
) -> bool {
    const CAPS: [&str; 7] = [
        "host_context",
        "host_notice",
        "map_view",
        "map_navigate",
        "map_selection",
        "map_overlay",
        "tool_register",
    ];
    let permissions = string_array(permissions);
    let commands = commands.as_array();
    let schemas = entity_schemas.as_object();
    let capabilities = string_array(capabilities);
    let ui = ui_contributions.as_array();
    let Some(capabilities) = capabilities else {
        return false;
    };
    let unique: HashSet<_> = capabilities.iter().copied().collect();
    if !extension_key.starts_with("partner_")
        || extension_key.len() <= "partner_".len()
        || permissions.is_none_or(|values| !values.is_empty())
        || commands.is_none_or(|values| !values.is_empty())
        || schemas.is_none_or(|values| !values.is_empty())
        || capabilities.is_empty()
        || unique.len() != capabilities.len()
        || capabilities.iter().any(|cap| !CAPS.contains(cap))
        || ui.is_none_or(|values| values.len() != 1)
    {
        return false;
    }
    let Some(object) = ui.and_then(|values| values[0].as_object()) else {
        return false;
    };
    let expected = [
        "id",
        "slot",
        "label",
        "route_suffix",
        "runtime",
        "url",
        "sdk_version",
    ];
    if object.len() != expected.len() || expected.iter().any(|key| !object.contains_key(*key)) {
        return false;
    }
    let label_ok = object
        .get("label")
        .and_then(Value::as_str)
        .is_some_and(|label| !label.trim().is_empty() && label.len() <= 200);
    let fixed = object.get("id").and_then(Value::as_str) == Some("partner_panel")
        && object.get("slot").and_then(Value::as_str) == Some("project_panel")
        && object.get("runtime").and_then(Value::as_str) == Some("sandboxed_iframe")
        && object.get("sdk_version").and_then(Value::as_str) == Some("1.0")
        && object.get("route_suffix").and_then(Value::as_str) == Some("/partner");
    fixed
        && label_ok
        && object
            .get("url")
            .and_then(Value::as_str)
            .is_some_and(valid_partner_url)
}

#[allow(clippy::too_many_arguments)]
pub fn validate_manifest_fields(
    extension_key: &str,
    version: &str,
    api_compatibility: &str,
    permissions: &Value,
    commands: &Value,
    capabilities: &Value,
    entity_schemas: &Value,
    ui_contributions: &Value,
) -> Result<(), AppError> {
    if !valid_extension_key(extension_key) {
        return Err(AppError::validation(
            "INVALID_EXTENSION_KEY",
            "extension_key must match ^[a-z][a-z0-9_-]{1,99}$",
        ));
    }
    if semver(version).is_none() {
        return Err(AppError::validation(
            "INVALID_VERSION",
            "version must be semver X.Y.Z",
        ));
    }
    if api_compatibility != "1.x" {
        return Err(AppError::validation(
            "INCOMPATIBLE_EXTENSION_API",
            format!("unsupported api_compatibility '{api_compatibility}'; expected '1.x'"),
        ));
    }
    if extension_key.starts_with("partner_") {
        if !validate_partner(
            extension_key,
            permissions,
            commands,
            capabilities,
            entity_schemas,
            ui_contributions,
        ) {
            return Err(AppError::validation(
                "VALIDATION_ERROR",
                "invalid isolated partner plugin manifest",
            ));
        }
        return Ok(());
    }
    let Some(ui) = ui_contributions.as_array() else {
        return Err(AppError::validation(
            "INVALID_MANIFEST",
            "invalid UI contributions",
        ));
    };
    if ui.iter().any(|item| {
        item.as_object()
            .is_some_and(|obj| obj.contains_key("runtime"))
    }) {
        return Err(AppError::validation(
            "VALIDATION_ERROR",
            "external runtime requires partner_ namespace",
        ));
    }
    let Some(perms) = string_array(permissions) else {
        return Err(AppError::validation(
            "INVALID_PERMISSION_NAMESPACE",
            "invalid permission identifiers",
        ));
    };
    let prefix = format!("ext.{extension_key}.");
    let perm_set: HashSet<_> = perms.iter().copied().collect();
    if perms.len() > 64 || perm_set.len() != perms.len() {
        return Err(AppError::validation(
            "VALIDATION_ERROR",
            "duplicate or too many permission identifiers",
        ));
    }
    for perm in &perms {
        if !perm.strip_prefix(&prefix).is_some_and(valid_identifier) {
            return Err(AppError::validation(
                "INVALID_PERMISSION_NAMESPACE",
                format!("permission '{perm}' must be namespaced under '{prefix}'"),
            ));
        }
    }
    let Some(schemas) = entity_schemas.as_object() else {
        return Err(AppError::validation(
            "EMPTY_SCHEMAS",
            "manifest must declare one to 100 entity schemas",
        ));
    };
    if schemas.is_empty() || schemas.len() > 100 {
        return Err(AppError::validation(
            "EMPTY_SCHEMAS",
            "manifest must declare one to 100 entity schemas",
        ));
    }
    for (entity, schema) in schemas {
        let Some(schema) = schema.as_object() else {
            return Err(AppError::validation(
                "VALIDATION_ERROR",
                "invalid entity schema",
            ));
        };
        if !valid_identifier(entity) || schema.len() != 1 || !schema.contains_key("fields") {
            return Err(AppError::validation(
                "VALIDATION_ERROR",
                "invalid entity schema",
            ));
        }
        let Some(fields) = schema["fields"].as_object() else {
            return Err(AppError::validation(
                "VALIDATION_ERROR",
                "entity fields must be a bounded mapping",
            ));
        };
        if fields.is_empty() || fields.len() > 100 {
            return Err(AppError::validation(
                "VALIDATION_ERROR",
                "entity fields must be a bounded mapping",
            ));
        }
        for (name, spec) in fields {
            let Some(spec) = spec.as_object() else {
                return Err(AppError::validation(
                    "VALIDATION_ERROR",
                    "invalid or unsupported field declaration",
                ));
            };
            if !valid_identifier(name)
                || spec.keys().any(|key| key != "type" && key != "required")
                || !spec
                    .get("type")
                    .and_then(Value::as_str)
                    .is_some_and(|kind| matches!(kind, "text" | "integer" | "number" | "boolean"))
                || spec
                    .get("required")
                    .is_some_and(|required| !required.is_boolean())
            {
                return Err(AppError::validation(
                    "VALIDATION_ERROR",
                    "invalid or unsupported field declaration",
                ));
            }
        }
    }
    let Some(cmds) = commands.as_array() else {
        return Err(AppError::validation(
            "VALIDATION_ERROR",
            "invalid command declarations",
        ));
    };
    if cmds.len() > 64 {
        return Err(AppError::validation(
            "VALIDATION_ERROR",
            "too many command declarations",
        ));
    }
    let mut cmd_ids = HashSet::new();
    for cmd in cmds {
        let Some(cmd) = cmd.as_object() else {
            return Err(AppError::validation(
                "VALIDATION_ERROR",
                "invalid or duplicate command declaration",
            ));
        };
        let id = cmd.get("id").and_then(Value::as_str);
        let label = cmd.get("label").and_then(Value::as_str);
        let required = cmd.get("required_permission").and_then(Value::as_str);
        if cmd.len() != 3
            || !cmd.contains_key("id")
            || !cmd.contains_key("label")
            || !cmd.contains_key("required_permission")
            || id.is_none_or(|id| !valid_identifier(id) || !cmd_ids.insert(id))
            || label.is_none_or(|label| label.trim().is_empty() || label.chars().count() > 200)
            || required.is_none_or(|required| !perm_set.contains(required))
        {
            return Err(AppError::validation(
                "VALIDATION_ERROR",
                "invalid or duplicate command declaration",
            ));
        }
    }
    let Some(caps) = string_array(capabilities) else {
        return Err(AppError::validation(
            "VALIDATION_ERROR",
            "invalid or duplicate capability declaration",
        ));
    };
    let cap_set: HashSet<_> = caps.iter().copied().collect();
    if caps.len() > 64
        || cap_set.len() != caps.len()
        || caps.iter().any(|cap| !valid_identifier(cap))
    {
        return Err(AppError::validation(
            "VALIDATION_ERROR",
            "invalid or duplicate capability declaration",
        ));
    }
    if ui.len() > 64 {
        return Err(AppError::validation(
            "INVALID_MANIFEST",
            "too many UI contributions",
        ));
    }
    let mut ui_ids = HashSet::new();
    for contribution in ui {
        let Some(contribution) = contribution.as_object() else {
            return Err(AppError::validation(
                "INVALID_MANIFEST",
                "UI contribution must be an object",
            ));
        };
        let id = contribution.get("id").and_then(Value::as_str);
        let label = contribution.get("label").and_then(Value::as_str);
        let route = contribution.get("route_suffix").and_then(Value::as_str);
        let slot = contribution.get("slot").and_then(Value::as_str);
        if id.is_none_or(|id| !valid_identifier(id) || !ui_ids.insert(id))
            || !slot.is_some_and(|slot| {
                matches!(slot, "project_tool" | "project_panel" | "project_route")
            })
            || label.is_none_or(|label| label.trim().is_empty())
            || route.is_none_or(|route| !route.starts_with('/') || route.contains(".."))
        {
            return Err(AppError::validation(
                "INVALID_MANIFEST",
                "invalid UI contribution",
            ));
        }
        if let Some(required) = contribution.get("required_permission") {
            if !required
                .as_str()
                .is_some_and(|required| perm_set.contains(required))
            {
                return Err(AppError::validation(
                    "INVALID_MANIFEST",
                    format!(
                        "UI contribution '{}' references undeclared permission",
                        id.unwrap_or_default()
                    ),
                ));
            }
        }
    }
    let bounded = json!({
        "permissions": permissions,
        "commands": commands,
        "capabilities": capabilities,
        "schemas": entity_schemas,
        "ui_contributions": ui_contributions,
    });
    if !portable(&bounded) {
        return Err(AppError::validation(
            "VALIDATION_ERROR",
            "invalid portable JSON",
        ));
    }
    canonical_bytes(&bounded, 128 * 1024)?;
    Ok(())
}

#[must_use]
pub fn validate_generic(manifest: &ManifestContractRow) -> bool {
    validate_manifest_fields(
        &manifest.extension_key,
        &manifest.version,
        &manifest.api_compatibility,
        &manifest.permissions,
        &manifest.commands,
        &manifest.capabilities,
        &manifest.entity_schemas,
        &manifest.ui_contributions,
    )
    .is_ok()
}

pub fn validate_record_against_schema(
    entity_name: &str,
    payload: &Value,
    entity_schemas: &Value,
) -> Result<Value, AppError> {
    let payload_map = payload
        .as_object()
        .ok_or_else(|| AppError::validation("SCHEMA_MISMATCH", "payload must be an object"))?;
    let schema = entity_schemas
        .as_object()
        .and_then(|s| s.get(entity_name))
        .and_then(Value::as_object)
        .ok_or_else(|| {
            AppError::validation(
                "UNKNOWN_ENTITY",
                format!("unknown entity '{entity_name}' for this extension"),
            )
        })?;
    let fields = schema
        .get("fields")
        .and_then(Value::as_object)
        .ok_or_else(|| AppError::validation("SCHEMA_MISMATCH", "invalid entity schema"))?;
    let mut unknown: Vec<&str> = payload_map
        .keys()
        .filter(|k| !fields.contains_key(*k))
        .map(String::as_str)
        .collect();
    unknown.sort_unstable();
    if !unknown.is_empty() {
        return Err(AppError::validation(
            "SCHEMA_MISMATCH",
            format!("unknown fields in extension record: {}", unknown.join(", ")),
        ));
    }
    let mut cleaned = Map::new();
    for (fname, spec) in fields {
        let spec_obj = spec
            .as_object()
            .ok_or_else(|| AppError::validation("SCHEMA_MISMATCH", "invalid field spec"))?;
        let req = spec_obj
            .get("required")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let ftype = spec_obj
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("text");
        match payload_map.get(fname) {
            None | Some(Value::Null) => {
                if req {
                    return Err(AppError::validation(
                        "SCHEMA_MISMATCH",
                        format!("missing required field '{fname}'"),
                    ));
                }
                cleaned.insert(fname.clone(), Value::Null);
            }
            Some(val) => {
                const LIMIT: i128 = 9_007_199_254_740_991;
                match ftype {
                    "integer" => {
                        let ok = match val.as_number() {
                            Some(n) if n.as_i64().is_some() => {
                                i128::from(n.as_i64().unwrap()).abs() <= LIMIT
                            }
                            Some(n) if n.as_u64().is_some() => {
                                i128::from(n.as_u64().unwrap()) <= LIMIT
                            }
                            _ => false,
                        };
                        if !ok {
                            return Err(AppError::validation(
                                "SCHEMA_MISMATCH",
                                format!("field '{fname}' expects integer"),
                            ));
                        }
                    }
                    "number" => {
                        let ok = match val.as_number() {
                            Some(n) if n.as_i64().is_some() => {
                                i128::from(n.as_i64().unwrap()).abs() <= LIMIT
                            }
                            Some(n) if n.as_u64().is_some() => {
                                i128::from(n.as_u64().unwrap()) <= LIMIT
                            }
                            Some(n) if n.as_f64().is_some() => n.as_f64().unwrap().is_finite(),
                            _ => false,
                        };
                        if !ok {
                            return Err(AppError::validation(
                                "SCHEMA_MISMATCH",
                                format!("field '{fname}' expects number"),
                            ));
                        }
                    }
                    "boolean" => {
                        if !val.is_boolean() {
                            return Err(AppError::validation(
                                "SCHEMA_MISMATCH",
                                format!("field '{fname}' expects boolean"),
                            ));
                        }
                    }
                    "text" => {
                        let ok = val
                            .as_str()
                            .is_some_and(|s| s.chars().count() <= 4096 && !s.contains('\0'));
                        if !ok {
                            return Err(AppError::validation(
                                "SCHEMA_MISMATCH",
                                format!("field '{fname}' expects text"),
                            ));
                        }
                    }
                    _ => {
                        return Err(AppError::validation(
                            "SCHEMA_MISMATCH",
                            format!("unsupported field type for '{fname}'"),
                        ));
                    }
                }
                cleaned.insert(fname.clone(), val.clone());
            }
        }
    }
    let out = Value::Object(cleaned);
    canonical_bytes(&out, 64 * 1024)?;
    Ok(out)
}

fn bad_label_char(c: char) -> bool {
    let code = c as u32;
    code <= 0x1f
        || code == 0x7f
        || (0x202a..=0x202e).contains(&code)
        || (0x2066..=0x2069).contains(&code)
}

fn clean_label(value: &Value, limit: usize) -> Result<String, AppError> {
    let s = value
        .as_str()
        .ok_or_else(|| AppError::validation("VALIDATION_ERROR", "invalid contribution label"))?;
    let trimmed = s.trim();
    if trimmed.is_empty() || s.chars().count() > limit || s.chars().any(bad_label_char) {
        return Err(AppError::validation(
            "VALIDATION_ERROR",
            "invalid contribution label",
        ));
    }
    Ok(trimmed.to_owned())
}

fn valid_tool_id(id: &str) -> bool {
    let bytes = id.as_bytes();
    (1..=32).contains(&bytes.len())
        && bytes[0].is_ascii_lowercase()
        && bytes[1..]
            .iter()
            .all(|b| matches!(*b, b'a'..=b'z' | b'0'..=b'9' | b'_'))
}

fn geometry_depth(kind: &str) -> Option<usize> {
    match kind {
        "Point" => Some(0),
        "MultiPoint" | "LineString" => Some(1),
        "MultiLineString" | "Polygon" => Some(2),
        "MultiPolygon" => Some(3),
        _ => None,
    }
}

fn pos_eq(a: &Value, b: &Value) -> bool {
    match (a.as_array(), b.as_array()) {
        (Some(a), Some(b)) if a.len() == 2 && b.len() == 2 => {
            a[0].as_f64() == b[0].as_f64() && a[1].as_f64() == b[1].as_f64()
        }
        _ => false,
    }
}

fn ring_valid(v: &Value) -> bool {
    v.as_array()
        .is_some_and(|arr| arr.len() >= 4 && pos_eq(&arr[0], &arr[arr.len() - 1]))
}

fn polygon_valid(v: &Value) -> bool {
    v.as_array()
        .is_some_and(|rings| rings.iter().all(ring_valid))
}

/// Port of `contributions.validate_contributions`.
pub fn validate_contributions(value: &Value, capabilities: &[String]) -> Result<Value, AppError> {
    let invalid = || AppError::validation("VALIDATION_ERROR", "invalid partner contributions");
    let obj = value.as_object().ok_or_else(invalid)?;
    if obj.len() != 2 || !obj.contains_key("overlay") || !obj.contains_key("tools") {
        return Err(invalid());
    }
    let tools = obj["tools"].as_array().ok_or_else(invalid)?;
    if tools.len() > 4 || (!tools.is_empty() && !capabilities.iter().any(|c| c == "tool_register"))
    {
        return Err(invalid());
    }
    let mut clean_tools = Vec::with_capacity(tools.len());
    let mut seen_ids = HashSet::new();
    for tool in tools {
        let t = tool.as_object().ok_or_else(invalid)?;
        if t.len() != 2 || !t.contains_key("id") || !t.contains_key("label") {
            return Err(invalid());
        }
        let id = t["id"].as_str().ok_or_else(invalid)?;
        if !valid_tool_id(id) || !seen_ids.insert(id.to_owned()) {
            return Err(invalid());
        }
        let label = clean_label(&t["label"], 40)?;
        clean_tools.push(json!({ "id": id, "label": label }));
    }

    let overlay = &obj["overlay"];
    if overlay.is_null() {
        return Ok(json!({ "overlay": null, "tools": clean_tools }));
    }
    if !capabilities.iter().any(|c| c == "map_overlay") {
        return Err(invalid());
    }
    let overlay_obj = overlay.as_object().ok_or_else(invalid)?;
    if overlay_obj.len() != 2
        || !overlay_obj.contains_key("title")
        || !overlay_obj.contains_key("geojson")
    {
        return Err(invalid());
    }
    let title = clean_label(&overlay_obj["title"], 60)?;
    let fc = overlay_obj["geojson"].as_object().ok_or_else(invalid)?;
    if fc.len() != 2
        || fc.get("type").and_then(Value::as_str) != Some("FeatureCollection")
        || !fc.contains_key("features")
    {
        return Err(invalid());
    }
    let raw_features = fc["features"].as_array().ok_or_else(invalid)?;
    if !(1..=100).contains(&raw_features.len()) {
        return Err(invalid());
    }

    let mut positions = 0usize;
    let mut nodes = 0usize;
    fn walk(
        val: &Value,
        depth: usize,
        positions: &mut usize,
        nodes: &mut usize,
    ) -> Result<Value, AppError> {
        let invalid = || AppError::validation("VALIDATION_ERROR", "invalid partner contributions");
        *nodes += 1;
        let arr = val.as_array().ok_or_else(invalid)?;
        if *nodes > 8192 || !(1..=2048).contains(&arr.len()) {
            return Err(invalid());
        }
        if depth == 0 {
            *positions += 1;
            if *positions > 4096 || arr.len() != 2 {
                return Err(invalid());
            }
            let x = arr[0].as_f64().ok_or_else(invalid)?;
            let y = arr[1].as_f64().ok_or_else(invalid)?;
            if !x.is_finite() || !y.is_finite() || x.abs() > 180.0 || y.abs() > 90.0 {
                return Err(invalid());
            }
            return Ok(val.clone());
        }
        let mut out = Vec::with_capacity(arr.len());
        for item in arr {
            out.push(walk(item, depth - 1, positions, nodes)?);
        }
        Ok(Value::Array(out))
    }

    let mut features = Vec::with_capacity(raw_features.len());
    for feature in raw_features {
        let f = feature.as_object().ok_or_else(invalid)?;
        if f.len() != 3
            || f.get("type").and_then(Value::as_str) != Some("Feature")
            || !f
                .get("properties")
                .and_then(Value::as_object)
                .is_some_and(Map::is_empty)
            || !f.contains_key("geometry")
        {
            return Err(invalid());
        }
        let g = f["geometry"].as_object().ok_or_else(invalid)?;
        if g.len() != 2 || !g.contains_key("type") || !g.contains_key("coordinates") {
            return Err(invalid());
        }
        let kind = g["type"].as_str().ok_or_else(invalid)?;
        let depth = geometry_depth(kind).ok_or_else(invalid)?;
        let coords = walk(&g["coordinates"], depth, &mut positions, &mut nodes)?;
        let geom_ok = match kind {
            "LineString" => coords.as_array().is_some_and(|c| c.len() >= 2),
            "MultiLineString" => coords.as_array().is_some_and(|lines| {
                lines
                    .iter()
                    .all(|l| l.as_array().is_some_and(|c| c.len() >= 2))
            }),
            "Polygon" => polygon_valid(&coords),
            "MultiPolygon" => coords
                .as_array()
                .is_some_and(|polys| polys.iter().all(polygon_valid)),
            _ => true,
        };
        if !geom_ok {
            return Err(invalid());
        }
        features.push(json!({
            "type": "Feature",
            "geometry": { "type": kind, "coordinates": coords },
            "properties": {},
        }));
    }

    Ok(json!({
        "overlay": {
            "title": title,
            "geojson": { "type": "FeatureCollection", "features": features }
        },
        "tools": clean_tools,
    }))
}
