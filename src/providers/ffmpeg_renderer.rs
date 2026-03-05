use async_trait::async_trait;
use tokio::process::Command;

use crate::error::{Result, SdkError};
use crate::traits::{RenderConfig, VideoRenderer};
use crate::types::{ContentOutput, MediaKind};

/// FFmpeg-based video renderer.
///
/// Downloads media segments, composites them with audio and subtitle overlay
/// into an MP4 file. Handles missing media gracefully by using a black background.
pub struct FfmpegRenderer {
    client: reqwest::Client,
}

impl FfmpegRenderer {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .expect("failed to build HTTP client");
        Self { client }
    }
}

impl Default for FfmpegRenderer {
    fn default() -> Self {
        Self::new()
    }
}

struct DownloadedMedia {
    path: String,
    /// How long this segment should play in the final video.
    duration_secs: f64,
    is_image: bool,
}

#[async_trait]
impl VideoRenderer for FfmpegRenderer {
    async fn render(&self, output: &ContentOutput, config: &RenderConfig) -> Result<String> {
        if output.audio.is_empty() {
            return Err(SdkError::VideoRender("no audio data to render".into()));
        }

        // Ensure output directory exists
        if let Some(parent) = std::path::Path::new(&config.output_path).parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| SdkError::VideoRender(format!("create output dir: {e}")))?;
        }

        // Create a temporary working directory
        let work_dir = std::env::temp_dir().join(format!("narrate-this-{}", std::process::id()));
        tokio::fs::create_dir_all(&work_dir)
            .await
            .map_err(|e| SdkError::VideoRender(format!("create work dir: {e}")))?;

        let audio_path = work_dir.join("audio.mp3");
        tokio::fs::write(&audio_path, &output.audio)
            .await
            .map_err(|e| SdkError::VideoRender(format!("write audio: {e}")))?;

        // Download media segments
        let mut media_files: Vec<DownloadedMedia> = Vec::new();

        for (i, seg) in output.media_segments.iter().enumerate() {
            let is_image = !seg.url.contains(".mp4") && seg.kind != MediaKind::Video;
            let ext = if is_image { "jpg" } else { "mp4" };
            let media_path = work_dir.join(format!("media_{i}.{ext}"));

            match self.client.get(&seg.url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    let bytes = resp
                        .bytes()
                        .await
                        .map_err(|e| SdkError::VideoRender(format!("download media {i}: {e}")))?;
                    tokio::fs::write(&media_path, &bytes)
                        .await
                        .map_err(|e| SdkError::VideoRender(format!("write media {i}: {e}")))?;
                    media_files.push(DownloadedMedia {
                        path: media_path.to_string_lossy().to_string(),
                        duration_secs: (seg.end_ms - seg.start_ms) / 1000.0,
                        is_image,
                    });
                }
                _ => {
                    tracing::warn!(index = i, url = %seg.url, "failed to download media segment");
                }
            }
        }

        // Generate subtitle file from captions
        let srt_path = work_dir.join("captions.srt");
        let srt_content = generate_srt(&output.captions);
        tokio::fs::write(&srt_path, &srt_content)
            .await
            .map_err(|e| SdkError::VideoRender(format!("write srt: {e}")))?;

        let srt_path_escaped = srt_path
            .to_string_lossy()
            .replace('\\', "/")
            .replace(':', "\\:");

        let has_subtitles = !output.captions.is_empty();
        let mut filter_parts = Vec::new();
        let mut inputs = Vec::new();

        if media_files.is_empty() {
            // No media — generate a black background for the audio duration
            let subtitle_filter = if has_subtitles {
                format!(
                    ",subtitles='{srt_path_escaped}':force_style='FontSize=24,PrimaryColour=&HFFFFFF&'"
                )
            } else {
                String::new()
            };
            filter_parts.push(format!(
                "color=c=black:s={w}x{h}:r={fps}{subtitle_filter}[vfinal]",
                w = config.width,
                h = config.height,
                fps = config.fps,
            ));
        } else {
            let last_idx = media_files.len() - 1;

            // Build filter complex from downloaded media.
            // The last segment runs until the audio ends:
            //   - Images: loop=-1 makes them infinite
            //   - Videos: -stream_loop -1 input option makes them infinite
            // All other segments are trimmed to their chunk duration.
            for (i, media) in media_files.iter().enumerate() {
                let is_last = i == last_idx;

                // For the last video, add -stream_loop -1 before -i to loop it
                if is_last && !media.is_image {
                    inputs.extend(["-stream_loop".to_string(), "-1".to_string()]);
                }
                inputs.extend(["-i".to_string(), media.path.clone()]);

                if media.is_image {
                    let trim = if is_last {
                        // Last image — no trim, loops infinitely via loop=-1
                        String::new()
                    } else {
                        format!(",trim=duration={:.3}", media.duration_secs)
                    };
                    filter_parts.push(format!(
                        "[{i}:v]loop=-1:1:0,setpts=N/{fps}/TB,\
                         scale={w}:{h}:force_original_aspect_ratio=decrease,\
                         pad={w}:{h}:(ow-iw)/2:(oh-ih)/2{trim}[v{i}]",
                        fps = config.fps,
                        w = config.width,
                        h = config.height,
                    ));
                } else {
                    let trim = if is_last {
                        // Last video — no trim, loops via -stream_loop -1
                        String::new()
                    } else {
                        format!(",trim=duration={:.3}", media.duration_secs)
                    };
                    filter_parts.push(format!(
                        "[{i}:v]scale={w}:{h}:force_original_aspect_ratio=decrease,\
                         pad={w}:{h}:(ow-iw)/2:(oh-ih)/2,\
                         setpts=PTS-STARTPTS{trim}[v{i}]",
                        w = config.width,
                        h = config.height,
                    ));
                }
            }

            // Concatenate all video segments
            let concat_inputs: String = (0..media_files.len())
                .map(|i| format!("[v{i}]"))
                .collect::<Vec<_>>()
                .join("");
            filter_parts.push(format!(
                "{concat_inputs}concat=n={}:v=1:a=0[vout]",
                media_files.len()
            ));

            // Add subtitles
            if has_subtitles {
                filter_parts.push(format!(
                    "[vout]subtitles='{srt_path_escaped}':force_style='FontSize=24,PrimaryColour=&HFFFFFF&'[vfinal]"
                ));
            } else {
                filter_parts.push("[vout]copy[vfinal]".into());
            }
        }

        let filter_complex;

        // Add narration audio input
        inputs.push("-i".to_string());
        inputs.push(audio_path.to_string_lossy().to_string());
        let narr_idx = media_files.len();

        // Add background audio tracks
        for track in &config.audio_tracks {
            if track.loop_track {
                inputs.extend(["-stream_loop".to_string(), "-1".to_string()]);
            }
            inputs.extend(["-i".to_string(), track.path.clone()]);
        }

        let audio_map;
        if config.audio_tracks.is_empty() {
            filter_complex = filter_parts.join("; ");
            audio_map = format!("{narr_idx}:a");
        } else {
            // Build audio mixing filter graph
            let mut audio_filters = Vec::new();
            audio_filters.push(format!("[{narr_idx}:a]volume=1.0[anarr]"));

            for (i, track) in config.audio_tracks.iter().enumerate() {
                let input_idx = narr_idx + 1 + i;
                let mut chain = format!("[{input_idx}:a]volume={:.2}", track.volume);
                if let Some(start) = track.start_ms {
                    let delay_s = start as f64 / 1000.0;
                    chain.push_str(&format!(",adelay={:.0}|{:.0}", delay_s * 1000.0, delay_s * 1000.0));
                }
                if let Some(end) = track.end_ms {
                    let end_s = end as f64 / 1000.0;
                    chain.push_str(&format!(",atrim=end={end_s:.3}"));
                }
                chain.push_str(&format!("[at{i}]"));
                audio_filters.push(chain);
            }

            // amix all tracks
            let mix_inputs: String = std::iter::once("[anarr]".to_string())
                .chain((0..config.audio_tracks.len()).map(|i| format!("[at{i}]")))
                .collect();
            let total_inputs = 1 + config.audio_tracks.len();
            audio_filters.push(format!(
                "{mix_inputs}amix=inputs={total_inputs}:duration=first:dropout_transition=0:normalize=0[afinal]"
            ));

            filter_parts.extend(audio_filters);
            filter_complex = filter_parts.join("; ");
            audio_map = "[afinal]".to_string();
        }

        let mut cmd = Command::new("ffmpeg");
        cmd.args(["-y"]);
        for arg in &inputs {
            cmd.arg(arg);
        }
        cmd.args([
            "-filter_complex",
            &filter_complex,
            "-map",
            "[vfinal]",
            "-map",
            &audio_map,
            "-c:v",
            "libx264",
            "-preset",
            "fast",
            "-c:a",
            "aac",
            "-shortest",
            "-r",
            &config.fps.to_string(),
            &config.output_path,
        ]);

        tracing::debug!(cmd = ?cmd, "running ffmpeg");

        let result = cmd
            .output()
            .await
            .map_err(|e| SdkError::VideoRender(format!("ffmpeg execution failed: {e}")))?;

        // Cleanup temp dir (best effort)
        let _ = tokio::fs::remove_dir_all(&work_dir).await;

        if !result.status.success() {
            let stderr = String::from_utf8_lossy(&result.stderr);
            // Take the last 1000 chars — the actual error is at the end,
            // the beginning is just the ffmpeg banner.
            let tail = if stderr.len() > 1000 {
                &stderr[stderr.len() - 1000..]
            } else {
                &stderr
            };
            return Err(SdkError::VideoRender(format!(
                "ffmpeg exited with {}: {}",
                result.status, tail
            )));
        }

        Ok(config.output_path.clone())
    }
}

