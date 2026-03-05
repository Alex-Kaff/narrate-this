use async_trait::async_trait;
use serde::Deserialize;

use crate::error::{Result, SdkError};
use crate::traits::{MediaSearchProvider, MediaSearchResult};
use crate::types::MediaKind;

/// Media search provider using the [Pexels](https://pexels.com) API.
///
/// Searches for stock videos first, falling back to images if no videos match.
pub struct PexelsSearch {
    api_key: String,
    client: reqwest::Client,
}

impl PexelsSearch {
    pub fn new(api_key: &str) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("failed to build HTTP client");
        Self {
            api_key: api_key.to_string(),
            client,
        }
    }
}

// ── Photo types ──

#[derive(Deserialize)]
struct PhotoSearchResponse {
    photos: Vec<Photo>,
}

#[derive(Deserialize)]
struct Photo {
    src: PhotoSrc,
}

#[derive(Deserialize)]
struct PhotoSrc {
    landscape: String,
}

// ── Video types ──

#[derive(Deserialize)]
struct VideoSearchResponse {
    videos: Vec<PexelsVideo>,
}

#[derive(Deserialize)]
struct PexelsVideo {
    video_files: Vec<VideoFile>,
}

#[derive(Deserialize)]
struct VideoFile {
    quality: Option<String>,
    file_type: Option<String>,
    width: Option<u32>,
    link: String,
}

#[async_trait]
impl MediaSearchProvider for PexelsSearch {
    async fn search(&self, query: &str, count: usize) -> Result<Vec<MediaSearchResult>> {
        // Try videos first
        let videos = self.search_videos(query, count).await;
        if !videos.is_empty() {
            return Ok(videos);
        }

        // Fall back to images
        self.search_photos(query, count).await
    }
}

impl PexelsSearch {
    async fn search_videos(&self, query: &str, count: usize) -> Vec<MediaSearchResult> {
        let resp = match self
            .client
            .get("https://api.pexels.com/videos/search")
            .header("Authorization", &self.api_key)
            .query(&[
                ("query", query),
                ("per_page", &count.to_string()),
                ("orientation", "landscape"),
            ])
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(error = %e, "pexels video search request failed");
                return vec![];
            }
        };

        if !resp.status().is_success() {
            return vec![];
        }

        let body = match resp.text().await {
            Ok(b) => b,
            Err(_) => return vec![],
        };

        let search: VideoSearchResponse = match serde_json::from_str(&body) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(error = %e, "pexels video search parse failed");
                return vec![];
            }
        };

        search
            .videos
            .into_iter()
            .filter_map(|v| pick_best_video_file(v.video_files))
            .take(count)
            .map(|url| MediaSearchResult {
                url,
                kind: MediaKind::Video,
            })
            .collect()
    }

    async fn search_photos(&self, query: &str, count: usize) -> Result<Vec<MediaSearchResult>> {
        let resp = self
            .client
            .get("https://api.pexels.com/v1/search")
            .header("Authorization", &self.api_key)
            .query(&[
                ("query", query),
                ("per_page", &count.to_string()),
                ("orientation", "landscape"),
            ])
            .send()
            .await
            .map_err(|e| SdkError::MediaSearch(format!("photo search failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            tracing::warn!(status = %status, body = %body, "pexels photo search returned error");
            return Ok(vec![]);
        }

        let search: PhotoSearchResponse = resp
            .json()
            .await
            .map_err(|e| SdkError::MediaSearch(format!("photo search parse failed: {e}")))?;

        Ok(search
            .photos
            .into_iter()
            .map(|p| MediaSearchResult {
                url: p.src.landscape,
                kind: MediaKind::Image,
            })
            .collect())
    }
}

/// Pick the smallest MP4 file, preferring SD quality.
fn pick_best_video_file(files: Vec<VideoFile>) -> Option<String> {
    let mp4s: Vec<_> = files
        .into_iter()
        .filter(|f| f.file_type.as_deref() == Some("video/mp4"))
        .collect();

    let best = mp4s
        .iter()
        .filter(|f| f.quality.as_deref() == Some("sd"))
        .min_by_key(|f| f.width.unwrap_or(u32::MAX))
        .or_else(|| mp4s.iter().min_by_key(|f| f.width.unwrap_or(u32::MAX)))?;

    Some(best.link.clone())
}
