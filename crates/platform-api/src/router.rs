use axum::extract::Extension;
use axum::middleware::from_fn;
use axum::routing::{get, post};
use axum::Router;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::middleware::{
    rate_limit_auth, rate_limit_signature, require_auth, require_internal_key,
};
use crate::routes;
use crate::state::SharedApiState;
use crate::ws;

pub fn build_router(state: SharedApiState) -> Router {
    let authed_user = Router::new()
        .route("/update", post(routes::user::update_user))
        .route("/block/:blocked_id", post(routes::user::block_user))
        .route("/blocked", get(routes::user::get_blocked))
        .route("/settings", get(routes::user::get_settings))
        .route("/setting", post(routes::user::upsert_setting))
        .route("/delete/setting", axum::routing::delete(routes::user::delete_setting))
        .route("/follow/:followee_id", post(routes::user::follow_user))
        .route("/markNotificationsAsRead", post(routes::user::mark_notifications_read))
        .route("/device-token", post(routes::user::register_device_token))
        .route("/notifications", post(routes::user::get_notifications))
        .route("/:id", get(routes::user::get_user))
        .route_layer(from_fn(require_auth));

    let public_user = Router::new()
        .route("/", post(routes::user::create_user))
        .route("/login", post(routes::user::login))
        .route("/refreshAction", post(routes::user::refresh_action))
        .route("/refreshSession", post(routes::user::refresh_session))
        .route_layer(from_fn(rate_limit_auth));

    let signature = Router::new()
        .route("/request-signature", post(routes::user::request_signature))
        .route_layer(from_fn(rate_limit_signature));

    let user = public_user.merge(signature).merge(authed_user);

    let post_routes = Router::new()
        .route(
            "/feed/following",
            get(routes::post::following_feed).route_layer(from_fn(require_auth)),
        )
        .route("/:user_id", get(routes::post::posts_by_user))
        .route("/:post_id/data", get(routes::post::post_data));

    let recommendations = Router::new()
        .route("/feed", get(routes::post::recommendation_feed))
        .route("/friends", get(routes::post::friend_recommendations))
        .route_layer(from_fn(require_auth));

    let indexer_metrics = Router::new()
        .route("/indexer/metrics", get(routes::post::indexer_metrics))
        .route_layer(from_fn(require_internal_key));

    let performance = Router::new()
        .route("/metrics", get(routes::health::performance_metrics))
        .route("/optimize", post(routes::health::performance_optimize))
        .route_layer(from_fn(require_internal_key));

    Router::new()
        .route("/health", get(routes::health::health))
        .route("/ws", get(ws::ws_handler))
        .nest("/user", user)
        .nest("/post", post_routes)
        .nest("/recommendations", recommendations.merge(indexer_metrics))
        .nest("/streams", Router::new().route("/webhook", post(routes::streams::webhook)))
        .nest("/performance", performance)
        .layer(TraceLayer::new_for_http())
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .layer(Extension(state))
}
