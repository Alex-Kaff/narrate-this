use async_trait::async_trait;
use serde::Deserialize;

use crate::error::{Result, SdkError};
use crate::traits::{
    KeywordExtractor, MediaPlanner, MediaSearchProvider, PlannedMedia, TextTransformer,
};
use crate::types::{KeywordResult, MediaAsset, MediaFallback, TimedChunk};

const KEYWORD_PROMPT: &str = "\
Extract 2-3 concrete, visual search keywords for a stock photo that would \
visually represent this paragraph of a news broadcast. Focus on tangible objects, \
scenes, or actions (e.g. \"stock market trading floor\", \"solar panels field\", \
\"doctor examining patient\"). Return ONLY a comma-separated list of keywords, \
nothing else.";

/// Configuration for OpenAI-backed providers.
///
/// Set `base_url` to point at any OpenAI-compatible API (Ollama, LM Studio,
/// vLLM, LocalAI, llama.cpp, etc.).
pub struct OpenAiConfig {
    pub api_key: String,
    pub model: String,
    pub timeout_secs: u64,
    /// Base URL for the API. Default: `"https://api.openai.com"`.
    ///
    /// For local LLMs, set this to the server address, e.g.
    /// `"http://localhost:11434/v1"` (Ollama) or `"http://localhost:1234"` (LM Studio).
    pub base_url: String,
}

impl Default for OpenAiConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            model: "gpt-4o-mini".into(),
            timeout_secs: 30,
            base_url: "https://api.openai.com".into(),
        }
    }
}

/// Keyword extractor using OpenAI's chat completions API.
///
/// Extracts 2–3 concrete, visual search keywords from narration text chunks
/// for stock media lookup.
pub struct OpenAiKeywords {
    config: OpenAiConfig,
    client: reqwest::Client,
}

impl OpenAiKeywords {
    pub fn new(config: OpenAiConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()
            .expect("failed to build HTTP client");
        Self { config, client }
    }
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: MessageContent,
}

#[derive(Deserialize)]
struct MessageContent {
    content: Option<String>,
}

#[async_trait]
impl KeywordExtractor for OpenAiKeywords {
    async fn extract_keywords(&self, text: &str) -> Result<KeywordResult> {
        let body = serde_json::json!({
            "model": &self.config.model,
            "messages": [
                { "role": "system", "content": KEYWORD_PROMPT },
                { "role": "user", "content": text },
            ],
            "max_tokens": 60,
            "temperature": 0.3,
        });

        let url = format!("{}/v1/chat/completions", self.config.base_url.trim_end_matches('/'));

        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.config.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| SdkError::Llm(format!("request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(SdkError::Llm(format!("API returned {status}: {body}")));
        }

        let chat: ChatResponse = resp
            .json()
            .await
            .map_err(|e| SdkError::Llm(format!("response parse failed: {e}")))?;

        let content = chat
            .choices
            .first()
            .and_then(|c| c.message.content.as_ref())
            .ok_or_else(|| SdkError::Llm("empty response".into()))?;

        let keywords: Vec<String> = content
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        Ok(KeywordResult { keywords })
    }
}

// ── Text Transformer ──

const REWRITE_SYSTEM_PROMPT: &str = "\
You are a text rewriter. You will receive a piece of narration text and then user instructions on how to modify it. \
Rewrite the narration to match the requested modification. \
Do NOT modify anything else than what's requested. \
The requested modification may be to change the style, tone, or to even rewrite it to focus on a different subject, or to extract only some information from it. \
Reply ONLY with the new formatted text exactly as asked.";

/// Text transformer using OpenAI's chat completions API.
///
/// Rewrites narration text according to custom style instructions (e.g.
/// "casual podcast tone", "formal news broadcast").
pub struct OpenAiTransform {
    client: reqwest::Client,
    api_key: String,
    model: String,
    base_url: String,
    style_instructions: String,
}

impl OpenAiTransform {
    /// Create a new text transformer with style instructions.
    ///
    /// `style_instructions` describes how to rewrite the text, e.g.
    /// "Serious tone, no cringe" or "Casual and funny, like a podcast host".
    pub fn new(api_key: &str, style_instructions: &str) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .expect("failed to build HTTP client");
        Self {
            client,
            api_key: api_key.to_string(),
            model: "gpt-4o-mini".into(),
            base_url: "https://api.openai.com".into(),
            style_instructions: style_instructions.to_string(),
        }
    }

