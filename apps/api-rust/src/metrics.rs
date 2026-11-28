//! Lightweight Prometheus text exposition metrics registry.

use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const HTTP_BUCKETS_MS: [u64; 8] = [5, 10, 25, 50, 100, 250, 500, 1000];

#[derive(Debug, Clone)]
pub struct MetricsRegistry {
    started_at: Instant,
    http_requests_total: Arc<AtomicU64>,
    http_errors_total: Arc<AtomicU64>,
    http_duration_micros_total: Arc<AtomicU64>,
    http_duration_buckets: Arc<[AtomicU64; 9]>,
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
            http_duration_micros_total: Arc::new(AtomicU64::new(0)),
            http_duration_buckets: Arc::new(std::array::from_fn(|_| AtomicU64::new(0))),
            db_ready_gauge: Arc::new(AtomicU8::new(0)),
        }
    }

    pub fn record_request(&self, status_code: u16, elapsed: Duration) {
        self.http_requests_total.fetch_add(1, Ordering::Relaxed);
        if status_code >= 500 {
            self.http_errors_total.fetch_add(1, Ordering::Relaxed);
        }
        self.http_duration_micros_total.fetch_add(elapsed.as_micros().min(u128::from(u64::MAX)) as u64, Ordering::Relaxed);
        let ms = elapsed.as_millis().min(u128::from(u64::MAX)) as u64;
        let bucket = HTTP_BUCKETS_MS.iter().position(|upper| ms <= *upper).unwrap_or(HTTP_BUCKETS_MS.len());
        self.http_duration_buckets[bucket].fetch_add(1, Ordering::Relaxed);
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
        let duration_sum = self.http_duration_micros_total.load(Ordering::Relaxed) as f64 / 1_000_000.0;
        let rss = process_rss_bytes().unwrap_or(0);
        let mut cumulative = 0_u64;
        let mut duration = String::from("# HELP ovg_http_request_duration_seconds HTTP request latency without route or tenant labels.\n# TYPE ovg_http_request_duration_seconds histogram\n");
        for (idx, upper) in HTTP_BUCKETS_MS.iter().enumerate() {
            cumulative += self.http_duration_buckets[idx].load(Ordering::Relaxed);
            duration.push_str(&format!("ovg_http_request_duration_seconds_bucket{{le=\"{:.3}\"}} {cumulative}\n", *upper as f64 / 1000.0));
        }
        cumulative += self.http_duration_buckets[HTTP_BUCKETS_MS.len()].load(Ordering::Relaxed);
        duration.push_str(&format!("ovg_http_request_duration_seconds_bucket{{le=\"+Inf\"}} {cumulative}\novg_http_request_duration_seconds_sum {duration_sum:.6}\novg_http_request_duration_seconds_count {requests}\n"));

        format!(
            "# HELP ovg_http_requests_total Total HTTP requests handled by openvietgis-api-rust.\n\
             # TYPE ovg_http_requests_total counter\n\
             ovg_http_requests_total {requests}\n\
             # HELP ovg_http_requests_5xx_total Total 5xx HTTP responses.\n\
             # TYPE ovg_http_requests_5xx_total counter\n\
             ovg_http_requests_5xx_total {errors}\n\
             {duration}\
             # HELP ovg_database_ready Last observed database readiness (1=ready, 0=unavailable).\n\
             # TYPE ovg_database_ready gauge\n\
             ovg_database_ready {db_ready}\n\
             # HELP ovg_registered_modules_total Number of registered vertical modules.\n\
             # TYPE ovg_registered_modules_total gauge\n\
             ovg_registered_modules_total {registered_modules}\n\
             # HELP ovg_processing_jobs_in_flight Current queued and running processing jobs.\n\
             # TYPE ovg_processing_jobs_in_flight gauge\n\
             ovg_processing_jobs_in_flight {in_flight_jobs}\n\
             # HELP ovg_process_resident_memory_bytes Resident memory of the API process; 0 when unavailable.\n\
             # TYPE ovg_process_resident_memory_bytes gauge\n\
             ovg_process_resident_memory_bytes {rss}\n\
             # HELP ovg_process_uptime_seconds Process uptime in seconds.\n\
             # TYPE ovg_process_uptime_seconds gauge\n\
             ovg_process_uptime_seconds {uptime:.3}\n"
        )
    }
}

fn process_rss_bytes() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let kb = status.lines().find_map(|line| line.strip_prefix("VmRSS:"))?.split_whitespace().next()?.parse::<u64>().ok()?;
    kb.checked_mul(1024)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn histogram_is_cumulative_and_has_no_sensitive_labels() {
        let metrics = MetricsRegistry::new();
        metrics.record_request(200, Duration::from_millis(4));
        metrics.record_request(503, Duration::from_millis(80));
        metrics.record_request(200, Duration::from_millis(1400));
        let out = metrics.render_prometheus(2, 3);
        assert!(out.contains("ovg_http_requests_total 3"));
        assert!(out.contains("ovg_http_requests_5xx_total 1"));
        assert!(out.contains("ovg_http_request_duration_seconds_bucket{le=\"0.005\"} 1"));
        assert!(out.contains("ovg_http_request_duration_seconds_bucket{le=\"0.100\"} 2"));
        assert!(out.contains("ovg_http_request_duration_seconds_bucket{le=\"+Inf\"} 3"));
        assert!(out.contains("ovg_process_resident_memory_bytes"));
        for forbidden in ["organization=", "project=", "actor=", "path=", "route="] {
            assert!(!out.contains(forbidden));
        }
    }
}
