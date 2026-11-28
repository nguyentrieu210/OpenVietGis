//! Bounded private MVT cache. A fresh storage authorization proof precedes
//! every lookup, including the retry after waiting for another request.
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::body::Bytes;
use ovg_storage::catalog::TileCacheScope;
use tokio::sync::watch;
use uuid::Uuid;

const MAX_ENTRIES: usize = 64;
const MAX_BYTES: usize = 32 * 1024 * 1024;
const MAX_FLIGHTS: usize = 64;
const TTL: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TileCacheKey {
    organization_id: Uuid,
    project_id: Uuid,
    actor_id: Uuid,
    layer_id: Uuid,
    source_id: Uuid,
    content_revision: String,
    schema_revision: String,
    z: u32,
    x: u32,
    y: u32,
}

impl TileCacheKey {
    pub fn authorized(
        scope: TileCacheScope,
        content_revision: &str,
        schema_revision: &str,
        z: u32,
        x: u32,
        y: u32,
    ) -> Self {
        Self {
            organization_id: scope.organization_id(),
            project_id: scope.project_id(),
            actor_id: scope.actor_id(),
            layer_id: scope.layer_id(),
            source_id: scope.source_id(),
            content_revision: content_revision.to_owned(),
            schema_revision: schema_revision.to_owned(),
            z,
            x,
            y,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TileCacheValue {
    pub body: Bytes,
    pub complete: bool,
}

struct Entry {
    value: TileCacheValue,
    expires: Instant,
    last_used: u64,
}

#[derive(Default)]
struct Inner {
    entries: HashMap<TileCacheKey, Entry>,
    flights: HashMap<TileCacheKey, watch::Sender<bool>>,
    bytes: usize,
    sequence: u64,
    hits: u64,
    misses: u64,
    waits: u64,
    bypasses: u64,
    evictions: u64,
    abandoned: u64,
}

#[derive(Clone)]
pub struct TileCache {
    inner: Arc<Mutex<Inner>>,
    max_entries: usize,
    max_bytes: usize,
    max_flights: usize,
    ttl: Duration,
}

pub enum TileCacheLookup {
    Hit(TileCacheValue),
    Leader(TileCacheLease),
    Wait(watch::Receiver<bool>),
    Bypass,
}

pub struct TileCacheLease {
    cache: TileCache,
    key: TileCacheKey,
    completed: bool,
}

impl Default for TileCache {
    fn default() -> Self {
        Self::new()
    }
}

impl TileCache {
    #[must_use]
    pub fn new() -> Self {
        Self::with_limits(MAX_ENTRIES, MAX_BYTES, MAX_FLIGHTS, TTL)
    }

    fn with_limits(
        max_entries: usize,
        max_bytes: usize,
        max_flights: usize,
        ttl: Duration,
    ) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner::default())),
            max_entries: max_entries.max(1),
            max_bytes: max_bytes.max(1),
            max_flights: max_flights.max(1),
            ttl,
        }
    }

    fn remove(inner: &mut Inner, key: &TileCacheKey) {
        if let Some(entry) = inner.entries.remove(key) {
            inner.bytes -= entry.value.body.len();
            inner.evictions += 1;
        }
    }

    pub fn lookup_or_reserve(&self, key: TileCacheKey) -> TileCacheLookup {
        let mut inner = self.inner.lock().expect("tile cache mutex poisoned");
        let now = Instant::now();
        let expired: Vec<_> = inner
            .entries
            .iter()
            .filter_map(|(key, entry)| (entry.expires <= now).then_some(key.clone()))
            .collect();
        for key in expired {
            Self::remove(&mut inner, &key);
        }
        inner.sequence = inner.sequence.wrapping_add(1);
        let sequence = inner.sequence;
        if let Some(entry) = inner.entries.get_mut(&key) {
            entry.last_used = sequence;
            let value = entry.value.clone();
            inner.hits += 1;
            return TileCacheLookup::Hit(value);
        }
        if let Some(sender) = inner.flights.get(&key) {
            // Subscribe while holding the same lock as completion. watch retains
            // completion even when it arrives before the receiver is polled.
            let wait = sender.subscribe();
            inner.waits += 1;
            return TileCacheLookup::Wait(wait);
        }
        if inner.flights.len() >= self.max_flights {
            inner.bypasses += 1;
            return TileCacheLookup::Bypass;
        }
        let (sender, _) = watch::channel(false);
        inner.flights.insert(key.clone(), sender);
        inner.misses += 1;
        TileCacheLookup::Leader(TileCacheLease {
            cache: self.clone(),
            key,
            completed: false,
        })
    }

    fn complete(&self, key: &TileCacheKey, value: Option<TileCacheValue>) {
        let mut inner = self.inner.lock().expect("tile cache mutex poisoned");
        if let Some(value) = value {
            let bytes = value.body.len();
            if bytes <= self.max_bytes {
                Self::remove(&mut inner, key);
                inner.sequence = inner.sequence.wrapping_add(1);
                let last_used = inner.sequence;
                inner.bytes += bytes;
                inner.entries.insert(
                    key.clone(),
                    Entry {
                        value,
                        expires: Instant::now() + self.ttl,
                        last_used,
                    },
                );
                while inner.entries.len() > self.max_entries || inner.bytes > self.max_bytes {
                    let oldest = inner
                        .entries
                        .iter()
                        .min_by_key(|(_, e)| e.last_used)
                        .map(|(k, _)| k.clone())
                        .expect("nonempty over-budget cache");
                    Self::remove(&mut inner, &oldest);
                }
            }
        } else {
            inner.abandoned += 1;
        }
        // Removing the last sender also wakes receivers on cancellation/error.
        if let Some(sender) = inner.flights.remove(key) {
            let _ = sender.send(true);
        }
    }

    #[must_use]
    pub fn render_prometheus(&self) -> String {
        let inner = self.inner.lock().expect("tile cache mutex poisoned");
        let mut out = String::new();
        for (name, kind, value) in [
            ("hits_total", "counter", inner.hits),
            ("misses_total", "counter", inner.misses),
            ("waits_total", "counter", inner.waits),
            ("bypasses_total", "counter", inner.bypasses),
            ("evictions_total", "counter", inner.evictions),
            ("abandoned_total", "counter", inner.abandoned),
            ("entries", "gauge", inner.entries.len() as u64),
            ("bytes", "gauge", inner.bytes as u64),
            ("flights", "gauge", inner.flights.len() as u64),
            ("capacity_bytes", "gauge", self.max_bytes as u64),
        ] {
            out.push_str(&format!(
                "# TYPE ovg_tile_cache_{name} {kind}\novg_tile_cache_{name} {value}\n"
            ));
        }
        out
    }

    #[cfg(test)]
    fn usage(&self) -> (usize, usize, usize) {
        let inner = self.inner.lock().unwrap();
        (inner.entries.len(), inner.bytes, inner.flights.len())
    }
}

