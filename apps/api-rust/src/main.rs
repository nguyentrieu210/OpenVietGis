//! Binary entry point for `openvietgis-api-rust` with graceful SIGINT/SIGTERM shutdown.

use openvietgis_api_rust::{build_router, init_tracing, AppConfig, AppState};
use ovg_storage::DbPool;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = AppConfig::from_env();
    init_tracing(config.log_json);

    let db_pool = match &config.database_url {
        Some(url) => match DbPool::connect_lazy(url, config.db_pool.clone()) {
            Ok(pool) => {
                tracing::info!("configured lazy PostgreSQL/PostGIS pool");
                Some(pool)
            }
            Err(err) => {
                tracing::error!(error = %err, "failed to configure database pool");
                None
            }
        },
        None => {
            tracing::warn!("DATABASE_URL not set; /api/v1/health/ready will report not_ready");
            None
        }
    };

    let bind_addr = config.bind_addr;
    let state = AppState::new(config, db_pool)?;
    let app = build_router(state);

    let listener = TcpListener::bind(bind_addr).await?;
    tracing::info!(addr = %bind_addr, "openvietgis-api-rust listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    tracing::info!("openvietgis-api-rust shutdown complete");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut sigterm) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            sigterm.recv().await;
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
    tracing::info!("shutdown signal received");
}
