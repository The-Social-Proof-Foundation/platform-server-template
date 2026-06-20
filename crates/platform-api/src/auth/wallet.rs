use k256::ecdsa::{RecoveryId, Signature, VerifyingKey};
use platform_core::{AppError, AppResult};
use platform_db::{consume_auth_nonce, store_auth_nonce};
use rand::Rng;
use redis::aio::ConnectionManager;
use sha3::{Digest, Keccak256};

pub fn normalize_wallet_address(address: &str) -> String {
    address.trim().to_lowercase()
}

pub fn generate_nonce() -> String {
    let mut rng = rand::thread_rng();
    let bytes: [u8; 16] = rng.gen();
    hex::encode(bytes)
}

pub fn generate_login_message(address: &str, nonce: &str) -> String {
    format!("Sign this message to authenticate. Nonce: {nonce}, Address: {address}")
}

pub async fn issue_auth_nonce(
    redis: &mut ConnectionManager,
    address: &str,
) -> AppResult<String> {
    let normalized = normalize_wallet_address(address);
    let nonce = generate_nonce();
    store_auth_nonce(redis, &normalized, &nonce, 300).await?;
    Ok(nonce)
}

pub async fn verify_wallet_signature(
    redis: &mut ConnectionManager,
    address: &str,
    signature: &str,
) -> AppResult<String> {
    let normalized = normalize_wallet_address(address);
    let nonce = consume_auth_nonce(redis, &normalized)
        .await?
        .ok_or_else(|| AppError::BadRequest("Nonce not found for this address. Please request a new signature.".into()))?;

    let message = generate_login_message(&normalized, &nonce);
    let recovered = recover_address(&message, signature)?;
    if recovered != normalized {
        return Err(AppError::BadRequest("Signature verification failed".into()));
    }
    Ok(normalized)
}

fn recover_address(message: &str, signature: &str) -> AppResult<String> {
    let sig_bytes = decode_signature(signature)?;
    let hash = eip191_hash(message);

    let signature = Signature::from_slice(&sig_bytes[..64])
        .map_err(|_| AppError::BadRequest("Invalid signature format".into()))?;
    let recid = RecoveryId::from_byte(sig_bytes[64])
        .ok_or_else(|| AppError::BadRequest("Invalid signature format".into()))?;

    let verifying_key = VerifyingKey::recover_from_prehash(&hash, &signature, recid)
        .map_err(|_| AppError::BadRequest("Signature verification failed".into()))?;

    let encoded = verifying_key.to_encoded_point(false);
    let address_bytes = &encoded.as_bytes()[1..];
    Ok(format!("0x{}", hex::encode(address_bytes)).to_lowercase())
}

fn eip191_hash(message: &str) -> [u8; 32] {
    let prefix = format!("\x19Ethereum Signed Message:\n{}", message.len());
    let mut hasher = Keccak256::new();
    hasher.update(prefix.as_bytes());
    hasher.update(message.as_bytes());
    hasher.finalize().into()
}

fn decode_signature(signature: &str) -> AppResult<Vec<u8>> {
    let sig = signature.strip_prefix("0x").unwrap_or(signature);
    hex::decode(sig).map_err(|_| AppError::BadRequest("Invalid signature format".into()))
}
