//! Bounded, process-local cache for revision-pinned viewport JSON.
//!
//! Authorization and source revision validation happen in ProjectRead before
//! this cache is queried. Values are private to the organization/actor/project
//! and are eligible only for non-GIS-scoped reads.
use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::sync::{Arc, Mutex as StdMutex, MutexGuard};
use tokio::sync::Mutex as AsyncMutex;
use uuid::Uuid;

use ovg_core::AppError;

const DEFAULT_MAX_BYTES: usize = 32 * 1024 * 1024;
const DEFAULT_MAX_ORGANIZATION_BYTES: usize = 8 * 1024 * 1024;
const DEFAULT_MAX_ENTRY_BYTES: usize = 2 * 1024 * 1024;
const DEFAULT_MAX_ENTRIES: usize = 128;
const DEFAULT_MAX_IN_FLIGHT: usize = 128;
const DEFAULT_MAX_ORGANIZATION_IN_FLIGHT: usize = 16;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ViewportCacheKey {
    pub organization_id: Uuid,
    pub actor_id: Uuid,
    pub project_id: Uuid,
    pub layer_id: Uuid,
    pub content_revision: String,
    pub schema_revision: String,
    pub query_fingerprint: [u8; 32],
}

#[derive(Clone, Copy)]
struct Limits {
    max_bytes: usize,
    max_organization_bytes: usize,
    max_entry_bytes: usize,
    max_entries: usize,
    max_in_flight: usize,
    max_organization_in_flight: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_bytes: DEFAULT_MAX_BYTES,
            max_organization_bytes: DEFAULT_MAX_ORGANIZATION_BYTES,
            max_entry_bytes: DEFAULT_MAX_ENTRY_BYTES,
            max_entries: DEFAULT_MAX_ENTRIES,
            max_in_flight: DEFAULT_MAX_IN_FLIGHT,
            max_organization_in_flight: DEFAULT_MAX_ORGANIZATION_IN_FLIGHT,
        }
    }
}

struct CacheState {
    entries: HashMap<ViewportCacheKey, Arc<[u8]>>,
    lru: VecDeque<ViewportCacheKey>,
    in_flight: HashMap<ViewportCacheKey, Arc<AsyncMutex<()>>>,
    bytes: usize,
    organization_bytes: HashMap<Uuid, usize>,
}

impl Default for CacheState {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
            lru: VecDeque::new(),
            in_flight: HashMap::new(),
            bytes: 0,
            organization_bytes: HashMap::new(),
        }
    }
}

#[derive(Clone)]
pub struct ViewportResponseCache {
    state: Arc<StdMutex<CacheState>>,
    limits: Limits,
}

impl Default for ViewportResponseCache {
    fn default() -> Self {
        Self::new()
    }
}

