use crate::config::PipelineBuilder;
use crate::error::{Result, SdkError};
use crate::traits::{
    AudioStorage, CacheCategory, CacheProvider, ContentProvider, MediaPlanner, RenderConfig,
    TextTransformer, TtsProvider, VideoRenderer,
};
use crate::types::{
    CaptionSegment, ContentOutput, ContentSource, MediaSegment, PipelineProgress, TtsResult,
};
use crate::util;

/// Serialized TTS cache entry.
#[derive(serde::Serialize, serde::Deserialize)]
struct CachedTts {
    audio_base64: String,
    caption_segments: Vec<CaptionSegment>,
}

/// The main pipeline that orchestrates narration, TTS, media, and video rendering.
///
/// Built via [`ContentPipeline::builder()`] which uses a type-state pattern
/// to enforce valid configuration at compile time.
///
/// # Example
///
/// ```rust,no_run
/// use narrate_this::{ContentPipeline, ContentSource, ElevenLabsConfig, ElevenLabsTts};
///
/// # async fn example() -> narrate_this::Result<()> {
/// let pipeline = ContentPipeline::builder()
///     .tts(ElevenLabsTts::new(ElevenLabsConfig {
///         api_key: "your-key".into(),
///         ..Default::default()
///     }))
///     .build()?;
///
/// let output = pipeline.process(ContentSource::Text("Hello world".into())).await?;
/// # Ok(())
/// # }
/// ```
pub struct ContentPipeline {
    pub(crate) tts: Box<dyn TtsProvider>,
    pub(crate) content: Option<Box<dyn ContentProvider>>,
    pub(crate) text_transforms: Vec<Box<dyn TextTransformer>>,
    pub(crate) media_planner: Option<Box<dyn MediaPlanner>>,
    pub(crate) audio_storage: Option<Box<dyn AudioStorage>>,
    pub(crate) cache: Option<Box<dyn CacheProvider>>,
    pub(crate) video_renderer: Option<Box<dyn VideoRenderer>>,
    pub(crate) render_config: Option<RenderConfig>,
}

type ProgressCb<'a> = Option<&'a (dyn Fn(PipelineProgress) + Send + Sync)>;

impl ContentPipeline {
    /// Create a new pipeline builder.
    pub fn builder() -> PipelineBuilder {
        PipelineBuilder::new()
    }

    /// Run the full pipeline: narration -> text transforms -> TTS -> media -> audio storage -> video render.
    pub async fn process(&self, source: ContentSource) -> Result<ContentOutput> {
        self.process_inner(source, None).await
    }

    /// Run the full pipeline with progress callbacks.
    pub async fn process_with_progress(
        &self,
        source: ContentSource,
        callback: impl Fn(PipelineProgress) + Send + Sync,
    ) -> Result<ContentOutput> {
        self.process_inner(source, Some(&callback)).await
    }

    /// Get narration text only (no TTS, no media).
    pub async fn narrate(&self, source: ContentSource) -> Result<String> {
        self.resolve_narration(&source, None).await
    }

    /// TTS synthesis only.
    pub async fn synthesize(&self, text: &str) -> Result<TtsResult> {
        self.tts.synthesize(text).await
    }

    async fn process_inner(
        &self,
        source: ContentSource,
        cb: ProgressCb<'_>,
    ) -> Result<ContentOutput> {
        // ── Narration ──
        let mut narration = self.resolve_narration(&source, cb).await?;

        // ── Text transforms ──
        if !self.text_transforms.is_empty() {
            emit(cb, PipelineProgress::TextTransformStarted);
            for transform in &self.text_transforms {
                narration = transform.transform(&narration).await?;
            }
            emit(cb, PipelineProgress::TextTransformComplete {
                narration_len: narration.len(),
            });
        }

        // ── TTS (with cache) ──
        emit(cb, PipelineProgress::TtsStarted);

        let tts_key = util::content_hash(&narration);
        let (audio, captions) = match self.cache_get(CacheCategory::Tts, &tts_key).await {
            Some(cached) => match serde_json::from_str::<CachedTts>(&cached) {
                Ok(ct) => {
                    let audio = util::b64_decode(&ct.audio_base64).unwrap_or_default();
                    (audio, ct.caption_segments)
                }
                Err(_) => self.synthesize_and_cache(&narration, &tts_key).await?,
            },
            None => self.synthesize_and_cache(&narration, &tts_key).await?,
        };

        emit(cb, PipelineProgress::TtsComplete {
            audio_bytes: audio.len(),
            caption_count: captions.len(),
        });

        // ── Media ──
        let media_segments = self
            .fetch_media_segments(&narration, &captions, cb)
            .await;

        // ── Audio storage ──
        let audio_path = if let Some(storage) = &self.audio_storage {
            emit(cb, PipelineProgress::AudioStorageStarted);
            let path = storage.store(&audio).await?;
            emit(cb, PipelineProgress::AudioStored {
                path: path.clone(),
            });
            Some(path)
        } else {
            None
        };

        let mut output = ContentOutput {
            narration,
            audio,
            captions,
            media_segments,
            audio_path,
            video_path: None,
        };

        // ── Video render ──
        if let Some(renderer) = &self.video_renderer {
            let config = self
                .render_config
                .as_ref()
                .cloned()
                .unwrap_or_default();
            emit(cb, PipelineProgress::RenderStarted);
            let path = renderer.render(&output, &config).await?;
            emit(cb, PipelineProgress::RenderComplete {
                path: path.clone(),
            });
            output.video_path = Some(path);
        }

        Ok(output)
    }

