use async_trait::async_trait;

use crate::error::Result;
use crate::types::KeywordResult;

/// Extracts visual search keywords from narration text for media lookup.
///
/// The built-in implementation is [`OpenAiKeywords`](crate::OpenAiKeywords).
#[async_trait]
pub trait KeywordExtractor: Send + Sync {
    /// Extract keywords from a text chunk that can be used to find relevant stock media.
    async fn extract_keywords(&self, text: &str) -> Result<KeywordResult>;
}
