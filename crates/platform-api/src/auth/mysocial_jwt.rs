use std::collections::HashMap;
use std::time::{Duration, Instant};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use platform_core::{AppError, AppResult, Config};
use serde::Deserialize;
use tokio::sync::Mutex;

const JWKS_TTL: Duration = Duration::from_secs(300);

#[derive(Debug, Clone)]
pub struct MySocialClaims {
    pub wallet_address: String,
}

#[derive(Deserialize)]
struct JwtHeader {
    alg: Option<String>,
    kid: Option<String>,
}

#[derive(Deserialize)]
struct JwtClaims {
    iss: String,
    aud: serde_json::Value,
    sub: String,
    wallet_address: String,
    provider: String,
    iat: i64,
    exp: i64,
    jti: String,
}

#[derive(Deserialize)]
struct Jwks {
    keys: Vec<Jwk>,
}

#[derive(Deserialize)]
struct Jwk {
    kty: String,
    crv: String,
    alg: String,
    kid: String,
    x: String,
}

struct CachedKeys {
    fetched_at: Instant,
    keys: HashMap<String, VerifyingKey>,
}

static KEY_CACHE: Mutex<Option<CachedKeys>> = Mutex::const_new(None);

pub fn token_uses_eddsa(token: &str) -> bool {
    let Some(header) = token.split('.').next() else {
        return false;
    };
    decode_json::<JwtHeader>(header)
        .ok()
        .and_then(|h| h.alg)
        .is_some_and(|alg| alg == "EdDSA")
}

pub fn normalize_wallet_address(raw: &str) -> Option<String> {
    let value = raw.trim().to_ascii_lowercase();
    if value.len() != 66 || !value.starts_with("0x") {
        return None;
    }
    if value[2..].chars().all(|c| c.is_ascii_hexdigit()) {
        Some(value)
    } else {
        None
    }
}

pub async fn verify_mysocial_token(config: &Config, token: &str) -> AppResult<MySocialClaims> {
    let jwks_url = config
        .mysocial_jwks_url
        .as_deref()
        .ok_or_else(|| AppError::Config("MYSOCIAL_JWKS_URL is required".into()))?;
    let issuer = config
        .mysocial_jwt_issuer
        .as_deref()
        .ok_or_else(|| AppError::Config("MYSOCIAL_JWT_ISSUER is required".into()))?;
    let client_ids = config
        .mysocial_client_id
        .as_deref()
        .ok_or_else(|| AppError::Config("MYSOCIAL_CLIENT_ID is required".into()))?;

    if config.is_production() && (!jwks_url.starts_with("https://") || !issuer.starts_with("https://"))
    {
        return Err(AppError::Config(
            "MySocial issuer and JWKS URLs must use HTTPS".into(),
        ));
    }

    let mut parts = token.split('.');
    let encoded_header = parts.next().ok_or(AppError::Unauthorized)?;
    let encoded_payload = parts.next().ok_or(AppError::Unauthorized)?;
    let encoded_signature = parts.next().ok_or(AppError::Unauthorized)?;
    if parts.next().is_some() {
        return Err(AppError::Unauthorized);
    }

    let header: JwtHeader = decode_json(encoded_header).map_err(|_| AppError::Unauthorized)?;
    if header.alg.as_deref() != Some("EdDSA") {
        return Err(AppError::Unauthorized);
    }
    let kid = header.kid.ok_or(AppError::Unauthorized)?;

    let mut keys = load_keys(jwks_url, false).await?;
    if !keys.contains_key(&kid) {
        keys = load_keys(jwks_url, true).await?;
    }
    let key = keys.get(&kid).ok_or(AppError::Unauthorized)?;

    let signature = decode_b64(encoded_signature).map_err(|_| AppError::Unauthorized)?;
    let signature = Signature::from_slice(&signature).map_err(|_| AppError::Unauthorized)?;
    let message = format!("{encoded_header}.{encoded_payload}");
    key.verify(message.as_bytes(), &signature)
        .map_err(|_| AppError::Unauthorized)?;

    let claims: JwtClaims = decode_json(encoded_payload).map_err(|_| AppError::Unauthorized)?;
    let now = chrono::Utc::now().timestamp();
    if claims.iss != issuer {
        return Err(AppError::Unauthorized);
    }
    if !audience_allowed(&claims.aud, client_ids) {
        return Err(AppError::Unauthorized);
    }
    if claims.exp <= now || claims.iat > now + 60 {
        return Err(AppError::Unauthorized);
    }
    if claims.sub.is_empty() || claims.jti.is_empty() || claims.provider.is_empty() {
        return Err(AppError::Unauthorized);
    }
    let wallet = normalize_wallet_address(&claims.wallet_address).ok_or(AppError::Unauthorized)?;
    Ok(MySocialClaims {
        wallet_address: wallet,
    })
}

