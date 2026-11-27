//! Atomic map saves with compatibility fingerprints and immutable revision evidence.
use super::*;
use serde::Deserialize;
use serde_json::Map;

const MAX_REVISION: i64 = 9_007_199_254_740_991;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapSaveIntent {
    operation_id: Uuid,
    expected_revision: i64,
    layers: Vec<MapLayer>,
    basemap: String,
    default_viewpoint: ViewpointIntent,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct MapLayer {
    layer_id: Uuid,
    visible: bool,
    style: Style,
    #[serde(skip_serializing_if = "Option::is_none")]
    group: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    display: Option<Map<String, Value>>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Style {
    color: String,
    opacity: f64,
    width: f64,
    radius: f64,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields, tag = "mode", rename_all = "lowercase")]
enum ViewpointIntent {
    Preserve,
    Clear,
    Set { value: Viewpoint },
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Viewpoint {
    longitude: f64,
    latitude: f64,
    zoom: f64,
    bearing: f64,
    pitch: f64,
    crs: u32,
}

fn invalid_payload() -> AppError {
    AppError::validation("INVALID_REQUEST", "invalid request payload")
}

fn invalid_map() -> AppError {
    AppError::validation("INVALID_MAP_REQUEST", "Invalid map request.")
}

// serde_json otherwise converts out-of-range integer literals to f64, silently
// changing opaque display data and the cross-runtime operation fingerprint.
fn reject_large_integers(bytes: &[u8]) -> Result<(), AppError> {
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                i += 1;
                while i < bytes.len() {
                    match bytes[i] {
                        b'\\' => i += 2,
                        b'"' => {
                            i += 1;
                            break;
                        }
                        _ => i += 1,
                    }
                }
            }
            b'-' | b'0'..=b'9' => {
                let start = i;
                i += 1;
                while i < bytes.len()
                    && matches!(bytes[i], b'0'..=b'9' | b'.' | b'e' | b'E' | b'+' | b'-')
                {
                    i += 1;
                }
                let literal =
                    std::str::from_utf8(&bytes[start..i]).map_err(|_| invalid_payload())?;
                if !literal.contains(['.', 'e', 'E'])
                    && literal.parse::<i64>().is_err()
                    && literal.parse::<u64>().is_err()
                {
                    return Err(invalid_map());
                }
            }
            _ => i += 1,
        }
    }
    Ok(())
}

fn normalize_number(n: &mut f64, min: f64, max: f64) -> Result<(), AppError> {
    if !n.is_finite() || !(min..=max).contains(n) {
        return Err(invalid_payload());
    }
    if *n == 0.0 {
        *n = 0.0;
    }
    Ok(())
}

impl MapSaveIntent {
    pub fn parse(bytes: &[u8]) -> Result<Self, AppError> {
        let mut value: Self = serde_json::from_slice(bytes).map_err(|_| invalid_payload())?;
        reject_large_integers(bytes)?;
        if !(0..=MAX_REVISION).contains(&value.expected_revision)
            || value.layers.len() > 32
            || value.basemap != "grid"
        {
            return Err(invalid_payload());
        }
        let mut ids = HashSet::new();
        for layer in &mut value.layers {
            if !ids.insert(layer.layer_id) {
                return Err(invalid_payload());
            }
            let style = &mut layer.style;
            if style.color.len() != 7
                || !style.color.starts_with('#')
                || !style.color.as_bytes()[1..]
                    .iter()
                    .all(u8::is_ascii_hexdigit)
            {
                return Err(invalid_payload());
            }
            style.color.make_ascii_uppercase();
            normalize_number(&mut style.opacity, 0.0, 1.0)?;
            normalize_number(&mut style.width, 1.0, 12.0)?;
            normalize_number(&mut style.radius, 2.0, 20.0)?;
            if let Some(group) = &layer.group {
                if group.is_empty() || group.chars().count() > 200 {
                    return Err(invalid_payload());
                }
                if group.split('/').count() > 4
                    || group
                        .split('/')
                        .any(|part| part.is_empty() || part.trim() != part)
                {
                    return Err(invalid_map());
                }
            }
            if let Some(display) = &layer.display {
                if python_json(&Value::Object(display.clone()), false, false)?.len() > 16_000 {
                    return Err(invalid_map());
                }
            }
        }
        if let ViewpointIntent::Set { value } = &mut value.default_viewpoint {
            if value.crs != 4326 {
                return Err(invalid_payload());
            }
            normalize_number(&mut value.longitude, -180.0, 180.0)?;
            normalize_number(&mut value.latitude, -85.05112878, 85.05112878)?;
            normalize_number(&mut value.zoom, 0.0, 22.0)?;
            normalize_number(&mut value.bearing, -180.0, 180.0)?;
            normalize_number(&mut value.pitch, 0.0, 60.0)?;
        }
        Ok(value)
    }

