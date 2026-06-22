use std::collections::HashSet;

use serde_json::Value;

use crate::indexer::SearchIndexerResponse;

use super::enrich::ContentVectorEnrichment;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SearchEntityType {
    Profile,
    Post,
    Platform,
}

impl SearchEntityType {
    fn from_filter_token(token: &str) -> Option<Self> {
        match token {
            "profile" | "profiles" => Some(Self::Profile),
            "post" | "posts" => Some(Self::Post),
            "platform" | "platforms" => Some(Self::Platform),
            _ => None,
        }
    }
}

pub fn parse_filter_types(raw: Option<&str>) -> Option<HashSet<SearchEntityType>> {
    let raw = raw?.trim();
    if raw.is_empty() {
        return None;
    }

    let set: HashSet<_> = raw
        .split(',')
        .filter_map(|part| SearchEntityType::from_filter_token(part.trim()))
        .collect();

    if set.is_empty() {
        None
    } else {
        Some(set)
    }
}

pub fn apply_filter_types(response: &mut SearchIndexerResponse, filters: &HashSet<SearchEntityType>) {
    if !filters.contains(&SearchEntityType::Profile) {
        response.profiles.clear();
    }
    if !filters.contains(&SearchEntityType::Post) {
        response.posts.clear();
    }
    if !filters.contains(&SearchEntityType::Platform) {
        response.platforms.clear();
        response.platforms_count = 0;
    } else {
        response.platforms_count = response.platforms.len() as i64;
    }
}

pub fn json_string_field(value: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        let Some(field) = value.get(*key) else {
            continue;
        };
        if let Some(text) = field.as_str() {
            if !text.is_empty() {
                return Some(text.to_string());
            }
        } else if let Some(number) = field.as_i64() {
            return Some(number.to_string());
        } else if let Some(number) = field.as_u64() {
            return Some(number.to_string());
        }
    }
    None
}

pub fn apply_user_filters(
    response: &mut SearchIndexerResponse,
    blocked: &HashSet<String>,
    allow_nsfw: bool,
    enrichments: &std::collections::HashMap<String, ContentVectorEnrichment>,
) {
    response.profiles.retain(|profile| {
        json_string_field(profile, &["owner_address", "ownerAddress"])
            .map(|addr| !blocked.contains(&addr))
            .unwrap_or(true)
    });

    response.posts.retain(|post| {
        !should_hide_post(post, enrichments, blocked, allow_nsfw)
    });

    response.platforms.retain(|platform| {
        json_string_field(platform, &["developer_address", "developerAddress"])
            .map(|addr| !blocked.contains(&addr))
            .unwrap_or(true)
    });
    response.platforms_count = response.platforms.len() as i64;
}

fn should_hide_post(
    post: &Value,
    enrichments: &std::collections::HashMap<String, ContentVectorEnrichment>,
    blocked: &HashSet<String>,
    allow_nsfw: bool,
) -> bool {
    if let Some(owner) = json_string_field(post, &["owner", "owner_address", "ownerAddress"]) {
        if blocked.contains(&owner) {
            return true;
        }
    }

    let Some(post_id) = json_string_field(post, &["post_id", "postId", "id"]) else {
        return false;
    };

    let Some(enrichment) = enrichments.get(&post_id) else {
        return false;
    };

    if enrichment.moderation_override.as_deref() == Some("force_block") {
        return true;
    }
    if enrichment.is_deleted() {
        return true;
    }
    if enrichment.nsfw && !allow_nsfw {
        return true;
    }
    if let Some(creator) = &enrichment.creator_wallet_address {
        if blocked.contains(creator) {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_filter_types_accepts_singular_and_plural() {
        let filters = parse_filter_types(Some("profile,posts,platforms")).unwrap();
        assert!(filters.contains(&SearchEntityType::Profile));
        assert!(filters.contains(&SearchEntityType::Post));
        assert!(filters.contains(&SearchEntityType::Platform));
    }

    #[test]
    fn apply_filter_types_clears_unselected_buckets() {
        let mut response = SearchIndexerResponse {
            profiles: vec![json!({"username": "a"})],
            posts: vec![json!({"post_id": "1"})],
            platforms: vec![json!({"platform_id": "p1"})],
            platforms_count: 1,
        };

        apply_filter_types(
            &mut response,
            &HashSet::from([SearchEntityType::Post]),
        );

        assert!(response.profiles.is_empty());
        assert_eq!(response.posts.len(), 1);
        assert!(response.platforms.is_empty());
        assert_eq!(response.platforms_count, 0);
    }

    #[test]
    fn should_hide_blocked_and_nsfw_posts() {
        let post = json!({"post_id": "42", "owner": "0xabc"});
        let mut enrichments = std::collections::HashMap::new();
        enrichments.insert(
            "42".into(),
            ContentVectorEnrichment {
                content_id: "42".into(),
                creator_wallet_address: Some("0xabc".into()),
                performance_metrics: None,
                nsfw: true,
                moderation_override: None,
                extra_metadata: None,
            },
        );

        let blocked = HashSet::from(["0xdef".to_string()]);
        assert!(should_hide_post(&post, &enrichments, &blocked, false));
        assert!(!should_hide_post(&post, &enrichments, &blocked, true));
    }
}
