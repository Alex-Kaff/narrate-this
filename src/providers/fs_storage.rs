use std::path::PathBuf;

use async_trait::async_trait;

use crate::error::{Result, SdkError};
use crate::traits::AudioStorage;
use crate::util;

pub struct FsAudioStorage {
    base_dir: PathBuf,
}

impl FsAudioStorage {
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
        }
    }
}

#[async_trait]
impl AudioStorage for FsAudioStorage {
    /// Store audio bytes on disk. Returns a relative path like "a3/a3b1c2...mp3".
    /// Content-addressed: identical audio bytes produce the same path (dedup).
    /// Skips writing if the file already exists.
    async fn store(&self, audio: &[u8]) -> Result<String> {
        let hash = util::content_hash(audio);
        let subdir = &hash[..2];
        let filename = format!("{hash}.mp3");
        let rel_path = format!("{subdir}/{filename}");
        let full_path = self.base_dir.join(subdir).join(&filename);

        if full_path.exists() {
            return Ok(rel_path);
        }

        tokio::fs::create_dir_all(self.base_dir.join(subdir))
            .await
            .map_err(|e| SdkError::AudioStorage(format!("create dir failed: {e}")))?;

        // Atomic write: write to temp file, then rename
        let tmp = full_path.with_extension("tmp");
        tokio::fs::write(&tmp, audio)
            .await
            .map_err(|e| SdkError::AudioStorage(format!("write tmp failed: {e}")))?;
        tokio::fs::rename(&tmp, &full_path)
            .await
            .map_err(|e| SdkError::AudioStorage(format!("rename failed: {e}")))?;

        Ok(rel_path)
    }

    /// Read audio bytes from a stored relative path.
    async fn read(&self, path: &str) -> Result<Vec<u8>> {
        let full_path = self.base_dir.join(path);
        tokio::fs::read(&full_path)
            .await
            .map_err(|e| SdkError::AudioStorage(format!("read failed: {e}")))
    }
}
