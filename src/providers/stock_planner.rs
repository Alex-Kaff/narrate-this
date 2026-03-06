use async_trait::async_trait;

use crate::error::Result;
use crate::traits::{KeywordExtractor, MediaPlanner, MediaSearchProvider, PlannedMedia};
use crate::types::TimedChunk;

/// Media planner that uses keyword extraction + stock media search.
///
/// For each narration chunk, extracts keywords and searches a stock media
/// provider (e.g. Pexels) for matching images or videos.
///
/// # Example
///
/// ```rust,no_run
/// use narrate_this::{OpenAiConfig, OpenAiKeywords, PexelsSearch, StockMediaPlanner};
///
/// let planner = StockMediaPlanner::new(
///     OpenAiKeywords::new(OpenAiConfig {
///         api_key: "your-openai-key".into(),
///         ..Default::default()
///     }),
///     PexelsSearch::new("your-pexels-key"),
/// );
/// ```
pub struct StockMediaPlanner {
    keyword_extractor: Box<dyn KeywordExtractor>,
    media_search: Box<dyn MediaSearchProvider>,
}

impl StockMediaPlanner {
    /// Create a new stock media planner with the given keyword extractor and search provider.
    pub fn new(
        keywords: impl KeywordExtractor + 'static,
        search: impl MediaSearchProvider + 'static,
    ) -> Self {
        Self {
            keyword_extractor: Box::new(keywords),
            media_search: Box::new(search),
        }
    }
}

#[async_trait]
impl MediaPlanner for StockMediaPlanner {
    async fn plan(&self, chunks: &[TimedChunk]) -> Result<Vec<Option<PlannedMedia>>> {
        let futs: Vec<_> = chunks
            .iter()
            .enumerate()
            .map(|(idx, chunk)| {
                let kw = &*self.keyword_extractor;
                let search = &*self.media_search;
                async move {
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
            })
            .collect();

        Ok(futures::future::join_all(futs).await)
    }
}
