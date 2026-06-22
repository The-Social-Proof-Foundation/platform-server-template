use std::collections::HashMap;

use platform_core::AppResult;
use serde_json::{json, Value};
use sqlx::FromRow;
use sqlx::PgPool;

use crate::indexer::SearchIndexerResponse;
use crate::recommend::cache::RecommendationCache;

use super::filter::{apply_filter_types, apply_user_filters, json_string_field, parse_filter_types};

#[derive(Debug, Clone, FromRow)]
pub struct ContentVectorEnrichment {
    pub content_id: String,
    pub creator_wallet_address: Option<String>,
    pub performance_metrics: Option<Value>,
    pub nsfw: bool,
    pub moderation_override: Option<String>,
    pub extra_metadata: Option<Value>,
}

impl ContentVectorEnrichment {
    pub fn is_deleted(&self) -> bool {
        self.extra_metadata
            .as_ref()
            .and_then(|meta| meta.get("deleted"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }

    fn to_platform_metrics_json(&self) -> Value {
        json!({
            "performanceMetrics": self.performance_metrics,
            "nsfw": self.nsfw,
            "moderationOverride": self.moderation_override,
            "deleted": self.is_deleted(),
            "creatorWalletAddress": self.creator_wallet_address,
        })
    }
}

pub struct SearchEnrichmentContext<'a> {
    pub pool: &'a PgPool,
    pub cache: &'a RecommendationCache,
    pub wallet_address: &'a str,
    pub allow_nsfw: bool,
    pub filter_types: Option<&'a str>,
}

pub async fn enrich_search_results(
    ctx: &SearchEnrichmentContext<'_>,
    response: &mut SearchIndexerResponse,
) -> AppResult<()> {
    let enrichments = load_content_enrichments(ctx.pool, &response.posts).await?;

    merge_platform_metrics(&mut response.posts, &enrichments);

    let blocked = ctx
        .cache
        .get_blocked_user_ids(ctx.wallet_address)
        .await?
        .into_iter()
        .collect();

    apply_user_filters(response, &blocked, ctx.allow_nsfw, &enrichments);

    if let Some(filters) = parse_filter_types(ctx.filter_types) {
        apply_filter_types(response, &filters);
    }

    Ok(())
}

async fn load_content_enrichments(
    pool: &PgPool,
    posts: &[Value],
) -> AppResult<HashMap<String, ContentVectorEnrichment>> {
    let mut post_ids = Vec::new();
    for post in posts {
        if let Some(id) = json_string_field(post, &["post_id", "postId", "id"]) {
            if !post_ids.contains(&id) {
                post_ids.push(id);
            }
        }
    }

    if post_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let rows: Vec<ContentVectorEnrichment> = sqlx::query_as(
        "SELECT content_id, creator_wallet_address, performance_metrics, nsfw,
                moderation_override, extra_metadata
         FROM content_vectors
         WHERE content_id = ANY($1)",
    )
    .bind(&post_ids)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| (row.content_id.clone(), row))
        .collect())
}

fn merge_platform_metrics(posts: &mut [Value], enrichments: &HashMap<String, ContentVectorEnrichment>) {
    for post in posts {
        let Some(post_id) = json_string_field(post, &["post_id", "postId", "id"]) else {
            continue;
        };
        let Some(enrichment) = enrichments.get(&post_id) else {
            continue;
        };
        let Some(obj) = post.as_object_mut() else {
            continue;
        };
        obj.insert(
            "platformMetrics".into(),
            enrichment.to_platform_metrics_json(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_platform_metrics_adds_nested_object() {
        let mut posts = vec![json!({"post_id": "99", "content": "hello"})];
        let mut enrichments = HashMap::new();
        enrichments.insert(
            "99".into(),
            ContentVectorEnrichment {
                content_id: "99".into(),
                creator_wallet_address: Some("0x1".into()),
                performance_metrics: Some(json!({"views": 10})),
                nsfw: false,
                moderation_override: None,
                extra_metadata: None,
            },
        );

        merge_platform_metrics(&mut posts, &enrichments);

        let metrics = posts[0].get("platformMetrics").unwrap();
        assert_eq!(metrics.get("performanceMetrics").unwrap()["views"], 10);
        assert_eq!(metrics.get("nsfw").unwrap(), false);
    }

    #[test]
    fn deleted_flag_reads_extra_metadata() {
        let row = ContentVectorEnrichment {
            content_id: "1".into(),
            creator_wallet_address: None,
            performance_metrics: None,
            nsfw: false,
            moderation_override: None,
            extra_metadata: Some(json!({"deleted": true})),
        };
        assert!(row.is_deleted());
    }
}
