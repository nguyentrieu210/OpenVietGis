//! Staging operations readiness, matching the Python dependency probe contract.

use std::fs::{self, OpenOptions};
use std::io::{Read, Seek, Write};
use std::path::Path;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use chrono::{SecondsFormat, Utc};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::AppState;

pub async fn readiness(State(state): State<AppState>) -> (StatusCode, Json<Value>) {
    let (postgres, postgis) = if let Some(pool) = state.db_pool.as_ref() {
        pool.dependency_versions().await
    } else {
        (None, None)
    };
    let artifact_dir = state.config.artifact_store_dir.clone();
    let artifacts_ok = tokio::task::spawn_blocking(move || probe_artifacts(&artifact_dir))
        .await
        .unwrap_or(false);
    let ready = postgres.is_some() && postgis.is_some() && artifacts_ok;

    (
        if ready {
            StatusCode::OK
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        },
        Json(json!({
            "status": if ready { "ready" } else { "not_ready" },
            "database": availability(postgres.is_some()),
            "postgis": availability(postgis.is_some()),
            "postgresql_version": postgres,
            "postgis_version": postgis,
            "artifact_store": availability(artifacts_ok),
            "scope": "dependency_probe_only",
            "production_ready": false,
            "checked_at": Utc::now().to_rfc3339_opts(SecondsFormat::Micros, false),
        })),
    )
}

fn availability(available: bool) -> &'static str {
    if available {
        "ok"
    } else {
        "unavailable"
    }
}

fn probe_artifacts(directory: &Path) -> bool {
    if !fs::symlink_metadata(directory).is_ok_and(|metadata| metadata.is_dir()) {
        return false;
    }
    let path = directory.join(format!(".readiness-{}", Uuid::new_v4()));
    let Ok(mut probe) = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&path)
    else {
        return false;
    };

    const CONTENT: [u8; 21] = *b"openvietgis-readiness";
    let content = CONTENT;
    let mut observed = [0; CONTENT.len()];
    let verified = probe
        .write_all(&content)
        .and_then(|()| probe.flush())
        .and_then(|()| probe.rewind())
        .and_then(|()| probe.read_exact(&mut observed))
        .is_ok()
        && observed == content;
    drop(probe);
    fs::remove_file(path).is_ok() && verified
}
