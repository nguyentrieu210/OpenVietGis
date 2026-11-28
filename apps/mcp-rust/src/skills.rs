//! Built-in GIS skill parsing adapted from RustGIS (Apache-2.0).
//! Upstream: https://github.com/opengeos/RustGIS/blob/aea84131974b57ff6359666dbe79f88054a64bf1/crates/assistant/src/skills.rs
//! Changes: built-in only, bounded input, MCP prompt interface, no filesystem or plugin loading.
use serde_json::{json, Value};
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source { BuiltIn }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Skill { pub name: String, pub description: String, pub body: String, pub source: Source }
pub fn parse(text: &str) -> Result<Skill, String> {
    if text.len() > 64 * 1024 { return Err("skill too large".into()); }
    let text = text.trim_start_matches('\u{feff}');
    let rest = text.strip_prefix("---").ok_or("a skill starts with front matter between --- lines")?;
    // The opening line is `---` alone; the front matter ends at a line that is `---` alone.
    let mut parts = rest.split_inclusive('\n');
    if !parts.next().unwrap_or("").trim().is_empty() {
        return Err("a skill starts with front matter between --- lines".into());
    }
    let mut lines: Vec<&str> = Vec::new();
    let mut body = None;
    let mut used = rest.find('\n').map_or(rest.len(), |i| i + 1);
    for l in parts {
        used += l.len();
        if l.trim_end() == "---" {
            body = rest.get(used..);
            break;
        }
        lines.push(l.trim_end_matches(['\n', '\r']));
    }
    let body = body.ok_or("the front matter has no closing --- line")?;
    let field = |key: &str| {
        // A key at the top level only: not one nested under another, nor a block's line.
        let i = lines.iter().position(|l| !l.starts_with([' ', '\t']) && l.split_once(':').is_some_and(|(k, _)| k.trim() == key))?;
        let value = lines.get(i)?.split_once(':')?.1.trim();
        // A folded (`>`) or literal (`|`) block: the indented lines after it, joined.
        if value.starts_with(['>', '|']) {
            let block: Vec<&str> =
                lines.iter().skip(i + 1).take_while(|l| l.starts_with([' ', '\t']) || l.trim().is_empty()).map(|l| l.trim()).collect();
            return Some(block.join(" ").split_whitespace().collect::<Vec<_>>().join(" "));
        }
        Some(value.trim_matches(|c| c == '"' || c == '\'').to_string())
    };
    let name = field("name").filter(|n| !n.is_empty()).ok_or("the front matter has no name")?;
    if !valid_name(&name) {
        return Err(format!("'{name}': a skill's name is lower case letters, digits and hyphens"));
    }
    let description = field("description").filter(|d| !d.is_empty()).ok_or("the front matter has no description")?;
    let body = body.trim().to_string();
    Ok(Skill {
        name,
        description: description.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(1024).collect(),
        body,
        source: Source::BuiltIn,
    })
}

/// A skill's own name: lower case letters, digits and hyphens, at most 64.
fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 64 && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}


pub fn built_in() -> Vec<Skill> {
    [include_str!("../skills/thematic-map/SKILL.md"),
     include_str!("../skills/explore-data/SKILL.md"),
     include_str!("../skills/print-layout/SKILL.md"),
     include_str!("../skills/geoprocessing/SKILL.md"),
     include_str!("../skills/add-data/SKILL.md"),
     include_str!("../skills/share-web-map/SKILL.md")]
    .iter().filter_map(|text| parse(text).ok()).collect()
}
/// Only compiled, public instructions are returned; no tenant data or execution.
pub fn list() -> Value {
    json!({"prompts": built_in().iter().map(|s| json!({"name":s.name,"description":s.description,"arguments":[]})).collect::<Vec<_>>()})
}
pub fn get(params: &Value) -> Option<Value> {
    let obj = params.as_object()?;
    if obj.keys().any(|k| k != "name" && k != "arguments") { return None; }
    if obj.get("arguments").is_some_and(|a| !a.as_object().is_some_and(|o| o.is_empty())) { return None; }
    let name = obj.get("name")?.as_str()?;
    let skill = built_in().into_iter().find(|s| s.name == name)?;
    Some(json!({"description":skill.description,"messages":[{"role":"user","content":{"type":"text","text":skill.body}}]}))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn six_distinct_prompts() {
        let s = built_in(); assert_eq!(s.len(), 6);
        let names: std::collections::HashSet<_> = s.iter().map(|x| &x.name).collect(); assert_eq!(names.len(),6);
        for skill in s { assert!(!skill.description.is_empty()); assert!(skill.body.contains("workspace")); assert!(get(&json!({"name":skill.name})).is_some()); }
    }
    #[test] fn reject_unknown_or_hostile_input() {
        assert!(get(&json!({"name":"../../etc/passwd"})).is_none());
        assert!(get(&json!({"name":"explore-data","arguments":{"injection":"x"}})).is_none());
        assert!(parse(&"x".repeat(65537)).is_err());
        assert!(parse("---\nname: ../../bad\ndescription: x\n---\ntext").is_err());
    }
}
