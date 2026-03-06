use std::marker::PhantomData;

use crate::error::{Result, SdkError};
use crate::pipeline::ContentPipeline;
use crate::traits::{
    AudioStorage, CacheProvider, ContentProvider, MediaPlanner, RenderConfig, TextTransformer,
    TtsProvider, VideoRenderer,
};

// ── Typestate markers ──

pub struct Init;
pub struct HasContent;
pub struct HasTts;

// ── Builder ──

/// Builder for [`ContentPipeline`](crate::ContentPipeline).
///
/// Uses a type-state pattern to enforce that a TTS provider is set before
/// building. Start with [`ContentPipeline::builder()`](crate::ContentPipeline::builder).
///
/// # States
///
/// - **Init** — starting state. Call `.content()` or `.tts()`.
/// - **HasContent** — content provider set. Call `.text_transform()` (chainable) then `.tts()`.
/// - **HasTts** — TTS set. Optionally configure `.media()`, `.renderer()`,
///   `.cache()`, `.audio_storage()`, then `.build()`.
pub struct PipelineBuilder<State = Init> {
    inner: PipelineBuilderInner,
    _state: PhantomData<State>,
}

struct PipelineBuilderInner {
    tts: Option<Box<dyn TtsProvider>>,
    content: Option<Box<dyn ContentProvider>>,
    text_transforms: Vec<Box<dyn TextTransformer>>,
    media_planner: Option<Box<dyn MediaPlanner>>,
    audio_storage: Option<Box<dyn AudioStorage>>,
    cache: Option<Box<dyn CacheProvider>>,
    video_renderer: Option<Box<dyn VideoRenderer>>,
    render_config: Option<RenderConfig>,
}

impl PipelineBuilderInner {
    fn new() -> Self {
        Self {
            tts: None,
            content: None,
            text_transforms: Vec::new(),
            media_planner: None,
            audio_storage: None,
            cache: None,
            video_renderer: None,
            render_config: None,
        }
    }
}

fn transition<From, To>(builder: PipelineBuilder<From>) -> PipelineBuilder<To> {
    PipelineBuilder {
        inner: builder.inner,
        _state: PhantomData,
    }
}

// ── Init state ──

impl PipelineBuilder<Init> {
    pub fn new() -> Self {
        Self {
            inner: PipelineBuilderInner::new(),
            _state: PhantomData,
        }
    }

    /// Set the content provider (web scraper). Transitions to HasContent.
    pub fn content(mut self, provider: impl ContentProvider + 'static) -> PipelineBuilder<HasContent> {
        self.inner.content = Some(Box::new(provider));
        transition(self)
    }

    /// Set the TTS provider directly (skip content for text-only pipelines). Transitions to HasTts.
    pub fn tts(mut self, provider: impl TtsProvider + 'static) -> PipelineBuilder<HasTts> {
        self.inner.tts = Some(Box::new(provider));
        transition(self)
    }
}

impl Default for PipelineBuilder<Init> {
    fn default() -> Self {
        Self::new()
    }
}

// ── HasContent state ──

impl PipelineBuilder<HasContent> {
    /// Add a text transformation step. Can be chained multiple times. Stays in HasContent.
    pub fn text_transform(mut self, t: impl TextTransformer + 'static) -> PipelineBuilder<HasContent> {
        self.inner.text_transforms.push(Box::new(t));
        self
    }

    /// Set the TTS provider. Transitions to HasTts.
    pub fn tts(mut self, provider: impl TtsProvider + 'static) -> PipelineBuilder<HasTts> {
        self.inner.tts = Some(Box::new(provider));
        transition(self)
    }
}

// ── HasTts state ──

impl PipelineBuilder<HasTts> {
    /// Set the media planner that provides visuals for narration chunks.
    ///
    /// Use [`StockMediaPlanner`](crate::StockMediaPlanner) for stock search only,
    /// or [`LlmMediaPlanner`](crate::LlmMediaPlanner) for AI-based asset matching
    /// with optional stock fallback.
    pub fn media(mut self, planner: impl MediaPlanner + 'static) -> Self {
        self.inner.media_planner = Some(Box::new(planner));
        self
    }

    /// Set the video renderer and its configuration.
    pub fn renderer(mut self, renderer: impl VideoRenderer + 'static, config: RenderConfig) -> Self {
        self.inner.video_renderer = Some(Box::new(renderer));
        self.inner.render_config = Some(config);
        self
    }

    /// Set a cache provider.
    pub fn cache(mut self, provider: impl CacheProvider + 'static) -> Self {
        self.inner.cache = Some(Box::new(provider));
        self
    }

    /// Set the audio storage provider.
    pub fn audio_storage(mut self, provider: impl AudioStorage + 'static) -> Self {
        self.inner.audio_storage = Some(Box::new(provider));
        self
    }

    /// Build the pipeline. TTS is always set at this point (enforced by typestate).
    pub fn build(self) -> Result<ContentPipeline> {
        let tts = self
            .inner
            .tts
            .ok_or_else(|| SdkError::Config("TTS provider is required".into()))?;

        Ok(ContentPipeline {
            tts,
            content: self.inner.content,
            text_transforms: self.inner.text_transforms,
            media_planner: self.inner.media_planner,
            audio_storage: self.inner.audio_storage,
            cache: self.inner.cache,
            video_renderer: self.inner.video_renderer,
            render_config: self.inner.render_config,
        })
    }
}
