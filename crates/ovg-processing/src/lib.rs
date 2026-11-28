//! Bounded CPU & async job execution engine with strict concurrency, queue, timeout,
//! and cancellation guarantees.

pub mod executor;
pub mod job;
pub mod registry;
pub mod photogrammetry;

pub use executor::{CancellationToken, ExecutorConfig, JobExecutor};
pub use job::{JobId, JobRecord, JobStatus};
