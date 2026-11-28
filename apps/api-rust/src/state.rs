//! Shared application state injected into Axum handlers.
use crate::config::AppConfig;
use crate::metrics::MetricsRegistry;
use crate::tile_cache::TileCache;
#[cfg(feature = "house-numbering")]
use ovg_module_house_numbering::HouseNumberingModule;
use ovg_module_sdk::ModuleRegistry;
use ovg_processing::{ExecutorConfig, JobExecutor};
use ovg_storage::DbPool;
use ovg_sync::SyncBroadcaster;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<AppConfig>,
    pub db_pool: Option<DbPool>,
    pub modules: ModuleRegistry,
    pub metrics: MetricsRegistry,
    pub broadcaster: SyncBroadcaster,
    pub jobs: JobExecutor,
    pub tile_cache: TileCache,
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
            broadcaster: SyncBroadcaster::new(256),
            jobs,
            tile_cache: TileCache::new(),
        })
    }
}
