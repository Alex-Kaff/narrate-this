use async_trait::async_trait;

/// The type of data being cached.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CacheCategory {
    /// Narration text generated from a content source.
    Narration,
    /// TTS audio and caption data.
    Tts,
    /// Media search results.
    Media,
}

/// Key-value cache for pipeline results, keyed by content hash.
///
/// Caching avoids redundant API calls for identical content. The built-in
/// implementation is `PgCache` (feature-gated behind `pg-cache`).
#[async_trait]
pub trait CacheProvider: Send + Sync {
    /// Look up a cached value. Returns `None` on cache miss.
    async fn get(&self, category: CacheCategory, key: &str) -> Option<String>;

    /// Store a value in the cache.
    async fn set(&self, category: CacheCategory, key: &str, value: &str);
}
