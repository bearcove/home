use ahash::AHashMap;
use rand::seq::IteratorRandom;
use std::{
    fmt::Display,
    sync::{LazyLock, RwLock, atomic::AtomicUsize},
};

static DUMBCACHE_TRACING_ENABLED: LazyLock<bool> =
    LazyLock::new(|| std::env::var("DUMBCACHE_TRACING").is_ok());

macro_rules! trace {
    ($($arg:tt)*) => {
        if *DUMBCACHE_TRACING_ENABLED {
            eprintln!($($arg)*);
        }
    };
}

pub struct CacheInner {
    data: AHashMap<String, String>,
}

pub struct Cache {
    name: String,
    inner: RwLock<CacheInner>,
    capacity: usize,
    hits: AtomicUsize,
    misses: AtomicUsize,
}

impl Cache {
    pub fn new(name: impl Display, capacity: usize) -> Self {
        Cache {
            name: name.to_string(),
            inner: RwLock::new(CacheInner {
                data: AHashMap::new(),
            }),
            capacity,
            hits: AtomicUsize::new(0),
            misses: AtomicUsize::new(0),
        }
    }

    #[inline(always)]
    pub fn get(&self, key: &str) -> Option<String> {
        self.with(key, |value| value.to_string())
    }

    #[inline(always)]
    pub fn with<R>(&self, key: &str, callback: impl FnOnce(&str) -> R) -> Option<R> {
        let inner = self.inner.read().unwrap();
        let result = inner.data.get(key).map(|value| callback(value));
        if result.is_some() {
            self.hits.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        } else {
            self.misses
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        self.print_stats(&inner);
        result
    }

    fn print_stats(&self, inner: &CacheInner) {
        if *DUMBCACHE_TRACING_ENABLED {
            let hits = self.hits.load(std::sync::atomic::Ordering::Relaxed);
            let misses = self.misses.load(std::sync::atomic::Ordering::Relaxed);
            let total = hits + misses;
            let hit_rate = if total > 0 {
                hits as f64 / total as f64
            } else {
                0.0
            };
            trace!(
                "Cache '{}' stats: Hit rate: {:.2}%, Load: {}/{} ({:.2}% full)",
                self.name,
                hit_rate * 100.0,
                inner.data.len(),
                self.capacity,
                (inner.data.len() as f64 / self.capacity as f64) * 100.0
            );
        }
    }

    pub fn insert(&self, key: String, value: String) {
        let mut inner = self.inner.write().unwrap();

        if inner.data.len() >= self.capacity {
            if inner.data.contains_key(&key) {
                // then we're merely replacing
            } else {
                let key_to_remove = inner.data.keys().cloned().choose(&mut rand::rng());
                if let Some(key) = key_to_remove {
                    inner.data.remove(&key);
                }
            }
        }

        inner.data.insert(key, value);
    }

    pub fn clear(&self) {
        let mut inner = self.inner.write().unwrap();
        inner.data.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_eviction_policy() {
        let cache = Cache::new("test", 3);

        cache.insert("key1".to_string(), "value1".to_string());
        cache.insert("key2".to_string(), "value2".to_string());
        cache.insert("key3".to_string(), "value3".to_string());

        assert_eq!(cache.get("key1"), Some("value1".to_string()));
        assert_eq!(cache.get("key2"), Some("value2".to_string()));
        assert_eq!(cache.get("key3"), Some("value3".to_string()));

        // Insert new item to trigger eviction
        cache.insert("key4".to_string(), "value4".to_string());

        let remaining_keys: HashSet<_> = vec!["key1", "key2", "key3", "key4"]
            .into_iter()
            .filter_map(|k| cache.get(k).map(|_| k))
            .collect();

        assert_eq!(remaining_keys.len(), 3);
    }

    #[test]
    fn test_eviction_randomness() {
        let cache = Cache::new("test", 3);
        let mut evicted_keys = HashSet::new();

        for _ in 0..100 {
            cache.insert("key1".to_string(), "value1".to_string());
            cache.insert("key2".to_string(), "value2".to_string());
            cache.insert("key3".to_string(), "value3".to_string());
            cache.insert("key4".to_string(), "value4".to_string());

            let remaining_keys: HashSet<_> = vec!["key1", "key2", "key3", "key4"]
                .into_iter()
                .filter_map(|k| cache.get(k).map(|_| k))
                .collect();

            assert_eq!(
                remaining_keys.len(),
                3,
                "set of remaining keys: {remaining_keys:?}"
            );

            for key in ["key1", "key2", "key3", "key4"] {
                evicted_keys.insert(key);
            }
        }

        // Ensure that each key has been evicted at least once
        assert_eq!(
            evicted_keys.len(),
            4,
            "set of evicted keys: {evicted_keys:?}"
        );
    }
}
