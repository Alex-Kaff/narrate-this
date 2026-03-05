use async_trait::async_trait;

use crate::error::Result;

/// Content provider that scrapes URLs or searches the web to produce narration text.
///
/// The built-in implementation is [`FirecrawlScraper`](crate::FirecrawlScraper).
#[async_trait]
pub trait ContentProvider: Send + Sync {
    /// Scrape a URL and return narration text. `title_hint` helps the LLM
    /// focus on the right content. Returns `None` if extraction fails.
    async fn extract_narration(&self, url: &str, title_hint: &str) -> Result<Option<String>>;

    /// Search the web for `query` and generate a narration from the results.
    async fn search_and_narrate(&self, query: &str) -> Result<Option<String>>;
}
