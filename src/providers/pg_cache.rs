use async_trait::async_trait;

use crate::traits::{CacheCategory, CacheProvider};

/// PostgreSQL-backed cache provider.
///
/// Stores pipeline results (narration, TTS, media) in a `cache` table with
/// optional TTL expiration. Requires the `pg-cache` feature.
///
/// # Table schema
///
/// ```sql
/// CREATE TABLE cache (
///     content_hash TEXT NOT NULL,
///     kind TEXT NOT NULL,
///     data BYTEA NOT NULL,
///     expires_at TIMESTAMPTZ,
///     PRIMARY KEY (content_hash, kind)
/// );
/// ```
pub struct PgCache {
    pool: sqlx::PgPool,
    ttl_days: i64,
}

impl PgCache {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool, ttl_days: 30 }
    }

    pub fn with_ttl_days(mut self, days: i64) -> Self {
        self.ttl_days = days;
        self
    }
}

fn category_to_str(category: &CacheCategory) -> &'static str {
    match category {
        CacheCategory::Narration => "narration",
        CacheCategory::Tts => "tts",
        CacheCategory::Media => "media",
    }
}

#[async_trait]
impl CacheProvider for PgCache {
    async fn get(&self, category: CacheCategory, key: &str) -> Option<String> {
        let kind_str = category_to_str(&category);

        let row: Option<(Vec<u8>,)> = sqlx::query_as(
            "SELECT data FROM cache WHERE content_hash = $1 AND kind = $2 \
             AND (expires_at IS NULL OR expires_at > now())",
        )
        .bind(key)
        .bind(kind_str)
        .fetch_optional(&self.pool)
        .await
        .ok()?;

        let (data,) = row?;
        String::from_utf8(data).ok()
    }

    async fn set(&self, category: CacheCategory, key: &str, value: &str) {
        let kind_str = category_to_str(&category);
        let expires_at = chrono::Utc::now() + chrono::Duration::days(self.ttl_days);

        let _ = sqlx::query(
            "INSERT INTO cache (content_hash, kind, data, expires_at) VALUES ($1, $2, $3, $4) \
             ON CONFLICT (content_hash, kind) DO UPDATE SET data = EXCLUDED.data, expires_at = EXCLUDED.expires_at",
        )
        .bind(key)
        .bind(kind_str)
        .bind(value.as_bytes())
        .bind(expires_at)
        .execute(&self.pool)
        .await;
    }
}
