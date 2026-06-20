use platform_core::{AppError, AppResult};
use reqwest::Client;
use serde::Deserialize;
use tracing::warn;

#[derive(Clone)]
pub struct MySocialClient {
    http: Client,
    graphql_url: String,
}

#[derive(Debug, Deserialize)]
struct GraphQlResponse {
    data: Option<FollowingPostsData>,
    errors: Option<Vec<GraphQlError>>,
}

#[derive(Debug, Deserialize)]
struct GraphQlError {
    message: String,
}

#[derive(Debug, Deserialize)]
struct FollowingPostsData {
    #[serde(rename = "socialGraphFollowing")]
    social_graph_following: Option<Vec<FollowingEntry>>,
}

#[derive(Debug, Deserialize)]
struct FollowingEntry {
    #[serde(rename = "profile")]
    profile: Option<ProfilePosts>,
}

#[derive(Debug, Deserialize)]
struct ProfilePosts {
    #[serde(rename = "postsPage")]
    posts_page: Option<PostsPage>,
}

#[derive(Debug, Deserialize)]
struct PostsPage {
    nodes: Option<Vec<PostNode>>,
}

#[derive(Debug, Deserialize)]
struct PostNode {
    id: String,
}

impl MySocialClient {
    pub fn new(graphql_url: Option<String>) -> Option<Self> {
        graphql_url.map(|graphql_url| Self {
            http: Client::new(),
            graphql_url,
        })
    }

    pub async fn following_post_ids(
        &self,
        viewer_address: &str,
        limit: i64,
    ) -> AppResult<Vec<String>> {
        let query = format!(
            r#"query FollowingPosts($viewer: String!, $limit: Int!) {{
  socialGraphFollowing(address: $viewer, limit: $limit) {{
    profile {{
      postsPage(first: $limit) {{
        nodes {{ id }}
      }}
    }}
  }}
}}"#,
        );

        let response = self
            .http
            .post(&self.graphql_url)
            .json(&serde_json::json!({
                "query": query,
                "variables": {
                    "viewer": viewer_address,
                    "limit": limit,
                }
            }))
            .send()
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            warn!(body, "MySocial GraphQL request failed");
            return Err(AppError::Internal(format!("GraphQL HTTP error: {body}")));
        }

        let payload: GraphQlResponse = response
            .json()
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

        if let Some(errors) = payload.errors {
            let msg = errors
                .into_iter()
                .map(|e| e.message)
                .collect::<Vec<_>>()
                .join("; ");
            return Err(AppError::Internal(format!("GraphQL errors: {msg}")));
        }

        let mut ids = Vec::new();
        if let Some(following) = payload.data.and_then(|d| d.social_graph_following) {
            for entry in following {
                if let Some(nodes) = entry
                    .profile
                    .and_then(|p| p.posts_page)
                    .and_then(|p| p.nodes)
                {
                    for node in nodes {
                        ids.push(node.id);
                    }
                }
            }
        }
        Ok(ids)
    }
}