    fn fingerprint(&self) -> Result<String, AppError> {
        let payload = json!({
            "expected_revision":self.expected_revision,
            "layers":self.layers,
            "basemap":self.basemap,
            "default_viewpoint":self.default_viewpoint,
        });
        Ok(format!(
            "{:x}",
            Sha256::digest(python_json(&payload, true, true)?.as_bytes())
        ))
    }

    fn configuration(&self, previous: Option<&Value>) -> Value {
        let viewpoint = match &self.default_viewpoint {
            ViewpointIntent::Preserve => previous
                .map(|config| config["default_viewpoint"].clone())
                .unwrap_or(Value::Null),
            ViewpointIntent::Clear => Value::Null,
            ViewpointIntent::Set { value } => json!(value),
        };
        json!({"layers":self.layers,"basemap":self.basemap,"default_viewpoint":viewpoint})
    }
}

/// Python json.dumps(sort_keys=True, ensure_ascii=ascii), including float exponent
/// spelling and the separator convention used by the Python map domain.
fn python_json(value: &Value, ascii: bool, compact: bool) -> Result<String, AppError> {
    fn render(
        value: &Value,
        output: &mut String,
        ascii: bool,
        compact: bool,
    ) -> Result<(), AppError> {
        let comma = if compact { "," } else { ", " };
        let colon = if compact { ":" } else { ": " };
        match value {
            Value::Null => output.push_str("null"),
            Value::Bool(b) => output.push_str(if *b { "true" } else { "false" }),
            Value::Number(number) => {
                let repr = number.to_string();
                if repr.contains(['.', 'e', 'E']) {
                    let number = number
                        .as_f64()
                        .filter(|n| n.is_finite())
                        .ok_or_else(invalid_map)?;
                    if number != 0.0 && !(0.0001..1e16).contains(&number.abs()) {
                        let repr = format!("{number:e}");
                        let (mantissa, exponent) = repr.split_once('e').ok_or_else(invalid_map)?;
                        let exponent: i32 = exponent.parse().map_err(|_| invalid_map())?;
                        write!(
                            output,
                            "{mantissa}e{}{:02}",
                            if exponent < 0 { '-' } else { '+' },
                            exponent.unsigned_abs()
                        )
                        .map_err(|_| invalid_map())?;
                    } else {
                        let repr = format!("{number}");
                        output.push_str(&repr);
                        if !repr.contains('.') {
                            output.push_str(".0");
                        }
                    }
                } else {
                    output.push_str(if repr == "-0" { "0" } else { &repr });
                }
            }
            Value::String(value) => {
                let repr = serde_json::to_string(value).map_err(|_| invalid_map())?;
                for c in repr.chars() {
                    if ascii && c >= '\u{7f}' {
                        for code in c.encode_utf16(&mut [0; 2]) {
                            write!(output, "\\u{code:04x}").map_err(|_| invalid_map())?;
                        }
                    } else {
                        output.push(c);
                    }
                }
            }
            Value::Array(values) => {
                output.push('[');
                for (i, value) in values.iter().enumerate() {
                    if i > 0 {
                        output.push_str(comma);
                    }
                    render(value, output, ascii, compact)?;
                }
                output.push(']');
            }
            Value::Object(values) => {
                output.push('{');
                let mut keys: Vec<_> = values.keys().collect();
                keys.sort_unstable();
                for (i, key) in keys.into_iter().enumerate() {
                    if i > 0 {
                        output.push_str(comma);
                    }
                    render(&Value::String(key.clone()), output, ascii, compact)?;
                    output.push_str(colon);
                    render(&values[key], output, ascii, compact)?;
                }
                output.push('}');
            }
        }
        Ok(())
    }
    let mut output = String::new();
    render(value, &mut output, ascii, compact)?;
    Ok(output)
}

