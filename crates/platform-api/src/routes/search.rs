use axum::extract::{Extension, Query};
use axum::Json;
use platform_core::AppError;
use platform_db::{
    clear_search_history, delete_search_history_entry, get_bool_setting, list_search_history,
    record_search, SearchHistoryRow,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::ApiResult;
use crate::indexer::SearchIndexerResponse;
use crate::middleware::AuthUser;
use crate::recommend::cache::RecommendationCache;
use crate::search::{enrich_search_results, SearchEnrichmentContext};
use crate::state::SharedApiState;

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub q: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default = "default_page")]
    pub page: i64,
    #[serde(rename = "filter_types")]
    pub filter_types: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SearchHistoryListQuery {
    #[serde(default = "default_history_limit")]
    pub limit: i64,
}

#[derive(Debug, Deserialize)]
pub struct DeleteSearchHistoryQuery {
    pub q: Option<String>,
}

fn default_limit() -> i64 {
    20
}

fn default_page() -> i64 {
    1
}

fn default_history_limit() -> i64 {
    20
}

pub(crate) fn clamp_search_limit(limit: i64) -> i64 {
    limit.clamp(1, 100)
}

#[derive(serde::Serialize)]
struct SearchHistoryItem {
    query: String,
    #[serde(rename = "filterTypes", skip_serializing_if = "Option::is_none")]
    filter_types: Option<String>,
    #[serde(rename = "searchedAt")]
    searched_at: chrono::DateTime<chrono::Utc>,
}

impl From<SearchHistoryRow> for SearchHistoryItem {
    fn from(row: SearchHistoryRow) -> Self {
        Self {
            query: row.query,
            filter_types: row.filter_types,
            searched_at: row.updated_at,
        }
    }
}

pub async fn search_handler(
    Extension(state): Extension<SharedApiState>,
    Extension(auth): Extension<AuthUser>,
    Query(query): Query<SearchQuery>,
) -> ApiResult<Json<SearchIndexerResponse>> {
    let q = query.q.as_deref().unwrap_or("").trim();
    if q.is_empty() {
        return Ok(Json(SearchIndexerResponse::empty()));
    }

    let _page = query.page.max(1);
    let limit = clamp_search_limit(query.limit);

    let client = state
        .indexer_search
        .as_ref()
        .ok_or_else(|| AppError::Config("SOCIAL_INDEXER_URL is not configured".into()))?;

    let wallet: (String,) =
        sqlx::query_as("SELECT wallet_address FROM users WHERE user_id = $1::uuid")
            .bind(&auth.user_id)
            .fetch_one(state.pg_read())
            .await?;

    let allow_nsfw =
        get_bool_setting(state.pg_read(), &auth.user_id, "content.nsfw.allow", false).await?;

    let mut response = client.search(q, limit).await?;

    let cache = RecommendationCache::new(state.redis());
    let ctx = SearchEnrichmentContext {
        pool: state.pg_read(),
        cache: &cache,
        wallet_address: &wallet.0,
        allow_nsfw,
        filter_types: query.filter_types.as_deref(),
    };
    enrich_search_results(&ctx, &mut response).await?;

    if let Err(err) = record_search(
        state.pg(),
        &auth.user_id,
        q,
        query.filter_types.as_deref(),
    )
    .await
    {
        tracing::warn!(error = %err, user_id = %auth.user_id, "failed to record search history");
    }

    state.metrics.inc_search_request();

    Ok(Json(response))
}

pub async fn list_search_history_handler(
    Extension(state): Extension<SharedApiState>,
    Extension(auth): Extension<AuthUser>,
    Query(query): Query<SearchHistoryListQuery>,
) -> ApiResult<Json<Value>> {
    let rows = list_search_history(state.pg_read(), &auth.user_id, query.limit).await?;
    let history: Vec<SearchHistoryItem> = rows.into_iter().map(SearchHistoryItem::from).collect();
    Ok(Json(json!({ "history": history })))
}

pub async fn delete_search_history_handler(
    Extension(state): Extension<SharedApiState>,
    Extension(auth): Extension<AuthUser>,
    Query(query): Query<DeleteSearchHistoryQuery>,
    body: Option<Json<Value>>,
) -> ApiResult<Json<Value>> {
    let query_text = query
        .q
        .or_else(|| {
            body.as_ref().and_then(|Json(value)| {
                value
                    .get("query")
                    .or_else(|| value.get("q"))
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
            })
        })
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    if let Some(query_text) = query_text {
        let deleted = delete_search_history_entry(state.pg(), &auth.user_id, &query_text).await?;
        return Ok(Json(json!({ "ok": true, "deleted": deleted })));
    }

    let deleted_count = clear_search_history(state.pg(), &auth.user_id).await?;
    Ok(Json(json!({ "ok": true, "deletedCount": deleted_count })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_search_limit_bounds() {
        assert_eq!(clamp_search_limit(0), 1);
        assert_eq!(clamp_search_limit(1), 1);
        assert_eq!(clamp_search_limit(20), 20);
        assert_eq!(clamp_search_limit(100), 100);
        assert_eq!(clamp_search_limit(500), 100);
    }
}