    async fn resolve_narration(
        &self,
        source: &ContentSource,
        cb: ProgressCb<'_>,
    ) -> Result<String> {
        emit(cb, PipelineProgress::NarrationStarted);

        let narration_input = match source {
            ContentSource::Text(t) => t.clone(),
            ContentSource::ArticleUrl { url, title } => {
                url.clone() + title.as_deref().unwrap_or("")
            }
            ContentSource::SearchQuery(q) => q.clone(),
        };
        let narration_key = util::content_hash(&narration_input);

        // Check cache
        if let Some(cached) = self.cache_get(CacheCategory::Narration, &narration_key).await {
            emit(cb, PipelineProgress::NarrationComplete {
                narration_len: cached.len(),
            });
            return Ok(cached);
        }

        let narration = match source {
            ContentSource::Text(text) => text.clone(),
            ContentSource::ArticleUrl { url, title } => {
                if let Some(scraper) = &self.content {
                    let hint = title.as_deref().unwrap_or("");
                    scraper
                        .extract_narration(url, hint)
                        .await?
                        .unwrap_or_else(|| {
                            title.clone().unwrap_or_else(|| url.clone())
                        })
                } else {
                    return Err(SdkError::Config(
                        "content provider required for ArticleUrl source".into(),
                    ));
                }
            }
            ContentSource::SearchQuery(query) => {
                if let Some(scraper) = &self.content {
                    scraper
                        .search_and_narrate(query)
                        .await?
                        .unwrap_or_else(|| query.clone())
                } else {
                    return Err(SdkError::Config(
                        "content provider required for SearchQuery source".into(),
                    ));
                }
            }
        };

        // Cache the result
        self.cache_set(CacheCategory::Narration, &narration_key, &narration)
            .await;

        emit(cb, PipelineProgress::NarrationComplete {
            narration_len: narration.len(),
        });

        Ok(narration)
    }

    async fn synthesize_and_cache(
        &self,
        narration: &str,
        tts_key: &str,
    ) -> Result<(Vec<u8>, Vec<CaptionSegment>)> {
        let result = self.tts.synthesize(narration).await?;

        let cached = CachedTts {
            audio_base64: util::b64_encode(&result.audio),
            caption_segments: result.captions.clone(),
        };
        if let Ok(json) = serde_json::to_string(&cached) {
            self.cache_set(CacheCategory::Tts, tts_key, &json).await;
        }

        Ok((result.audio, result.captions))
    }

    async fn fetch_media_segments(
        &self,
        narration: &str,
        captions: &[CaptionSegment],
        cb: ProgressCb<'_>,
    ) -> Vec<MediaSegment> {
        let planner = match &self.media_planner {
            Some(p) => p,
            None => return vec![],
        };

        if narration.is_empty() || captions.is_empty() {
            return vec![];
        }

        // Check cache
        let media_key = util::content_hash(narration);
        if let Some(cached) = self.cache_get(CacheCategory::Media, &media_key).await
            && let Ok(segments) = serde_json::from_str::<Vec<MediaSegment>>(&cached)
        {
            emit(cb, PipelineProgress::MediaSearchComplete {
                segment_count: segments.len(),
            });
            return segments;
        }

        let chunks = util::split_into_timed_chunks(narration, captions);
        if chunks.is_empty() {
            return vec![];
        }

        emit(cb, PipelineProgress::MediaSearchStarted {
            chunk_count: chunks.len(),
        });

        let planned = match planner.plan(&chunks).await {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(error = %e, "media planning failed");
                return vec![];
            }
        };

        let mut segments: Vec<MediaSegment> = Vec::new();
        for (i, media) in planned.into_iter().enumerate() {
            if i >= chunks.len() {
                break;
            }
            if let Some(m) = media {
                emit(cb, PipelineProgress::MediaSegmentFound {
                    index: i,
                    kind: m.kind,
                });
                segments.push(MediaSegment {
                    source: m.source,
                    start_ms: chunks[i].start_ms,
                    end_ms: chunks[i].end_ms,
                    kind: m.kind,
                });
            }
        }

        // Cache
        if let Ok(json) = serde_json::to_string(&segments) {
            self.cache_set(CacheCategory::Media, &media_key, &json).await;
        }

        emit(cb, PipelineProgress::MediaSearchComplete {
            segment_count: segments.len(),
        });

        segments
    }

    async fn cache_get(&self, category: CacheCategory, key: &str) -> Option<String> {
        if let Some(cache) = &self.cache {
            cache.get(category, key).await
        } else {
            None
        }
    }

    async fn cache_set(&self, category: CacheCategory, key: &str, value: &str) {
        if let Some(cache) = &self.cache {
            cache.set(category, key, value).await;
        }
    }
}

fn emit(cb: ProgressCb<'_>, progress: PipelineProgress) {
    if let Some(f) = cb {
        f(progress);
    }
}
