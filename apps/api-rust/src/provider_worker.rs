//! Restricted provider format/transport engine. Rust owns the tenant transaction,
//! session/CSRF and response lifecycle; worker receives no database credentials.

use axum::http::StatusCode;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Semaphore;
use uuid::Uuid;

const MODULES: &[(&str, &str)] = &[
    ("openvietgis_api/domains/platform_errors.py", include_str!("../../api/openvietgis_api/domains/platform_errors.py")),
    ("openvietgis_api/domains/external_sources/domain.py", include_str!("../../api/openvietgis_api/domains/external_sources/domain.py")),
    ("openvietgis_api/domains/external_sources/provider.py", include_str!("../../api/openvietgis_api/domains/external_sources/provider.py")),
    ("openvietgis_api/domains/external_sources/tile_provider.py", include_str!("../../api/openvietgis_api/domains/external_sources/tile_provider.py")),
    ("openvietgis_api/domains/external_sources/geotiff_provider.py", include_str!("../../api/openvietgis_api/domains/external_sources/geotiff_provider.py")),
    ("openvietgis_api/domains/external_sources/cog_provider.py", include_str!("../../api/openvietgis_api/domains/external_sources/cog_provider.py")),
    ("openvietgis_api/adapters/http/external_provider_transport.py", include_str!("../../api/openvietgis_api/adapters/http/external_provider_transport.py")),
];
const DRIVER: &str = r####"import datetime
import json
import resource
import sys
import uuid

resource.setrlimit(resource.RLIMIT_AS, (512 * 1024 * 1024, 512 * 1024 * 1024))
resource.setrlimit(resource.RLIMIT_CPU, (15, 18))
from openvietgis_api.domains.external_sources.domain import (
    AllowlistedHostRecord, ExternalConnectionRecord, ExternalProtocol,
)
from openvietgis_api.domains.external_sources.provider import (
    approved_profile, provider_preview, provider_viewport,
)
from openvietgis_api.adapters.http.external_provider_transport import (
    fetch_provider_bytes, fetch_provider_range,
)
from openvietgis_api.domains.platform_errors import PlatformDomainError

def record(raw, cls):
    # Only fields in the immutable dataclass contract; no unexpected DB columns.
    from dataclasses import fields
    values = {field.name: raw[field.name] for field in fields(cls)}
    for key in values:
        if key == "id" or key.endswith("_id"):
            values[key] = uuid.UUID(values[key])
        elif key.endswith("_at"):
            values[key] = datetime.datetime.fromisoformat(values[key])
    if cls is ExternalConnectionRecord:
        values["protocol"] = ExternalProtocol(values["protocol"])
        values["crs_codes"] = tuple(values["crs_codes"])
    else:
        values["allowed_schemes"] = tuple(values["allowed_schemes"])
        values["allowed_ports"] = tuple(values["allowed_ports"])
    return cls(**values)

try:
    request = json.load(sys.stdin)
    conn = record(request["context"]["connection"], ExternalConnectionRecord)
    allow = record(request["context"]["allowlist"], AllowlistedHostRecord)
    profile = approved_profile(conn, allow)
    viewport = request.get("viewport")
    if viewport is None:
        result = provider_preview(profile, fetch_provider_bytes, fetch_provider_range)
    else:
        result = provider_viewport(
            profile, fetch_provider_range, crs=viewport["crs"],
            bbox=tuple(viewport["bbox"]), width=viewport["width"], height=viewport["height"],
        )
    print(json.dumps({"result": result}, allow_nan=False))
except PlatformDomainError as exc:
    # The transport deliberately emits only stable, customer-safe messages.
    error = {"code": exc.code, "message": exc.message}
    if exc.details:
        error["details"] = exc.details
    print(json.dumps({"status": exc.status_code, "error": error}, allow_nan=False))
except Exception:
    print(json.dumps({"status": 502, "error": {
        "code": "PROVIDER_UNAVAILABLE",
        "message": "external provider response could not be verified",
        "details": {"availability": "unavailable"},
    }}))