    pub fn with_model(mut self, model: &str) -> Self {
        self.model = model.to_string();
        self
    }

    /// Set a custom base URL for any OpenAI-compatible API.
    pub fn with_base_url(mut self, base_url: &str) -> Self {
        self.base_url = base_url.trim_end_matches('/').to_string();
        self
    }
}

// ── Media Planner ──

/// LLM-based media planner that uses OpenAI to assign user-provided media
/// assets to narration chunks based on semantic relevance, with optional
/// stock search fallback for unmatched chunks.
///
/// # Example
///
/// ```rust,no_run
/// use narrate_this::{
///     LlmMediaPlanner, MediaAsset, MediaFallback, OpenAiConfig, OpenAiKeywords, PexelsSearch,
/// };
///
/// // Assets only
/// let planner = LlmMediaPlanner::new(OpenAiConfig {
///     api_key: "your-key".into(),
///     ..Default::default()
/// })
/// .assets(vec![
///     MediaAsset::image("./hero.jpg", "A rocket launching into space"),
/// ]);
///
/// // Assets + stock fallback
/// let planner = LlmMediaPlanner::new(OpenAiConfig {
///     api_key: "your-key".into(),
///     ..Default::default()
/// })
/// .assets(vec![
///     MediaAsset::image("./hero.jpg", "A rocket launching into space"),
/// ])
/// .stock_search(
///     OpenAiKeywords::new(OpenAiConfig {
///         api_key: "your-key".into(),
///         ..Default::default()
///     }),
///     PexelsSearch::new("your-pexels-key"),
/// )
/// .fallback(MediaFallback::StockSearch);
/// ```
pub struct LlmMediaPlanner {
    client: reqwest::Client,
    config: OpenAiConfig,
    allow_reuse: bool,
    max_reuse: Option<usize>,
    assets: Vec<MediaAsset>,
    keyword_extractor: Option<Box<dyn KeywordExtractor>>,
    media_search: Option<Box<dyn MediaSearchProvider>>,
    fallback: MediaFallback,
}

impl LlmMediaPlanner {
    /// Create a new LLM-based media planner with the given OpenAI configuration.
    ///
    /// Configure with `.assets()`, `.stock_search()`, `.fallback()`, `.allow_reuse()`,
    /// and `.max_reuse()` before passing to [`PipelineBuilder::media()`](crate::PipelineBuilder).
    pub fn new(config: OpenAiConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()
            .expect("failed to build HTTP client");
        Self {
            client,
            config,
            allow_reuse: true,
            max_reuse: None,
            assets: Vec::new(),
            keyword_extractor: None,
            media_search: None,
            fallback: MediaFallback::default(),
        }
    }

    /// Provide predetermined media assets with descriptions for the LLM to match.
    pub fn assets(mut self, assets: Vec<MediaAsset>) -> Self {
        self.assets = assets;
        self
    }

    /// Add stock media search as a source. Used for fallback when no user asset
    /// matches, or when no assets are provided.
    pub fn stock_search(
        mut self,
        keywords: impl KeywordExtractor + 'static,
        search: impl MediaSearchProvider + 'static,
    ) -> Self {
        self.keyword_extractor = Some(Box::new(keywords));
        self.media_search = Some(Box::new(search));
        self
    }

    /// What to do when the LLM can't match a user asset to a chunk.
    /// Default: [`MediaFallback::StockSearch`] (requires `.stock_search()` to be set).
    pub fn fallback(mut self, fallback: MediaFallback) -> Self {
        self.fallback = fallback;
        self
    }

    /// Whether the same asset can be assigned to multiple chunks. Default: `true`.
    pub fn allow_reuse(mut self, allow: bool) -> Self {
        self.allow_reuse = allow;
        self
    }

    /// Maximum number of times a single asset can be reused. `None` means unlimited.
    pub fn max_reuse(mut self, max: Option<usize>) -> Self {
        self.max_reuse = max;
        self
    }

