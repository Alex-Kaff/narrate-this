use async_trait::async_trait;

use crate::error::Result;

#[async_trait]
pub trait ContentProvider: Send + Sync {
    async fn extract_narration(&self, url: &str, title_hint: &str) -> Result<Option<String>>;
    async fn search_and_narrate(&self, query: &str) -> Result<Option<String>>;
}
