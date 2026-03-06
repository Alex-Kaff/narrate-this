//! Example: local TTS server + predetermined media assets with AI-based planning.
//!
//! Demonstrates:
//!   - Custom `TtsProvider` for a local TTS API (FastAPI with Qwen3-TTS + faster-whisper for cc) with word-level captions
//!   - User-provided `MediaAsset`s matched to narration chunks via `LlmMediaPlanner`
//!   - Hybrid fallback to Pexels stock search for unmatched chunks
//!
//! Required environment variables:
//!   LOCAL_TTS_URL     — Base URL of your local TTS server (e.g. http://localhost:8880)
//!   TTS_SPEAKER       — Speaker name for the local TTS
//!   OPENAI_API_KEY    — OpenAI API key (for media planner + keyword extraction)
//!   PEXELS_API_KEY    — Pexels API key (for stock search fallback)
//!
//! Requires `ffmpeg` on PATH for video rendering.
//!
//! Run:
//!   cargo run --example local_tts

use async_trait::async_trait;
use narrate_this::{
    CaptionSegment, ContentPipeline, ContentSource, FfmpegRenderer, FsAudioStorage,
    LlmMediaPlanner, MediaAsset, MediaFallback, OpenAiConfig, OpenAiKeywords, PexelsSearch,
    PipelineProgress, RenderConfig, SdkError, TtsProvider, TtsResult,
};
use serde::{Deserialize, Serialize};

// ── Local TTS provider ──

struct LocalTts {
    client: reqwest::Client,
    base_url: String,
    speaker: String,
}

impl LocalTts {
    fn new(base_url: &str, speaker: &str) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(3600))
            .build()
            .expect("failed to build HTTP client");

        Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            speaker: speaker.to_string(),
        }
    }
}

#[derive(Serialize)]
struct TtsRequest<'a> {
    text: &'a str,
    language: &'a str,
    speaker: &'a str,
}

#[derive(Deserialize)]
struct TtsResponse {
    audio_base64: String,
    #[allow(dead_code)]
    sample_rate: u32,
    #[allow(dead_code)]
    duration: f64,
    captions: Vec<CaptionEntry>,
}

#[derive(Deserialize)]
struct CaptionEntry {
    word: String,
    start: f64,
    end: f64,
}

#[async_trait]
impl TtsProvider for LocalTts {
    async fn synthesize(&self, text: &str) -> narrate_this::Result<TtsResult> {
        use base64::Engine;

        let url = format!("{}/tts", self.base_url);

        let body = TtsRequest {
            text,
            language: "English",
            speaker: &self.speaker,
        };

        eprintln!("[tts] sending {len} chars to {url}", len = text.len());

        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                let mut msg = format!("local TTS request failed: {e}");
                let mut source = std::error::Error::source(&e);
                while let Some(cause) = source {
                    msg.push_str(&format!(" -> {cause}"));
                    source = std::error::Error::source(cause);
                }
                SdkError::Tts(msg)
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body: String = resp.text().await.unwrap_or_default();
            return Err(SdkError::Tts(format!(
                "local TTS returned {status}: {body}"
            )));
        }

        let tts_resp: TtsResponse = resp
            .json()
            .await
            .map_err(|e| SdkError::Tts(format!("failed to parse TTS response: {e}")))?;

        let audio = base64::engine::general_purpose::STANDARD
            .decode(&tts_resp.audio_base64)
            .map_err(|e| SdkError::Tts(format!("failed to decode audio base64: {e}")))?;

        let captions = tts_resp
            .captions
            .into_iter()
            .map(|c| {
                let start_ms = (c.start * 1000.0) as u64;
                let end_ms = (c.end * 1000.0) as u64;
                CaptionSegment {
                    text: c.word,
                    start_ms,
                    duration_ms: end_ms.saturating_sub(start_ms),
                }
            })
            .collect();

        Ok(TtsResult { audio, captions })
    }
}

// ── Main ──

fn env(key: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| panic!("{key} env var required"))
}