    /// Run the LLM to assign assets to chunks. Returns `Some(asset_index)` or `None` per chunk.
    async fn plan_assets(
        &self,
        chunks: &[TimedChunk],
    ) -> Result<Vec<Option<usize>>> {
        if chunks.is_empty() {
            return Ok(vec![]);
        }
        if self.assets.is_empty() {
            return Ok(vec![None; chunks.len()]);
        }

        let prompt = self.build_prompt(chunks);

        let body = serde_json::json!({
            "model": &self.config.model,
            "messages": [
                { "role": "user", "content": prompt },
            ],
            "max_tokens": 256,
            "temperature": 0.2,
        });

        let url = format!("{}/v1/chat/completions", self.config.base_url.trim_end_matches('/'));

        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.config.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| SdkError::MediaPlanner(format!("request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(SdkError::MediaPlanner(format!(
                "API returned {status}: {body}"
            )));
        }

        let chat: ChatResponse = resp
            .json()
            .await
            .map_err(|e| SdkError::MediaPlanner(format!("response parse failed: {e}")))?;

        let content = chat
            .choices
            .first()
            .and_then(|c| c.message.content.as_ref())
            .ok_or_else(|| SdkError::MediaPlanner("empty response from OpenAI".into()))?;

        let json_str = extract_json_array(content)
            .ok_or_else(|| SdkError::MediaPlanner(format!("no JSON array in response: {content}")))?;

        let raw: Vec<Option<usize>> = serde_json::from_str(json_str)
            .map_err(|e| SdkError::MediaPlanner(format!("JSON parse failed: {e} — raw: {json_str}")))?;

        let asset_count = self.assets.len();
        let result: Vec<Option<usize>> = (0..chunks.len())
            .map(|i| {
                raw.get(i)
                    .copied()
                    .flatten()
                    .filter(|&idx| idx < asset_count)
            })
            .collect();

        Ok(result)
    }

    /// Stock-search a single chunk via keyword extraction + media search.
    async fn stock_search_chunk(&self, idx: usize, chunk: &TimedChunk) -> Option<PlannedMedia> {
        let kw = self.keyword_extractor.as_ref()?;
        let search = self.media_search.as_ref()?;

        let keywords = match kw.extract_keywords(&chunk.text).await {
            Ok(kr) => kr.keywords,
            Err(e) => {
                tracing::warn!(error = %e, chunk = idx, "keyword extraction failed");
                return None;
            }
        };

        let query = keywords.join(", ");
        let results = match search.search(&query, 1).await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(error = %e, chunk = idx, "media search failed");
                return None;
            }
        };

        results.into_iter().next().map(|r| PlannedMedia {
            source: r.source,
            kind: r.kind,
        })
    }

    fn build_prompt(&self, chunks: &[TimedChunk]) -> String {
        let mut prompt = String::from(
            "You are a media planner for a narrated video. \
             You will be given narration text chunks (each with timing) and a list of \
             available media assets (images/videos with descriptions).\n\n\
             For each chunk, select the most visually appropriate media asset by returning \
             its index, or null if no asset fits well.\n\n",
        );

        if !self.allow_reuse {
            prompt.push_str("IMPORTANT: Each asset may only be used ONCE.\n\n");
        } else if let Some(max) = self.max_reuse {
            prompt.push_str(&format!(
                "IMPORTANT: Each asset may be used at most {max} times.\n\n"
            ));
        }

        prompt.push_str("Rules:\n\
             - Match assets to chunks based on semantic relevance between the asset description and chunk content\n\
             - Consider narrative flow — avoid jarring visual transitions\n\
             - Return null for chunks where no asset is a good fit\n\n");

        prompt.push_str("Chunks:\n");
        for (i, chunk) in chunks.iter().enumerate() {
            prompt.push_str(&format!(
                "  {i}: [{:.1}s–{:.1}s] \"{}\"\n",
                chunk.start_ms / 1000.0,
                chunk.end_ms / 1000.0,
                truncate_for_prompt(&chunk.text, 200),
            ));
        }

        prompt.push_str("\nAvailable assets:\n");
        for (i, asset) in self.assets.iter().enumerate() {
            prompt.push_str(&format!(
                "  {i}: [{:?}] \"{}\"\n",
                asset.kind, asset.description,
            ));
        }

        prompt.push_str(&format!(
            "\nReturn ONLY a JSON array of length {} where each element is either \
             an asset index (0-based integer) or null. Example: [0, 1, null, 0, 2]\n",
            chunks.len()
        ));

        prompt
    }
}

