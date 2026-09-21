pub mod jwt;
pub mod mysocial_jwt;
pub mod wallet;

use platform_core::AppResult;

use crate::middleware::AuthUser;
use crate::state::SharedApiState;

use self::jwt::verify_access_token;
use self::mysocial_jwt::{token_uses_eddsa, verify_mysocial_token};

/// Resolve a bearer token to a local user. MySocial EdDSA tokens upsert the wallet row.
pub async fn authenticate_access_token(state: &SharedApiState, token: &str) -> AppResult<AuthUser> {
    let config = state.config();
    let mysocial_configured = config.mysocial_jwks_url.is_some();
    let use_mysocial = mysocial_configured && (!config.wallet_auth_enabled || token_uses_eddsa(token));

    if use_mysocial {
        let claims = verify_mysocial_token(config, token).await?;
        let (user_id, wallet) =
            platform_db::upsert_wallet_user(state.pg(), &claims.wallet_address).await?;
        return Ok(AuthUser {
            user_id,
            wallet_address: wallet,
        });
    }

    if config.wallet_auth_enabled {
        let claims = verify_access_token(token, &config.jwt_secret)?;
        let wallet = platform_db::wallet_for_user(state.pg(), &claims.user_id)
            .await?
            .unwrap_or_default();
        return Ok(AuthUser {
            user_id: claims.user_id,
            wallet_address: wallet,
        });
    }

    Err(platform_core::AppError::Unauthorized)
}
