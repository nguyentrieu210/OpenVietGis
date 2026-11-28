//! Shared-core native runtime. No server, tenant credentials or network globals.
use ovg_spatial::{Coord2D, PortableTransformKernel, TransformBackend};
use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex};
use uuid::Uuid;
pub mod batch;
pub mod measure;
pub mod viewport;
pub mod staging;
pub mod pmtiles;
pub mod crs;
pub use ovg_core::{CrsProfile, Feature, Revision};
pub use ovg_processing::registry::{algorithm_registry, AlgorithmId};
pub use ovg_sync::field::{validate_offline_ops, PackageLifecycle};

pub const MAX_ROWS: usize = 4096;
pub const MAX_BYTES: usize = 4 * 1024 * 1024;
const MAX_SESSIONS: usize = 8;
const MAX_WORKSPACES: usize = 8;
const MAX_JOBS: usize = 16;
const MAX_RUNNING: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Session(Uuid);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Workspace { session: Session, generation: Uuid }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Job { workspace: Workspace, generation: Uuid }

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("STALE_HANDLE")] Stale,
    #[error("RESOURCE_LIMIT")] Limit,
    #[error("INVALID_BATCH")] InvalidBatch,
    #[error("CANCELLED")] Cancelled,
    #[error("SESSION_FAILED")] Failed,
    #[error("SPATIAL_ERROR: {0}")] Spatial(String),
    #[error("JOB_PENDING")] Pending,
}
#[derive(Debug, Clone, PartialEq)]
pub enum JobStatus { Pending, Finished(Result<Vec<f64>, Error>) }

