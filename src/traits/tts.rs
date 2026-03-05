use async_trait::async_trait;

use crate::error::Result;
use crate::types::TtsResult;

#[async_trait]
pub trait TtsProvider: Send + Sync {
    async fn synthesize(&self, text: &str) -> Result<TtsResult>;
}
