mod error;
mod types;
pub(crate) mod util;

pub mod traits;
pub mod providers;

mod config;
mod pipeline;

// ── Re-exports: core ──

pub use error::{Result, SdkError};
pub use types::{
    AudioTrack, CaptionSegment, ContentOutput, ContentSource, KeywordResult, MediaKind,
    MediaSegment, NarrationStyle, PipelineProgress, TtsResult,
};

// ── Re-exports: traits ──

pub use traits::{
    AudioStorage, CacheCategory, CacheProvider, ContentProvider, KeywordExtractor,
    MediaSearchProvider, MediaSearchResult, RenderConfig, TextTransformer, TtsProvider,
    VideoRenderer,
};

// ── Re-exports: providers ──

pub use providers::elevenlabs::{ElevenLabsConfig, ElevenLabsTts};
pub use providers::firecrawl::{FirecrawlConfig, FirecrawlScraper};
pub use providers::ffmpeg_renderer::FfmpegRenderer;
pub use providers::fs_storage::FsAudioStorage;
pub use providers::openai::{OpenAiConfig, OpenAiKeywords, OpenAiTransform};
pub use providers::pexels::PexelsSearch;

#[cfg(feature = "pg-cache")]
pub use providers::pg_cache::PgCache;

// ── Re-exports: builder + pipeline ──

pub use config::PipelineBuilder;
pub use pipeline::ContentPipeline;
