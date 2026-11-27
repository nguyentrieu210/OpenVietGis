//! Fixed-destination invitation transport. Credentials and links are never
//! placed on a command line, in a log, or in the API response.

use serde_json::{json, Value};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use uuid::Uuid;

pub(crate) struct MailConfig {
    key: String,
    from: String,
    base: String,
}

impl MailConfig {
    pub(crate) fn from_env(base: &str) -> Option<Self> {
        let key = std::env::var("OPENVIETGIS_RESEND_API_KEY").ok()?;
        let from = std::env::var("OPENVIETGIS_INVITATION_FROM_EMAIL").ok()?;
        let path = std::env::var("OPENVIETGIS_INVITATION_WEB_PATH")
            .unwrap_or_else(|_| "/rewrite/?live=1".to_owned());
        if key.is_empty()
            || key.len() > 4096
            || key.bytes().any(|b| b.is_ascii_control() || b.is_ascii_whitespace())
            || from.trim().is_empty()
            || !path.starts_with('/')
            || path.starts_with("//")
            || path.contains('#')
            || path.chars().any(char::is_control)
            || !(base.starts_with("https://") || base.starts_with("http://"))
            || base.contains(['#', '?'])
            || base.chars().any(char::is_control)
        {
            return None;
        }
        Some(Self {
            key,
            from,
            base: format!("{}{path}", base.trim_end_matches('/')),
        })
    }

    pub(crate) async fn deliver(
        &self,
        recipient: &str,
        organization: &str,
        org: Uuid,
        id: Uuid,
        secret: &str,
    ) -> &'static str {
        let link = format!(
            "{}#organization_id={org}&invitation_id={id}&secret={secret}",
            self.base
        );
        let payload = json!({
            "from": self.from,
            "to": [recipient],
            "subject": "OpenVietGIS · Lời mời tham gia nhóm",
            "text": format!("Bạn được mời vào {organization}. Mở lời mời: {link}\nLời mời dùng một lần, hết hạn sau 48 giờ."),
        });
        let config = format!(
            "header = {}\nheader = {}\nheader = \"Content-Type: application/json\"\ndata-binary = {}\n",
            quote(&format!("Authorization: Bearer {}", self.key)),
            quote(&format!("Idempotency-Key: ovg-invite-{id}")),
            quote(&payload.to_string()),
        );
        let mut child = match tokio::process::Command::new("/usr/bin/curl")
            .args([
                "-q",
                "--config",
                "-",
                "--silent",
                "--request",
                "POST",
                "--proto",
                "=https",
                "--max-time",
                "12",
                "--max-filesize",
                "65536",
                "--write-out",
                "\n%{http_code}",
                "https://api.resend.com/emails",
            ])
            .env_clear()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
        {
            Ok(child) => child,
            Err(_) => return "unknown",
        };
        let Some(mut input) = child.stdin.take() else {
            return "unknown";
        };
        let Some(output) = child.stdout.take() else {
            return "unknown";
        };
        let response = tokio::time::timeout(Duration::from_secs(13), async {
            input.write_all(config.as_bytes()).await?;
            drop(input);
            let mut bytes = Vec::new();
            output.take(65541).read_to_end(&mut bytes).await?;
            if bytes.len() > 65540 {
                let _ = child.kill().await;
                return Err(std::io::Error::other("email response limit"));
            }
            if !child.wait().await?.success() {
                return Err(std::io::Error::other("email transport failed"));
            }
            Ok::<_, std::io::Error>(bytes)
        })
        .await;
        match response {
            Ok(Ok(bytes)) => classify(&bytes),
            _ => "unknown",
        }
    }
}

fn quote(value: &str) -> String {
    format!(
        "\"{}\"",
        value.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n").replace('\r', "\\r").replace('\t', "\\t"),
    )
}

fn classify(bytes: &[u8]) -> &'static str {
    let Some(index) = bytes.iter().rposition(|b| *b == b'\n') else {
        return "unknown";
    };
    let status = std::str::from_utf8(&bytes[index + 1..])
        .ok()
        .and_then(|s| s.parse::<u16>().ok());
    match status {
        Some(200..=299) => {
            let valid = serde_json::from_slice::<Value>(&bytes[..index])
                .ok()
                .and_then(|v| v.get("id").and_then(Value::as_str).map(str::to_owned))
                .is_some();
            if valid {
                "accepted_by_provider"
            } else {
                "unknown"
            }
        }
        Some(400..=499) => "rejected",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_acceptance_is_not_inbox_delivery() {
        assert_eq!(classify(b"{\"id\":\"fixture\"}\n200"), "accepted_by_provider");
        assert_eq!(classify(b"{}\n200"), "unknown");
        assert_eq!(classify(b"{}\n429"), "rejected");
        assert_eq!(classify(b"{}\n503"), "unknown");
    }

    #[test]
    fn config_values_cannot_inject_options() {
        assert_eq!(quote("a\"\nb\\c"), "\"a\\\"\\nb\\\\c\"");
    }
}
