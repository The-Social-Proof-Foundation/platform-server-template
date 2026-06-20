use redis::aio::ConnectionManager;
use redis::Client;
use sqlx::PgPool;

use crate::config::Config;
use crate::error::AppResult;

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub pg_pool: PgPool,
    pub pg_read_pool: PgPool,
    pub redis: ConnectionManager,
}

impl AppState {
    pub async fn new(config: Config) -> AppResult<Self> {
        let pg_pool = crate::config::build_pg_pool(&config.postgres_url, config.postgres_ssl.as_deref())?;
        let read_url = config
            .postgres_read_url
            .clone()
            .unwrap_or_else(|| config.postgres_url.clone());
        let pg_read_pool =
            crate::config::build_pg_pool(&read_url, config.postgres_ssl.as_deref())?;

        let redis_client = Client::open(config.redis_url.clone())?;
        let redis = ConnectionManager::new(redis_client).await?;

        Ok(Self {
            config,
            pg_pool,
            pg_read_pool,
            redis,
        })
    }
}
