use async_trait::async_trait;

use crate::error::Result;
use crate::types::TtsResult;

/// Text-to-speech provider.
///
/// Implement this trait to plug in a custom TTS engine. Built-in
/// implementations: [`ElevenLabsTts`](crate::ElevenLabsTts) and
/// [`OpenAiTts`](crate::OpenAiTts) (works with any OpenAI-compatible
/// TTS server, including local ones like Kokoro, AllTalk, and LocalAI).
#[async_trait]
pub trait TtsProvider: Send + Sync {
    /// Synthesize speech from text, returning audio bytes and word-level captions.
    async fn synthesize(&self, text: &str) -> Result<TtsResult>;
}
