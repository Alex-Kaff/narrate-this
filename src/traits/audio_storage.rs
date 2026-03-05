use async_trait::async_trait;

use crate::error::Result;

/// Persists synthesized audio to a storage backend.
///
/// The built-in implementation is [`FsAudioStorage`](crate::FsAudioStorage),
/// which writes content-addressed files to a local directory.
#[async_trait]
pub trait AudioStorage: Send + Sync {
    /// Store audio bytes and return a path or identifier for retrieval.
    async fn store(&self, audio: &[u8]) -> Result<String>;

    /// Read previously stored audio bytes by path.
    async fn read(&self, path: &str) -> Result<Vec<u8>>;
}