#[tokio::main]
async fn main() -> narrate_this::Result<()> {
    let _ = dotenvy::from_path("examples/.env");

    let openai_key = env("OPENAI_API_KEY");

    let tts = LocalTts::new(&env("LOCAL_TTS_URL"), &env("TTS_SPEAKER"));

    // Build the pipeline with LLM-planned media
    let pipeline = ContentPipeline::builder()
        .tts(tts)
        .media(
            LlmMediaPlanner::new(OpenAiConfig {
                api_key: openai_key.clone(),
                ..Default::default()
            })
            .assets(vec![
                MediaAsset::image(
                    "https://images.unsplash.com/photo-1451187580459-43490279c0fa?w=1280",
                    "Earth from space at night showing city lights and atmosphere",
                ),
                MediaAsset::image(
                    "https://images.unsplash.com/photo-1518770660439-4636190af475?w=1280",
                    "Close-up of a circuit board with microchips and electronic components",
                ),
                MediaAsset::image(
                    "https://images.unsplash.com/photo-1504639725590-34d0984388bd?w=1280",
                    "Code on a computer screen with colorful syntax highlighting",
                ),
            ])
            .stock_search(
                OpenAiKeywords::new(OpenAiConfig {
                    api_key: openai_key,
                    ..Default::default()
                }),
                PexelsSearch::new(&env("PEXELS_API_KEY")),
            )
            .fallback(MediaFallback::StockSearch),
        )
        .renderer(
            FfmpegRenderer::new(),
            RenderConfig {
                output_path: "./audio_output/local_tts_output.mp4".into(),
                ..Default::default()
            },
        )
        .audio_storage(FsAudioStorage::new("./audio_output"))
        .build()?;

    let narration = "\
        The world of technology is evolving at an unprecedented pace. \
        From artificial intelligence breakthroughs to quantum computing advances, \
        we're witnessing a revolution in how we interact with machines. \
        Meanwhile, software developers are building the tools that power this transformation, \
        writing millions of lines of code, every day to shape our digital future.";

    let output = pipeline
        .process_with_progress(ContentSource::Text(narration.into()), |p| match p {
            PipelineProgress::NarrationStarted => println!("[1/6] Preparing narration..."),
            PipelineProgress::NarrationComplete { narration_len } => {
                println!("      Narration ready ({narration_len} chars)")
            }
            PipelineProgress::TtsStarted => println!("[2/6] Synthesizing speech (local TTS)..."),
            PipelineProgress::TtsComplete {
                audio_bytes,
                caption_count,
            } => println!("      Audio: {audio_bytes} bytes, {caption_count} captions"),
            PipelineProgress::MediaSearchStarted { chunk_count } => {
                println!("[3/6] Planning media for {chunk_count} chunks...")
            }
            PipelineProgress::MediaSegmentFound { index, kind } => {
                println!("      Segment {index}: {kind:?}")
            }
            PipelineProgress::MediaSearchComplete { segment_count } => {
                println!("      Total media segments: {segment_count}")
            }
            PipelineProgress::AudioStorageStarted => println!("[4/6] Storing audio..."),
            PipelineProgress::AudioStored { ref path } => println!("      Saved to {path}"),
            PipelineProgress::RenderStarted => println!("[5/6] Rendering video..."),
            PipelineProgress::RenderComplete { ref path } => {
                println!("      Video saved to {path}")
            }
            _ => {}
        })
        .await?;

    println!("\n--- Results ---");
    println!("Narration: {} chars", output.narration.len());
    println!("Audio: {} bytes", output.audio.len());
    println!("Captions: {} words", output.captions.len());
    println!("Media segments: {}", output.media_segments.len());

    if let Some(path) = &output.audio_path {
        println!("Audio: {path}");
    }
    if let Some(path) = &output.video_path {
        println!("Video: {path}");
    }

    for seg in &output.media_segments {
        println!(
            "  [{:.1}s-{:.1}s] {:?}: {}",
            seg.start_ms / 1000.0,
            seg.end_ms / 1000.0,
            seg.kind,
            seg.source.display_short()
        );
    }

    Ok(())
}
