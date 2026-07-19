use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use rand::RngCore;

#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    #[error("encryption failed: {0}")]
    Encrypt(String),
    #[error("decryption failed: invalid key or corrupted data")]
    Decrypt,
    #[error("invalid key length: expected 32 bytes, got {0}")]
    InvalidKey(usize),
}

const NONCE_SIZE: usize = 12;

/// Encrypt plaintext using AES-256-GCM.
/// Returns base64-encoded "nonce || ciphertext" string.
pub fn encrypt_token(plaintext: &str, key: &[u8; 32]) -> Result<String, CryptoError> {
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|_| CryptoError::InvalidKey(key.len()))?;

    let mut nonce_bytes = [0u8; NONCE_SIZE];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| CryptoError::Encrypt(e.to_string()))?;

    let mut combined = nonce_bytes.to_vec();
    combined.extend_from_slice(&ciphertext);
    Ok(BASE64.encode(&combined))
}

/// Decrypt base64-encoded "nonce || ciphertext" string.
pub fn decrypt_token(encoded: &str, key: &[u8; 32]) -> Result<String, CryptoError> {
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|_| CryptoError::InvalidKey(key.len()))?;

    let combined = BASE64.decode(encoded).map_err(|_| CryptoError::Decrypt)?;

    if combined.len() < NONCE_SIZE + 16 {
        return Err(CryptoError::Decrypt);
    }

    let (nonce_bytes, ciphertext) = combined.split_at(NONCE_SIZE);
    let nonce = Nonce::from_slice(nonce_bytes);

    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| CryptoError::Decrypt)?;

    String::from_utf8(plaintext).map_err(|_| CryptoError::Decrypt)
}

/// Mask a token for API responses (show first 4 and last 4 chars).
pub fn mask_token(token: &str) -> String {
    if token.len() <= 8 {
        return "****".to_string();
    }
    format!("{}****{}", &token[..4], &token[token.len() - 4..])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key() -> [u8; 32] {
        let mut key = [0u8; 32];
        OsRng.fill_bytes(&mut key);
        key
    }

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let key = test_key();
        let plaintext = "ghp_testToken1234567890abcdef";
        let encrypted = encrypt_token(plaintext, &key).unwrap();
        let decrypted = decrypt_token(&encrypted, &key).unwrap();
        assert_eq!(plaintext, decrypted);
    }

    #[test]
    fn different_keys_fail() {
        let key1 = test_key();
        let key2 = test_key();
        let encrypted = encrypt_token("test", &key1).unwrap();
        assert!(decrypt_token(&encrypted, &key2).is_err());
    }

    #[test]
    fn mask_token_works() {
        assert_eq!(mask_token("ghp_abcdefgh12345678"), "ghp_****5678");
        assert_eq!(mask_token("short"), "****");
    }
}
