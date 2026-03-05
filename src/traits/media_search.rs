use async_trait::async_trait;

use crate::error::Result;
use crate::types::MediaKind;

pub struct MediaSearchResult {
    pub url: String,
    pub kind: MediaKind,
}

#[async_trait]
pub trait MediaSearchProvider: Send + Sync {
    async fn search(&self, query: &str, count: usize) -> Result<Vec<MediaSearchResult>>;
}
