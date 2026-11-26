//! Bounded SQLx PostgreSQL/PostGIS connection pool and readiness checks.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use ovg_core::AppError;
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPoolOptions;
use sqlx::{Connection, PgConnection, PgPool};
use tokio::sync::watch;

const VERIFY_RUNTIME_ROLE_SQL: &str = "SELECT
    session_user = 'openvietgis_runtime'
    AND current_user = 'openvietgis_runtime'
    AND NOT EXISTS (
        SELECT 1 FROM pg_roles
        WHERE pg_has_role(current_user, oid, 'MEMBER')
          AND (rolsuper OR rolbypassrls OR rolcreaterole OR rolcreatedb)
    )
    AND NOT EXISTS (
        SELECT 1
        FROM pg_class AS relation
        JOIN pg_namespace AS namespace ON namespace.oid = relation.relnamespace
        WHERE relation.relkind IN ('r', 'p')
          AND relation.relpersistence <> 't'
          AND namespace.nspname NOT IN ('pg_catalog', 'information_schema')
          AND pg_has_role(current_user, relation.relowner, 'MEMBER')
    )";

/// Bounded connection pool configuration suitable for constrained and production nodes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DbPoolConfig {
    pub max_connections: u32,
    pub min_connections: u32,
    pub acquire_timeout_secs: u64,
    pub idle_timeout_secs: u64,
    pub max_lifetime_secs: u64,
}

impl Default for DbPoolConfig {
    fn default() -> Self {
        Self {
            max_connections: 16,
            min_connections: 1,
            acquire_timeout_secs: 5,
            idle_timeout_secs: 300,
            max_lifetime_secs: 1800,
        }
    }
}

/// Database readiness report including PostGIS availability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DbHealth {
    pub database_ready: bool,
    pub postgis_version: Option<String>,
}

/// Thin, bounded wrapper around `sqlx::PgPool`.
#[derive(Debug, Clone)]
pub struct DbPool {
    inner: PgPool,
    config: DbPoolConfig,
    /// True while the latest connect attempt was refused by the runtime role check.
    role_rejected: Arc<watch::Sender<bool>>,
}

impl DbPool {
    /// Connect eagerly to PostgreSQL with bounded pool limits.
    pub async fn connect(database_url: &str, config: DbPoolConfig) -> Result<Self, AppError> {
        let role_rejected = Arc::new(watch::Sender::new(false));
        let pool = Self::build_options(&config, Arc::clone(&role_rejected))
            .connect(database_url)
            .await
            .map_err(|err| AppError::internal(format!("database connect failed: {err}")))?;
        Ok(Self {
            inner: pool,
            config,
            role_rejected,
        })
    }

    /// Create a lazy pool that establishes connections on first query.
    pub fn connect_lazy(database_url: &str, config: DbPoolConfig) -> Result<Self, AppError> {
        let role_rejected = Arc::new(watch::Sender::new(false));
        let pool = Self::build_options(&config, Arc::clone(&role_rejected))
            .connect_lazy(database_url)
            .map_err(|err| AppError::internal(format!("database lazy pool failed: {err}")))?;
        Ok(Self {
            inner: pool,
            config,
            role_rejected,
        })
    }

    #[must_use]
    pub fn inner(&self) -> &PgPool {
        &self.inner
    }

    #[must_use]
    pub fn config(&self) -> &DbPoolConfig {
        &self.config
    }

    /// A one-connection pool with the same runtime-role check as this pool, for a
    /// dedicated session such as LISTEN.
    pub async fn dedicated(&self) -> Result<PgPool, sqlx::Error> {
        let config = DbPoolConfig {
            max_connections: 1,
            min_connections: 0,
            ..self.config.clone()
        };
        let options = (*self.inner.connect_options()).clone();
        Self::build_options(&config, Arc::clone(&self.role_rejected))
            .connect_with(options)
            .await
    }