impl ProjectRead<'_> {
    pub async fn save_map(
        &mut self,
        intent: &MapSaveIntent,
        display_name: &str,
    ) -> Result<Value, AppError> {
        self.require_unrestricted()?;
        if !self.edit_authorized {
            return Err(AppError::forbidden("missing permission: project.edit"));
        }
        let organization_id = self.tx.organization_id();
        let project_id = self.project_id;
        let actor_id = self.tx.actor_id();
        let map_id = map_identity(project_id);
        let operation_id = intent.operation_id;
        let hash = intent.fingerprint()?;
        let prior: Option<(String,Uuid,String,i64)>=sqlx::query_as(
            "SELECT actor_type,actor_id,request_hash,committed_revision FROM maps.map_operations WHERE organization_id=$1 AND project_id=$2 AND operation_id=$3"
        ).bind(organization_id).bind(project_id).bind(operation_id).fetch_optional(&mut **self.tx.tx_mut()).await.map_err(db_error)?;
        if let Some((kind, actor, previous_hash, revision)) = prior {
            if kind != "human" || actor != actor_id || previous_hash != hash {
                return Err(AppError::conflict(
                    "OPERATION_INTENT_CONFLICT",
                    "This save operation cannot be reused.",
                ));
            }
            return Ok(
                json!({"map_id":map_id,"operation_id":operation_id,"configuration_revision":revision}),
            );
        }
        let previous:Option<(i64,Value)>=sqlx::query_as(
            "SELECT configuration_revision,configuration FROM maps.project_maps WHERE organization_id=$1 AND project_id=$2 FOR UPDATE"
        ).bind(organization_id).bind(project_id).fetch_optional(&mut **self.tx.tx_mut()).await.map_err(db_error)?;
        let revision = previous
            .as_ref()
            .map(|(revision, _)| *revision)
            .unwrap_or(0);
        if intent.expected_revision != revision || revision == MAX_REVISION {
            return Err(AppError::stale_map_revision());
        }
        let before = previous.map(|(_, value)| value);
        let configuration = intent.configuration(before.as_ref());
        if let Some(before) = &before {
            let equal: bool = sqlx::query_scalar("SELECT $1::jsonb = $2::jsonb")
                .bind(before)
                .bind(&configuration)
                .fetch_one(&mut **self.tx.tx_mut())
                .await
                .map_err(db_error)?;
            if equal {
                return Err(AppError::conflict(
                    "NO_CHANGES",
                    "The map configuration has no material changes.",
                ));
            }
        }
        let mut ids: Vec<_> = intent.layers.iter().map(|layer| layer.layer_id).collect();
        ids.sort_unstable();
        for id in ids {
            let layer = self
                .layer_rows(Some(id), None, 1)
                .await?
                .pop()
                .ok_or_else(resource_not_found)?;
            let source_id: Uuid = layer.try_get("source_id").map_err(db_error)?;
            let source = self
                .get_source_record(source_id)
                .await
                .map_err(|error| match error {
                    AppError::NotFound { .. } => AppError::validation(
                        "MAP_REFERENCE_UNAVAILABLE",
                        "A requested map Layer is unavailable.",
                    ),
                    other => other,
                })?;
            if !source.ready || !source.render_supported {
                return Err(AppError::validation(
                    "MAP_REFERENCE_UNAVAILABLE",
                    "A requested map Layer is unavailable.",
                ));
            }
        }
        let now = Utc::now();
        let new_revision = revision + 1;
        if revision == 0 {
            sqlx::query("INSERT INTO maps.project_maps(id,organization_id,project_id,configuration_revision,configuration,updated_at) VALUES($1,$2,$3,$4,$5,$6)")
                .bind(map_id).bind(organization_id).bind(project_id).bind(new_revision).bind(&configuration).bind(now)
                .execute(&mut **self.tx.tx_mut()).await.map_err(db_error)?;
        } else {
            let changed=sqlx::query("UPDATE maps.project_maps SET configuration_revision=$1,configuration=$2,updated_at=$3 WHERE organization_id=$4 AND project_id=$5 AND id=$6 AND configuration_revision=$7")
                .bind(new_revision).bind(&configuration).bind(now).bind(organization_id).bind(project_id).bind(map_id).bind(revision)
                .execute(&mut **self.tx.tx_mut()).await.map_err(db_error)?;
            if changed.rows_affected() != 1 {
                return Err(AppError::stale_map_revision());
            }
        }
        let audit_id = Uuid::new_v4();
        sqlx::query("INSERT INTO audit.audit_events(id,organization_id,actor_type,actor_id,actor_display_snapshot,action,resource_type,resource_id,outcome,metadata,created_at)
                     VALUES($1,$2,'human',$3,$4,'map.configuration.saved','project_map',$5,'success',$6,$7)")
            .bind(audit_id).bind(organization_id).bind(actor_id).bind(display_name).bind(map_id)
            .bind(json!({"project_id":project_id.to_string(),"operation_id":operation_id.to_string(),"configuration_revision":new_revision})).bind(now)
            .execute(&mut **self.tx.tx_mut()).await.map_err(db_error)?;
        sqlx::query("INSERT INTO maps.map_operations(organization_id,project_id,operation_id,map_id,actor_type,actor_id,request_hash,committed_revision,created_at)
                     VALUES($1,$2,$3,$4,'human',$5,$6,$7,$8)")
            .bind(organization_id).bind(project_id).bind(operation_id).bind(map_id).bind(actor_id).bind(hash).bind(new_revision).bind(now)
            .execute(&mut **self.tx.tx_mut()).await.map_err(db_error)?;
        sqlx::query("INSERT INTO maps.map_revisions(organization_id,project_id,map_id,configuration_revision,prior_revision,operation_id,actor_type,actor_id,actor_display_snapshot,created_at,before_configuration,after_configuration,audit_event_id)
                     VALUES($1,$2,$3,$4,$5,$6,'human',$7,$8,$9,$10,$11,$12)")
            .bind(organization_id).bind(project_id).bind(map_id).bind(new_revision).bind(revision).bind(operation_id).bind(actor_id).bind(display_name).bind(now).bind(before).bind(configuration).bind(audit_id)
            .execute(&mut **self.tx.tx_mut()).await.map_err(db_error)?;
        Ok(
            json!({"map_id":map_id,"operation_id":operation_id,"configuration_revision":new_revision}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opaque_integer_overflow_fails_closed() {
        assert!(reject_large_integers(br#"{"value":18446744073709551616}"#).is_err());
        assert!(reject_large_integers(br#"{"value":-9223372036854775809}"#).is_err());
        assert!(reject_large_integers(
            br#"{"value":"18446744073709551616", "max":18446744073709551615}"#
        )
        .is_ok());
    }

    #[test]
    fn python_json_formats_unicode_and_exponents() {
        let value: Value = serde_json::from_str(
            r#"{"b":1e-5,"a":"Việt 😀","c":1e16,"d":1.0,"e":18446744073709551615}"#,
        )
        .unwrap();
        assert_eq!(
            python_json(&value, true, true).unwrap(),
            r#"{"a":"Vi\u1ec7t \ud83d\ude00","b":1e-05,"c":1e+16,"d":1.0,"e":18446744073709551615}"#
        );
    }
}
