use async_trait::async_trait;

use crate::error::Result;

/// Transforms narration text before TTS synthesis.
///
/// Multiple transformers can be chained via
/// [`PipelineBuilder::text_transform`](crate::PipelineBuilder::text_transform)
/// and are applied in order.
///
/// The built-in implementation is [`OpenAiTransform`](crate::OpenAiTransform).
#[async_trait]
pub trait TextTransformer: Send + Sync {
    /// Rewrite or modify the narration text.
    async fn transform(&self, text: &str) -> Result<String>;
}
