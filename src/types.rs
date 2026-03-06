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

/// A single word-level caption with timing information.
///
/// Produced by TTS providers that support alignment data (e.g. ElevenLabs).
/// Used for subtitle rendering and media segment timing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptionSegment {
    /// The word or token text.
    pub text: String,
    /// Start time in milliseconds from the beginning of the audio.
    pub start_ms: u64,
    /// Duration in milliseconds.
    pub duration_ms: u64,
}

/// The source of a media asset — a URL, local file path, or raw bytes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum MediaSource {
    /// A remote URL (http/https).
    Url(String),
    /// A local file path.
    FilePath(String),
    /// Raw bytes (e.g. an in-memory image).
    Bytes(Vec<u8>),
}

impl MediaSource {
    /// Returns a short display string for logging, truncating URLs beyond 80 characters.
    pub fn display_short(&self) -> String {
        match self {
            MediaSource::Url(u) => {
                if u.len() > 80 {
                    let truncated: String = u.chars().take(80).collect();
                    format!("{truncated}…")
                } else {
                    u.clone()
                }
            }
            MediaSource::FilePath(p) => p.clone(),
            MediaSource::Bytes(b) => format!("<{} bytes>", b.len()),
        }
    }
}

impl From<&str> for MediaSource {
    fn from(s: &str) -> Self {
        if s.starts_with("http://") || s.starts_with("https://") {
            MediaSource::Url(s.to_string())
        } else {
            MediaSource::FilePath(s.to_string())
        }
    }
}

impl From<String> for MediaSource {
    fn from(s: String) -> Self {
        if s.starts_with("http://") || s.starts_with("https://") {
            MediaSource::Url(s)
        } else {
            MediaSource::FilePath(s)
        }
    }
}

impl From<Vec<u8>> for MediaSource {
    fn from(data: Vec<u8>) -> Self {
        MediaSource::Bytes(data)
    }
}

/// The type of media asset (image or video).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum MediaKind {
    #[default]
    Image,
    Video,
}

/// A media segment tied to a time range in the narration audio.
///
/// Each segment maps a media asset (image or video) to a portion of the timeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaSegment {
    /// Source of the media asset.
    pub source: MediaSource,
    /// Start time in milliseconds.
    pub start_ms: f64,
    /// End time in milliseconds.
    pub end_ms: f64,
    /// Whether this is an image or video.
    #[serde(default)]
    pub kind: MediaKind,
}

/// Output from a TTS synthesis call.
#[derive(Debug, Clone)]
pub struct TtsResult {
    /// Raw audio bytes (typically MP3).
    pub audio: Vec<u8>,
    /// Word-level caption segments with timing.
    pub captions: Vec<CaptionSegment>,
}

/// Output from keyword extraction.
#[derive(Debug, Clone)]
pub struct KeywordResult {
    /// Extracted search keywords for media lookup.
    pub keywords: Vec<String>,
}

/// Complete pipeline output returned by [`ContentPipeline::process`](crate::ContentPipeline::process).
#[derive(Debug, Clone)]
pub struct ContentOutput {
    /// The narration text (after any text transforms).
    pub narration: String,
    /// Raw audio bytes (MP3).
    pub audio: Vec<u8>,
    /// Word-level captions with timing data.
    pub captions: Vec<CaptionSegment>,
    /// Visual media segments matched to the narration timeline.
    pub media_segments: Vec<MediaSegment>,
    /// Path where audio was stored, if an [`AudioStorage`](crate::AudioStorage) was configured.
    pub audio_path: Option<String>,
    /// Path to the rendered video, if a [`VideoRenderer`](crate::VideoRenderer) was configured.
    pub video_path: Option<String>,
}

/// A background audio track to mix with the narration audio.
///
/// Tracks loop by default and play at 30% volume. Use the builder methods
/// to customize.
///
/// # Example
///
/// ```
/// use narrate_this::AudioTrack;
///
/// let track = AudioTrack::new("./music.mp3")
///     .volume(0.15)
///     .start_at(2000) // delay by 2 seconds
///     .no_loop();
/// ```
#[derive(Debug, Clone)]
pub struct AudioTrack {
    /// File path to the audio file.
    pub path: String,
    /// Volume level from 0.0 (silent) to 1.0 (full). Default: 0.3.
    pub volume: f32,
    /// Optional delay before the track starts (milliseconds).
    pub start_ms: Option<u64>,
    /// Optional end time — the track is trimmed at this point (milliseconds).
    pub end_ms: Option<u64>,
    /// Whether to loop the track for the duration of the narration. Default: `true`.
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

/// A user-provided media asset with a description for AI-based media planning.
///
/// # Example
///
/// ```
/// use narrate_this::MediaAsset;
///
/// let assets = vec![
///     MediaAsset::image("./hero.jpg", "A rocket launching into space"),
///     MediaAsset::video("https://example.com/demo.mp4", "App demo walkthrough"),
///     MediaAsset::image_bytes(vec![/* png bytes */], "Dashboard screenshot"),
/// ];
/// ```
#[derive(Debug, Clone)]
pub struct MediaAsset {
    /// The media source (URL, file path, or bytes).
    pub source: MediaSource,
    /// A text description of what this asset depicts, used by the media planner
    /// to match assets to narration chunks.
    pub description: String,
    /// Whether this is an image or video.
    pub kind: MediaKind,
}

impl MediaAsset {
    /// Create an image asset from a URL or file path.
    pub fn image(source: impl Into<MediaSource>, description: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            description: description.into(),
            kind: MediaKind::Image,
        }
    }

    /// Create a video asset from a URL or file path.
    pub fn video(source: impl Into<MediaSource>, description: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            description: description.into(),
            kind: MediaKind::Video,
        }
    }

    /// Create an image asset from raw bytes.
    pub fn image_bytes(data: Vec<u8>, description: impl Into<String>) -> Self {
        Self {
            source: MediaSource::Bytes(data),
            description: description.into(),
            kind: MediaKind::Image,
        }
    }

    /// Create a video asset from raw bytes.
    pub fn video_bytes(data: Vec<u8>, description: impl Into<String>) -> Self {
        Self {
            source: MediaSource::Bytes(data),
            description: description.into(),
            kind: MediaKind::Video,
        }
    }
}

/// What to do when the media planner can't match a user asset to a narration chunk.
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub enum MediaFallback {
    /// Fall back to keyword extraction + stock media search
    /// (requires `.stock_search()` to be configured on the planner).
    #[default]
    StockSearch,
    /// Skip the chunk — no media for that time range.
    Skip,
}

/// A narration chunk with timing information, used by media planners.
#[derive(Debug, Clone)]
pub struct TimedChunk {
    /// The text content of this chunk.
    pub text: String,
    /// Start time in milliseconds.
    pub start_ms: f64,
    /// End time in milliseconds.
    pub end_ms: f64,
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
