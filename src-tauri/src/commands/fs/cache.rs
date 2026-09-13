//! Bounded, memory-only presentation caches. Never consulted by mutation checks.
use super::dir::DirListing;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use ts_rs::TS;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Key {
    pub store: String,
    pub identity: String,
    pub path: String,
    pub options: String,
}
struct Entry<T> {
    value: Arc<T>,
    bytes: usize,
    inserted: Instant,
    used: u64,
}
#[derive(Clone, Debug, Default, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct CacheMetrics {
    pub entries: u32,
    #[ts(type = "number")]
    pub bytes: u64,
    #[ts(type = "number")]
    pub hits: u64,
    #[ts(type = "number")]
    pub misses: u64,
    #[ts(type = "number")]
    pub evictions: u64,
    #[ts(type = "number")]
    pub invalidations: u64,
}
pub struct Cache<T> {
    entries: HashMap<Key, Entry<T>>,
    bytes: usize,
    maximum_bytes: usize,
    maximum_entries: usize,
    per_store_bytes: usize,
    lifetime: Duration,
    epoch: u64,
    mutations: u32,
    sequence: u64,
    metrics: CacheMetrics,
}
impl<T> Cache<T> {
    pub fn new(bytes: usize, entries: usize, per_store: usize, lifetime: Duration) -> Self {
        Self {
            entries: HashMap::new(),
            bytes: 0,
            maximum_bytes: bytes,
            maximum_entries: entries,
            per_store_bytes: per_store,
            lifetime,
            epoch: 0,
            mutations: 0,
            sequence: 0,
            metrics: CacheMetrics::default(),
        }
    }
    fn remove(&mut self, key: &Key) {
        if let Some(entry) = self.entries.remove(key) {
            self.bytes -= entry.bytes;
        }
    }
    pub fn ticket(&self) -> u64 {
        self.epoch
    }
    pub fn get(&mut self, key: &Key, now: Instant) -> Option<Arc<T>> {
        self.prune(now);
        if self.mutations > 0 {
            self.metrics.misses += 1;
            return None;
        }
        self.sequence = self.sequence.saturating_add(1);
        if let Some(entry) = self.entries.get_mut(key) {
            entry.used = self.sequence;
            self.metrics.hits += 1;
            return Some(entry.value.clone());
        }
        self.metrics.misses += 1;
        None
    }
    fn prune(&mut self, now: Instant) {
        let expired: Vec<_> = self
            .entries
            .iter()
            .filter(|(_, v)| now.saturating_duration_since(v.inserted) >= self.lifetime)
            .map(|(k, _)| k.clone())
            .collect();
        for key in expired {
            self.remove(&key);
            self.metrics.evictions += 1;
        }
    }
    /// False means an invalidation won the race. Oversized live reads are still
    /// returned to the caller, but never displace all other cached directories.
    pub fn put(
        &mut self,
        key: Key,
        value: Arc<T>,
        bytes: usize,
        ticket: u64,
        now: Instant,
    ) -> bool {
        if ticket != self.epoch {
            return false;
        }
        if self.mutations > 0 {
            return true;
        }
        self.prune(now);
        if bytes > self.per_store_bytes || bytes > self.maximum_bytes || self.maximum_entries == 0 {
            return true;
        }
        self.remove(&key);
        while self
            .entries
            .iter()
            .filter(|(k, _)| k.store == key.store)
            .map(|(_, v)| v.bytes)
            .sum::<usize>()
            + bytes
            > self.per_store_bytes
            || self.entries.keys().filter(|k| k.store == key.store).count()
                >= (self.maximum_entries / 4).max(1)
        {
            let oldest = self
                .entries
                .iter()
                .filter(|(k, _)| k.store == key.store)
                .min_by_key(|(_, v)| v.used)
                .map(|(k, _)| k.clone());
            if let Some(oldest) = oldest {
                self.remove(&oldest);
                self.metrics.evictions += 1;
            } else {
                break;
            }
        }
        while self.bytes + bytes > self.maximum_bytes || self.entries.len() >= self.maximum_entries
        {
            let oldest = self
                .entries
                .iter()
                .min_by_key(|(_, v)| v.used)
                .map(|(k, _)| k.clone());
            if let Some(oldest) = oldest {
                self.remove(&oldest);
                self.metrics.evictions += 1;
            } else {
                break;
            }
        }
        self.sequence = self.sequence.saturating_add(1);
        self.bytes += bytes;
        self.entries.insert(
            key,
            Entry {
                value,
                bytes,
                inserted: now,
                used: self.sequence,
            },
        );
        true
    }
    pub fn clear(&mut self) {
        self.entries.clear();
        self.bytes = 0;
        self.epoch = self.epoch.wrapping_add(1);
        self.metrics.invalidations += 1;
    }
    pub fn stats(&mut self) -> CacheMetrics {
        self.prune(Instant::now());
        CacheMetrics {
            entries: self.entries.len() as u32,
            bytes: self.bytes as u64,
            ..self.metrics.clone()
        }
    }
}
pub fn listings() -> &'static Mutex<Cache<DirListing>> {
    static CACHE: OnceLock<Mutex<Cache<DirListing>>> = OnceLock::new();
    CACHE.get_or_init(|| {
        Mutex::new(Cache::new(
            32 * 1024 * 1024,
            256,
            8 * 1024 * 1024,
            Duration::from_secs(2),
        ))
    })
}
fn metadata() -> &'static Mutex<Cache<std::fs::Metadata>> {
    static CACHE: OnceLock<Mutex<Cache<std::fs::Metadata>>> = OnceLock::new();
    CACHE.get_or_init(|| {
        Mutex::new(Cache::new(
            2 * 1024 * 1024,
            4096,
            512 * 1024,
            Duration::from_secs(1),
        ))
    })
}
pub fn invalidate() {
    listings().lock().unwrap_or_else(|e| e.into_inner()).clear();
    metadata().lock().unwrap_or_else(|e| e.into_inner()).clear();
}
/// Disable cache insertion while an application mutation is running. Drop also
/// invalidates failed/partial operations: they can still have changed files.
pub struct Mutation;
impl Mutation {
    pub fn begin() -> Self {
        let mut listings = listings().lock().unwrap_or_else(|e| e.into_inner());
        listings.clear();
        listings.mutations += 1;
        let mut metadata = metadata().lock().unwrap_or_else(|e| e.into_inner());
        metadata.clear();
        metadata.mutations += 1;
        Self
    }
}
impl Drop for Mutation {
    fn drop(&mut self) {
        let mut listings = listings().lock().unwrap_or_else(|e| e.into_inner());
        listings.clear();
        listings.mutations = listings.mutations.saturating_sub(1);
        let mut metadata = metadata().lock().unwrap_or_else(|e| e.into_inner());
        metadata.clear();
        metadata.mutations = metadata.mutations.saturating_sub(1);
    }
}
pub fn display_metadata(path: &std::path::Path) -> std::io::Result<std::fs::Metadata> {
    let parent = path.parent().unwrap_or(path).canonicalize()?;
    let canonical = path
        .file_name()
        .map_or(parent.clone(), |name| parent.join(name));
    let key = Key {
        store: "local".into(),
        identity: format!(
            "local-properties-v1:{:?}",
            super::transfer::recovery::durability::object_identity(&parent)?
        ),
        path: canonical.to_string_lossy().into_owned(),
        options: "lstat".into(),
    };
    let ticket;
    {
        let mut cache = metadata().lock().unwrap_or_else(|e| e.into_inner());
        if let Some(value) = cache.get(&key, Instant::now()) {
            return Ok((*value).clone());
        }
        ticket = cache.ticket();
    }
    let result = std::fs::symlink_metadata(path)?;
    let weight = 512 + key.path.len();
    if !metadata().lock().unwrap_or_else(|e| e.into_inner()).put(
        key,
        Arc::new(result.clone()),
        weight,
        ticket,
        Instant::now(),
    ) {
        return std::fs::symlink_metadata(path);
    }
    Ok(result)
}
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct CacheReport {
    pub listings: CacheMetrics,
    pub metadata: CacheMetrics,
}
#[tauri::command]
pub fn cache_metrics() -> CacheReport {
    CacheReport {
        listings: listings().lock().unwrap_or_else(|e| e.into_inner()).stats(),
        metadata: metadata().lock().unwrap_or_else(|e| e.into_inner()).stats(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn key(store: &str, path: &str) -> Key {
        Key {
            store: store.into(),
            identity: "generation-1".into(),
            path: path.into(),
            options: "v1".into(),
        }
    }
    #[test]
    fn hits_expiry_invalidation_races_and_generations_are_isolated() {
        let mut cache = Cache::new(100, 10, 50, Duration::from_secs(2));
        let now = Instant::now();
        let key = key("a", "/x");
        let ticket = cache.ticket();
        assert!(cache.put(key.clone(), Arc::new(1), 10, ticket, now));
        assert_eq!(*cache.get(&key, now).unwrap(), 1);
        let mut other = key.clone();
        other.identity = "generation-2".into();
        assert!(cache.get(&other, now).is_none());
        assert!(cache.get(&key, now + Duration::from_secs(3)).is_none());
        cache.clear();
        assert!(!cache.put(key.clone(), Arc::new(2), 10, ticket, now));
        assert!(cache.get(&key, now).is_none());
    }
    #[test]
    fn a_huge_directory_and_one_busy_store_cannot_evict_every_other_store() {
        let mut cache = Cache::new(100, 10, 50, Duration::from_secs(2));
        let now = Instant::now();
        cache.put(key("other", "/keep"), Arc::new(1), 40, 0, now);
        for i in 0..100 {
            cache.put(key("busy", &i.to_string()), Arc::new(i), 20, 0, now);
        }
        cache.put(key("huge", "/too-large"), Arc::new(2), 1000, 0, now);
        assert!(cache.get(&key("other", "/keep"), now).is_some());
        assert!(cache.stats().bytes <= 100);
        assert!(cache.stats().entries <= 10);
    }
    #[test]
    fn principal_capabilities_and_metrics_are_isolated() {
        let mut cache = Cache::new(1000, 16, 250, Duration::from_secs(2));
        let now = Instant::now();
        let original = key("private-host:user", "/private-canary");
        cache.put(original.clone(), Arc::new(42), 100, 0, now);
        for changed in [
            Key {
                store: "other-user".into(),
                ..original.clone()
            },
            Key {
                options: "changed-trust".into(),
                ..original.clone()
            },
        ] {
            assert!(cache.get(&changed, now).is_none());
        }
        cache.mutations = 1;
        assert!(cache.get(&original, now).is_none());
        cache.put(key("new", "/during-mutation"), Arc::new(5), 10, 0, now);
        assert_eq!(cache.stats().entries, 1);
        let metrics = serde_json::to_string(&cache.stats()).unwrap();
        for secret in [
            "private-host",
            "private-canary",
            "other-user",
            "changed-trust",
        ] {
            assert!(!metrics.contains(secret));
        }
    }
}
