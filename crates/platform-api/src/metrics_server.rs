use axum::http::header::CONTENT_TYPE;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Extension, Router};
use platform_core::SharedPlatformMetrics;

async fn metrics_handler(Extension(metrics): Extension<SharedPlatformMetrics>) -> impl IntoResponse {
    (
        [(CONTENT_TYPE, "text/plain; version=0.0.4; charset=utf-8")],
        metrics.gather(),
    )
}

pub fn build_metrics_router(metrics: SharedPlatformMetrics) -> Router {
    Router::new()
        .route("/metrics", get(metrics_handler))
        .layer(Extension(metrics))
}
