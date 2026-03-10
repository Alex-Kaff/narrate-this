use async_trait::async_trait;
use serde::Deserialize;

use crate::error::{Result, SdkError};
use crate::traits::TtsProvider;
use crate::types::{CaptionSegment, TtsResult};

/// Configuration for the OpenAI-compatible TTS provider.
///
/// Works with OpenAI's hosted API and any compatible local server
/// (Kokoro, AllTalk, OpenedAI Speech, LocalAI, etc.).
///
/// # Captions
///
/// Set `caption_model` to generate word-level captions via a Whisper-compatible
/// `/v1/audio/transcriptions` endpoint (uses the same `base_url`). When `None`,
/// the provider returns audio with no captions — suitable for audio-only output,
/// but media planning and subtitles won't work.
///
/// # Example
///
/// ```rust,no_run
/// use narrate_this::{OpenAiTts, OpenAiTtsConfig};
///
/// // Cloud OpenAI with captions
/// let tts = OpenAiTts::new(OpenAiTtsConfig {
///     api_key: "sk-...".into(),
///     ..Default::default()
/// });
///
/// // Local Kokoro server, no captions needed
/// let tts = OpenAiTts::new(OpenAiTtsConfig {
///     base_url: "http://localhost:8880".into(),
///     caption_model: None,
///     ..Default::default()
/// });
/// ```
pub struct OpenAiTtsConfig {
    /// API key. Can be empty for local servers that don't require auth.
    pub api_key: String,
    /// Base URL for the API. Default: `"https://api.openai.com"`.
    pub base_url: String,
    /// TTS model. Default: `"tts-1"`.
    pub model: String,
    /// Voice name. Default: `"alloy"`.
    pub voice: String,
    /// Playback speed multiplier (0.25–4.0). Default: 1.0.
    pub speed: f64,
    /// Audio format. Default: `"mp3"`.
    pub response_format: String,
    /// HTTP request timeout in seconds. Default: 90.
    pub timeout_secs: u64,
    /// Whisper model name for generating word-level captions.
    /// Set to `None` to skip caption generation.
    /// Default: `Some("whisper-1")`.
    pub caption_model: Option<String>,
}

impl Default for OpenAiTtsConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            base_url: "https://api.openai.com".into(),
            model: "tts-1".into(),
            voice: "alloy".into(),
            speed: 1.0,
            response_format: "mp3".into(),
            timeout_secs: 90,
            caption_model: Some("whisper-1".into()),
        }
    }
}

/// OpenAI-compatible TTS provider.
///
/// Calls `/v1/audio/speech` for synthesis and optionally `/v1/audio/transcriptions`
/// (Whisper) for word-level caption alignment.
pub struct OpenAiTts {
    config: OpenAiTtsConfig,
    client: reqwest::Client,
}

impl OpenAiTts {
    pub fn new(config: OpenAiTtsConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()
            .expect("failed to build HTTP client");
        Self { config, client }
    }

    /// Generate word-level captions by transcribing the audio with Whisper,
    /// then aligning the timestamps to the original source text.
    ///
    /// The source `text` is passed as a prompt to guide Whisper's transcription
    /// and is used for post-processing alignment: Whisper provides the timing,
    /// but the original words are preserved to avoid transcription errors showing
    /// up in captions.
    async fn transcribe_captions(
        &self,
        audio: &[u8],
        model: &str,
        text: &str,
    ) -> Result<Vec<CaptionSegment>> {
        let url = format!(
            "{}/v1/audio/transcriptions",
            self.config.base_url.trim_end_matches('/')
        );

        let file_part = reqwest::multipart::Part::bytes(audio.to_vec())
            .file_name(format!("speech.{}", self.config.response_format))
            .mime_str(&mime_for_format(&self.config.response_format))
            .map_err(|e| SdkError::Tts(format!("failed to build multipart: {e}")))?;

        let form = reqwest::multipart::Form::new()
            .part("file", file_part)
            .text("model", model.to_string())
            .text("response_format", "verbose_json")
            .text("timestamp_granularities[]", "word")
            .text("prompt", text.to_string());

        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.config.api_key)
            .multipart(form)
            .send()
            .await
            .map_err(|e| SdkError::Tts(format!("whisper request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(SdkError::Tts(format!(
                "whisper API returned {status}: {body}"
            )));
        }

        let transcript: WhisperResponse = resp
            .json()
            .await
            .map_err(|e| SdkError::Tts(format!("whisper response parse failed: {e}")))?;

        let whisper_words = transcript.words.unwrap_or_default();

        Ok(align_to_source_text(text, &whisper_words))
    }
}