fn truncate_for_prompt(s: &str, max_chars: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars).collect();
        format!("{truncated}…")
    }
}

#[async_trait]
impl MediaPlanner for LlmMediaPlanner {
    async fn plan(&self, chunks: &[TimedChunk]) -> Result<Vec<Option<PlannedMedia>>> {
        if chunks.is_empty() {
            return Ok(vec![]);
        }

        let has_assets = !self.assets.is_empty();
        let has_stock = self.keyword_extractor.is_some() && self.media_search.is_some();

        // Phase 1: LLM asset assignment (if assets provided)
        let mut results: Vec<Option<PlannedMedia>> = vec![None; chunks.len()];
        let mut unmatched: Vec<usize> = Vec::new();

        if has_assets {
            match self.plan_assets(chunks).await {
                Ok(plan) => {
                    let plan_len = plan.len();
                    for (i, assignment) in plan.into_iter().enumerate() {
                        if i >= chunks.len() {
                            break;
                        }
                        match assignment {
                            Some(idx) if idx < self.assets.len() => {
                                let asset = &self.assets[idx];
                                results[i] = Some(PlannedMedia {
                                    source: asset.source.clone(),
                                    kind: asset.kind,
                                });
                            }
                            _ => unmatched.push(i),
                        }
                    }
                    for i in plan_len..chunks.len() {
                        unmatched.push(i);
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "LLM media planning failed, falling back");
                    unmatched = (0..chunks.len()).collect();
                }
            }
        } else {
            unmatched = (0..chunks.len()).collect();
        }

        // Phase 2: stock search fallback for unmatched chunks
        if !unmatched.is_empty() && has_stock {
            match self.fallback {
                MediaFallback::StockSearch => {
                    let futs: Vec<_> = unmatched
                        .iter()
                        .map(|&idx| self.stock_search_chunk(idx, &chunks[idx]))
                        .collect();
                    let stock_results = futures::future::join_all(futs).await;
                    for (i, result) in unmatched.iter().zip(stock_results) {
                        results[*i] = result;
                    }
                }
                MediaFallback::Skip => {}
            }
        }

        Ok(results)
    }
}

/// Extract a JSON array substring from LLM output that may contain markdown fences.
fn extract_json_array(s: &str) -> Option<&str> {
    let trimmed = s.trim();
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        return Some(trimmed);
    }
    if let Some(start) = s.find('[')
        && let Some(end) = s.rfind(']')
        && start < end
    {
        return Some(&s[start..=end]);
    }
    None
}

#[async_trait]
impl TextTransformer for OpenAiTransform {
    async fn transform(&self, text: &str) -> Result<String> {
        let user_message = format!(
            "Style instructions: {}\n\n---\n\nText to rewrite:\n{}",
            self.style_instructions, text
        );

        let body = serde_json::json!({
            "model": &self.model,
            "messages": [
                { "role": "system", "content": REWRITE_SYSTEM_PROMPT },
                { "role": "user", "content": user_message },
            ],
            "temperature": 0.7,
        });

        let url = format!("{}/v1/chat/completions", self.base_url.trim_end_matches('/'));

        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| SdkError::Llm(format!("transform request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(SdkError::Llm(format!("transform API returned {status}: {body}")));
        }

        let chat: ChatResponse = resp
            .json()
            .await
            .map_err(|e| SdkError::Llm(format!("transform response parse failed: {e}")))?;

        let content = chat
            .choices
            .first()
            .and_then(|c| c.message.content.as_ref())
            .ok_or_else(|| SdkError::Llm("empty transform response from OpenAI".into()))?;

        Ok(content.trim().to_string())
    }
}
