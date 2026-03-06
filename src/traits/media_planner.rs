use async_trait::async_trait;

use crate::error::Result;
use crate::types::{MediaKind, MediaSource, TimedChunk};

/// A resolved media item for a narration chunk.
#[derive(Debug, Clone)]
pub struct PlannedMedia {
    /// Source of the media asset.
    pub source: MediaSource,
    /// Whether this is an image or video.
    pub kind: MediaKind,
}

/// Provides media for narration chunks.
///
/// This is the single entry point for all media selection — whether from
/// user-provided assets, stock search, or a combination of both.
///
/// Built-in implementations:
/// - [`StockMediaPlanner`](crate::StockMediaPlanner) — keyword extraction + stock media search
/// - [`LlmMediaPlanner`](crate::LlmMediaPlanner) — AI-based asset matching with optional stock fallback
#[async_trait]
pub trait MediaPlanner: Send + Sync {
    /// Given narration chunks, return media for each one.
    ///
    /// Returns a `Vec` of the same length as `chunks`. Each element is either
    /// `Some(PlannedMedia)` or `None` if no media was found for that chunk.
    async fn plan(&self, chunks: &[TimedChunk]) -> Result<Vec<Option<PlannedMedia>>>;
}
