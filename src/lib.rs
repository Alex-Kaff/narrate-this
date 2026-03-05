//! # narrate-this
//!
//! A Rust SDK that turns text, URLs, or search queries into narrated videos —
//! complete with TTS, captions, and stock visuals.
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use narrate_this::{
//!     ContentPipeline, ContentSource, ElevenLabsConfig, ElevenLabsTts,
//!     FfmpegRenderer, FirecrawlScraper, FsAudioStorage, OpenAiConfig,
//!     OpenAiKeywords, PexelsSearch, RenderConfig,
//! };
//!
//! # async fn example() -> narrate_this::Result<()> {
//! let pipeline = ContentPipeline::builder()
//!     .content(FirecrawlScraper::new("http://localhost:3002"))
//!     .tts(ElevenLabsTts::new(ElevenLabsConfig {
//!         api_key: "your-key".into(),
//!         ..Default::default()
//!     }))
//!     .media(
//!         OpenAiKeywords::new(OpenAiConfig {
//!             api_key: "your-key".into(),
//!             ..Default::default()
//!         }),
//!         PexelsSearch::new("your-key"),
//!     )
//!     .renderer(FfmpegRenderer::new(), RenderConfig::default())
//!     .audio_storage(FsAudioStorage::new("./output"))
//!     .build()?;
//!
//! let output = pipeline
//!     .process(ContentSource::ArticleUrl {
//!         url: "https://example.com/article".into(),
//!         title: Some("My Article".into()),
//!     })
//!     .await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Pipeline stages
//!
//! ```text
//! Content Source → Narration → Text Transforms → TTS → Media Search → Audio Storage → Video Render
//! ```
//!
//! Only TTS is required. Everything else is optional — skip content sourcing if
//! you pass raw text, skip media search if you just want audio, skip rendering
//! if you don't need video.
//!
//! ## Custom providers
//!
//! Swap any stage by implementing the matching trait: [`TtsProvider`],
//! [`ContentProvider`], [`KeywordExtractor`], [`MediaSearchProvider`],
//! [`TextTransformer`], [`AudioStorage`], [`CacheProvider`], or [`VideoRenderer`].

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
