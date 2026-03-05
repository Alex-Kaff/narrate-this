use async_trait::async_trait;

use crate::error::Result;
use crate::types::KeywordResult;

#[async_trait]
pub trait KeywordExtractor: Send + Sync {
    async fn extract_keywords(&self, text: &str) -> Result<KeywordResult>;
}
