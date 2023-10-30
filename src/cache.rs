use std::{
    collections::HashMap,
    future::Future,
    hash::Hash,
    sync::Arc,
    time::{Duration, Instant},
};

use serenity::{model::prelude::*, prelude::*};

use crate::{consts::CACHE_LIFETIME, utils::ChannelMessage};

pub(crate) type MessageCache = MemoryCache<ChannelMessage, Message>;

type CacheMap<TKey, TValue> = HashMap<TKey, Cached<TValue>>;

#[derive(Debug)]
struct Cached<T> {
    pub data: Arc<T>,
    pub timestamp: Instant,
}

impl<T> Cached<T> {
    /// Create a new cache item.
    pub fn new(data: &Arc<T>) -> Self {
        Self { data: Arc::clone(data), timestamp: Instant::now() }
    }

    /// Returns true if the cache entry is older than the defined maximum lifetime.
    pub fn expired(&self, max_lifetime: Duration) -> bool {
        Instant::now() - self.timestamp > max_lifetime
    }
}

#[derive(Debug)]
pub(crate) struct MemoryCache<TKey, TData>
where
    TKey: PartialEq + Eq + Hash + Clone,
{
    storage: Arc<RwLock<CacheMap<TKey, TData>>>,
}

impl<TKey, TData> MemoryCache<TKey, TData>
where
    TKey: PartialEq + Eq + Hash + Clone,
{
    /// Create a new MemoryCache.
    pub fn new() -> Self {
        let storage = Arc::new(RwLock::new(HashMap::new()));
        Self { storage }
    }

    /// Get an entry out of the cache.
    pub async fn get(&self, id: &TKey) -> Option<Arc<TData>> {
        self.storage.read().await.get(id).map(|c| Arc::clone(&c.data))
    }

    /// Remove an entry from the cache.
    pub async fn remove(&self, id: &TKey) -> Option<Arc<TData>> {
        self.storage.write().await.remove(id).map(|c| c.data)
    }

    /// Check if the cache contains an entry with the specified key
    pub async fn contains_key(&self, id: &TKey) -> bool {
        self.storage.read().await.contains_key(id)
    }

    /// Get an entry out of the cache, or use the provided closure to retrieve the data
    /// and then store it immediately in the cache before returning the data.
    pub async fn get_or_else<TErr, F, Fut>(&self, id: &TKey, f: F) -> Result<Arc<TData>, TErr>
    where
        F: Fn() -> Fut,
        Fut: Future<Output = Result<TData, TErr>>,
    {
        match self.get(id).await {
            Some(value) => Ok(value),
            None => Ok(self.store(id.clone(), f().await?).await),
        }
    }

    /// Store an item in the cache. This will overwrite an existing item if its key is already present.
    pub async fn store(&self, key: TKey, value: TData) -> Arc<TData> {
        let mut cache = self.storage.write().await;

        let value = Arc::new(value);
        cache.insert(key, Cached::new(&value));

        value
    }

    /// Remove any expired cache entries.
    pub async fn purge_expired(&self) {
        let mut cache = self.storage.write().await;

        cache.retain(|_, v| !v.expired(CACHE_LIFETIME));
        cache.shrink_to_fit();
    }
}

impl<TKey, TValue> Clone for MemoryCache<TKey, TValue>
where
    TKey: PartialEq + Eq + Hash + Clone,
{
    fn clone(&self) -> Self {
        let storage = Arc::clone(&self.storage);
        Self { storage }
    }
}
