use async_trait::async_trait;

use crate::error::Result;
use crate::types::MediaKind;

/// A single result from a media search.
pub struct MediaSearchResult {
    /// URL of the media asset.
    pub url: String,
    /// Whether this is an image or video.
    pub kind: MediaKind,
}

/// Searches for stock images or videos matching a keyword query.
///
/// The built-in implementation is [`PexelsSearch`](crate::PexelsSearch).
#[async_trait]
pub trait MediaSearchProvider: Send + Sync {
    /// Search for up to `count` media assets matching `query`.
    async fn search(&self, query: &str, count: usize) -> Result<Vec<MediaSearchResult>>;
}