impl ViewportResponseCache {
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: Arc::new(StdMutex::new(CacheState::default())),
            limits: Limits::default(),
        }
    }

    /// Return a cached body or run the builder once for concurrent requests
    /// with the same key. Errors and oversized values are never cached.
    pub(crate) async fn get_or_try_init<F, Fut>(
        &self,
        key: ViewportCacheKey,
        build: F,
    ) -> Result<Vec<u8>, AppError>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<Vec<u8>, AppError>>,
    {
        if let Some(body) = self.get(&key) {
            return Ok(body);
        }

        let Some(flight) = self.flight(&key) else {
            // The in-flight table is bounded. Under unusually high key
            // cardinality, bypass coalescing rather than grow memory without
            // limit.
            return build().await;
        };

        // Keep the flight registered while any request is waiting on or
        // holding its mutex. This also lets a waiter take over if the leader
        // is cancelled without creating a second mutex for the same key.
        let cleanup = FlightCleanup {
            state: Arc::clone(&self.state),
            key: key.clone(),
            flight,
        };
        let guard = cleanup.flight.lock().await;
        if let Some(body) = self.get(&key) {
            drop(guard);
            drop(cleanup);
            return Ok(body);
        }

        let result = build().await;
        if let Ok(body) = &result {
            self.insert(&key, body);
        }

        drop(guard);
        drop(cleanup);
        result
    }

    /// Drop entries for a principal after an authorization failure.
    pub fn invalidate_actor(&self, organization_id: Uuid, actor_id: Uuid) {
        let mut state = self.lock();
        let keys = state
            .entries
            .keys()
            .filter(|key| key.organization_id == organization_id && key.actor_id == actor_id)
            .cloned()
            .collect::<Vec<_>>();
        for key in keys {
            state.remove(&key);
        }
    }

    fn get(&self, key: &ViewportCacheKey) -> Option<Vec<u8>> {
        let mut state = self.lock();
        let body = state.entries.get(key).cloned()?;
        state.touch(key);
        Some(body.as_ref().to_vec())
    }

    fn flight(&self, key: &ViewportCacheKey) -> Option<Arc<AsyncMutex<()>>> {
        let mut state = self.lock();
        if let Some(flight) = state.in_flight.get(key) {
            return Some(Arc::clone(flight));
        }

        let organization_flights = state
            .in_flight
            .keys()
            .filter(|pending| pending.organization_id == key.organization_id)
            .count();
        if state.in_flight.len() >= self.limits.max_in_flight
            || organization_flights >= self.limits.max_organization_in_flight
        {
            return None;
        }

        let flight = Arc::new(AsyncMutex::new(()));
        state.in_flight.insert(key.clone(), Arc::clone(&flight));
        Some(flight)
    }

    fn insert(&self, key: &ViewportCacheKey, body: &[u8]) {
        if body.is_empty()
            || body.len() > self.limits.max_entry_bytes
            || body.len() > self.limits.max_organization_bytes
            || body.len() > self.limits.max_bytes
            || self.limits.max_entries == 0
        {
            return;
        }

        let mut state = self.lock();
        state.remove(key);

        while state.bytes + body.len() > self.limits.max_bytes
            || state
                .organization_bytes
                .get(&key.organization_id)
                .copied()
                .unwrap_or(0)
                + body.len()
                > self.limits.max_organization_bytes
            || state.entries.len() >= self.limits.max_entries
        {
            let over_organization_budget = state
                .organization_bytes
                .get(&key.organization_id)
                .copied()
                .unwrap_or(0)
                + body.len()
                > self.limits.max_organization_bytes;
            let index = if over_organization_budget {
                state
                    .lru
                    .iter()
                    .position(|candidate| candidate.organization_id == key.organization_id)
            } else if state.lru.is_empty() {
                None
            } else {
                Some(0)
            };
            let Some(index) = index else {
                return;
            };
            if let Some(evicted) = state.lru.remove(index) {
                state.remove_entry(&evicted);
            }
        }

        state.entries.insert(key.clone(), Arc::from(body));
        state.lru.push_back(key.clone());
        state.bytes += body.len();
        *state
            .organization_bytes
            .entry(key.organization_id)
            .or_default() += body.len();
    }

    fn lock(&self) -> MutexGuard<'_, CacheState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl CacheState {
    fn touch(&mut self, key: &ViewportCacheKey) {
        if let Some(index) = self.lru.iter().position(|candidate| candidate == key) {
            self.lru.remove(index);
            self.lru.push_back(key.clone());
        }
    }

    fn remove(&mut self, key: &ViewportCacheKey) {
        if let Some(index) = self.lru.iter().position(|candidate| candidate == key) {
            self.lru.remove(index);
        }
        self.remove_entry(key);
    }

    fn remove_entry(&mut self, key: &ViewportCacheKey) {
        let Some(body) = self.entries.remove(key) else {
            return;
        };
        let size = body.len();
        self.bytes = self.bytes.saturating_sub(size);
        if let Some(bytes) = self.organization_bytes.get_mut(&key.organization_id) {
            *bytes = bytes.saturating_sub(size);
            if *bytes == 0 {
                self.organization_bytes.remove(&key.organization_id);
            }
        }
    }
}

