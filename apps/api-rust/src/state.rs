//! Shared application state injected into Axum handlers.

use std::sync::Arc;

#[cfg(feature = "house-numbering")]
use ovg_module_house_numbering::HouseNumberingModule;
use ovg_module_sdk::ModuleRegistry;
use ovg_processing::{ExecutorConfig, JobExecutor};
use ovg_storage::viewport_cache::ViewportResponseCache;
use ovg_storage::DbPool;
use ovg_sync::SyncBroadcaster;

use crate::config::AppConfig;
use crate::metrics::MetricsRegistry;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<AppConfig>,
    pub db_pool: Option<DbPool>,
    pub modules: ModuleRegistry,
    pub metrics: MetricsRegistry,
    pub viewport_cache: ViewportResponseCache,
    pub broadcaster: SyncBroadcaster,
    pub jobs: JobExecutor,
}

impl AppState {
    pub fn new(config: AppConfig, db_pool: Option<DbPool>) -> Result<Self, ovg_core::AppError> {
        let modules = ModuleRegistry::new();
        #[cfg(feature = "house-numbering")]
        let modules = {
            let mut modules = modules;
            modules.register(Arc::new(HouseNumberingModule))?;
            modules
        };
        let jobs = JobExecutor::new(ExecutorConfig::default())?;
        Ok(Self {
            config: Arc::new(config),
            db_pool,
            modules,
            metrics: MetricsRegistry::new(),
            viewport_cache: ViewportResponseCache::new(),
            broadcaster: SyncBroadcaster::new(256),
            jobs,
        })
    }
}
