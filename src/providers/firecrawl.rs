use async_trait::async_trait;
use base64::Engine;
use serde::Deserialize;

use crate::error::{Result, SdkError};
use crate::traits::ContentProvider;
use crate::types::NarrationStyle;

const PAYWALL_SIGNAL: &str = "PAYWALL_DETECTED";

const PAYWALL_SUFFIX: &str = "\
\n\nIMPORTANT: If the page content is blocked by a paywall, login wall, cookie wall, \
or is otherwise inaccessible (e.g. \"subscribe to read\", \"create an account\", \
\"this content is for members only\"), respond with EXACTLY the text: PAYWALL_DETECTED \
— nothing else.";

/// Configuration for the Firecrawl content scraper.
pub struct FirecrawlConfig {
    pub base_url: String,
    /// Full override for the article narration prompt. When `None`, a prompt is
    /// generated from [`style`](Self::style).
    pub narration_prompt: Option<String>,
    /// Full override for the search-narration prompt. When `None`, a prompt is
    /// generated from [`style`](Self::style).
    pub search_narration_prompt: Option<String>,
    /// Style variables interpolated into the default prompt templates.
    pub style: NarrationStyle,
    pub timeout_secs: u64,
}

impl Default for FirecrawlConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:3002".into(),
            narration_prompt: None,
            search_narration_prompt: None,
            style: NarrationStyle::default(),
            timeout_secs: 120,
        }
    }
}

/// Content provider backed by a [Firecrawl](https://firecrawl.dev) instance.
///
/// Scrapes article URLs and searches the web, using Firecrawl's LLM extraction
/// to produce narration-ready text. Handles paywalls by falling back to search.
pub struct FirecrawlScraper {
    config: FirecrawlConfig,
    client: reqwest::Client,
}

impl FirecrawlScraper {
    pub fn new(base_url: &str) -> Self {
        Self::with_config(FirecrawlConfig {
            base_url: base_url.to_string(),
            ..Default::default()
        })
    }

    pub fn with_config(config: FirecrawlConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()
            .expect("failed to build HTTP client");
        Self { config, client }
    }
}

#[derive(Deserialize)]
struct ScrapeResponse {
    success: bool,
    data: Option<ScrapeData>,
    error: Option<String>,
}

#[derive(Deserialize)]
struct ScrapeData {
    markdown: Option<String>,
    extract: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct SearchResponse {
    success: bool,
    data: Option<Vec<SearchResult>>,
}

#[derive(Deserialize)]
struct SearchResult {
    #[allow(dead_code)]
    markdown: Option<String>,
    description: Option<String>,
    #[serde(rename = "json")]
    extract_json: Option<serde_json::Value>,
}

enum ScrapeResult {
    Narration(String),
    Paywall,
    Failed,
}

#[async_trait]
impl ContentProvider for FirecrawlScraper {
    async fn extract_narration(&self, url: &str, title_hint: &str) -> Result<Option<String>> {
        // Step 1: Try direct scrape
        match self.scrape_article(url).await {
            ScrapeResult::Narration(text) => return Ok(Some(text)),
            ScrapeResult::Paywall => {
                tracing::warn!(url = %url, "paywall detected, falling back to search");
            }
            ScrapeResult::Failed => {
                tracing::warn!(url = %url, "scrape failed, falling back to search");
            }
        }

        // Step 2: Paywall or failure — search for the topic instead (best-effort)
        match self.search_and_narrate(title_hint).await {
            Ok(result) => Ok(result),
            Err(e) => {
                tracing::warn!(error = %e, "search fallback failed, returning None");
                Ok(None)
            }
        }
    }

