use thiserror::Error;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SdkError {
    #[error("TTS error: {0}")]
    Tts(String),

    #[error("LLM error: {0}")]
    Llm(String),

    #[error("Media search error: {0}")]
    MediaSearch(String),

    #[error("Web scraper error: {0}")]
    WebScraper(String),

    #[error("Audio storage error: {0}")]
    AudioStorage(String),

    #[error("Cache error: {0}")]
    Cache(String),

    #[error("Video render error: {0}")]
    VideoRender(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, SdkError>;