    fn build_options(
        config: &DbPoolConfig,
        role_rejected: Arc<watch::Sender<bool>>,
    ) -> PgPoolOptions {
        PgPoolOptions::new()
            .after_connect(move |connection, _| {
                let role_rejected = Arc::clone(&role_rejected);
                Box::pin(async move {
                    let valid = runtime_role_valid(connection).await?;
                    role_rejected.send_replace(!valid);
                    if !valid {
                        return Err(sqlx::Error::Protocol(
                            "restricted OpenVietGIS runtime database role required".to_string(),
                        ));
                    }
                    Ok(())
                })
            })
            .max_connections(config.max_connections.max(1))
            .min_connections(config.min_connections.min(config.max_connections.max(1)))
            .acquire_timeout(Duration::from_secs(config.acquire_timeout_secs.max(1)))
            .idle_timeout(Some(Duration::from_secs(config.idle_timeout_secs.max(10))))
            .max_lifetime(Some(Duration::from_secs(config.max_lifetime_secs.max(60))))
    }

    /// Run a readiness query, failing fast once a connect attempt rejects the role:
    /// sqlx retries `after_connect` errors until the acquire timeout otherwise.
    async fn probe<T>(
        &self,
        query: impl Future<Output = Result<T, sqlx::Error>>,
    ) -> Result<T, sqlx::Error> {
        let rejected_before = *self.role_rejected.borrow();
        if rejected_before {
            // One direct attempt (no sqlx retry loop) so readiness recovers once the role is fixed.
            let options = self.inner.connect_options();
            let attempt = async {
                let mut connection = PgConnection::connect_with(&options).await?;
                let valid = runtime_role_valid(&mut connection).await;
                let _ = connection.close().await;
                valid
            };
            let timeout = Duration::from_secs(self.config.acquire_timeout_secs.max(1));
            let valid = tokio::time::timeout(timeout, attempt)
                .await
                .map_err(|_| sqlx::Error::PoolTimedOut)??;
            self.role_rejected.send_replace(!valid);
            if !valid {
                return Err(sqlx::Error::PoolTimedOut);
            }
        }
        let mut rejected = self.role_rejected.subscribe();
        tokio::select! {
            result = query => result,
            _ = rejected.wait_for(|rejected| *rejected) => Err(sqlx::Error::PoolTimedOut),
        }
    }

    /// Report safe dependency versions for the legacy operations readiness contract.
    pub async fn dependency_versions(&self) -> (Option<String>, Option<String>) {
        let postgres = self
            .probe(sqlx::query_scalar::<_, String>("SELECT version()").fetch_one(&self.inner))
            .await
            .ok()
            .and_then(|version| version.split(',').next().map(str::to_owned));
        let postgis = if postgres.is_some() {
            self.probe(sqlx::query_scalar("SELECT PostGIS_Lib_Version()").fetch_one(&self.inner))
                .await
                .ok()
        } else {
            None
        };
        (postgres, postgis)
    }

    /// Run `SELECT 1` and probe `PostGIS_Version()` for readiness reporting.
    pub async fn health_check(&self) -> Result<DbHealth, AppError> {
        let ping: i32 = self
            .probe(sqlx::query_scalar("SELECT 1").fetch_one(&self.inner))
            .await
            .map_err(|err| {
                tracing::warn!(error = %err, "database readiness ping failed");
                AppError::service_unavailable("DATABASE_UNAVAILABLE", "database unavailable")
            })?;

        if ping != 1 {
            return Err(AppError::service_unavailable(
                "DATABASE_UNAVAILABLE",
                "database ping returned unexpected result",
            ));
        }

        let postgis_version: Option<String> = self
            .probe(sqlx::query_scalar("SELECT PostGIS_Version()").fetch_one(&self.inner))
            .await
            .ok();

        Ok(DbHealth {
            database_ready: true,
            postgis_version,
        })
    }
}

async fn runtime_role_valid(connection: &mut PgConnection) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(VERIFY_RUNTIME_ROLE_SQL)
        .fetch_one(connection)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_pool_config_is_bounded() {
        let cfg = DbPoolConfig::default();
        assert!(cfg.max_connections >= 1 && cfg.max_connections <= 64);
        assert!(cfg.acquire_timeout_secs >= 1);
    }
}