"####;
static SLOTS: Semaphore = Semaphore::const_new(4);
type Failure = (StatusCode, Value);

fn unavailable() -> Failure {
    (
        StatusCode::BAD_GATEWAY,
        json!({
            "code": "PROVIDER_UNAVAILABLE",
            "message": "external provider response could not be verified",
            "details": {"availability": "unavailable"}
        }),
    )
}

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

async fn scratch() -> Result<Scratch, Failure> {
    let path = std::env::temp_dir().join(format!("ovg-provider-{}", Uuid::new_v4()));
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(&path).map_err(|_| unavailable())?;
    let scratch = Scratch(path);
    for (name, source) in MODULES {
        let file = scratch.0.join(name);
        let parent = file.parent().ok_or_else(unavailable)?;
        tokio::fs::create_dir_all(parent).await.map_err(|_| unavailable())?;
        tokio::fs::write(file, source).await.map_err(|_| unavailable())?;
    }
    tokio::fs::write(scratch.0.join("driver.py"), DRIVER)
        .await
        .map_err(|_| unavailable())?;
    Ok(scratch)
}

pub(crate) async fn prove(context: &Value, viewport: Option<Value>) -> Result<Value, Failure> {
    if !crate::raster_compute::worker_nonroot() {
        return Err(unavailable());
    }
    let _permit = SLOTS.try_acquire().map_err(|_| unavailable())?;
    let profiles = std::env::var("OPENVIETGIS_EXTERNAL_PROVIDER_PROFILES")
        .unwrap_or_else(|_| "[]".to_owned());
    if profiles.len() > 64 * 1024 {
        return Err(unavailable());
    }
    let scratch = scratch().await?;
    let request = json!({"context": context, "viewport": viewport}).to_string();
    if request.len() > 128 * 1024 {
        return Err(unavailable());
    }
    let mut child = tokio::process::Command::new("/usr/bin/python3")
        .args(["-E", "-s", "-B"])
        .arg(scratch.0.join("driver.py"))
        .current_dir(&scratch.0)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("LANG", "C.UTF-8")
        .env("HOME", &scratch.0)
        .env("OPENVIETGIS_EXTERNAL_PROVIDER_PROFILES", profiles)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| unavailable())?;
    let mut input = child.stdin.take().ok_or_else(unavailable)?;
    let output = child.stdout.take().ok_or_else(unavailable)?;
    let bytes = tokio::time::timeout(Duration::from_secs(30), async {
        input.write_all(request.as_bytes()).await?;
        drop(input);
        let mut bytes = Vec::new();
        output.take(4 * 1024 * 1024 + 1).read_to_end(&mut bytes).await?;
        if bytes.len() > 4 * 1024 * 1024 {
            let _ = child.kill().await;
            return Err(std::io::Error::other("provider response limit"));
        }
        if !child.wait().await?.success() {
            return Err(std::io::Error::other("provider worker failed"));
        }
        Ok::<_, std::io::Error>(bytes)
    })
    .await
    .map_err(|_| unavailable())?
    .map_err(|_| unavailable())?;
    let mut result: Value = serde_json::from_slice(&bytes).map_err(|_| unavailable())?;
    if let Some(body) = result.as_object_mut().and_then(|v| v.remove("result")) {
        if body.is_object() && body["availability"] == "available" {
            return Ok(body);
        }
        return Err(unavailable());
    }
    let status = match result["status"].as_u64() {
        Some(403) => StatusCode::FORBIDDEN,
        Some(422) => StatusCode::UNPROCESSABLE_ENTITY,
        Some(502) => StatusCode::BAD_GATEWAY,
        _ => return Err(unavailable()),
    };
    let error = result.as_object_mut().and_then(|v| v.remove("error"))
        .filter(Value::is_object).ok_or_else(unavailable)?;
    Err((status, error))
}