struct Record { cancel: Arc<AtomicBool>, result: Option<Result<Vec<f64>, Error>> }
#[derive(Default)]
struct State {
    sessions: HashMap<Session, Vec<Workspace>>,
    jobs: HashMap<Job, Record>,
    running: usize,
}
#[derive(Clone, Default)]
pub struct Engine { state: Arc<Mutex<State>> }
impl Engine {
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, State>, Error> {
        self.state.lock().map_err(|_| Error::Failed)
    }
    pub fn open_session(&self) -> Result<Session, Error> {
        let mut s = self.lock()?;
        if s.sessions.len() >= MAX_SESSIONS { return Err(Error::Limit); }
        let id = Session(Uuid::new_v4());
        s.sessions.insert(id, vec![]);
        Ok(id)
    }
    pub fn open_workspace(&self, session: Session) -> Result<Workspace, Error> {
        let mut s = self.lock()?;
        let workspaces = s.sessions.get_mut(&session).ok_or(Error::Stale)?;
        if workspaces.len() >= MAX_WORKSPACES { return Err(Error::Limit); }
        let h = Workspace { session, generation: Uuid::new_v4() };
        workspaces.push(h);
        Ok(h)
    }
    pub fn close_workspace(&self, workspace: Workspace) -> Result<(), Error> {
        let mut s = self.lock()?;
        if let Some(v) = s.sessions.get_mut(&workspace.session) {
            v.retain(|w| *w != workspace);
        }
        s.jobs.retain(|j, r| {
            if j.workspace == workspace { r.cancel.store(true, Ordering::Release); false } else { true }
        });
        Ok(())
    }
    pub fn close_session(&self, session: Session) -> Result<(), Error> {
        let mut s = self.lock()?;
        s.sessions.remove(&session);
        s.jobs.retain(|j, r| {
            if j.workspace.session == session { r.cancel.store(true, Ordering::Release); false } else { true }
        });
        Ok(())
    }
    pub fn start_transform(&self, workspace: Workspace, source: u32, target: u32, xy: Vec<f64>) -> Result<Job, Error> {
        if xy.is_empty() || xy.len() % 2 != 0 || xy.len() / 2 > MAX_ROWS { return Err(Error::InvalidBatch); }
        self.start(workspace, move |cancel| {
            let kernel = PortableTransformKernel::new();
            let mut out = Vec::with_capacity(xy.len());
            for p in xy.chunks_exact(2) {
                if cancel.load(Ordering::Acquire) { return Err(Error::Cancelled); }
                let v = kernel.transform_point(source, target, Coord2D::new(p[0], p[1]))
                    .map_err(|e| Error::Spatial(e.to_string()))?;
                out.extend([v.x, v.y]);
            }
            Ok(out)
        })
    }
    fn start<F>(&self, workspace: Workspace, f: F) -> Result<Job, Error>
    where F: FnOnce(&AtomicBool) -> Result<Vec<f64>, Error> + Send + 'static {
        let mut s = self.lock()?;
        if !s.sessions.get(&workspace.session).is_some_and(|v| v.contains(&workspace)) { return Err(Error::Stale); }
        if s.running >= MAX_RUNNING || s.jobs.len() >= MAX_JOBS { return Err(Error::Limit); }
        let id = Job { workspace, generation: Uuid::new_v4() };
        let cancel = Arc::new(AtomicBool::new(false));
        s.jobs.insert(id, Record { cancel: cancel.clone(), result: None });
        s.running += 1;
        let state = Arc::clone(&self.state);
        let spawned = std::thread::Builder::new().name("ovg-mobile-job".into()).spawn(move || {
            let outcome = catch_unwind(AssertUnwindSafe(|| f(&cancel)));
            if let Ok(mut s) = state.lock() {
                s.running -= 1;
                match outcome {
                    Ok(value) => {
                        if let Some(r) = s.jobs.get_mut(&id) {
                            r.result = Some(if cancel.load(Ordering::Acquire) { Err(Error::Cancelled) } else { value });
                        }
                    }
                    Err(_) => {
                        s.sessions.remove(&workspace.session);
                        s.jobs.retain(|j, r| {
                            if j.workspace.session == workspace.session { r.cancel.store(true, Ordering::Release); false } else { true }
                        });
                    }
                }
            }
        });
        if spawned.is_err() { s.jobs.remove(&id); s.running -= 1; return Err(Error::Failed); }
        Ok(id)
    }
    /// Inspect completion without copying result buffers.
    pub fn is_finished(&self, job: Job) -> Result<bool, Error> {
        let s = self.lock()?;
        Ok(s.jobs.get(&job).ok_or(Error::Stale)?.result.is_some())
    }
    /// Ownership of the result transfers exactly once; second take is stale.
    pub fn take_result(&self, job: Job) -> Result<Vec<f64>, Error> {
        let mut s = self.lock()?;
        if s.jobs.get(&job).ok_or(Error::Stale)?.result.is_none() { return Err(Error::Pending); }
        s.jobs.remove(&job).ok_or(Error::Stale)?.result.ok_or(Error::Pending)?
    }
    pub fn cancel(&self, job: Job) -> Result<(), Error> {
        let s = self.lock()?;
        let r = s.jobs.get(&job).ok_or(Error::Stale)?;
        if r.result.is_none() { r.cancel.store(true, Ordering::Release); }
        Ok(())
    }
    pub fn dispose_job(&self, job: Job) -> Result<(), Error> {
        if let Some(r) = self.lock()?.jobs.remove(&job) { r.cancel.store(true, Ordering::Release); }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};
    fn wait(e: &Engine, j: Job) {
        let start = Instant::now();
        while !e.is_finished(j).unwrap() {
            assert!(start.elapsed() < Duration::from_secs(5));
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    #[test]
    fn real_core_result_taken_once_and_stale_workspace() {
        let e=Engine::default(); let s=e.open_session().unwrap(); let w=e.open_workspace(s).unwrap();
        let j=e.start_transform(w,4326,3857,vec![1.,0.]).unwrap(); wait(&e,j);
        assert!((e.take_result(j).unwrap()[0]-111319.49079327357).abs()<0.001);
        assert_eq!(e.take_result(j),Err(Error::Stale));
        e.close_workspace(w).unwrap(); e.close_workspace(w).unwrap();
        assert_eq!(e.start_transform(w,4326,3857,vec![1.,0.]),Err(Error::Stale));
    }
    #[test]
    fn bounded_cancel_and_close_do_not_reuse_handles() {
        let e=Engine::default(); let s=e.open_session().unwrap(); let w=e.open_workspace(s).unwrap();
        assert_eq!(e.start_transform(w,4326,3857,vec![0.;8194]),Err(Error::InvalidBatch));
        let j=e.start(w, |c| { while !c.load(Ordering::Acquire) { std::thread::yield_now(); } Err(Error::Cancelled) }).unwrap();
        e.cancel(j).unwrap(); wait(&e,j); assert_eq!(e.take_result(j),Err(Error::Cancelled));
        let j=e.start(w, |c| { while !c.load(Ordering::Acquire) { std::thread::yield_now(); } Err(Error::Cancelled) }).unwrap();
        e.close_session(s).unwrap(); assert_eq!(e.is_finished(j),Err(Error::Stale));
        assert_eq!(e.open_workspace(s),Err(Error::Stale));
    }
    #[test]
    fn panic_invalidates_only_affected_session() {
        let e=Engine::default(); let a=e.open_session().unwrap(); let b=e.open_session().unwrap();
        let w=e.open_workspace(a).unwrap(); let j=e.start(w, |_| panic!("test panic")).unwrap();
        let start=Instant::now();
        while e.is_finished(j).is_ok() { assert!(start.elapsed()<Duration::from_secs(5)); std::thread::yield_now(); }
        assert_eq!(e.open_workspace(a),Err(Error::Stale));
        assert!(e.open_workspace(b).is_ok());
    }
}
