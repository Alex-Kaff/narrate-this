use async_trait::async_trait;
use serde::Deserialize;

use crate::error::{Result, SdkError};
use crate::traits::{KeywordExtractor, TextTransformer};
use crate::types::KeywordResult;

const KEYWORD_PROMPT: &str = "\
Extract 2-3 concrete, visual search keywords for a stock photo that would \
visually represent this paragraph of a news broadcast. Focus on tangible objects, \
scenes, or actions (e.g. \"stock market trading floor\", \"solar panels field\", \
\"doctor examining patient\"). Return ONLY a comma-separated list of keywords, \
nothing else.";

pub struct OpenAiConfig {
    pub api_key: String,
    pub model: String,
    pub timeout_secs: u64,
}

impl Default for OpenAiConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            model: "gpt-4o-mini".into(),
            timeout_secs: 30,
        }
    }
}

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

        let resp = self
            .client
            .post("https://api.openai.com/v1/chat/completions")
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
            .ok_or_else(|| SdkError::Llm("empty response from OpenAI".into()))?;

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

pub struct OpenAiTransform {
    client: reqwest::Client,
    api_key: String,
    model: String,
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
            style_instructions: style_instructions.to_string(),
        }
    }

    pub fn with_model(mut self, model: &str) -> Self {
        self.model = model.to_string();
        self
    }
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

        let resp = self
            .client
            .post("https://api.openai.com/v1/chat/completions")
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
