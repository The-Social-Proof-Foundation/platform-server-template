use platform_core::{AppError, AppResult};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::warn;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchIndexerResponse {
    #[serde(default)]
    pub profiles: Vec<serde_json::Value>,
    #[serde(default)]
    pub posts: Vec<serde_json::Value>,
    #[serde(default)]
    pub platforms: Vec<serde_json::Value>,
    #[serde(default)]
    pub platforms_count: i64,
}

impl SearchIndexerResponse {
    pub fn empty() -> Self {
        Self {
            profiles: Vec::new(),
            posts: Vec::new(),
            platforms: Vec::new(),
            platforms_count: 0,
        }
    }
}

#[derive(Clone)]
pub struct IndexerSearchClient {
    http: Client,
    base_url: String,
}

impl IndexerSearchClient {
    pub fn new(url: Option<String>) -> Option<Self> {
        url.map(|base_url| Self {
            http: Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
        })
    }

    pub async fn search(&self, q: &str, limit: i64) -> AppResult<SearchIndexerResponse> {
        let limit_str = limit.to_string();
        let response = self
            .http
            .get(format!("{}/search", self.base_url))
            .query(&[("q", q), ("limit", limit_str.as_str())])
            .send()
            .await
            .map_err(|e| {
                warn!(error = %e, "social indexer search request failed");
                AppError::BadGateway(format!("social indexer unreachable: {e}"))
            })?;

        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            warn!(body, "social indexer search HTTP error");
            return Err(AppError::Internal(format!(
                "social indexer search HTTP error: {body}"
            )));
        }

        response
            .json::<SearchIndexerResponse>()
            .await
            .map_err(|e| AppError::Internal(format!("social indexer search parse error: {e}")))
    }
}