fn audience_allowed(aud: &serde_json::Value, allowed_csv: &str) -> bool {
    let allowed: Vec<&str> = allowed_csv
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    let audiences: Vec<&str> = match aud {
        serde_json::Value::String(value) => vec![value.as_str()],
        serde_json::Value::Array(values) => values.iter().filter_map(|v| v.as_str()).collect(),
        _ => Vec::new(),
    };
    audiences.iter().any(|aud| allowed.contains(aud))
}

async fn load_keys(jwks_url: &str, force: bool) -> AppResult<HashMap<String, VerifyingKey>> {
    {
        let cache = KEY_CACHE.lock().await;
        if !force {
            if let Some(cached) = cache.as_ref() {
                if cached.fetched_at.elapsed() < JWKS_TTL {
                    return Ok(cached.keys.clone());
                }
            }
        }
    }

    let response = reqwest::Client::new()
        .get(jwks_url)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|_| AppError::Unauthorized)?;
    if !response.status().is_success() {
        return Err(AppError::Unauthorized);
    }
    let jwks: Jwks = response.json().await.map_err(|_| AppError::Unauthorized)?;
    let mut keys = HashMap::new();
    for jwk in jwks.keys {
        if jwk.kty != "OKP" || jwk.crv != "Ed25519" || jwk.alg != "EdDSA" {
            continue;
        }
        let raw = decode_b64(&jwk.x).map_err(|_| AppError::Unauthorized)?;
        let bytes: [u8; 32] = raw.try_into().map_err(|_| AppError::Unauthorized)?;
        let key = VerifyingKey::from_bytes(&bytes).map_err(|_| AppError::Unauthorized)?;
        keys.insert(jwk.kid, key);
    }
    if keys.is_empty() {
        return Err(AppError::Unauthorized);
    }
    *KEY_CACHE.lock().await = Some(CachedKeys {
        fetched_at: Instant::now(),
        keys: keys.clone(),
    });
    Ok(keys)
}

fn decode_json<T: serde::de::DeserializeOwned>(part: &str) -> Result<T, ()> {
    let bytes = decode_b64(part).map_err(|_| ())?;
    serde_json::from_slice(&bytes).map_err(|_| ())
}

fn decode_b64(value: &str) -> Result<Vec<u8>, ()> {
    let trimmed = value.trim_end_matches('=');
    URL_SAFE_NO_PAD.decode(trimmed).map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    #[test]
    fn normalizes_64_hex_wallets() {
        let raw = "0xABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789";
        let normalized = normalize_wallet_address(raw).expect("wallet");
        assert!(normalized.starts_with("0x"));
        assert_eq!(normalized, raw.to_ascii_lowercase());
        assert!(normalize_wallet_address("0xabc").is_none());
    }

    #[test]
    fn audience_matches_comma_separated_clients() {
        let aud = serde_json::json!("web-client");
        assert!(audience_allowed(&aud, "ios-client, web-client"));
        assert!(!audience_allowed(&aud, "ios-client"));
    }

    #[test]
    fn ed25519_round_trip() {
        let signing = SigningKey::from_bytes(&[7u8; 32]);
        let message = b"header.payload";
        let signature = signing.sign(message);
        signing
            .verifying_key()
            .verify(message, &signature)
            .expect("signature");
    }
}
