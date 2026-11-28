//! Bounded E15 codec/render subprocess. Never pass database credentials or
//! authorize an operation here; only Rust handlers can publish an artifact.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use ovg_core::AppError;
use serde_json::Value;
use tokio::sync::Semaphore;
use uuid::Uuid;

const LIMIT: usize = 64 * 1024 * 1024;
const DRIVER: &str = include_str!("file_codec.py");
static SLOTS: Semaphore = Semaphore::const_new(2);

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn unavailable() -> AppError {
    AppError::service_unavailable("FORMAT_ENGINE_UNAVAILABLE", "House-numbering format engine unavailable")
}

fn error_code(code: &str) -> &'static str {
    match code {
        "IMPORT_FORMAT" => "IMPORT_FORMAT",
        "IMPORT_HEADER" => "IMPORT_HEADER",
        "IMPORT_ENCODING" => "IMPORT_ENCODING",
        "IMPORT_XLSX" => "IMPORT_XLSX",
        "IMPORT_GEOJSON" => "IMPORT_GEOJSON",
        "IMPORT_DXF" => "IMPORT_DXF",
        "IMPORT_SHP" => "IMPORT_SHP",
        "IMPORT_ARCHIVE" => "IMPORT_ARCHIVE",
        "IMPORT_ROW_LIMIT" => "IMPORT_ROW_LIMIT",
        "REPORT_SIZE_EXCEEDED" => "REPORT_SIZE_EXCEEDED",
        "APPROVAL_REQUIRED" => "APPROVAL_REQUIRED",
        "STALE_APPROVAL" => "STALE_APPROVAL",
        "INVALID_FEATURE_ID" => "INVALID_FEATURE_ID",
        "INVALID_PACKET_KIND" => "INVALID_PACKET_KIND",
        _ => "FORMAT_ENGINE_FAILED",
    }
}

pub(super) async fn run(req: &Value, input: Option<&[u8]>, result_json: bool) -> Result<Vec<u8>, AppError> {
    if !crate::raster_compute::worker_nonroot() {
        return Err(unavailable());
    }
    let _slot = SLOTS.try_acquire().map_err(|_| unavailable())?;
    let request = serde_json::to_vec(req).map_err(|_| unavailable())?;
    if request.len() > LIMIT || input.is_some_and(|v| v.len() > LIMIT) {
        return Err(AppError::validation("REPORT_SIZE_EXCEEDED", "Format payload exceeds byte limit"));
    }
    let path = std::env::temp_dir().join(format!("ovg-hn-{}", Uuid::new_v4()));
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(&path).map_err(|_| unavailable())?;
    let scratch = Scratch(path);
    tokio::fs::write(scratch.0.join("driver.py"), DRIVER).await.map_err(|_| unavailable())?;
    tokio::fs::write(scratch.0.join("request.json"), request).await.map_err(|_| unavailable())?;
    if let Some(input) = input {
        tokio::fs::write(scratch.0.join("input.bin"), input).await.map_err(|_| unavailable())?;
    }
    let python = std::env::var_os("OPENVIETGIS_CAD_PYTHON")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("VIRTUAL_ENV").map(|p| PathBuf::from(p).join("bin/python3")))
        .unwrap_or_else(|| PathBuf::from("/usr/bin/python3"));
    let out = tokio::time::timeout(
        Duration::from_secs(35),
        tokio::process::Command::new(python)
            .args(["-I", "-B", "driver.py"])
            .current_dir(&scratch.0)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("LANG", "C.UTF-8")
            .env("HOME", &scratch.0)
            .env("TMPDIR", &scratch.0)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .output(),
    )
    .await
    .map_err(|_| unavailable())?
    .map_err(|_| unavailable())?;
    if !out.status.success() || out.stdout.len() > 8192 {
        return Err(unavailable());
    }
    let reply: Value = serde_json::from_slice(&out.stdout).map_err(|_| unavailable())?;
    if reply["ok"].as_bool() != Some(true) {
        let code = error_code(reply["code"].as_str().unwrap_or_default());
        let message = reply["message"].as_str().unwrap_or("Format processing failed");
        if reply["conflict"].as_bool() == Some(true) {
            return Err(AppError::conflict(code, message));
        }
        return Err(AppError::validation(code, message));
    }
    let output = scratch.0.join(if result_json { "result.json" } else { "output.bin" });
    let meta = tokio::fs::metadata(&output).await.map_err(|_| unavailable())?;
    if meta.len() == 0 || meta.len() > LIMIT as u64 {
        return Err(unavailable());
    }
    tokio::fs::read(output).await.map_err(|_| unavailable())
}
