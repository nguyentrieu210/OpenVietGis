//! Lightweight Prometheus text exposition metrics registry.

use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct MetricsRegistry {
    started_at: Instant,
    http_requests_total: Arc<AtomicU64>,
    http_errors_total: Arc<AtomicU64>,
    db_ready_gauge: Arc<AtomicU8>,
}

impl Default for MetricsRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricsRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self {
            started_at: Instant::now(),
            http_requests_total: Arc::new(AtomicU64::new(0)),
            http_errors_total: Arc::new(AtomicU64::new(0)),
            db_ready_gauge: Arc::new(AtomicU8::new(0)),
        }
    }

    pub fn record_request(&self, status_code: u16) {
        self.http_requests_total.fetch_add(1, Ordering::Relaxed);
        if status_code >= 500 {
            self.http_errors_total.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn set_db_ready(&self, ready: bool) {
        self.db_ready_gauge
            .store(u8::from(ready), Ordering::Relaxed);
    }

    #[must_use]
    pub fn render_prometheus(&self, registered_modules: usize, in_flight_jobs: usize) -> String {
        let requests = self.http_requests_total.load(Ordering::Relaxed);
        let errors = self.http_errors_total.load(Ordering::Relaxed);
        let db_ready = self.db_ready_gauge.load(Ordering::Relaxed);
        let uptime = self.started_at.elapsed().as_secs_f64();

        format!(
            "# HELP ovg_http_requests_total Total HTTP requests handled by openvietgis-api-rust.\n\
             # TYPE ovg_http_requests_total counter\n\
             ovg_http_requests_total {requests}\n\
             # HELP ovg_http_requests_5xx_total Total 5xx HTTP responses.\n\
             # TYPE ovg_http_requests_5xx_total counter\n\
             ovg_http_requests_5xx_total {errors}\n\
             # HELP ovg_database_ready Last observed database readiness (1=ready, 0=unavailable).\n\
             # TYPE ovg_database_ready gauge\n\
             ovg_database_ready {db_ready}\n\
             # HELP ovg_registered_modules_total Number of registered vertical modules.\n\
             # TYPE ovg_registered_modules_total gauge\n\
             ovg_registered_modules_total {registered_modules}\n\
             # HELP ovg_processing_jobs_in_flight Current queued and running processing jobs.\n\
             # TYPE ovg_processing_jobs_in_flight gauge\n\
             ovg_processing_jobs_in_flight {in_flight_jobs}\n\
             # HELP ovg_process_uptime_seconds Process uptime in seconds.\n\
             # TYPE ovg_process_uptime_seconds gauge\n\
             ovg_process_uptime_seconds {uptime:.3}\n"
        )
    }
}
