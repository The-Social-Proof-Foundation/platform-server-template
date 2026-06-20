use platform_core::{AppError, AppResult, Config};
use reqwest::Client;
use serde::Deserialize;
use tracing::warn;

use crate::EXPECTED_EMBEDDING_DIM;

#[derive(Clone)]
pub struct OpenAiEmbeddingClient {
    http: Client,
    api_key: String,
    model: String,
}

#[derive(Debug, Deserialize)]
struct EmbeddingsResponse {
    data: Vec<EmbeddingData>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
}

impl OpenAiEmbeddingClient {
    pub fn from_config(config: &Config) -> AppResult<Option<Self>> {
        let Some(api_key) = config.openai_api_key.clone() else {
            return Ok(None);
        };
        Ok(Some(Self {
            http: Client::new(),
            api_key,
            model: config.openai_embedding_model.clone(),
        }))
    }

    pub async fn embed_text(&self, text: &str) -> AppResult<Vec<f32>> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Err(AppError::BadRequest("empty text for embedding".into()));
        }

        let response = self
            .http
            .post("https://api.openai.com/v1/embeddings")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&serde_json::json!({
                "model": self.model,
                "input": trimmed,
            }))
            .send()
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            warn!(body, "OpenAI embeddings request failed");
            return Err(AppError::Internal(format!("OpenAI error: {body}")));
        }

        let payload: EmbeddingsResponse = response
            .json()
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

        let vector = payload
            .data
            .into_iter()
            .next()
            .map(|d| d.embedding)
            .ok_or_else(|| AppError::Internal("OpenAI returned no embedding".into()))?;

        if vector.len() != EXPECTED_EMBEDDING_DIM {
            return Err(AppError::Internal(format!(
                "unexpected embedding dimension {} (expected {EXPECTED_EMBEDDING_DIM})",
                vector.len()
            )));
        }

        Ok(vector)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expected_dim_is_3072() {
        assert_eq!(EXPECTED_EMBEDDING_DIM, 3072);
    }
}
