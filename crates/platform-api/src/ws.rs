use axum::extract::{Extension, Query, ws::{WebSocket, WebSocketUpgrade}};
use axum::response::IntoResponse;

use crate::auth::jwt::verify_access_token;
use crate::error::ApiResult;
use crate::state::SharedApiState;
use platform_db::set_user_online;

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    Extension(state): Extension<SharedApiState>,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> ApiResult<impl IntoResponse> {
    let token = query
        .get("token")
        .ok_or(platform_core::AppError::Unauthorized)?;
    let claims = verify_access_token(token, &state.config().jwt_secret)?;
    Ok(ws.on_upgrade(move |socket| handle_ws(state, claims.user_id, socket)))
}

async fn handle_ws(state: SharedApiState, user_id: String, socket: WebSocket) {
    let mut redis = state.redis();
    let _ = set_user_online(
        &mut redis,
        &user_id,
        state.config().redis_store_duration_secs,
    )
    .await;
    state.notify.ws_hub.handle_socket(user_id, socket).await;
}
