//! Bounded `JobExecutor` combining `tokio::sync::Semaphore` + dedicated `rayon::ThreadPool`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ovg_core::AppError;
use rayon::{ThreadPool, ThreadPoolBuilder};
use serde_json::Value;
use tokio::sync::{oneshot, Notify, Semaphore};
use uuid::Uuid;

use crate::job::{JobId, JobRecord, JobStatus};

/// Lightweight cooperative cancellation token shared between async orchestrator and CPU worker.
#[derive(Debug, Clone, Default)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
    notify: Arc<Notify>,
}

impl CancellationToken {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        if !self.cancelled.swap(true, Ordering::SeqCst) {
            self.notify.notify_waiters();
        }
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    pub async fn cancelled(&self) {
        if self.is_cancelled() {
            return;
        }
        self.notify.notified().await;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecutorConfig {
    pub max_concurrent_jobs: usize,
    pub max_queue_depth: usize,
    pub worker_threads: usize,
    pub default_timeout: Duration,
}

impl Default for ExecutorConfig {
    fn default() -> Self {
        Self {
            max_concurrent_jobs: 2,
            max_queue_depth: 16,
            worker_threads: 2,
            default_timeout: Duration::from_secs(60),
        }
    }
}

struct ExecutorInner {
    semaphore: Arc<Semaphore>,
    pool: ThreadPool,
    max_queue_depth: usize,
    queued_and_running: AtomicUsize,
    default_timeout: Duration,
    records: Mutex<HashMap<JobId, JobRecord>>,
    tokens: Mutex<HashMap<JobId, CancellationToken>>,
}

/// Strictly bounded CPU/async job executor.
#[derive(Clone)]
pub struct JobExecutor {
    inner: Arc<ExecutorInner>,
}

impl JobExecutor {
    pub fn new(config: ExecutorConfig) -> Result<Self, AppError> {
        let max_concurrent = config.max_concurrent_jobs.max(1);
        let worker_threads = config.worker_threads.max(1);
        let max_queue_depth = config.max_queue_depth.max(max_concurrent);

        let pool = ThreadPoolBuilder::new()
            .num_threads(worker_threads)
            .thread_name(|i| format!("ovg-proc-{i}"))
            .build()
            .map_err(|err| AppError::internal(format!("failed to build rayon pool: {err}")))?;

        Ok(Self {
            inner: Arc::new(ExecutorInner {
                semaphore: Arc::new(Semaphore::new(max_concurrent)),
                pool,
                max_queue_depth,
                queued_and_running: AtomicUsize::new(0),
                default_timeout: config.default_timeout,
                records: Mutex::new(HashMap::new()),
                tokens: Mutex::new(HashMap::new()),
            }),
        })
    }

    #[must_use]
    pub fn in_flight_count(&self) -> usize {
        self.inner.queued_and_running.load(Ordering::SeqCst)
    }

    #[must_use]
    pub fn get_job(&self, id: JobId) -> Option<JobRecord> {
        self.inner
            .records
            .lock()
            .ok()
            .and_then(|map| map.get(&id).cloned())
    }

    /// Request cancellation of a queued or running job.
    pub fn cancel_job(&self, id: JobId) -> bool {
        if let Ok(tokens) = self.inner.tokens.lock() {
            if let Some(tok) = tokens.get(&id) {
                tok.cancel();
                return true;
            }
        }
        false
    }

    /// Submit a bounded CPU job onto the executor with queue admission control,
    /// concurrency gating, timeout, and cooperative cancellation.
    pub fn submit<F>(
        &self,
        organization_id: Uuid,
        job_type: impl Into<String>,
        timeout_override: Option<Duration>,
        work: F,
    ) -> Result<(JobId, tokio::task::JoinHandle<JobRecord>), AppError>
    where
        F: FnOnce(CancellationToken) -> Result<Value, String> + Send + 'static,
    {
        // Enforce hard queue depth bound before spawning anything.
        let prev = self.inner.queued_and_running.fetch_add(1, Ordering::SeqCst);
        if prev >= self.inner.max_queue_depth {
            self.inner.queued_and_running.fetch_sub(1, Ordering::SeqCst);
            return Err(AppError::rate_limited(Some(1)));
        }

        let record = JobRecord::new_queued(organization_id, job_type);
        let job_id = record.id;
        let token = CancellationToken::new();

        {
            let mut records = self.inner.records.lock().expect("records mutex poisoned");
            records.insert(job_id, record);
        }
        {
            let mut tokens = self.inner.tokens.lock().expect("tokens mutex poisoned");
            tokens.insert(job_id, token.clone());
        }

        let inner = Arc::clone(&self.inner);
        let timeout = timeout_override.unwrap_or(self.inner.default_timeout);

        let handle = tokio::spawn(async move {
            let result_record =
                run_bounded_job(Arc::clone(&inner), job_id, token, timeout, work).await;
            inner.queued_and_running.fetch_sub(1, Ordering::SeqCst);
            if let Ok(mut tokens) = inner.tokens.lock() {
                tokens.remove(&job_id);
            }
            result_record
        });

        Ok((job_id, handle))
    }
}

async fn run_bounded_job<F>(
    inner: Arc<ExecutorInner>,
    job_id: JobId,
    token: CancellationToken,
    timeout: Duration,
    work: F,
) -> JobRecord
where
    F: FnOnce(CancellationToken) -> Result<Value, String> + Send + 'static,
{
    // Wait for a concurrency slot OR early cancellation while queued.
    let permit = tokio::select! {
        biased;
        () = token.cancelled() => {
            return finalize_record(&inner, job_id, JobStatus::Cancelled, None, Some("cancelled while queued".to_string()));
        }
        acquired = Arc::clone(&inner.semaphore).acquire_owned() => {
            match acquired {
                Ok(p) => p,
                Err(_) => {
                    return finalize_record(&inner, job_id, JobStatus::Failed, None, Some("executor closed".to_string()));
                }
            }
        }
    };

    if token.is_cancelled() {
        drop(permit);
        return finalize_record(
            &inner,
            job_id,
            JobStatus::Cancelled,
            None,
            Some("cancelled before start".to_string()),
        );
    }

    update_status(&inner, job_id, JobStatus::Running);

    let (tx, rx) = oneshot::channel();
    let work_token = token.clone();
    inner.pool.spawn(move || {
        let outcome = work(work_token);
        let _ = tx.send(outcome);
    });

    let final_record = match tokio::time::timeout(timeout, rx).await {
        Ok(Ok(Ok(value))) => {
            if token.is_cancelled() {
                finalize_record(
                    &inner,
                    job_id,
                    JobStatus::Cancelled,
                    None,
                    Some("cancelled during execution".to_string()),
                )
            } else {
                finalize_record(&inner, job_id, JobStatus::Succeeded, Some(value), None)
            }
        }
        Ok(Ok(Err(err_msg))) => {
            if token.is_cancelled() {
                finalize_record(&inner, job_id, JobStatus::Cancelled, None, Some(err_msg))
            } else {
                finalize_record(&inner, job_id, JobStatus::Failed, None, Some(err_msg))
            }
        }
        Ok(Err(_)) => finalize_record(
            &inner,
            job_id,
            JobStatus::Failed,
            None,
            Some("worker terminated unexpectedly".to_string()),
        ),
        Err(_) => {
            token.cancel();
            finalize_record(
                &inner,
                job_id,
                JobStatus::Failed,
                None,
                Some("job timed out".to_string()),
            )
        }
    };

    drop(permit);
    final_record
}

fn update_status(inner: &ExecutorInner, job_id: JobId, status: JobStatus) {
    if let Ok(mut records) = inner.records.lock() {
        if let Some(rec) = records.get_mut(&job_id) {
            rec.transition(status);
        }
    }
}

fn finalize_record(
    inner: &ExecutorInner,
    job_id: JobId,
    status: JobStatus,
    result: Option<Value>,
    error: Option<String>,
) -> JobRecord {
    let mut records = inner.records.lock().expect("records mutex poisoned");
    let rec = records
        .get_mut(&job_id)
        .expect("job record must exist while executing");
    rec.transition(status);
    rec.result = result;
    rec.error = error;
    rec.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn enforces_strict_concurrency_bound() {
        let executor = JobExecutor::new(ExecutorConfig {
            max_concurrent_jobs: 2,
            max_queue_depth: 8,
            worker_threads: 2,
            default_timeout: Duration::from_secs(5),
        })
        .unwrap();

        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let org = Uuid::new_v4();

        let mut handles = Vec::new();
        for i in 0..6 {
            let a = Arc::clone(&active);
            let p = Arc::clone(&peak);
            let (_, handle) = executor
                .submit(org, format!("batch-{i}"), None, move |_tok| {
                    let cur = a.fetch_add(1, Ordering::SeqCst) + 1;
                    p.fetch_max(cur, Ordering::SeqCst);
                    std::thread::sleep(Duration::from_millis(25));
                    a.fetch_sub(1, Ordering::SeqCst);
                    Ok(json!({ "idx": i }))
                })
                .unwrap();
            handles.push(handle);
        }

        for h in handles {
            let rec = h.await.unwrap();
            assert_eq!(rec.status, JobStatus::Succeeded);
        }
        assert!(peak.load(Ordering::SeqCst) <= 2);
        assert_eq!(executor.in_flight_count(), 0);
    }

    #[tokio::test]
    async fn supports_cancellation_and_queue_bounds() {
        let executor = JobExecutor::new(ExecutorConfig {
            max_concurrent_jobs: 1,
            max_queue_depth: 2,
            worker_threads: 1,
            default_timeout: Duration::from_secs(5),
        })
        .unwrap();

        let org = Uuid::new_v4();
        let (_, h1) = executor
            .submit(org, "blocker", None, |tok| {
                for _ in 0..20 {
                    if tok.is_cancelled() {
                        return Err("cancelled".to_string());
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                Ok(json!({ "done": true }))
            })
            .unwrap();

        let (id2, h2) = executor
            .submit(org, "queued-cancel", None, |_tok| Ok(json!({ "ok": true })))
            .unwrap();

        // Queue depth is 2 -> 3rd job must be rejected with 429
        let err = executor
            .submit(org, "overflow", None, |_tok| Ok(json!({})))
            .unwrap_err();
        assert_eq!(err.status_code(), 429);

        // Cancel the queued 2nd job immediately
        assert!(executor.cancel_job(id2));
        let rec2 = h2.await.unwrap();
        assert_eq!(rec2.status, JobStatus::Cancelled);

        let rec1 = h1.await.unwrap();
        assert_eq!(rec1.status, JobStatus::Succeeded);
    }

    #[tokio::test]
    async fn fails_timed_out_job() {
        let executor = JobExecutor::new(ExecutorConfig {
            max_concurrent_jobs: 1,
            max_queue_depth: 2,
            worker_threads: 1,
            default_timeout: Duration::from_millis(30),
        })
        .unwrap();

        let (_, h) = executor
            .submit(Uuid::new_v4(), "slow", None, |tok| {
                for _ in 0..50 {
                    if tok.is_cancelled() {
                        return Err("cancelled".to_string());
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                Ok(json!({}))
            })
            .unwrap();

        let rec = h.await.unwrap();
        assert_eq!(rec.status, JobStatus::Failed);
        assert_eq!(rec.error.as_deref(), Some("job timed out"));
    }
}