/// Align Whisper timestamps to the original source text.
///
/// Whisper provides accurate timing but may slightly alter words (e.g.
/// contractions, homophones). This function uses Whisper only for timing
/// and keeps the original source words.
fn align_to_source_text(source_text: &str, whisper_words: &[WhisperWord]) -> Vec<CaptionSegment> {
    if whisper_words.is_empty() {
        return Vec::new();
    }

    let source_words: Vec<&str> = source_text.split_whitespace().collect();
    if source_words.is_empty() {
        // No source text — fall back to Whisper's words directly
        return whisper_words
            .iter()
            .map(|w| {
                let start_ms = (w.start * 1000.0) as u64;
                let end_ms = (w.end * 1000.0) as u64;
                CaptionSegment {
                    text: w.word.clone(),
                    start_ms,
                    duration_ms: end_ms.saturating_sub(start_ms),
                }
            })
            .collect();
    }

    if source_words.len() == whisper_words.len() {
        // Perfect 1:1 match — use source words with Whisper timestamps
        return source_words
            .iter()
            .zip(whisper_words.iter())
            .map(|(sw, ww)| {
                let start_ms = (ww.start * 1000.0) as u64;
                let end_ms = (ww.end * 1000.0) as u64;
                CaptionSegment {
                    text: (*sw).to_string(),
                    start_ms,
                    duration_ms: end_ms.saturating_sub(start_ms),
                }
            })
            .collect();
    }

    // Word count mismatch — interpolate Whisper's total time range across source words
    let total_start = whisper_words.first().unwrap().start;
    let total_end = whisper_words.last().unwrap().end;
    let total_duration = total_end - total_start;
    let n = source_words.len() as f64;

    source_words
        .iter()
        .enumerate()
        .map(|(i, word)| {
            let frac_start = i as f64 / n;
            let frac_end = (i + 1) as f64 / n;
            let start = total_start + frac_start * total_duration;
            let end = total_start + frac_end * total_duration;
            let start_ms = (start * 1000.0) as u64;
            let end_ms = (end * 1000.0) as u64;
            CaptionSegment {
                text: (*word).to_string(),
                start_ms,
                duration_ms: end_ms.saturating_sub(start_ms),
            }
        })
        .collect()
}

#[derive(Deserialize)]
struct WhisperResponse {
    words: Option<Vec<WhisperWord>>,
}

#[derive(Deserialize)]
struct WhisperWord {
    word: String,
    start: f64,
    end: f64,
}

fn mime_for_format(format: &str) -> String {
    match format {
        "mp3" => "audio/mpeg".into(),
        "opus" => "audio/opus".into(),
        "aac" => "audio/aac".into(),
        "flac" => "audio/flac".into(),
        "wav" => "audio/wav".into(),
        "pcm" => "audio/L16".into(),
        _ => "application/octet-stream".into(),
    }
}

#[async_trait]
impl TtsProvider for OpenAiTts {
    async fn synthesize(&self, text: &str) -> Result<TtsResult> {
        let url = format!(
            "{}/v1/audio/speech",
            self.config.base_url.trim_end_matches('/')
        );

        let body = serde_json::json!({
            "model": &self.config.model,
            "input": text,
            "voice": &self.config.voice,
            "speed": self.config.speed,
            "response_format": &self.config.response_format,
        });

        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.config.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| SdkError::Tts(format!("request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(SdkError::Tts(format!("API returned {status}: {body}")));
        }

        let audio = resp
            .bytes()
            .await
            .map_err(|e| SdkError::Tts(format!("failed to read audio bytes: {e}")))?
            .to_vec();

        let captions = match &self.config.caption_model {
            Some(model) => self.transcribe_captions(&audio, model, text).await?,
            None => Vec::new(),
        };

        Ok(TtsResult { audio, captions })
    }
}