impl TileCacheLease {
    pub fn finish(mut self, body: Vec<u8>, complete: bool) -> TileCacheValue {
        let value = TileCacheValue {
            body: body.into(),
            complete,
        };
        self.cache.complete(&self.key, Some(value.clone()));
        self.completed = true;
        value
    }
}

impl Drop for TileCacheLease {
    fn drop(&mut self) {
        if !self.completed {
            self.cache.complete(&self.key, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(n: u32) -> TileCacheKey {
        TileCacheKey {
            organization_id: Uuid::nil(),
            project_id: Uuid::nil(),
            actor_id: Uuid::nil(),
            layer_id: Uuid::nil(),
            source_id: Uuid::nil(),
            content_revision: "a".repeat(64),
            schema_revision: "b".repeat(64),
            z: 4,
            x: n,
            y: 0,
        }
    }

    fn leader(cache: &TileCache, key: TileCacheKey) -> TileCacheLease {
        match cache.lookup_or_reserve(key) {
            TileCacheLookup::Leader(v) => v,
            _ => panic!("leader expected"),
        }
    }

    #[tokio::test]
    async fn delayed_poll_observes_completion_and_shares_bytes() {
        let cache = TileCache::with_limits(4, 1024, 4, TTL);
        let lead = leader(&cache, key(1));
        let mut wait = match cache.lookup_or_reserve(key(1)) {
            TileCacheLookup::Wait(v) => v,
            _ => panic!("waiter expected"),
        };
        let value = lead.finish(vec![1, 2, 3], true);
        tokio::time::timeout(Duration::from_secs(1), wait.changed())
            .await
            .unwrap()
            .unwrap();
        match cache.lookup_or_reserve(key(1)) {
            TileCacheLookup::Hit(hit) => {
                assert_eq!(hit.body.as_ref(), &[1, 2, 3]);
                assert_eq!(hit.body.as_ptr(), value.body.as_ptr());
            }
            _ => panic!("cache hit expected"),
        }
        assert_eq!(cache.usage(), (1, 3, 0));
    }

    #[tokio::test]
    async fn cancelled_leader_releases_all_waiters_and_slot() {
        let cache = TileCache::with_limits(2, 64, 1, TTL);
        let lead = leader(&cache, key(1));
        let mut waiters = Vec::new();
        for _ in 0..32 {
            match cache.lookup_or_reserve(key(1)) {
                TileCacheLookup::Wait(v) => waiters.push(v),
                _ => panic!("waiter expected"),
            }
        }
        drop(lead);
        for mut wait in waiters {
            let _ = tokio::time::timeout(Duration::from_secs(1), wait.changed())
                .await
                .unwrap();
        }
        assert_eq!(cache.usage(), (0, 0, 0));
        drop(leader(&cache, key(1)));
    }

    #[test]
    fn bytes_entries_flights_and_oversized_values_are_bounded() {
        let cache = TileCache::with_limits(2, 7, 1, TTL);
        let lead = leader(&cache, key(1));
        assert!(matches!(
            cache.lookup_or_reserve(key(2)),
            TileCacheLookup::Bypass
        ));
        lead.finish(vec![0; 4], true);
        for n in 2..=4 {
            leader(&cache, key(n)).finish(vec![0; 4], false);
        }
        assert_eq!(cache.usage(), (1, 4, 0));
        leader(&cache, key(5)).finish(vec![0; 8], true);
        assert_eq!(cache.usage(), (1, 4, 0));
        let cache = TileCache::with_limits(2, 1024, 2, TTL);
        for n in 0..10 {
            leader(&cache, key(n)).finish(vec![0], true);
        }
        assert_eq!(cache.usage(), (2, 2, 0));
    }

    #[test]
    fn identity_source_revision_and_address_are_isolated() {
        let cache = TileCache::new();
        let original = key(1);
        leader(&cache, original.clone()).finish(vec![7], true);
        let mut keys = Vec::new();
        let mut other = original.clone();
        other.organization_id = Uuid::from_u128(1);
        keys.push(other);
        let mut other = original.clone();
        other.project_id = Uuid::from_u128(1);
        keys.push(other);
        let mut other = original.clone();
        other.actor_id = Uuid::from_u128(1);
        keys.push(other);
        let mut other = original.clone();
        other.layer_id = Uuid::from_u128(1);
        keys.push(other);
        let mut other = original.clone();
        other.source_id = Uuid::from_u128(1);
        keys.push(other);
        let mut other = original.clone();
        other.content_revision = "c".repeat(64);
        keys.push(other);
        let mut other = original.clone();
        other.schema_revision = "c".repeat(64);
        keys.push(other);
        let mut other = original.clone();
        other.z += 1;
        keys.push(other);
        let mut other = original.clone();
        other.x += 1;
        keys.push(other);
        let mut other = original.clone();
        other.y += 1;
        keys.push(other);
        for key in keys {
            drop(leader(&cache, key));
        }
    }

    #[test]
    fn expiry_and_metrics_have_no_scope_labels_or_identifiers() {
        let cache = TileCache::with_limits(2, 64, 2, Duration::ZERO);
        leader(&cache, key(1)).finish(vec![1], true);
        drop(leader(&cache, key(1)));
        let text = cache.render_prometheus();
        assert!(text.contains("ovg_tile_cache_evictions_total 1"));
        assert!(text.contains("ovg_tile_cache_abandoned_total 1"));
        assert!(!text.contains('{'));
        assert!(!text.contains(&Uuid::nil().to_string()));
    }
}
