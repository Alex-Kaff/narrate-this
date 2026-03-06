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
    /// Video codec (`-c:v`). Default: `"libx264"`.
    pub video_codec: Option<String>,
    /// Audio codec (`-c:a`). Default: `"aac"`.
    pub audio_codec: Option<String>,
    /// Encoder preset (`-preset`). Default: `"fast"`.
    pub preset: Option<String>,
    /// Constant Rate Factor (`-crf`) for quality control. Lower = better quality.
    /// When `None`, ffmpeg uses its own default (typically 23 for libx264).
    pub crf: Option<u32>,
    /// Pixel format (`-pix_fmt`), e.g. `"yuv420p"` for broad player compatibility.
    pub pix_fmt: Option<String>,
    /// ASS/SSA subtitle force_style string. Default: `"FontSize=24,PrimaryColour=&HFFFFFF&"`.
    pub subtitle_style: Option<String>,
    /// Extra ffmpeg arguments inserted before the output path.
    /// Use this as a catch-all for flags not covered above.
    pub extra_output_args: Vec<String>,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
            fps: 30,
            output_path: "output.mp4".into(),
            audio_tracks: Vec::new(),
            video_codec: None,
            audio_codec: None,
            preset: None,
            crf: None,
            pix_fmt: None,
            subtitle_style: None,
            extra_output_args: Vec::new(),
        }
    }
}

impl RenderConfig {
    /// Set the video codec (`-c:v`), e.g. `"libx265"`, `"h264_nvenc"`.
    pub fn video_codec(mut self, codec: impl Into<String>) -> Self {
        self.video_codec = Some(codec.into());
        self
    }

    /// Set the audio codec (`-c:a`), e.g. `"libopus"`, `"libmp3lame"`.
    pub fn audio_codec(mut self, codec: impl Into<String>) -> Self {
        self.audio_codec = Some(codec.into());
        self
    }

    /// Set the encoder preset (`-preset`), e.g. `"ultrafast"`, `"slow"`.
    pub fn preset(mut self, preset: impl Into<String>) -> Self {
        self.preset = Some(preset.into());
        self
    }

    /// Set the Constant Rate Factor (`-crf`) for quality control.
    pub fn crf(mut self, crf: u32) -> Self {
        self.crf = Some(crf);
        self
    }

    /// Set the pixel format (`-pix_fmt`), e.g. `"yuv420p"`.
    pub fn pix_fmt(mut self, fmt: impl Into<String>) -> Self {
        self.pix_fmt = Some(fmt.into());
        self
    }

    /// Set a custom subtitle style (ASS force_style string).
    pub fn subtitle_style(mut self, style: impl Into<String>) -> Self {
        self.subtitle_style = Some(style.into());
        self
    }

    /// Add extra ffmpeg arguments before the output path.
    pub fn extra_output_args(mut self, args: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.extra_output_args = args.into_iter().map(|a| a.into()).collect();
        self
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
