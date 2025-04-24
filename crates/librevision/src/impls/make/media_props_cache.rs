use conflux::{InputHashRef, MediaProps};
use eyre::Context;
use merde::IntoStatic;
use redb::{ReadableTable, TableDefinition};
use std::sync::{atomic::AtomicUsize, Arc, Mutex};

/// Whenever the revision schema change we don't necessarily want to recompute the properties of all
/// the media — grabbing metadata from mp4s or draw.io files is especially resource intensive.
pub struct MediaPropsCache {
    // note: Maybe we could store the table instead of storing the transaction 🤷
    pub wtx: Mutex<redb::WriteTransaction>,
    pub stats: Arc<CacheStats>,
}

const MEDIA_PROPS_CACHE_TABLE: redb::TableDefinition<&str, &str> =
    TableDefinition::new("media_props_cache_v1");

impl MediaPropsCache {
    pub fn new(wtx: redb::WriteTransaction) -> Self {
        MediaPropsCache {
            wtx: Mutex::new(wtx),
            stats: Arc::new(CacheStats::new()),
        }
    }

    pub fn get(&self, hash: &InputHashRef) -> eyre::Result<Option<MediaProps>> {
        let wtx = self.wtx.lock().unwrap();

        self.stats
            .lookups
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let table = wtx
            .open_table(MEDIA_PROPS_CACHE_TABLE)
            .wrap_err("opening media props table for reading")?;
        if let Some(value) = table
            .get(hash.as_str())
            .wrap_err("getting media props from cache")?
        {
            self.stats
                .hits
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let props: MediaProps =
                merde::json::from_str(value.value()).map_err(|e| e.into_static())?;
            return Ok(Some(props));
        };
        Ok(None)
    }

    pub fn insert(&self, hash: &InputHashRef, props: &MediaProps) -> eyre::Result<()> {
        let wtx = self.wtx.lock().unwrap();
        let mut table = wtx
            .open_table(MEDIA_PROPS_CACHE_TABLE)
            .wrap_err("opening media props table for writing")?;
        table
            .insert(hash.as_str(), merde::json::to_string(props)?.as_str())
            .wrap_err("putting media props into cache")?;
        Ok(())
    }

    pub async fn get_or_insert_with<F>(&self, hash: &InputHashRef, f: F) -> eyre::Result<MediaProps>
    where
        F: AsyncFnOnce() -> eyre::Result<MediaProps>,
    {
        if let Some(props) = self.get(hash)? {
            return Ok(props);
        }
        let props = f().await?;
        self.insert(hash, &props)?;
        Ok(props)
    }

    pub fn commit(self) -> eyre::Result<()> {
        let wtx = self.wtx.into_inner().unwrap();
        wtx.commit().wrap_err("committing media props cache")
    }

    pub fn get_stats(&self) -> Arc<CacheStats> {
        self.stats.clone()
    }
}

pub struct CacheStats {
    pub lookups: AtomicUsize,
    pub hits: AtomicUsize,
}

impl CacheStats {
    fn new() -> Self {
        CacheStats {
            lookups: AtomicUsize::new(0),
            hits: AtomicUsize::new(0),
        }
    }

    fn hit_rate(&self) -> f64 {
        let lookups = self.lookups.load(std::sync::atomic::Ordering::Relaxed);
        let hits = self.hits.load(std::sync::atomic::Ordering::Relaxed);
        if lookups == 0 {
            0.0
        } else {
            hits as f64 / lookups as f64
        }
    }
}

impl std::fmt::Display for CacheStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let lookups = self.lookups.load(std::sync::atomic::Ordering::Relaxed);
        let hit_rate = self.hit_rate() * 100.0;
        write!(f, "Lookups: {lookups}, Hit rate: {hit_rate:.2}%")
    }
}
