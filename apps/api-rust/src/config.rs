//! Environment-backed runtime configuration for `openvietgis-api-rust`.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::PathBuf;

use ovg_storage::DbPoolConfig;
use serde::Deserialize;

/// Public part of one `OPENVIETGIS_OIDC_PROVIDERS` entry; secrets are ignored.
#[derive(Clone, Debug, Deserialize)]
pub struct OidcProvider {
    pub issuer: String,
    pub display_name: String,
}

#[derive(Clone)]
pub struct AppConfig {
    pub bind_addr: SocketAddr,
    pub database_url: Option<String>,
    pub db_pool: DbPoolConfig,
    pub log_json: bool,
    pub cors_origins: Vec<String>,
    pub session_cookie_name: String,
    pub session_cookie_secure: bool,
    pub web_base_url: String,
    pub api_public_base_url: String,
    pub oidc_providers: BTreeMap<String, OidcProvider>,
    pub csrf_secret: Option<String>,
    pub artifact_store_dir: PathBuf,
    pub service_version: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            bind_addr: SocketAddr::from(([0, 0, 0, 0], 8080)),
            database_url: None,
            db_pool: DbPoolConfig::default(),
            log_json: false,
            cors_origins: vec![
                "http://127.0.0.1:5173".to_string(),
                "http://localhost:5173".to_string(),
                "http://127.0.0.1:5178".to_string(),
            ],
            session_cookie_name: "ovg_session".to_string(),
            session_cookie_secure: false,
            web_base_url: "http://127.0.0.1:5173".to_string(),
            api_public_base_url: "http://127.0.0.1:8000".to_string(),
            oidc_providers: BTreeMap::new(),
            csrf_secret: None,
            artifact_store_dir: PathBuf::from("/tmp/openvietgis-artifacts"),
            service_version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}

impl AppConfig {
    #[must_use]
    pub fn from_env() -> Self {
        let mut cfg = Self::default();
        if let Ok(addr) =
            std::env::var("OPENVIETGIS_BIND_ADDR").or_else(|_| std::env::var("OVG_BIND_ADDR"))
        {
            if let Ok(parsed) = addr.parse() {
                cfg.bind_addr = parsed;
            }
        }
        if let Ok(url) =
            std::env::var("OPENVIETGIS_DATABASE_URL").or_else(|_| std::env::var("DATABASE_URL"))
        {
            let trimmed = url.trim();
            if !trimmed.is_empty() {
                cfg.database_url = Some(trimmed.to_string());
            }
        }
        if let Ok(max_conn) = std::env::var("OPENVIETGIS_DB_MAX_CONNECTIONS")
            .or_else(|_| std::env::var("OVG_DB_MAX_CONNECTIONS"))
        {
            if let Ok(n) = max_conn.parse::<u32>() {
                cfg.db_pool.max_connections = n.clamp(1, 128);
            }
        }
        if let Ok(name) = std::env::var("OPENVIETGIS_SESSION_COOKIE_NAME") {
            cfg.session_cookie_name = name;
        }
        if let Ok(secure) = std::env::var("OPENVIETGIS_SESSION_COOKIE_SECURE") {
            cfg.session_cookie_secure = matches!(
                secure.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            );
        }
        if let Ok(url) = std::env::var("OPENVIETGIS_WEB_BASE_URL") {
            cfg.web_base_url = url;
        }
        if let Ok(url) = std::env::var("OPENVIETGIS_API_PUBLIC_BASE_URL") {
            cfg.api_public_base_url = url;
        }
        if let Ok(providers) = std::env::var("OPENVIETGIS_OIDC_PROVIDERS") {
            match serde_json::from_str(&providers) {
                Ok(parsed) => cfg.oidc_providers = parsed,
                Err(error) => {
                    tracing::error!(%error, "ignoring invalid OPENVIETGIS_OIDC_PROVIDERS")
                }
            }
        }
        cfg.csrf_secret = std::env::var("OPENVIETGIS_CSRF_SECRET")
            .ok()
            .filter(|secret| !secret.is_empty());
        if let Some(dir) = std::env::var_os("OPENVIETGIS_ARTIFACT_STORE_DIR") {
            cfg.artifact_store_dir = PathBuf::from(dir);
        }
        if let Ok(fmt) = std::env::var("OVG_LOG_FORMAT") {
            cfg.log_json = fmt.eq_ignore_ascii_case("json");
        }
        if let Ok(origins) = std::env::var("OVG_CORS_ORIGINS") {
            let parsed: Vec<String> = origins
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToString::to_string)
                .collect();
            if !parsed.is_empty() {
                cfg.cors_origins = parsed;
            }
        }
        cfg
    }
}
