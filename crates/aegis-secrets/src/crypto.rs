use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use argon2::Argon2;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use rand::RngCore;

/// Derive a 256-bit key from a master password using Argon2id
pub fn derive_key(master_password: &str, salt: &[u8]) -> anyhow::Result<[u8; 32]> {
    let argon2 = Argon2::new(
        argon2::Algorithm::Argon2id,
        argon2::Version::V0x13,
        argon2::Params::new(32 * 1024, 3, 1, Some(32))
            .map_err(|e| anyhow::anyhow!("argon2 params: {}", e))?,
    );

    let mut key = [0u8; 32];
    argon2
        .hash_password_into(master_password.as_bytes(), salt, &mut key)
        .map_err(|e| anyhow::anyhow!("argon2 hash: {}", e))?;
    Ok(key)
}

/// Encrypt plaintext with AES-256-GCM.
/// Returns base64(nonce || ciphertext).
pub fn encrypt(plaintext: &str, key: &[u8; 32]) -> anyhow::Result<String> {
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| anyhow::anyhow!("cipher init: {}", e))?;

    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| anyhow::anyhow!("encrypt: {}", e))?;

    let mut combined = Vec::with_capacity(12 + ciphertext.len());
    combined.extend_from_slice(&nonce_bytes);
    combined.extend_from_slice(&ciphertext);

    Ok(STANDARD.encode(combined))
}

/// Decrypt base64(nonce || ciphertext) with AES-256-GCM.
pub fn decrypt(encrypted: &str, key: &[u8; 32]) -> anyhow::Result<String> {
    let combined = STANDARD.decode(encrypted)?;

    if combined.len() < 12 {
        anyhow::bail!("ciphertext too short (need at least 12 bytes for nonce)");
    }

    let (nonce_bytes, ciphertext) = combined.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);

    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| anyhow::anyhow!("cipher init: {}", e))?;

    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| anyhow::anyhow!("decryption failed (wrong password or corrupted data)"))?;

    String::from_utf8(plaintext).map_err(|e| anyhow::anyhow!("invalid utf8: {}", e))
}

/// Generate a random 16-byte salt
pub fn generate_salt() -> [u8; 16] {
    let mut salt = [0u8; 16];
    OsRng.fill_bytes(&mut salt);
    salt
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let key = derive_key("test-password", &[1u8; 16]).unwrap();
        let encrypted = encrypt("hello world", &key).unwrap();
        let decrypted = decrypt(&encrypted, &key).unwrap();
        assert_eq!(decrypted, "hello world");
    }

    #[test]
    fn wrong_password_fails() {
        let key1 = derive_key("correct", &[1u8; 16]).unwrap();
        let key2 = derive_key("wrong", &[1u8; 16]).unwrap();
        let encrypted = encrypt("secret", &key1).unwrap();
        assert!(decrypt(&encrypted, &key2).is_err());
    }

    #[test]
    fn different_salt_different_key() {
        let key1 = derive_key("pw", &[1u8; 16]).unwrap();
        let key2 = derive_key("pw", &[2u8; 16]).unwrap();
        assert_ne!(key1, key2);
    }
}
