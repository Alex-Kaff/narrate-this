use async_trait::async_trait;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CacheCategory {
    Narration,
    Tts,
    Media,
}

#[async_trait]
pub trait CacheProvider: Send + Sync {
    async fn get(&self, category: CacheCategory, key: &str) -> Option<String>;
    async fn set(&self, category: CacheCategory, key: &str, value: &str);
}
