use async_trait::async_trait;

use crate::error::Result;
use crate::types::{AudioTrack, ContentOutput};

/// Render configuration for video output.
#[derive(Clone)]
pub struct RenderConfig {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub output_path: String,
    pub audio_tracks: Vec<AudioTrack>,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
            fps: 30,
            output_path: "output.mp4".into(),
            audio_tracks: Vec::new(),
        }
    }
}

#[async_trait]
pub trait VideoRenderer: Send + Sync {
    /// Render a ContentOutput into a video file. Returns the output file path.
    async fn render(&self, output: &ContentOutput, config: &RenderConfig) -> Result<String>;
}
