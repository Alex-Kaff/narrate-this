use async_trait::async_trait;

use crate::error::Result;

#[async_trait]
pub trait TextTransformer: Send + Sync {
    async fn transform(&self, text: &str) -> Result<String>;
}