    async fn search_and_narrate(&self, query: &str) -> Result<Option<String>> {
        let endpoint = format!("{}/v1/search", &self.config.base_url);

        let body = serde_json::json!({
            "query": query,
            "limit": 3,
            "scrapeOptions": {
                "formats": ["extract"],
                "extract": {
                    "prompt": &self.search_narration_prompt(),
                },
            },
        });

        let resp = self
            .client
            .post(&endpoint)
            .json(&body)
            .send()
            .await
            .map_err(|e| SdkError::WebScraper(format!("search request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            tracing::warn!(status = %status, body = %body, "firecrawl search returned error");
            return Ok(None);
        }

        let search: SearchResponse = resp
            .json()
            .await
            .map_err(|e| SdkError::WebScraper(format!("search parse failed: {e}")))?;

        if !search.success {
            return Ok(None);
        }

        let results = match search.data {
            Some(r) if !r.is_empty() => r,
            _ => return Ok(None),
        };

        // Firecrawl search with extract returns LLM-generated content in the `json` field.
        // Pick the first result that has usable extracted text.
        for result in &results {
            if let Some(extracted) = &result.extract_json {
                if let Some(text) = extract_text_from_json(extracted) {
                    if !text.is_empty() {
                        return Ok(Some(text));
                    }
                }
            }
        }

        // Fallback: collect descriptions and narrate from context
        let mut context = String::new();
        for result in &results {
            if let Some(desc) = &result.description {
                if !desc.trim().is_empty() {
                    context.push_str(desc.trim());
                    context.push('\n');
                }
            }
        }

        if context.is_empty() {
            return Ok(None);
        }

        self.narrate_from_context(query, &context).await
    }
}

impl FirecrawlScraper {
    fn narration_prompt(&self) -> String {
        if let Some(p) = &self.config.narration_prompt {
            return p.clone();
        }
        let s = &self.config.style;
        format!(
            "You are a {role}. Given this article, write a narration segment \
             suitable for a spoken audio broadcast. Requirements:\n\
             - {length}\n\
             - {tone} tone, as if {persona} is reading it\n\
             - {structure}\n\
             - Do NOT include any stage directions, speaker labels, or meta-commentary\n\
             - Return ONLY the narration text, nothing else",
            role = s.role,
            length = s.length,
            tone = s.tone,
            persona = s.persona,
            structure = s.structure,
        )
    }

    fn search_narration_prompt(&self) -> String {
        if let Some(p) = &self.config.search_narration_prompt {
            return p.clone();
        }
        let s = &self.config.style;
        format!(
            "You are a {role}. You will receive search results about a topic. \
             Using the information from these sources, write a narration segment suitable for a spoken \
             audio broadcast. Requirements:\n\
             - {length}\n\
             - {tone} tone, as if {persona} is reading it\n\
             - Synthesize information from the available sources\n\
             - {structure}\n\
             - Do NOT include any stage directions, speaker labels, or meta-commentary\n\
             - Do NOT mention that you used search results or multiple sources\n\
             - Return ONLY the narration text, nothing else",
            role = s.role,
            length = s.length,
            tone = s.tone,
            persona = s.persona,
            structure = s.structure,
        )
    }

    async fn scrape_article(&self, url: &str) -> ScrapeResult {
        let endpoint = format!("{}/v1/scrape", &self.config.base_url);

        let body = serde_json::json!({
            "url": url,
            "formats": ["extract"],
            "extract": {
                "prompt": format!("{}{}", self.narration_prompt(), PAYWALL_SUFFIX),
            },
        });

        let resp = match self.client.post(&endpoint).json(&body).send().await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(error = %e, "firecrawl scrape request failed");
                return ScrapeResult::Failed;
            }
        };

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            tracing::warn!(status = %status, body = %body, "firecrawl scrape returned error");
            return ScrapeResult::Failed;
        }

        // Read the raw body so we can inspect and parse it.
        let raw = match resp.text().await {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(error = %e, "failed to read scrape response body");
                return ScrapeResult::Failed;
            }
        };

