use crate::recommend::timeline::ContentRecommendation;

#[derive(Debug, Clone, serde::Serialize)]
pub struct BlendedFeedItem {
    pub content_id: String,
    pub source: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
}

pub fn merge_blended_feed(
    following_ids: Vec<String>,
    discovery: Vec<ContentRecommendation>,
    chrono_limit: usize,
    discover_limit: usize,
) -> Vec<BlendedFeedItem> {
    let mut seen = std::collections::HashSet::new();
    let mut items = Vec::new();

    for id in following_ids.into_iter().take(chrono_limit) {
        if seen.insert(id.clone()) {
            items.push(BlendedFeedItem {
                content_id: id,
                source: "following",
                score: None,
            });
        }
    }

    for rec in discovery {
        if items.len() >= chrono_limit + discover_limit {
            break;
        }
        if seen.insert(rec.content_id.clone()) {
            items.push(BlendedFeedItem {
                content_id: rec.content_id,
                source: "discovery",
                score: Some(rec.score),
            });
        }
    }

    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recommend::timeline::ContentRecommendation;

    #[test]
    fn merge_dedupes_and_interleaves_following_first() {
        let following = vec!["a".into(), "b".into(), "a".into()];
        let discovery = vec![
            ContentRecommendation {
                content_id: "b".into(),
                score: 0.1,
                reason: None,
                nsfw: false,
            },
            ContentRecommendation {
                content_id: "c".into(),
                score: 0.2,
                reason: None,
                nsfw: false,
            },
        ];
        let merged = merge_blended_feed(following, discovery, 10, 10);
        assert_eq!(merged.len(), 3);
        assert_eq!(merged[0].content_id, "a");
        assert_eq!(merged[0].source, "following");
        assert_eq!(merged[1].content_id, "b");
        assert_eq!(merged[2].content_id, "c");
        assert_eq!(merged[2].source, "discovery");
    }
}
