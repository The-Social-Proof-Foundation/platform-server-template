use platform_core::{AppError, AppResult, Config};
use platform_db::embeddings::{
    build_profile_text, upsert_content_embedding, upsert_profile_embedding,
};
use sqlx::PgPool;
use tracing::{info, warn};

use crate::openai::OpenAiEmbeddingClient;

#[derive(Clone)]
pub struct EmbeddingService {
    client: Option<OpenAiEmbeddingClient>,
    enabled: bool,
    model: String,
}

impl EmbeddingService {
    pub fn from_config(config: &Config) -> AppResult<Self> {
        let client = OpenAiEmbeddingClient::from_config(config)?;
        Ok(Self {
            client,
            enabled: config.embeddings_enabled,
            model: config.openai_embedding_model.clone(),
        })
    }

    pub fn is_active(&self) -> bool {
        self.enabled && self.client.is_some()
    }

    pub async fn embed_and_store_content(
        &self,
        pool: &PgPool,
        content_id: &str,
        description: &str,
    ) -> AppResult<()> {
        if !self.enabled {
            info!(content_id, "embeddings disabled; skipping content embed");
            return Ok(());
        }
        let Some(client) = &self.client else {
            warn!("embeddings enabled but OPENAI_API_KEY not set; skipping");
            return Ok(());
        };

        let vector = client.embed_text(description).await?;
        upsert_content_embedding(pool, content_id, &vector, &self.model, None).await
    }

    pub async fn embed_and_store_profile(
        &self,
        pool: &PgPool,
        wallet: &str,
        username: Option<&str>,
        full_name: Option<&str>,
        bio: Option<&str>,
    ) -> AppResult<()> {
        if !self.enabled {
            return Ok(());
        }
        let Some(client) = &self.client else {
            return Ok(());
        };

        let text = build_profile_text(username, full_name, bio);
        if text.trim().is_empty() {
            return Ok(());
        }

        let vector = client.embed_text(&text).await?;
        upsert_profile_embedding(pool, wallet, &vector, &self.model).await
    }

    pub async fn embed_text(&self, text: &str) -> AppResult<Vec<f32>> {
        let Some(client) = &self.client else {
            return Err(AppError::Config("OpenAI client not configured".into()));
        };
        if !self.enabled {
            return Err(AppError::Config("embeddings disabled".into()));
        }
        client.embed_text(text).await
    }
}
