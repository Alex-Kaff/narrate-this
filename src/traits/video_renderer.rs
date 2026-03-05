use async_trait::async_trait;

use crate::error::Result;
use crate::types::{AudioTrack, ContentOutput};

/// Configuration for video rendering.
#[derive(Clone)]
pub struct RenderConfig {
    /// Output width in pixels. Default: 1920.
    pub width: u32,
    /// Output height in pixels. Default: 1080.
    pub height: u32,
    /// Frames per second. Default: 30.
    pub fps: u32,
    /// Output file path. Default: `"output.mp4"`.
    pub output_path: String,
    /// Background audio tracks to mix with the narration.
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

/// Renders pipeline output into a video file.
///
/// The built-in implementation is [`FfmpegRenderer`](crate::FfmpegRenderer),
/// which composites media segments with audio and subtitles using FFmpeg.
#[async_trait]
pub trait VideoRenderer: Send + Sync {
    /// Render a [`ContentOutput`] into a video file. Returns the output file path.
    async fn render(&self, output: &ContentOutput, config: &RenderConfig) -> Result<String>;
}
