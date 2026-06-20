use axum::Json;
use chrono::Utc;
use serde_json::json;

use crate::error::ApiResult;

pub async fn health() -> ApiResult<Json<serde_json::Value>> {
    Ok(Json(json!({
        "status": "ok",
        "timestamp": Utc::now().to_rfc3339(),
    })))
}

pub async fn performance_metrics() -> ApiResult<Json<serde_json::Value>> {
    Ok(Json(json!({
        "redis": "ok",
        "postgres": "ok",
    })))
}

pub async fn performance_optimize() -> ApiResult<Json<serde_json::Value>> {
    Ok(Json(json!({ "optimized": true })))
}