fn generate_srt(captions: &[crate::types::CaptionSegment]) -> String {
    let mut srt = String::new();

    // Group words into subtitle lines (roughly 6 words per line)
    let mut line_start_ms = 0u64;
    let mut line_end_ms = 0u64;
    let mut line_words: Vec<String> = Vec::new();
    let mut sub_index = 1u32;

    for cap in captions {
        if line_words.is_empty() {
            line_start_ms = cap.start_ms;
        }
        line_words.push(cap.text.clone());
        line_end_ms = cap.start_ms + cap.duration_ms;

        if line_words.len() >= 6 {
            srt.push_str(&format_srt_entry(
                sub_index,
                line_start_ms,
                line_end_ms,
                &line_words.join(" "),
            ));
            sub_index += 1;
            line_words.clear();
        }
    }

    // Flush remaining words
    if !line_words.is_empty() {
        srt.push_str(&format_srt_entry(
            sub_index,
            line_start_ms,
            line_end_ms,
            &line_words.join(" "),
        ));
    }

    srt
}

fn format_srt_entry(index: u32, start_ms: u64, end_ms: u64, text: &str) -> String {
    format!(
        "{index}\n{} --> {}\n{text}\n\n",
        format_srt_time(start_ms),
        format_srt_time(end_ms),
    )
}

fn format_srt_time(ms: u64) -> String {
    let hours = ms / 3_600_000;
    let minutes = (ms % 3_600_000) / 60_000;
    let seconds = (ms % 60_000) / 1_000;
    let millis = ms % 1_000;
    format!("{hours:02}:{minutes:02}:{seconds:02},{millis:03}")
}
