# narrate-this

[![Crates.io](https://img.shields.io/crates/v/narrate-this)](https://crates.io/crates/narrate-this)
[![docs.rs](https://img.shields.io/docsrs/narrate-this)](https://docs.rs/narrate-this)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A Rust SDK that turns text, URLs, or search queries into narrated videos — complete with TTS, captions, and stock visuals.

- **Build pipeline** from text content to rendered video in a single call
- **Pluggable providers** — swap any stage by implementing a trait

Here's a cringe demo made using this:

https://github.com/user-attachments/assets/a164e9d6-2fcf-4fd6-905f-0f0d001474d2

I watch random videos when I code, like the news, or random stuff in the background. I made this to be used in an automated pipeline for another personal app, which, in realtime reads from RSS feeds I m interested in and generates this content to satisfy my ADHD brain

## Quick start

```toml
[dependencies]
narrate-this = "0.1"
tokio = { version = "1", features = ["full"] }
```

```rust
use narrate_this::{
    ContentPipeline, ContentSource, ElevenLabsConfig, ElevenLabsTts,
    FfmpegRenderer, FirecrawlScraper, FsAudioStorage, OpenAiConfig,
    OpenAiKeywords, PexelsSearch, RenderConfig, StockMediaPlanner,
};

#[tokio::main]
async fn main() -> narrate_this::Result<()> {
    let pipeline = ContentPipeline::builder()
        .content(FirecrawlScraper::new("http://localhost:3002"))
        .tts(ElevenLabsTts::new(ElevenLabsConfig {
            api_key: "your-elevenlabs-key".into(),
            ..Default::default()
        }))
        .media(StockMediaPlanner::new(
            OpenAiKeywords::new(OpenAiConfig {
                api_key: "your-openai-key".into(),
                ..Default::default()
            }),
            PexelsSearch::new("your-pexels-key"),
        ))
        .renderer(FfmpegRenderer::new(), RenderConfig::default())
        .audio_storage(FsAudioStorage::new("./output"))
        .build()?;

    let output = pipeline
        .process(ContentSource::ArticleUrl {
            url: "https://example.com/article".into(),
            title: Some("My Article".into()),
        })
        .await?;

    println!("Video: {}", output.video_path.unwrap());
    Ok(())
}
```

## How the pipeline works

```
Content Source -> Narration -> Text Transforms -> TTS -> Media -> Audio Storage -> Video Render
```

Only TTS is required. Everything else is optional — skip content sourcing if you pass raw text, skip media if you just want audio, skip rendering if you don't need video.

### Content sources

```rust
// Scrape and narrate an article
ContentSource::ArticleUrl {
    url: "https://example.com/article".into(),
    title: Some("Optional title hint".into()),
}

// Search the web and narrate the results
ContentSource::SearchQuery("latest Rust async developments".into())

// Just narrate some text directly (no content provider needed)
ContentSource::Text("Your text to narrate...".into())
```

## Builder API

The builder uses type-state to enforce valid configuration at compile time:

```rust
ContentPipeline::builder()
    // Content provider (optional — skip for raw text)
    .content(FirecrawlScraper::new("http://localhost:3002"))

    // Text transforms (chainable, applied in order)
    .text_transform(OpenAiTransform::new(
        &openai_key,
        "Rewrite in a casual podcast style",
    ))

    // TTS provider (the only required piece)
    .tts(ElevenLabsTts::new(ElevenLabsConfig {
        api_key: tts_key,
        voice_id: "custom-voice-id".into(),  // default: "Gr7mLjPA3HhuWxZidxPW"
        speed: 1.2,
        ..Default::default()
    }))

    // Everything below is optional, in any order

    // Media planner — see "Media" section below
    .media(StockMediaPlanner::new(keywords_provider, search_provider))

    .renderer(FfmpegRenderer::new(), RenderConfig {
        width: 1920,
        height: 1080,
        fps: 30,
        output_path: "./output.mp4".into(),
        audio_tracks: vec![
            AudioTrack::new("./background.mp3").volume(0.15),
        ],
    })
    .audio_storage(FsAudioStorage::new("./audio_cache"))
    .cache(my_cache_provider)
    .build()?;
```

## Media

The `.media()` builder method takes a `MediaPlanner` — a single trait that owns all media selection logic.

### Stock media only

Use `StockMediaPlanner` for keyword extraction + stock search (e.g. Pexels):

```rust
.media(StockMediaPlanner::new(
    OpenAiKeywords::new(OpenAiConfig {
        api_key: "your-openai-key".into(),
        ..Default::default()
    }),
    PexelsSearch::new("your-pexels-key"),
))
```

### User-provided assets with AI matching

Use `LlmMediaPlanner` to provide your own images/videos with descriptions. An LLM matches them to narration chunks based on semantic relevance:

```rust
use narrate_this::{LlmMediaPlanner, MediaAsset, MediaFallback, OpenAiConfig};

.media(
    LlmMediaPlanner::new(OpenAiConfig {
        api_key: "your-openai-key".into(),
        ..Default::default()
    })
    .assets(vec![
        MediaAsset::image("./hero.jpg", "A rocket launching into space"),
        MediaAsset::video("https://example.com/demo.mp4", "App demo walkthrough"),
        MediaAsset::image_bytes(screenshot_bytes, "Dashboard screenshot"),
    ])
    // Optional: fall back to stock search for unmatched chunks
    .stock_search(
        OpenAiKeywords::new(OpenAiConfig {
            api_key: "your-openai-key".into(),
            ..Default::default()
        }),
        PexelsSearch::new("your-pexels-key"),
    )
    .fallback(MediaFallback::StockSearch) // default
    .allow_reuse(true)                    // same asset can appear multiple times
    .max_reuse(Some(2)),                  // but at most twice
)
```

Media sources can be URLs, local file paths, or raw bytes — the renderer handles all three.

## Processing

```rust
// Run the full pipeline
let output = pipeline.process(source).await?;

// With progress callbacks
let output = pipeline.process_with_progress(source, |event| {
    match event {
        PipelineProgress::NarrationStarted => println!("Scraping..."),
        PipelineProgress::TtsComplete { audio_bytes, caption_count } => {
            println!("Audio: {audio_bytes} bytes, {caption_count} captions");
        }
        PipelineProgress::RenderComplete { ref path } => {
            println!("Video saved to {path}");
        }
        _ => {}
    }
}).await?;

// Or just parts of it
let text = pipeline.narrate(source).await?;           // narration only
let tts_result = pipeline.synthesize("Text").await?;   // TTS only
```

## Output

```rust
pub struct ContentOutput {
    pub narration: String,
    pub audio: Vec<u8>,                    // MP3
    pub captions: Vec<CaptionSegment>,     // word-level timing
    pub media_segments: Vec<MediaSegment>, // source: MediaSource (URL, file path, or bytes)
    pub audio_path: Option<String>,        // if audio storage configured
    pub video_path: Option<String>,        // if renderer configured
}
```

## Narration style

You can control how the LLM writes the narration:

```rust
let scraper = FirecrawlScraper::with_config(FirecrawlConfig {
    base_url: "http://localhost:3002".into(),
    style: NarrationStyle::default()
        .role("podcast host")
        .persona("a friendly tech enthusiast")
        .tone("Casual and upbeat")
        .length("3-5 paragraphs, 60-120 seconds when read aloud")
        .structure("Open with a hook, then dive into details"),
    ..Default::default()
});
```

## Background audio

```rust
let config = RenderConfig {
    output_path: "./output.mp4".into(),
    audio_tracks: vec![
        AudioTrack::new("./music.mp3")
            .volume(0.15)       // 0.0–1.0, default 0.3
            .start_at(2000)     // delay start by 2s
            .no_loop(),         // loops by default
    ],
    ..Default::default()
};
```

## Providers

Built-in:

| Provider | Service |
|----------|---------|
| `ElevenLabsTts` | [ElevenLabs](https://elevenlabs.io) |
| `FirecrawlScraper` | [Firecrawl](https://firecrawl.dev) |
| `OpenAiKeywords` / `OpenAiTransform` | OpenAI (gpt-4o-mini) |
| `PexelsSearch` | [Pexels](https://pexels.com) |
| `StockMediaPlanner` | Keywords + stock search |
| `LlmMediaPlanner` | AI asset matching + stock fallback |
| `FfmpegRenderer` | Local FFmpeg |
| `FsAudioStorage` | Local filesystem |
| `PgCache` | PostgreSQL (feature-gated: `pg-cache`) |

You can swap in your own by implementing the matching trait:

```rust
#[async_trait]
impl TtsProvider for MyTtsProvider {
    async fn synthesize(&self, text: &str) -> Result<TtsResult> {
        // ...
    }
}
```

Traits: `TtsProvider`, `ContentProvider`, `MediaPlanner`, `KeywordExtractor`, `MediaSearchProvider`, `TextTransformer`, `AudioStorage`, `CacheProvider`, `VideoRenderer`.

## PostgreSQL cache

```toml
[dependencies]
narrate-this = { version = "0.1", features = ["pg-cache"] }
```

```rust
let pool = sqlx::PgPool::connect("postgres://localhost/narrate").await?;
let cache = PgCache::new(pool);

let pipeline = ContentPipeline::builder()
    .tts(my_tts)
    .cache(cache)
    .build()?;
```

## Prerequisites

- Rust 2024 edition (1.85+)
- FFmpeg on PATH for video rendering
- A Firecrawl instance for URL/search sources
- API keys for whichever providers you use

## Running the examples

```bash
cp examples/.env.example examples/.env
# fill in your API keys
cargo run --example basic
cargo run --example local_tts
```

## Error handling

All errors come back as `narrate_this::SdkError` with variants for each stage (`Tts`, `Llm`, `MediaSearch`, `MediaPlanner`, `WebScraper`, etc.). Non-fatal errors (like a media search miss) are logged as warnings via `tracing` and won't stop the pipeline.

## License

MIT
