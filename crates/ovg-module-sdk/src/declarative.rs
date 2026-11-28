//! Declarative contribution contract for SPEC 075. No script/HTML/network/FFI
//! execution. The trusted host resolves and dispatches only declared commands.
use ovg_core::{AppError, TenantScope};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;
fn invalid(s: &str) -> AppError {
    AppError::validation("INVALID_PLUGIN_CONTRIBUTION", s.to_owned())
}
fn id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Slot {
    Workspace,
    MapTool,
    Form,
    Dashboard,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Consent {
    Read,
    Mutation,
    Publish,
    Sensitive,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldType {
    Text,
    Number,
    Integer,
    Boolean,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Field {
    pub kind: FieldType,
    pub required: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Action {
    pub id: String,
    pub command_id: String,
    pub permission: String,
    pub consent: Consent,
    pub timeout_ms: u32,
    pub input: BTreeMap<String, Field>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Widget {
    Text {
        id: String,
        text: String,
    },
    Input {
        id: String,
        label: String,
        field: String,
    },
    Button {
        id: String,
        label: String,
        action: String,
    },
    Group {
        id: String,
        children: Vec<Widget>,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub slot: Slot,
    pub widgets: Vec<Widget>,
    pub actions: Vec<Action>,
}
#[derive(Debug, Clone)]
pub struct CommandPolicy {
    pub permission: String,
    pub consent: Consent,
}
impl Document {
    /// Validation only; does not authorize an invocation. Input bytes are bounded
    /// before deserialization. Host command registry/policy is independently trusted.
    pub fn parse(bytes: &[u8]) -> Result<Self, AppError> {
        if bytes.len() > 256 * 1024 {
            return Err(invalid("document limit"));
        }
        serde_json::from_slice(bytes).map_err(|_| invalid("invalid document"))
    }
    pub fn validate(
        &self,
        permissions: &BTreeSet<String>,
        commands: &BTreeMap<String, CommandPolicy>,
    ) -> Result<(), AppError> {
        if self.actions.len() > 32 {
            return Err(invalid("action limit"));
        }
        let mut action_ids = BTreeSet::new();
        let mut fields = BTreeSet::new();
        for a in &self.actions {
            let policy = commands
                .get(&a.command_id)
                .ok_or_else(|| invalid("unknown host command"))?;
            if !id(&a.id)
                || !id(&a.command_id)
                || !action_ids.insert(a.id.as_str())
                || a.permission != policy.permission
                || a.consent != policy.consent
                || !permissions.contains(&a.permission)
                || !(1..=30000).contains(&a.timeout_ms)
                || a.input.len() > 64
                || a.input.keys().any(|s| !id(s))
            {
                return Err(invalid("action policy mismatch"));
            }
            fields.extend(a.input.keys().map(String::as_str));
        }
        fn walk(
            nodes: &[Widget],
            depth: usize,
            count: &mut usize,
            ids: &mut BTreeSet<String>,
            actions: &BTreeSet<&str>,
            fields: &BTreeSet<&str>,
        ) -> Result<(), AppError> {
            if depth > 16 {
                return Err(invalid("widget depth"));
            }
            for n in nodes {
                *count += 1;
                if *count > 500 {
                    return Err(invalid("widget count"));
                }
                let key = match n {
                    Widget::Text { id, text } => {
                        if text.chars().count() > 2000 {
                            return Err(invalid("text limit"));
                        }
                        id
                    }
                    Widget::Input { id, label, field } => {
                        if label.chars().count() > 200 || !fields.contains(field.as_str()) {
                            return Err(invalid("invalid input"));
                        }
                        id
                    }
                    Widget::Button { id, label, action } => {
                        if label.chars().count() > 200 || !actions.contains(action.as_str()) {
                            return Err(invalid("unknown action"));
                        }
                        id
                    }
                    Widget::Group { id, children } => {
                        walk(children, depth + 1, count, ids, actions, fields)?;
                        id
                    }
                };
                if !id(key) || !ids.insert(key.clone()) {
                    return Err(invalid("duplicate or invalid widget id"));
                }
            }
            Ok(())
        }
        walk(
            &self.widgets,
            1,
            &mut 0,
            &mut BTreeSet::new(),
            &action_ids,
            &fields,
        )
    }
}
/// Trusted active installation pin from persistence. Never deserialize this from
/// an untrusted invocation. Resource grants and active ACL must be rechecked by
/// the caller immediately before invoking actual Core operations.
pub struct ActiveGrant<'a> {
    pub organization_id: Uuid,
    pub project_id: Uuid,
    pub actor_id: Uuid,
    pub release_digest: &'a str,
    pub active: bool,
    pub permissions: &'a BTreeSet<String>,
    pub release_permissions: &'a BTreeSet<String>,
}
/// Construct only from the authenticated host and server-side confirmation record.
/// Never copy a plugin-supplied confirmation boolean.
pub struct Invocation<'a> {
    pub release_digest: &'a str,
    pub action_id: &'a str,
    pub input: &'a Value,
    pub human_confirmed: bool,
}
/// Preflight for a typed invocation; never executes the command or grants data
/// permission. A server-side broker must subsequently resolve resource scope.
pub fn authorize<'a>(
    doc: &'a Document,
    scope: &TenantScope,
    grant: &ActiveGrant<'_>,
    request: &Invocation<'_>,
    policies: &BTreeMap<String, CommandPolicy>,
) -> Result<&'a Action, AppError> {
    scope.validate()?;
    if !grant.active
        || scope.organization_id != grant.organization_id
        || scope.project_id != Some(grant.project_id)
        || scope.actor.actor_id != grant.actor_id
        || grant.release_digest != request.release_digest
    {
        return Err(AppError::forbidden("inactive or mismatched plugin scope"));
    }
    doc.validate(grant.release_permissions, policies)?;
    let action = doc
        .actions
        .iter()
        .find(|a| a.id == request.action_id)
        .ok_or_else(|| invalid("unknown action"))?;
    if !grant.permissions.contains(&action.permission) {
        return Err(AppError::forbidden("missing action permission"));
    }
    if action.consent != Consent::Read
        && (!request.human_confirmed || scope.actor.actor_type != ovg_core::ActorType::Human)
    {
        return Err(AppError::forbidden("explicit host confirmation required"));
    }
    let encoded = serde_json::to_vec(request.input).map_err(|_| invalid("invalid input"))?;
    if encoded.len() > 256 * 1024 {
        return Err(invalid("input limit"));
    }
    let values = request
        .input
        .as_object()
        .ok_or_else(|| invalid("input must be an object"))?;
    for (key, value) in values {
        let field = action
            .input
            .get(key)
            .ok_or_else(|| invalid("unknown input field"))?;
        let ok = match field.kind {
            FieldType::Text => value.as_str().is_some_and(|s| s.chars().count() <= 4096),
            FieldType::Number => value.is_number(),
            FieldType::Integer => value.as_i64().is_some(),
            FieldType::Boolean => value.is_boolean(),
        };
        if !ok {
            return Err(invalid("invalid field type"));
        }
    }
    if action
        .input
        .iter()
        .any(|(key, f)| f.required && !values.contains_key(key))
    {
        return Err(invalid("missing required field"));
    }
    Ok(action)
}
#[cfg(test)]
mod tests {
    use super::*;
    use ovg_core::{ActorContext, ActorType};
    use serde_json::json;
    #[test]
    fn schema_permission_scope_and_consent() {
        let doc=Document::parse(br#"{"slot":"form","widgets":[{"kind":"button","id":"save","label":"Save","action":"save"}],"actions":[{"id":"save","command_id":"save","permission":"ext.demo.write","consent":"mutation","timeout_ms":1000,"input":{"name":{"kind":"text","required":true}}}]}"#).unwrap();
        let permissions = BTreeSet::from(["ext.demo.write".into()]);
        let policies = BTreeMap::from([(
            "save".into(),
            CommandPolicy {
                permission: "ext.demo.write".into(),
                consent: Consent::Mutation,
            },
        )]);
        let scope = TenantScope {
            organization_id: Uuid::new_v4(),
            workspace_id: None,
            project_id: Some(Uuid::new_v4()),
            actor: ActorContext {
                actor_type: ActorType::Human,
                actor_id: Uuid::new_v4(),
                credential_id: None,
                email: None,
            },
            permissions: vec![],
        };
        let mut grant = ActiveGrant {
            organization_id: scope.organization_id,
            project_id: scope.project_id.unwrap(),
            actor_id: scope.actor.actor_id,
            release_digest: "pin",
            active: true,
            permissions: &permissions,
            release_permissions: &permissions,
        };
        let payload = json!({"name":"test"});
        let mut req = Invocation {
            release_digest: "pin",
            action_id: "save",
            input: &payload,
            human_confirmed: false,
        };
        assert!(authorize(&doc, &scope, &grant, &req, &policies).is_err());
        req.human_confirmed = true;
        assert!(authorize(&doc, &scope, &grant, &req, &policies).is_ok());
        let bad = json!({"name":42});
        req.input = &bad;
        assert!(authorize(&doc, &scope, &grant, &req, &policies).is_err());
        req.input = &payload;
        grant.project_id = Uuid::new_v4();
        assert!(authorize(&doc, &scope, &grant, &req, &policies).is_err());
        assert!(Document::parse(
            br#"{"slot":"form","widgets":[{"kind":"script","id":"x"}],"actions":[]}"#
        )
        .is_err());
    }
}
