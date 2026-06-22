use axum::extract::Extension;
use axum::middleware::from_fn;
use axum::routing::{delete, get, post};
use axum::Router;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::middleware::{
    http_metrics_middleware, rate_limit_auth, rate_limit_circulation, rate_limit_signature,
    require_auth, require_internal_key, require_platform_access,
};
use crate::routes;
use crate::state::SharedApiState;
use crate::ws;
use platform_core::SharedPlatformMetrics;

pub fn build_router(state: SharedApiState, metrics: SharedPlatformMetrics) -> Router {
    let config = state.config().clone();

    let authed_platform = Router::new()
        .route("/update", post(routes::user::update_user))
        .route("/settings", get(routes::settings::get_settings))
        .route("/settings/catalog", get(routes::settings::get_settings_catalog))
        .route("/setting", post(routes::settings::upsert_setting_handler))
        .route("/setting", delete(routes::settings::delete_setting_handler))
        .route(
            "/delete/setting",
            delete(routes::settings::delete_setting_handler),
        )
        .route("/references", get(routes::settings::get_references))
        .route("/reference", post(routes::settings::upsert_reference_handler))
        .route("/reference", delete(routes::settings::delete_reference_handler))
        .route("/markNotificationsAsRead", post(routes::user::mark_notifications_read))
        .route("/device-token", post(routes::user::register_device_token))
        .route("/email", post(routes::user::set_email))
        .route("/email/verify", get(routes::user::verify_email))
        .route("/notifications", post(routes::user::get_notifications))
        .route("/{id}", get(routes::user::get_user))
        .route_layer(from_fn(require_platform_access))
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

    let user = public_user.merge(signature).merge(authed_platform);

    let recommendations = Router::new()
        .route("/feed", get(routes::recommendations::recommendation_feed))
        .route("/blended-feed", get(routes::recommendations::blended_feed))
        .route("/friends", get(routes::recommendations::friend_recommendations))
        .route_layer(from_fn(require_platform_access))
        .route_layer(from_fn(require_auth));

    let interactions = Router::new()
        .route("/", post(routes::interactions::record_interaction))
        .route("/batch", post(routes::interactions::record_interactions_batch))
        .route_layer(from_fn(require_platform_access))
        .route_layer(from_fn(require_auth));

    let search = Router::new()
        .route("/", get(routes::search::search_handler))
        .route("/history", get(routes::search::list_search_history_handler))
        .route("/history", delete(routes::search::delete_search_history_handler))
        .route_layer(from_fn(require_platform_access))
        .route_layer(from_fn(require_auth));

    let indexer_metrics = Router::new()
        .route("/indexer/metrics", get(routes::recommendations::indexer_metrics))
        .route(
            "/admin/content/{content_id}/moderate",
            post(routes::recommendations::moderate_content),
        )
        .route_layer(from_fn(require_internal_key));

    let performance = Router::new()
        .route("/metrics", get(routes::health::performance_metrics))
        .route("/optimize", post(routes::health::performance_optimize))
        .route_layer(from_fn(require_internal_key));

    let mut router = Router::new()
        .route("/health", get(routes::health::health))
        .route("/ws", get(ws::ws_handler))
        .nest("/user", user)
        .nest("/interactions", interactions)
        .nest("/search", search)
        .nest("/recommendations", recommendations.merge(indexer_metrics))
        .nest("/streams", Router::new().route("/webhook", post(routes::streams::webhook)))
        .nest("/social", Router::new().route("/events", post(routes::social::social_events)))
        .nest("/performance", performance);

    if config.referrals_enabled {
        let referrals = Router::new()
            .route("/stats", get(routes::referral::referral_stats_handler))
            .route("/code", get(routes::waitlist::referral_code_handler))
            .route("/", get(routes::referral::list_referrals_handler))
            .route("/record", post(routes::referral::record_referral_handler))
            .route_layer(from_fn(require_auth));
        router = router.nest("/referrals", referrals);
    }

    if config.waitlist_enabled {
        let waitlist = Router::new()
            .route("/status", get(routes::waitlist::waitlist_status_handler))
            .route_layer(from_fn(require_auth));
        router = router.nest("/waitlist", waitlist);

        let admin = Router::new()
            .route("/config", get(routes::waitlist::admin_get_config_handler))
            .route("/config", post(routes::waitlist::admin_set_config_handler))
            .route("/pause", post(routes::waitlist::admin_pause_handler))
            .route("/resume", post(routes::waitlist::admin_resume_handler))
            .route("/run-batch", post(routes::waitlist::admin_run_batch_handler))
            .route(
                "/users/grant-access",
                post(routes::waitlist::admin_grant_access_handler),
            )
            .route(
                "/users/{id}/approve",
                post(routes::waitlist::admin_approve_user_handler),
            )
            .route(
                "/users/{id}/invites",
                post(routes::waitlist::admin_user_invites_handler),
            )
            .route_layer(from_fn(require_internal_key));
        router = router.nest("/waitlist/admin", admin);
    }

    if config.invites_enabled {
        let authed_invites = Router::new()
            .route("/", get(routes::invite::list_invites_handler))
            .route("/", post(routes::invite::create_invite_handler))
            .route("/accept", post(routes::invite::accept_invite_handler))
            .route_layer(from_fn(require_auth));
        let invites = Router::new()
            .merge(authed_invites)
            .route("/{code}", get(routes::invite::preview_invite_handler));
        router = router.nest("/invites", invites);
    }

    if config.invites_enabled && config.invite_circulation_public {
        router = router.route(
            "/waitlist/invites/circulation",
            get(routes::waitlist::invite_circulation_handler).route_layer(from_fn(rate_limit_circulation)),
        );
    }

    router
        .route_layer(from_fn(http_metrics_middleware))
        .layer(TraceLayer::new_for_http())
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .layer(Extension(metrics))
        .layer(Extension(state))
}