struct FlightCleanup {
    state: Arc<StdMutex<CacheState>>,
    key: ViewportCacheKey,
    flight: Arc<AsyncMutex<()>>,
}

impl Drop for FlightCleanup {
    fn drop(&mut self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.in_flight.get(&self.key).is_some_and(|current| {
            Arc::ptr_eq(current, &self.flight) && Arc::strong_count(&self.flight) == 2
        }) {
            // The map and this cleanup lease are the only remaining owners.
            // If a waiter still owns the flight, it will clean up after it
            // observes the cached result or finishes building one.
            state.in_flight.remove(&self.key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;
    use tokio::sync::Barrier;

    fn key(organization_id: Uuid, actor_id: Uuid, layer_id: Uuid) -> ViewportCacheKey {
        ViewportCacheKey {
            organization_id,
            actor_id,
            project_id: Uuid::from_u128(10),
            layer_id,
            content_revision: "a".repeat(64),
            schema_revision: "b".repeat(64),
            query_fingerprint: [3; 32],
        }
    }

    #[tokio::test]
    async fn coalesces_concurrent_requests_for_one_key() {
        let cache = Arc::new(ViewportResponseCache::new());
        let org = Uuid::from_u128(1);
        let actor = Uuid::from_u128(2);
        let layer = Uuid::from_u128(3);
        let calls = Arc::new(AtomicUsize::new(0));
        let barrier = Arc::new(Barrier::new(12));
        let mut tasks = Vec::new();

        for _ in 0..12 {
            let cache = Arc::clone(&cache);
            let calls = Arc::clone(&calls);
            let barrier = Arc::clone(&barrier);
            let key = key(org, actor, layer);
            tasks.push(tokio::spawn(async move {
                barrier.wait().await;
                cache
                    .get_or_try_init(key, || async {
                        calls.fetch_add(1, Ordering::SeqCst);
                        tokio::time::sleep(Duration::from_millis(20)).await;
                        Ok(b"bounded viewport".to_vec())
                    })
                    .await
            }));
        }

        for task in tasks {
            assert_eq!(task.await.unwrap().unwrap(), b"bounded viewport");
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn a_waiter_takes_over_when_the_singleflight_leader_is_cancelled() {
        let cache = Arc::new(ViewportResponseCache::new());
        let cache_key = key(Uuid::from_u128(1), Uuid::from_u128(2), Uuid::from_u128(3));
        let calls = Arc::new(AtomicUsize::new(0));
        let started = Arc::new(tokio::sync::Notify::new());

        let leader = {
            let cache = Arc::clone(&cache);
            let calls = Arc::clone(&calls);
            let started = Arc::clone(&started);
            let cache_key = cache_key.clone();
            tokio::spawn(async move {
                cache
                    .get_or_try_init(cache_key, || async move {
                        calls.fetch_add(1, Ordering::SeqCst);
                        started.notify_one();
                        std::future::pending::<()>().await;
                        Ok(b"unreachable".to_vec())
                    })
                    .await
            })
        };
        started.notified().await;

        let waiter = {
            let cache = Arc::clone(&cache);
            let calls = Arc::clone(&calls);
            let cache_key = cache_key.clone();
            tokio::spawn(async move {
                cache
                    .get_or_try_init(cache_key, || async move {
                        calls.fetch_add(1, Ordering::SeqCst);
                        Ok(b"recovered".to_vec())
                    })
                    .await
            })
        };

        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                let state = cache.lock();
                let owners = Arc::strong_count(state.in_flight.get(&cache_key).unwrap());
                drop(state);
                if owners >= 3 {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("waiter should join the leader's flight");

        leader.abort();
        assert!(leader.await.unwrap_err().is_cancelled());
        assert_eq!(waiter.await.unwrap().unwrap(), b"recovered");
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert!(cache.lock().in_flight.is_empty());
    }

    #[test]
    fn actor_and_organization_are_part_of_the_cache_key() {
        let key_a = key(Uuid::from_u128(1), Uuid::from_u128(2), Uuid::from_u128(3));
        let different_actor = key(Uuid::from_u128(1), Uuid::from_u128(4), Uuid::from_u128(3));
        let different_org = key(Uuid::from_u128(5), Uuid::from_u128(2), Uuid::from_u128(3));
        let mut different_revision = key_a.clone();
        different_revision.content_revision = "c".repeat(64);
        let mut different_query = key_a.clone();
        different_query.query_fingerprint = [4; 32];
        assert_ne!(key_a, different_actor);
        assert_ne!(key_a, different_org);
        assert_ne!(key_a, different_revision);
        assert_ne!(key_a, different_query);
    }

    #[test]
    fn actor_invalidation_removes_cached_bodies() {
        let cache = ViewportResponseCache::new();
        let org = Uuid::from_u128(1);
        let actor = Uuid::from_u128(2);
        let key = key(org, actor, Uuid::from_u128(3));
        cache.insert(&key, b"private");
        assert_eq!(cache.get(&key).as_deref(), Some(b"private".as_slice()));
        cache.invalidate_actor(org, actor);
        assert!(cache.get(&key).is_none());
    }

    #[test]
    fn cache_evicts_lru_when_global_or_org_budget_is_reached() {
        let cache = ViewportResponseCache {
            state: Arc::new(StdMutex::new(CacheState::default())),
            limits: Limits {
                max_bytes: 10,
                max_organization_bytes: 6,
                max_entry_bytes: 6,
                max_entries: 3,
                max_in_flight: 3,
                max_organization_in_flight: 2,
            },
        };
        let org_a = Uuid::from_u128(1);
        let org_b = Uuid::from_u128(2);
        let actor = Uuid::from_u128(9);
        let a1 = key(org_a, actor, Uuid::from_u128(11));
        let a2 = key(org_a, actor, Uuid::from_u128(12));
        let b1 = key(org_b, actor, Uuid::from_u128(13));

        cache.insert(&a1, b"1234");
        cache.insert(&b1, b"5678");
        assert!(cache.get(&a1).is_some());
        cache.insert(&a2, b"abcd"); // per-organization cap evicts a1.
        assert!(cache.get(&a1).is_none());
        assert!(cache.get(&b1).is_some());
        assert!(cache.get(&a2).is_some());

        cache.insert(&b1, b"123456"); // replacement stays within the global byte cap.
        let c1 = key(Uuid::from_u128(3), actor, Uuid::from_u128(14));
        cache.insert(&c1, b"7890"); // global pressure evicts the least-recent entry.
        assert!(cache.get(&a2).is_none());
        assert!(cache.get(&b1).is_some());
        assert!(cache.get(&c1).is_some());
        let state = cache.lock();
        assert!(state.bytes <= 10);
        assert!(state
            .organization_bytes
            .values()
            .all(|bytes| *bytes <= cache.limits.max_organization_bytes));
        assert!(state.entries.len() <= cache.limits.max_entries);
    }

    #[tokio::test]
    async fn does_not_cache_errors() {
        let cache = ViewportResponseCache::new();
        let key = key(Uuid::from_u128(1), Uuid::from_u128(2), Uuid::from_u128(3));
        let error = cache
            .get_or_try_init(key.clone(), || async {
                Err(AppError::internal("expected test failure"))
            })
            .await;
        assert!(error.is_err());
        assert!(cache.get(&key).is_none());
    }

    #[test]
    fn rejects_entries_larger_than_the_cap() {
        let cache = ViewportResponseCache::new();
        let key = key(Uuid::from_u128(1), Uuid::from_u128(2), Uuid::from_u128(3));
        let body = vec![0; DEFAULT_MAX_ENTRY_BYTES + 1];
        cache.insert(&key, &body);
        assert!(cache.get(&key).is_none());
    }
}
