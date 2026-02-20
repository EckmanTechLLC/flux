//! AES-256-GCM encryption for credential tokens.
//!
//! Each token is encrypted separately with a unique nonce for maximum security.
//! The master key must be 32 bytes (256 bits) and is provided from an environment variable.

use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use anyhow::{anyhow, Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};

/// Size of the encryption key in bytes (256 bits)
const KEY_SIZE: usize = 32;

/// Size of the nonce in bytes (96 bits, standard for GCM)
const NONCE_SIZE: usize = 12;

/// Validates that the master key is exactly 32 bytes when base64 decoded.
///
/// # Arguments
/// * `key_base64` - Base64-encoded master key
///
/// # Returns
/// * `Ok(Vec<u8>)` - Decoded key bytes (32 bytes)
/// * `Err` - If key is invalid length or invalid base64
pub fn validate_key(key_base64: &str) -> Result<Vec<u8>> {
    let key_bytes = BASE64
        .decode(key_base64)
        .context("Failed to decode base64 encryption key")?;

    if key_bytes.len() != KEY_SIZE {
        return Err(anyhow!(
            "Encryption key must be {} bytes (256 bits), got {} bytes",
            KEY_SIZE,
            key_bytes.len()
        ));
    }

    Ok(key_bytes)
}

/// Encrypts plaintext using AES-256-GCM with a random nonce.
///
/// # Arguments
/// * `plaintext` - Data to encrypt (e.g., access token)
/// * `key` - 32-byte encryption key
///
/// # Returns
/// * `Ok((ciphertext, nonce))` - Encrypted data and the nonce used (both base64-encoded)
/// * `Err` - If encryption fails
///
/// # Security
/// - Uses a cryptographically secure random nonce (never reuse)
/// - Authenticated encryption (tampering detected)
/// - Key must be kept secret and never stored on disk
pub fn encrypt(plaintext: &str, key: &[u8]) -> Result<(String, String)> {
    if key.len() != KEY_SIZE {
        return Err(anyhow!("Encryption key must be {} bytes", KEY_SIZE));
    }

    // Create cipher instance
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| anyhow!("Failed to create cipher: {}", e))?;

    // Generate random nonce (never reuse!)
    let nonce_bytes = Aes256Gcm::generate_nonce(&mut OsRng);

    // Encrypt
    let ciphertext_bytes = cipher
        .encrypt(&nonce_bytes, plaintext.as_bytes())
        .map_err(|e| anyhow!("Encryption failed: {}", e))?;

    // Encode to base64 for storage
    let ciphertext = BASE64.encode(&ciphertext_bytes);
    let nonce = BASE64.encode(&nonce_bytes);

    Ok((ciphertext, nonce))
}

/// Decrypts ciphertext using AES-256-GCM.
///
/// # Arguments
/// * `ciphertext` - Base64-encoded encrypted data
/// * `nonce` - Base64-encoded nonce (must match the one used during encryption)
/// * `key` - 32-byte encryption key (must match the one used during encryption)
///
/// # Returns
/// * `Ok(String)` - Decrypted plaintext
/// * `Err` - If decryption fails (wrong key, corrupted data, or tampered)
pub fn decrypt(ciphertext: &str, nonce: &str, key: &[u8]) -> Result<String> {
    if key.len() != KEY_SIZE {
        return Err(anyhow!("Encryption key must be {} bytes", KEY_SIZE));
    }

    // Decode from base64
    let ciphertext_bytes = BASE64
        .decode(ciphertext)
        .context("Failed to decode ciphertext")?;
    let nonce_bytes = BASE64.decode(nonce).context("Failed to decode nonce")?;

    if nonce_bytes.len() != NONCE_SIZE {
        return Err(anyhow!("Invalid nonce size: expected {}, got {}", NONCE_SIZE, nonce_bytes.len()));
    }

    // Create cipher instance
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| anyhow!("Failed to create cipher: {}", e))?;

    let nonce = Nonce::from_slice(&nonce_bytes);

    // Decrypt
    let plaintext_bytes = cipher
        .decrypt(nonce, ciphertext_bytes.as_ref())
        .map_err(|e| anyhow!("Decryption failed (wrong key or corrupted data): {}", e))?;

    // Convert to string
    String::from_utf8(plaintext_bytes).context("Decrypted data is not valid UTF-8")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_validation() {
        // Valid 32-byte key (base64-encoded)
        let valid_key = BASE64.encode(&[0u8; 32]);
        assert!(validate_key(&valid_key).is_ok());

        // Too short
        let short_key = BASE64.encode(&[0u8; 16]);
        assert!(validate_key(&short_key).is_err());

        // Too long
        let long_key = BASE64.encode(&[0u8; 64]);
        assert!(validate_key(&long_key).is_err());

        // Invalid base64
        assert!(validate_key("not-valid-base64!@#$").is_err());
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let key = [0u8; 32]; // Test key
        let plaintext = "my-secret-access-token-12345";

        // Encrypt
        let (ciphertext, nonce) = encrypt(plaintext, &key).expect("Encryption failed");

        // Ciphertext should be different from plaintext
        assert_ne!(ciphertext, plaintext);

        // Decrypt
        let decrypted = decrypt(&ciphertext, &nonce, &key).expect("Decryption failed");

        // Should match original
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_different_nonces() {
        let key = [0u8; 32];
        let plaintext = "same-plaintext";

        // Encrypt twice
        let (ciphertext1, nonce1) = encrypt(plaintext, &key).unwrap();
        let (ciphertext2, nonce2) = encrypt(plaintext, &key).unwrap();

        // Nonces should be different (random)
        assert_ne!(nonce1, nonce2);

        // Ciphertexts should be different (different nonces)
        assert_ne!(ciphertext1, ciphertext2);

        // Both should decrypt correctly
        assert_eq!(decrypt(&ciphertext1, &nonce1, &key).unwrap(), plaintext);
        assert_eq!(decrypt(&ciphertext2, &nonce2, &key).unwrap(), plaintext);
    }

    #[test]
    fn test_wrong_key_fails() {
        let key1 = [0u8; 32];
        let key2 = [1u8; 32]; // Different key
        let plaintext = "secret";

        let (ciphertext, nonce) = encrypt(plaintext, &key1).unwrap();

        // Decrypting with wrong key should fail
        assert!(decrypt(&ciphertext, &nonce, &key2).is_err());
    }

    #[test]
    fn test_wrong_nonce_fails() {
        let key = [0u8; 32];
        let plaintext = "secret";

        let (ciphertext, _) = encrypt(plaintext, &key).unwrap();
        let (_, wrong_nonce) = encrypt("other", &key).unwrap();

        // Decrypting with wrong nonce should fail
        assert!(decrypt(&ciphertext, &wrong_nonce, &key).is_err());
    }

    #[test]
    fn test_tampered_ciphertext_fails() {
        let key = [0u8; 32];
        let plaintext = "secret";

        let (mut ciphertext, nonce) = encrypt(plaintext, &key).unwrap();

        // Tamper with ciphertext
        ciphertext.push('X');

        // Should fail (authenticated encryption detects tampering)
        assert!(decrypt(&ciphertext, &nonce, &key).is_err());
    }
}
