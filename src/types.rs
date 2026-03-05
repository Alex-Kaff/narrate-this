use serde::{Deserialize, Serialize};

/// Configurable style variables for narration prompt templates.
///
/// These values are interpolated into the default narration and search-narration
/// prompts. To override a prompt entirely, set `narration_prompt` or
/// `search_narration_prompt` on [`super::providers::firecrawl::FirecrawlConfig`].
#[derive(Debug, Clone)]
pub struct NarrationStyle {
    /// Writer role, e.g. "news broadcast scriptwriter", "podcast host"
    pub role: String,
    /// Reader persona, e.g. "a news anchor", "a podcast host"
    pub persona: String,
    /// Output length, e.g. "2-4 paragraphs, 30-90 seconds when read aloud"
    pub length: String,
    /// Tone description, e.g. "Conversational and engaging"
    pub tone: String,
    /// Structural guidance, e.g. "Start with the key headline/finding, then provide context"
    pub structure: String,
}

impl Default for NarrationStyle {
    fn default() -> Self {
        Self {
            role: "news broadcast scriptwriter".into(),
            persona: "a news anchor".into(),
            length: "2-4 paragraphs, 30-90 seconds when read aloud".into(),
            tone: "Conversational and engaging".into(),
            structure: "Start with the key headline/finding, then provide context".into(),
        }
    }
}

impl NarrationStyle {
    pub fn role(mut self, role: impl Into<String>) -> Self {
        self.role = role.into();
        self
    }

    pub fn persona(mut self, persona: impl Into<String>) -> Self {
        self.persona = persona.into();
        self
    }

    pub fn length(mut self, length: impl Into<String>) -> Self {
        self.length = length.into();
        self
    }

    pub fn tone(mut self, tone: impl Into<String>) -> Self {
        self.tone = tone.into();
        self
    }

    pub fn structure(mut self, structure: impl Into<String>) -> Self {
        self.structure = structure.into();
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptionSegment {
    pub text: String,
    pub start_ms: u64,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum MediaKind {
    #[default]
    Image,
    Video,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaSegment {
    pub url: String,
    pub start_ms: f64,
    pub end_ms: f64,
    #[serde(default)]
    pub kind: MediaKind,
}

#[derive(Debug, Clone)]
pub struct TtsResult {
    pub audio: Vec<u8>,
    pub captions: Vec<CaptionSegment>,
}

#[derive(Debug, Clone)]
pub struct KeywordResult {
    pub keywords: Vec<String>,
}

/// Complete pipeline output.
#[derive(Debug, Clone)]
pub struct ContentOutput {
    pub narration: String,
    pub audio: Vec<u8>,
    pub captions: Vec<CaptionSegment>,
    pub media_segments: Vec<MediaSegment>,
    pub audio_path: Option<String>,
    pub video_path: Option<String>,
}

/// A background audio track to mix with narration.
#[derive(Debug, Clone)]
pub struct AudioTrack {
    pub path: String,
    pub volume: f32,
    pub start_ms: Option<u64>,
    pub end_ms: Option<u64>,
    pub loop_track: bool,
}

impl AudioTrack {
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            volume: 0.3,
            start_ms: None,
            end_ms: None,
            loop_track: true,
        }
    }

    pub fn volume(mut self, volume: f32) -> Self {
        self.volume = volume.clamp(0.0, 1.0);
        self
    }

    pub fn start_at(mut self, ms: u64) -> Self {
        self.start_ms = Some(ms);
        self
    }

    pub fn end_at(mut self, ms: u64) -> Self {
        self.end_ms = Some(ms);
        self
    }

    pub fn no_loop(mut self) -> Self {
        self.loop_track = false;
        self
    }
}

/// Input source for content creation.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum ContentSource {
    /// Raw text to narrate directly.
    Text(String),
    /// Article URL to scrape and narrate.
    ArticleUrl { url: String, title: Option<String> },
    /// Search query — scraper searches and narrates results.
    SearchQuery(String),
}

/// Progress events during pipeline execution.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum PipelineProgress {
    NarrationStarted,
    NarrationComplete { narration_len: usize },
    TextTransformStarted,
    TextTransformComplete { narration_len: usize },
    TtsStarted,
    TtsComplete { audio_bytes: usize, caption_count: usize },
    MediaSearchStarted { chunk_count: usize },
    MediaSegmentFound { index: usize, kind: MediaKind },
    MediaSearchComplete { segment_count: usize },
    AudioStorageStarted,
    AudioStored { path: String },
    RenderStarted,
    RenderComplete { path: String },
}
