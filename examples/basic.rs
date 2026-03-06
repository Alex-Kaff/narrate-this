//! Basic example: article URL → narrated video with visuals.
//!
//! Uses all default providers (ElevenLabs, OpenAI, Pexels, Firecrawl, filesystem storage)
//! plus FFmpeg for video rendering.
//!
//! Required environment variables:
//!   TTS_API_KEY       — ElevenLabs API key
//!   OPENAI_API_KEY    — OpenAI API key
//!   PEXELS_API_KEY    — Pexels API key
//!   FIRECRAWL_URL     — Firecrawl instance URL (e.g. http://localhost:3002)
//!
//! Requires `ffmpeg` to be installed and on PATH.
//!
//! Run:
//!   cargo run --example basic

use narrate_this::{
    ContentPipeline, ContentSource, ElevenLabsConfig, ElevenLabsTts, FfmpegRenderer,
    FirecrawlScraper, FsAudioStorage, OpenAiConfig, OpenAiKeywords, PexelsSearch,
    PipelineProgress, RenderConfig, StockMediaPlanner,
};

fn env(key: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| panic!("{key} env var required — see .env.example"))
}

#[tokio::main]
async fn main() -> narrate_this::Result<()> {
    // Load .env from the crate directory.
    let _ = dotenvy::from_path("examples/.env");

    // Build the pipeline with step-oriented builder.
    let pipeline = ContentPipeline::builder()
        .content(FirecrawlScraper::new(&env("FIRECRAWL_URL")))
        .tts(ElevenLabsTts::new(ElevenLabsConfig {
            api_key: env("TTS_API_KEY"),
            ..Default::default()
        }))
        .media(StockMediaPlanner::new(
            OpenAiKeywords::new(OpenAiConfig {
                api_key: env("OPENAI_API_KEY"),
                ..Default::default()
            }),
            PexelsSearch::new(&env("PEXELS_API_KEY")),
        ))
        .renderer(
            FfmpegRenderer::new(),
            RenderConfig {
                output_path: "./audio_output/output.mp4".into(),
                ..Default::default()
            },
        )
        .audio_storage(FsAudioStorage::new("./audio_output"))
        .build()?;

    // Process an article URL through the full pipeline with progress tracking.
    let output = pipeline
        .process_with_progress(
            ContentSource::ArticleUrl {
                url: "https://engineering.fb.com/2026/03/02/data-infrastructure/investing-in-infrastructure-metas-renewed-commitment-to-jemalloc/".into(),
                title: Some("Investing in infrastructure, metas renewed commitment".into()),
            },
            |p| match p {
                PipelineProgress::NarrationStarted => {
                    println!("[1/6] Scraping article and generating narration...")
                }
                PipelineProgress::NarrationComplete { narration_len } => {
                    println!("      Narration ready ({narration_len} chars)")
                }
                PipelineProgress::TextTransformStarted => {
                    println!("[2/6] Applying text transforms...")
                }
                PipelineProgress::TextTransformComplete { narration_len } => {
                    println!("      Transform complete ({narration_len} chars)")
                }
                PipelineProgress::TtsStarted => {
                    println!("[3/6] Synthesizing speech...")
                }
                PipelineProgress::TtsComplete {
                    audio_bytes,
                    caption_count,
                } => {
                    println!("      Audio ready ({audio_bytes} bytes, {caption_count} captions)")
                }
                PipelineProgress::MediaSearchStarted { chunk_count } => {
                    println!("[4/6] Searching media for {chunk_count} segments...")
                }
                PipelineProgress::MediaSegmentFound { index, kind } => {
                    println!("      Segment {index}: {kind:?}")
                }
                PipelineProgress::MediaSearchComplete { segment_count } => {
                    println!("      Found {segment_count} media segments")
                }
                PipelineProgress::AudioStorageStarted => {
                    println!("[5/6] Storing audio...")
                }
                PipelineProgress::AudioStored { ref path } => {
                    println!("      Saved to {path}")
                }
                PipelineProgress::RenderStarted => {
                    println!("[6/6] Rendering video with FFmpeg...")
                }
                PipelineProgress::RenderComplete { ref path } => {
                    println!("      Video saved to {path}")
                }
                _ => {}
            },
        )
        .await?;

    // Print results.
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
            "  [{:.1}s–{:.1}s] {:?}: {}",
            seg.start_ms / 1000.0,
            seg.end_ms / 1000.0,
            seg.kind,
            seg.source.display_short()
        );
    }

    Ok(())
}
