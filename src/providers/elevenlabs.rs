use async_trait::async_trait;
use base64::Engine;

use crate::error::{Result, SdkError};
use crate::traits::TtsProvider;
use crate::types::{CaptionSegment, TtsResult};

const ELEVENLABS_TTS_URL: &str =
    "https://api.elevenlabs.io/v1/text-to-speech/{voice_id}/stream/with-timestamps";

pub struct ElevenLabsConfig {
    pub api_key: String,
    pub voice_id: String,
    pub model_id: String,
    pub speed: f64,
    pub timeout_secs: u64,
}

impl Default for ElevenLabsConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            voice_id: "Gr7mLjPA3HhuWxZidxPW".into(),
            model_id: "eleven_flash_v2_5".into(),
            speed: 1.0,
            timeout_secs: 90,
        }
    }
}

pub struct ElevenLabsTts {
    config: ElevenLabsConfig,
    client: reqwest::Client,
}

impl ElevenLabsTts {
    pub fn new(config: ElevenLabsConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()
            .expect("failed to build HTTP client");
        Self { config, client }
    }
}

/// A single chunk from the streaming response.
#[derive(serde::Deserialize)]
struct StreamChunk {
    audio_base64: Option<String>,
    alignment: Option<AlignmentData>,
}

#[derive(serde::Deserialize)]
struct AlignmentData {
    characters: Vec<String>,
    character_start_times_seconds: Vec<f64>,
    character_end_times_seconds: Vec<f64>,
}

#[async_trait]
impl TtsProvider for ElevenLabsTts {
    async fn synthesize(&self, text: &str) -> Result<TtsResult> {
        let url = ELEVENLABS_TTS_URL.replace("{voice_id}", &self.config.voice_id);
        let body = serde_json::json!({
            "text": text,
            "model_id": &self.config.model_id,
            "speed": self.config.speed,
        });

        let resp = self
            .client
            .post(&url)
            .header("xi-api-key", &self.config.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| SdkError::Tts(format!("request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(SdkError::Tts(format!("API returned {status}: {body}")));
        }

        let response_text = resp
            .text()
            .await
            .map_err(|e| SdkError::Tts(format!("read body: {e}")))?;

        let b64 = base64::engine::general_purpose::STANDARD;
        let mut audio_bytes: Vec<u8> = Vec::new();
        let mut all_chars: Vec<String> = Vec::new();
        let mut all_starts: Vec<f64> = Vec::new();
        let mut all_ends: Vec<f64> = Vec::new();

        for line in response_text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let chunk: StreamChunk = match serde_json::from_str(line) {
                Ok(c) => c,
                Err(_) => continue,
            };

            if let Some(audio_b64) = &chunk.audio_base64 {
                if !audio_b64.is_empty() {
                    if let Ok(decoded) = b64.decode(audio_b64) {
                        audio_bytes.extend_from_slice(&decoded);
                    }
                }
            }

            if let Some(alignment) = chunk.alignment {
                all_chars.extend(alignment.characters);
                all_starts.extend(alignment.character_start_times_seconds);
                all_ends.extend(alignment.character_end_times_seconds);
            }
        }

        let captions = chars_to_word_segments(&all_chars, &all_starts, &all_ends);

        Ok(TtsResult {
            audio: audio_bytes,
            captions,
        })
    }
}

/// Convert character-level alignment data into word-level CaptionSegments.
fn chars_to_word_segments(
    chars: &[String],
    starts: &[f64],
    ends: &[f64],
) -> Vec<CaptionSegment> {
    let len = chars.len().min(starts.len()).min(ends.len());
    if len == 0 {
        return vec![];
    }

    let mut segments = Vec::new();
    let mut word = String::new();
    let mut word_start: Option<f64> = None;
    let mut word_end: f64 = 0.0;

    for i in 0..len {
        let ch = &chars[i];

        if ch.trim().is_empty() {
            // Space or whitespace — flush current word if any.
            if !word.is_empty() {
                segments.push(CaptionSegment {
                    text: std::mem::take(&mut word),
                    start_ms: (word_start.unwrap_or(0.0) * 1000.0) as u64,
                    duration_ms: ((word_end - word_start.unwrap_or(0.0)) * 1000.0) as u64,
                });
                word_start = None;
            }
        } else {
            if word_start.is_none() {
                word_start = Some(starts[i]);
            }
            word.push_str(ch);
            word_end = ends[i];
        }
    }

    // Flush last word.
    if !word.is_empty() {
        segments.push(CaptionSegment {
            text: word,
            start_ms: (word_start.unwrap_or(0.0) * 1000.0) as u64,
            duration_ms: ((word_end - word_start.unwrap_or(0.0)) * 1000.0) as u64,
        });
    }

    segments
}
