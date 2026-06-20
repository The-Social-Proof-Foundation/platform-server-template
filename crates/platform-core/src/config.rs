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
    pub referrals_enabled: bool,
    pub invites_enabled: bool,
    pub waitlist_enabled: bool,
    pub waitlist_batch_admission_enabled: bool,
    pub waitlist_invite_bypass_enabled: bool,
    pub invite_circulation_public: bool,
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
            postgres_read_url: env_opt("POSTGRES_READ_URL"),
            postgres_ssl: env_opt("POSTGRES_SSL").or_else(|| env_opt("DATABASE_SSL")),
            redis_url: std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".into()),
            jwt_secret: std::env::var("JWT_SECRET")
                .unwrap_or_else(|_| "dev-jwt-secret-change-me".into()),
            jwt_refresh_secret: std::env::var("JWT_REFRESH_SECRET")
                .unwrap_or_else(|_| "dev-refresh-secret-change-me".into()),
            jwt_access_token_duration_secs: env_parse("JWT_ACCESS_TOKEN_DURATION", 3600i64),
            jwt_refresh_token_duration_secs: env_parse("JWT_REFRESH_TOKEN_DURATION", 604_800i64),
            redis_store_duration_secs: env_parse("REDIS_STORE_DURATION", 3600u64),
            internal_api_key: env_opt("INTERNAL_API_KEY"),
            indexer_enabled: env_bool("INDEXER_ENABLED", false),
            myso_grpc_url: env_opt("MYSO_GRPC_URL"),
            myso_network: std::env::var("MYSO_NETWORK").unwrap_or_else(|_| "devnet".into()),
            platform_id: env_opt("PLATFORM_ID").or_else(|| env_opt("DRIPDROP_PLATFORM_ID")),
            stream_webhook_secret: env_opt("STREAM_WEBHOOK_SECRET"),
            redpanda_brokers: env_opt("REDPANDA_BROKERS"),
            redpanda_ssl_enabled: env_bool("REDPANDA_SSL_ENABLED", false),
            analytics_enabled: env_bool("ANALYTICS_ENABLED", false),
            clickhouse_ingest_enabled: env_bool("CLICKHOUSE_INGEST_ENABLED", false),
            clickhouse_url: env_opt("CLICKHOUSE_URL"),
            clickhouse_database: std::env::var("CLICKHOUSE_DATABASE")
                .unwrap_or_else(|_| "platform_analytics".into()),
            apns_key_id: env_opt("APNS_KEY_ID"),
            apns_team_id: env_opt("APNS_TEAM_ID"),
            apns_bundle_id: env_opt("APNS_BUNDLE_ID"),
            apns_key_path: env_opt("APNS_KEY_PATH"),
            apns_environment: std::env::var("APNS_ENVIRONMENT")
                .unwrap_or_else(|_| "sandbox".into()),
            resend_api_key: env_opt("RESEND_API_KEY"),
            resend_from_email: env_opt("RESEND_FROM_EMAIL"),
            referrals_enabled: env_bool("REFERRALS_ENABLED", false),
            invites_enabled: env_bool("INVITES_ENABLED", false),
            waitlist_enabled: env_bool("WAITLIST_ENABLED", false),
            waitlist_batch_admission_enabled: env_bool("WAITLIST_BATCH_ADMISSION_ENABLED", true),
            waitlist_invite_bypass_enabled: env_bool("WAITLIST_INVITE_BYPASS_ENABLED", true),
            invite_circulation_public: env_bool("INVITE_CIRCULATION_PUBLIC", true),
        })
    }

    pub fn effective_invite_bypass(&self) -> bool {
        self.waitlist_invite_bypass_enabled && self.invites_enabled
    }

    pub fn effective_batch_admission(&self) -> bool {
        self.waitlist_enabled && self.waitlist_batch_admission_enabled
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

fn env_opt(key: &str) -> Option<String> {
    std::env::var(key).ok().and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_config(
        waitlist_enabled: bool,
        batch_admission: bool,
        invites_enabled: bool,
        invite_bypass: bool,
    ) -> Config {
        Config {
            port: 8080,
            environment: "test".into(),
            postgres_url: "postgres://localhost/test".into(),
            postgres_read_url: None,
            postgres_ssl: None,
            redis_url: "redis://127.0.0.1".into(),
            jwt_secret: "secret".into(),
            jwt_refresh_secret: "refresh".into(),
            jwt_access_token_duration_secs: 3600,
            jwt_refresh_token_duration_secs: 604_800,
            redis_store_duration_secs: 3600,
            internal_api_key: None,
            indexer_enabled: false,
            myso_grpc_url: None,
            myso_network: "devnet".into(),
            platform_id: None,
            stream_webhook_secret: None,
            redpanda_brokers: None,
            redpanda_ssl_enabled: false,
            analytics_enabled: false,
            clickhouse_ingest_enabled: false,
            clickhouse_url: None,
            clickhouse_database: "platform_analytics".into(),
            apns_key_id: None,
            apns_team_id: None,
            apns_bundle_id: None,
            apns_key_path: None,
            apns_environment: "sandbox".into(),
            resend_api_key: None,
            resend_from_email: None,
            referrals_enabled: true,
            invites_enabled,
            waitlist_enabled,
            waitlist_batch_admission_enabled: batch_admission,
            waitlist_invite_bypass_enabled: invite_bypass,
            invite_circulation_public: true,
        }
    }

    #[test]
    fn effective_batch_admission_requires_waitlist_enabled() {
        let on = sample_config(true, true, true, true);
        assert!(on.effective_batch_admission());

        let off = sample_config(true, false, true, true);
        assert!(!off.effective_batch_admission());

        let waitlist_off = sample_config(false, true, true, true);
        assert!(!waitlist_off.effective_batch_admission());
    }

    #[test]
    fn effective_invite_bypass_requires_invites_enabled() {
        let on = sample_config(true, true, true, true);
        assert!(on.effective_invite_bypass());

        let bypass_off = sample_config(true, true, true, false);
        assert!(!bypass_off.effective_invite_bypass());

        let invites_off = sample_config(true, true, false, true);
        assert!(!invites_off.effective_invite_bypass());
    }
}
