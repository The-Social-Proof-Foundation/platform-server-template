use std::time::Duration;

use serde::Deserialize;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions, PgSslMode};
use sqlx::PgPool;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub port: u16,
    pub environment: String,
    pub postgres_url: String,
    pub postgres_read_url: Option<String>,
    pub postgres_ssl: Option<String>,
    pub redis_url: String,
    pub jwt_secret: String,
    pub jwt_refresh_secret: String,
    pub jwt_access_token_duration_secs: i64,
    pub jwt_refresh_token_duration_secs: i64,
    pub redis_store_duration_secs: u64,
    pub internal_api_key: Option<String>,
    pub indexer_enabled: bool,
    pub myso_grpc_url: Option<String>,
    pub myso_network: String,
    pub platform_id: Option<String>,
    pub myso_social_package_id: Option<String>,
    pub stream_webhook_secret: Option<String>,
    pub redpanda_brokers: Option<String>,
    pub redpanda_ssl_enabled: bool,
    pub analytics_enabled: bool,
    pub clickhouse_ingest_enabled: bool,
    pub clickhouse_url: Option<String>,
    pub clickhouse_database: String,
    pub apns_key_id: Option<String>,
    pub apns_team_id: Option<String>,
    pub apns_bundle_id: Option<String>,
    pub apns_key_path: Option<String>,
    pub apns_environment: String,
    pub resend_api_key: Option<String>,
    pub resend_from_email: Option<String>,
}

impl Config {
    pub fn from_env() -> AppResult<Self> {
        dotenvy::dotenv().ok();

        Ok(Self {
            port: env_parse("PORT", 8080u16),
            environment: std::env::var("ENVIRONMENT")
                .or_else(|_| std::env::var("NODE_ENV"))
                .unwrap_or_else(|_| "development".into()),
            postgres_url: std::env::var("POSTGRES_URL")
                .or_else(|_| std::env::var("DATABASE_URL"))
                .map_err(|_| AppError::Config("POSTGRES_URL or DATABASE_URL is required".into()))?,
            postgres_read_url: std::env::var("POSTGRES_READ_URL").ok(),
            postgres_ssl: std::env::var("POSTGRES_SSL")
                .or_else(|_| std::env::var("DATABASE_SSL"))
                .ok(),
            redis_url: std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".into()),
            jwt_secret: std::env::var("JWT_SECRET")
                .unwrap_or_else(|_| "dev-jwt-secret-change-me".into()),
            jwt_refresh_secret: std::env::var("JWT_REFRESH_SECRET")
                .unwrap_or_else(|_| "dev-refresh-secret-change-me".into()),
            jwt_access_token_duration_secs: env_parse("JWT_ACCESS_TOKEN_DURATION", 3600i64),
            jwt_refresh_token_duration_secs: env_parse("JWT_REFRESH_TOKEN_DURATION", 604_800i64),
            redis_store_duration_secs: env_parse("REDIS_STORE_DURATION", 3600u64),
            internal_api_key: std::env::var("INTERNAL_API_KEY").ok(),
            indexer_enabled: env_bool("INDEXER_ENABLED", false),
            myso_grpc_url: std::env::var("MYSO_GRPC_URL").ok(),
            myso_network: std::env::var("MYSO_NETWORK").unwrap_or_else(|_| "devnet".into()),
            platform_id: std::env::var("PLATFORM_ID")
                .or_else(|_| std::env::var("DRIPDROP_PLATFORM_ID"))
                .ok(),
            myso_social_package_id: std::env::var("MYSO_SOCIAL_PACKAGE_ID").ok(),
            stream_webhook_secret: std::env::var("STREAM_WEBHOOK_SECRET").ok(),
            redpanda_brokers: std::env::var("REDPANDA_BROKERS").ok(),
            redpanda_ssl_enabled: env_bool("REDPANDA_SSL_ENABLED", false),
            analytics_enabled: env_bool("ANALYTICS_ENABLED", false),
            clickhouse_ingest_enabled: env_bool("CLICKHOUSE_INGEST_ENABLED", false),
            clickhouse_url: std::env::var("CLICKHOUSE_URL").ok(),
            clickhouse_database: std::env::var("CLICKHOUSE_DATABASE")
                .unwrap_or_else(|_| "platform_analytics".into()),
            apns_key_id: std::env::var("APNS_KEY_ID").ok(),
            apns_team_id: std::env::var("APNS_TEAM_ID").ok(),
            apns_bundle_id: std::env::var("APNS_BUNDLE_ID").ok(),
            apns_key_path: std::env::var("APNS_KEY_PATH").ok(),
            apns_environment: std::env::var("APNS_ENVIRONMENT")
                .unwrap_or_else(|_| "sandbox".into()),
            resend_api_key: std::env::var("RESEND_API_KEY").ok(),
            resend_from_email: std::env::var("RESEND_FROM_EMAIL").ok(),
        })
    }

    pub fn is_production(&self) -> bool {
        self.environment.eq_ignore_ascii_case("production")
    }
}

pub fn build_pg_pool(database_url: &str, ssl_override: Option<&str>) -> AppResult<PgPool> {
    let mut options: PgConnectOptions = database_url.parse()?;

    let ssl_mode = resolve_ssl_mode(database_url, ssl_override);
    options = options.ssl_mode(ssl_mode);

    Ok(PgPoolOptions::new()
        .max_connections(20)
        .acquire_timeout(Duration::from_secs(30))
        .connect_lazy_with(options))
}

fn resolve_ssl_mode(database_url: &str, ssl_override: Option<&str>) -> PgSslMode {
    if let Some(mode) = ssl_override {
        return match mode.to_ascii_lowercase().as_str() {
            "require" | "true" | "1" => PgSslMode::Require,
            "disable" | "false" | "0" => PgSslMode::Disable,
            _ => PgSslMode::Prefer,
        };
    }

    if database_url.contains("localhost")
        || database_url.contains("127.0.0.1")
        || database_url.contains("@postgres:")
    {
        PgSslMode::Prefer
    } else {
        PgSslMode::Require
    }
}

fn env_parse<T: std::str::FromStr>(key: &str, default: T) -> T {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn env_bool(key: &str, default: bool) -> bool {
    std::env::var(key)
        .ok()
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(default)
}
