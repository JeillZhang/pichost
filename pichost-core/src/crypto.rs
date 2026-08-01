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

/// Decode a base64-encoded 32-byte key string into `[u8; 32]`.
pub fn decode_key(encoded: &str) -> Result<[u8; 32], CryptoError> {
    let bytes = BASE64.decode(encoded).map_err(|_| {
        CryptoError::InvalidKey(0)
    })?;
    if bytes.len() != 32 {
        return Err(CryptoError::InvalidKey(bytes.len()));
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&bytes);
    Ok(key)
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

    #[test]
    fn decode_key_valid_32_bytes() {
        let raw = [7u8; 32];
        let encoded = BASE64.encode(raw);
        let key = decode_key(&encoded).unwrap();
        assert_eq!(key, raw);
    }

    #[test]
    fn decode_key_wrong_length() {
        let encoded = BASE64.encode([1u8; 16]);
        let err = decode_key(&encoded).unwrap_err();
        assert!(matches!(err, CryptoError::InvalidKey(16)));
    }

    #[test]
    fn decode_key_invalid_base64() {
        let err = decode_key("!!!not-base64!!!").unwrap_err();
        assert!(matches!(err, CryptoError::InvalidKey(0)));
    }

    #[test]
    fn decrypt_token_invalid_base64() {
        let key = test_key();
        let err = decrypt_token("###", &key).unwrap_err();
        assert!(matches!(err, CryptoError::Decrypt));
    }

    #[test]
    fn decrypt_token_truncated() {
        let key = test_key();
        let err = decrypt_token(&BASE64.encode([0u8; 10]), &key).unwrap_err();
        assert!(matches!(err, CryptoError::Decrypt));
    }

    #[test]
    fn decrypt_token_corrupted_ciphertext() {
        let key = test_key();
        let encrypted = encrypt_token("hello", &key).unwrap();
        let mut bytes = BASE64.decode(&encrypted).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0xFF;
        let err = decrypt_token(&BASE64.encode(bytes), &key).unwrap_err();
        assert!(matches!(err, CryptoError::Decrypt));
    }

    #[test]
    fn decrypt_token_non_utf8_plaintext() {
        let key = test_key();
        let cipher = Aes256Gcm::new_from_slice(&key).unwrap();
        let nonce = Nonce::from_slice(&[0u8; 12]);
        let ct = cipher.encrypt(nonce, b"\xff\xfe\xfd".as_slice()).unwrap();
        let mut combined = nonce.to_vec();
        combined.extend_from_slice(&ct);
        let err = decrypt_token(&BASE64.encode(combined), &key).unwrap_err();
        assert!(matches!(err, CryptoError::Decrypt));
    }

    #[test]
    fn crypto_error_display() {
        assert_eq!(
            CryptoError::Encrypt("boom".into()).to_string(),
            "encryption failed: boom"
        );
        assert_eq!(
            CryptoError::Decrypt.to_string(),
            "decryption failed: invalid key or corrupted data"
        );
        assert_eq!(
            CryptoError::InvalidKey(7).to_string(),
            "invalid key length: expected 32 bytes, got 7"
        );
    }
}
