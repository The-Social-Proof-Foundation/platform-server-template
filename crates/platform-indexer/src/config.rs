use platform_core::AppResult;

#[derive(Debug, Clone)]
pub struct IndexerConfig {
    pub grpc_url: String,
    pub network: String,
    pub platform_id: String,
}

pub fn load_from_env() -> AppResult<Option<IndexerConfig>> {
    let enabled = std::env::var("INDEXER_ENABLED")
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false);
    if !enabled {
        return Ok(None);
    }

    let grpc_url = std::env::var("MYSO_GRPC_URL").map_err(|_| {
        platform_core::AppError::Config("MYSO_GRPC_URL required when INDEXER_ENABLED=true".into())
    })?;
    let platform_id = std::env::var("PLATFORM_ID")
        .or_else(|_| std::env::var("DRIPDROP_PLATFORM_ID"))
        .map_err(|_| {
            platform_core::AppError::Config("PLATFORM_ID required when INDEXER_ENABLED=true".into())
        })?;

    Ok(Some(IndexerConfig {
        grpc_url,
        network: std::env::var("MYSO_NETWORK").unwrap_or_else(|_| "devnet".into()),
        platform_id: platform_id.to_lowercase(),
    }))
}
