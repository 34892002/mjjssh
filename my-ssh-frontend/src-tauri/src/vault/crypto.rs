use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use argon2::Argon2;
use rand_core::RngCore;
use zeroize::Zeroize;

pub const SALT_LEN: usize = 16;
pub const NONCE_LEN: usize = 12;
pub const KEY_LEN: usize = 32;

#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    #[error("Argon2 key derivation failed: {0}")]
    KeyDerivation(String),
    #[error("AES-GCM encryption failed: {0}")]
    Encryption(String),
    #[error("AES-GCM decryption failed (wrong password or corrupted data)")]
    Decryption,
    #[error("Invalid data format")]
    InvalidFormat,
}

pub fn generate_salt() -> [u8; SALT_LEN] {
    let mut salt = [0u8; SALT_LEN];
    rand_core::OsRng.fill_bytes(&mut salt);
    salt
}

pub fn derive_key(password: &[u8], salt: &[u8]) -> Result<[u8; KEY_LEN], CryptoError> {
    let mut key = [0u8; KEY_LEN];
    let params = argon2::Params::new(65536, 3, 4, None)
        .map_err(|e| CryptoError::KeyDerivation(e.to_string()))?;
    let argon2 = Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    argon2
        .hash_password_into(password, salt, &mut key)
        .map_err(|e| CryptoError::KeyDerivation(e.to_string()))?;
    Ok(key)
}

pub fn encrypt_field(key: &[u8; KEY_LEN], plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let cipher =
        Aes256Gcm::new_from_slice(key).map_err(|e| CryptoError::Encryption(e.to_string()))?;

    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand_core::OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| CryptoError::Encryption(e.to_string()))?;

    let mut result = Vec::with_capacity(NONCE_LEN + ciphertext.len());
    result.extend_from_slice(&nonce_bytes);
    result.extend_from_slice(&ciphertext);
    Ok(result)
}

pub fn decrypt_field(key: &[u8; KEY_LEN], data: &[u8]) -> Result<Vec<u8>, CryptoError> {
    if data.len() < NONCE_LEN {
        return Err(CryptoError::InvalidFormat);
    }

    let cipher =
        Aes256Gcm::new_from_slice(key).map_err(|e| CryptoError::Encryption(e.to_string()))?;

    let nonce = Nonce::from_slice(&data[..NONCE_LEN]);
    let ciphertext = &data[NONCE_LEN..];

    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| CryptoError::Decryption)
}

pub fn encrypt_string(key: &[u8; KEY_LEN], text: &str) -> Result<Vec<u8>, CryptoError> {
    encrypt_field(key, text.as_bytes())
}

pub fn decrypt_string(key: &[u8; KEY_LEN], data: &[u8]) -> Result<String, CryptoError> {
    let bytes = decrypt_field(key, data)?;
    String::from_utf8(bytes).map_err(|_| CryptoError::InvalidFormat)
}

#[derive(Zeroize)]
#[zeroize(drop)]
pub struct MasterKey {
    pub key: [u8; KEY_LEN],
}
