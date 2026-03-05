use async_trait::async_trait;

use crate::error::Result;

#[async_trait]
pub trait AudioStorage: Send + Sync {
    async fn store(&self, audio: &[u8]) -> Result<String>;
    async fn read(&self, path: &str) -> Result<Vec<u8>>;
}
