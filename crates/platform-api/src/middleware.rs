use std::time::Instant;

use axum::extract::{Extension, MatchedPath, Request};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use platform_core::SharedPlatformMetrics;
use serde_json::json;

use crate::auth::jwt::verify_access_token;
use crate::error::ApiError;
use crate::state::SharedApiState;

#[derive(Clone)]
pub struct AuthUser {
    pub user_id: String,
}

pub async fn require_auth(
    Extension(state): Extension<SharedApiState>,
    mut req: Request,
    next: Next,
) -> Response {
    match require_auth_inner(state, &mut req) {
        Ok(()) => next.run(req).await,
        Err(err) => err.into_response(),
    }
}

fn require_auth_inner(state: SharedApiState, req: &mut Request) -> Result<(), ApiError> {
    let auth_header = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or(platform_core::AppError::Unauthorized)?;

    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or(platform_core::AppError::Unauthorized)?;

    let claims = verify_access_token(token, &state.config().jwt_secret)?;
    req.extensions_mut().insert(AuthUser {
        user_id: claims.user_id,
    });
    Ok(())
}

pub async fn require_internal_key(
    Extension(state): Extension<SharedApiState>,
    req: Request,
    next: Next,
) -> Response {
    match require_internal_key_check(&state, &req) {
        Ok(()) => next.run(req).await,
        Err(err) => err.into_response(),
    }
}

fn require_internal_key_check(state: &SharedApiState, req: &Request) -> Result<(), ApiError> {
    if state.config().is_production() {
        let expected = state
            .config()
            .internal_api_key
            .as_deref()
            .ok_or(platform_core::AppError::Config(
                "INTERNAL_API_KEY not configured".into(),
            ))?;
        let provided = req
            .headers()
            .get("x-internal-api-key")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        if provided != expected {
            return Err(platform_core::AppError::Unauthorized.into());
        }
    }
    Ok(())
}

pub async fn rate_limit_auth(
    Extension(state): Extension<SharedApiState>,
    req: Request,
    next: Next,
) -> Response {
    let ip = client_ip(&req);
    match rate_limit_with_ip(&state, &ip, "rl:auth", 30).await {
        Ok(()) => next.run(req).await,
        Err(err) => err.into_response(),
    }
}

pub async fn rate_limit_signature(
    Extension(state): Extension<SharedApiState>,
    req: Request,
    next: Next,
) -> Response {
    let ip = client_ip(&req);
    match rate_limit_with_ip(&state, &ip, "rl:signature", 20).await {
        Ok(()) => next.run(req).await,
        Err(err) => err.into_response(),
    }
}

async fn rate_limit_with_ip(
    state: &SharedApiState,
    ip: &str,
    prefix: &str,
    max: u64,
) -> Result<(), ApiError> {
    let mut redis = state.redis();
    let count = platform_db::rate_limit_incr(&mut redis, prefix, ip, 60).await?;
    if count > max {
        return Err(platform_core::AppError::TooManyRequests.into());
    }
    Ok(())
}

pub async fn require_platform_access(
    Extension(state): Extension<SharedApiState>,
    Extension(auth): Extension<AuthUser>,
    req: Request,
    next: Next,
) -> Response {
    if !state.config().waitlist_enabled {
        return next.run(req).await;
    }

    match platform_db::is_approved(state.pg_read(), &auth.user_id).await {
        Ok(true) => next.run(req).await,
        Ok(false) => (
            StatusCode::FORBIDDEN,
            Json(json!({
                "code": "waitlist_pending",
                "message": "Platform access pending waitlist approval",
            })),
        )
            .into_response(),
        Err(err) => ApiError(err).into_response(),
    }
}

pub async fn rate_limit_circulation(
    Extension(state): Extension<SharedApiState>,
    req: Request,
    next: Next,
) -> Response {
    let ip = client_ip(&req);
    match rate_limit_with_ip(&state, &ip, "rl:circulation", 60).await {
        Ok(()) => next.run(req).await,
        Err(err) => err.into_response(),
    }
}

fn client_ip(req: &Request) -> String {
    req.headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("unknown")
        .to_string()
}

pub async fn http_metrics_middleware(
    Extension(metrics): Extension<SharedPlatformMetrics>,
    req: Request,
    next: Next,
) -> Response {
    let method = req.method().as_str().to_string();
    let route = req
        .extensions()
        .get::<MatchedPath>()
        .map(|matched| matched.as_str().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let start = Instant::now();
    let response = next.run(req).await;
    metrics.observe_http_request(&method, &route, response.status().as_u16(), start.elapsed());
    response
}