        let scrape: ScrapeResponse = match serde_json::from_str(&raw) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    raw_preview = %&raw[..raw.len().min(500)],
                    "scrape response parse failed"
                );
                return ScrapeResult::Failed;
            }
        };

        if !scrape.success {
            let err = scrape.error.unwrap_or_default();
            tracing::warn!(error = %err, "firecrawl scrape failed");
            return ScrapeResult::Failed;
        }

        let data = match scrape.data {
            Some(d) => d,
            None => {
                tracing::warn!("scrape success=true but data is null");
                return ScrapeResult::Failed;
            }
        };

        if let Some(text) = extract_text_from_data(&data) {
            if text.contains(PAYWALL_SIGNAL) {
                return ScrapeResult::Paywall;
            }
            if !text.is_empty() {
                return ScrapeResult::Narration(text);
            }
        }

        tracing::warn!(
            extract_preview = ?data.extract.as_ref().map(|v| v.to_string().chars().take(200).collect::<String>()),
            markdown_len = ?data.markdown.as_ref().map(|m| m.len()),
            "extract_text_from_data returned None"
        );
        ScrapeResult::Failed
    }

    async fn narrate_from_context(
        &self,
        title: &str,
        context: &str,
    ) -> Result<Option<String>> {
        let endpoint = format!("{}/v1/scrape", &self.config.base_url);

        let html_content = format!(
            "<html><body><h1>{}</h1><article>{}</article></body></html>",
            html_escape(title),
            html_escape(context),
        );
        let data_url = format!(
            "data:text/html;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(html_content.as_bytes())
        );

        let body = serde_json::json!({
            "url": data_url,
            "formats": ["extract"],
            "extract": {
                "prompt": &self.search_narration_prompt(),
            },
        });

        let resp = self
            .client
            .post(&endpoint)
            .json(&body)
            .send()
            .await
            .map_err(|e| SdkError::WebScraper(format!("narrate-from-context failed: {e}")))?;

        if !resp.status().is_success() {
            return Ok(None);
        }

        let scrape: ScrapeResponse = match resp.json().await {
            Ok(s) => s,
            Err(_) => return Ok(None),
        };

        if !scrape.success {
            return Ok(None);
        }

        let data = match scrape.data {
            Some(d) => d,
            None => return Ok(None),
        };

        Ok(extract_text_from_data(&data))
    }
}

/// Pull the narration text out of Firecrawl's extract response.
fn extract_text_from_data(data: &ScrapeData) -> Option<String> {
    if let Some(extract) = &data.extract {
        let text = match extract {
            serde_json::Value::String(s) => Some(s.clone()),
            serde_json::Value::Object(map) => {
                map.get("narration")
                    .or_else(|| map.get("text"))
                    .or_else(|| map.get("content"))
                    .or_else(|| map.get("script"))
                    .and_then(|v| v.as_str())
                    .map(String::from)
                    .or_else(|| {
                        let strings: Vec<_> =
                            map.values().filter_map(|v| v.as_str()).collect();
                        if strings.len() == 1 {
                            Some(strings[0].to_string())
                        } else {
                            None
                        }
                    })
            }
            _ => None,
        };

        if let Some(t) = text {
            let trimmed = t.trim().to_string();
            if !trimmed.is_empty() {
                return Some(trimmed);
            }
        }
    }

    // Fallback to markdown if extract is empty
    if let Some(md) = &data.markdown {
        let trimmed = md.trim();
        if !trimmed.is_empty() && trimmed.len() > 50 {
            return Some(trimmed.to_string());
        }
    }

    None
}

/// Extract narration text from the `json` field returned by Firecrawl search with extract.
fn extract_text_from_json(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() { None } else { Some(trimmed.to_string()) }
        }
        serde_json::Value::Object(map) => {
            // Try common field names first
            for key in &["narration", "broadcastNarration", "text", "content", "script"] {
                if let Some(serde_json::Value::String(s)) = map.get(*key) {
                    let trimmed = s.trim();
                    if !trimmed.is_empty() {
                        return Some(trimmed.to_string());
                    }
                }
            }
            // If only one string value, use it
            let strings: Vec<_> = map.values().filter_map(|v| v.as_str()).collect();
            if strings.len() == 1 && !strings[0].trim().is_empty() {
                Some(strings[0].trim().to_string())
            } else {
                None
            }
        }
        _ => None,
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
