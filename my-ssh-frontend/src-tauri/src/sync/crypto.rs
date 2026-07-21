use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    Aes256Gcm, Nonce,
};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::{engine::general_purpose::STANDARD, Engine};
use rand::{rngs::OsRng, RngCore};
use serde::Serialize;
use zeroize::Zeroize;

use super::models::{
    EncryptedVault, EncryptionMetadata, SyncEnvelopeError, ARGON2ID_ITERATIONS,
    ARGON2ID_MEMORY_KIB, ARGON2ID_PARALLELISM, NONCE_LENGTH, SALT_LENGTH,
};

pub const SYNC_KEY_LENGTH: usize = 32;
const MIN_SYNC_PASSWORD_CHARACTERS: usize = 8;

#[derive(Debug, thiserror::Error)]
pub enum SyncCryptoError {
    #[error("sync encryption input is invalid: {0}")]
    InvalidInput(String),
    #[error("sync envelope is invalid: {0}")]
    InvalidEnvelope(#[from] SyncEnvelopeError),
    #[error("sync key derivation failed")]
    KeyDerivation,
    #[error("sync encryption failed")]
    Encryption,
    #[error("sync password is incorrect or sync data is corrupted")]
    Decryption,
}

pub fn encrypt_vault(
    plaintext: &[u8],
    mut password: String,
    vault_id: String,
    revision: u64,
    updated_at: String,
    updated_by_device_id: String,
) -> Result<EncryptedVault, SyncCryptoError> {
    validate_metadata(&vault_id, &updated_by_device_id)?;
    if plaintext.is_empty() {
        return Err(SyncCryptoError::InvalidInput("Vault JSON is empty".into()));
    }
    if password.chars().count() < MIN_SYNC_PASSWORD_CHARACTERS {
        password.zeroize();
        return Err(SyncCryptoError::InvalidInput(format!(
            "Sync password must contain at least {MIN_SYNC_PASSWORD_CHARACTERS} characters"
        )));
    }

    let mut salt = [0u8; SALT_LENGTH];
    let mut nonce = [0u8; NONCE_LENGTH];
    OsRng.fill_bytes(&mut salt);
    OsRng.fill_bytes(&mut nonce);
    let encryption = EncryptionMetadata::new(salt, nonce);
    let aad = associated_data(
        &vault_id,
        revision,
        &updated_at,
        &updated_by_device_id,
        &encryption,
    )?;
    let mut key = derive_key(password, &salt)?;
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| SyncCryptoError::Encryption)?;
    let nonce = Nonce::try_from(nonce.as_slice()).map_err(|_| SyncCryptoError::Encryption)?;
    let ciphertext = cipher
        .encrypt(
            &nonce,
            Payload {
                msg: plaintext,
                aad: &aad,
            },
        )
        .map_err(|_| SyncCryptoError::Encryption);
    key.zeroize();
    let ciphertext = ciphertext?;

    let envelope = EncryptedVault {
        format_version: super::models::REMOTE_FORMAT_VERSION,
        vault_id,
        revision,
        updated_at,
        updated_by_device_id,
        encryption,
        ciphertext: STANDARD.encode(ciphertext),
    };
    envelope.validate()?;
    Ok(envelope)
}

pub fn derive_sync_key(
    password: String,
    salt: &[u8; SALT_LENGTH],
) -> Result<[u8; SYNC_KEY_LENGTH], SyncCryptoError> {
    if password.chars().count() < MIN_SYNC_PASSWORD_CHARACTERS {
        return Err(SyncCryptoError::InvalidInput(format!(
            "Sync password must contain at least {MIN_SYNC_PASSWORD_CHARACTERS} characters"
        )));
    }
    derive_key(password, salt)
}

pub fn decrypt_vault(
    envelope: &EncryptedVault,
    password: String,
) -> Result<Vec<u8>, SyncCryptoError> {
    let salt = envelope.encryption.validate()?.0;
    let key = derive_sync_key(password, &salt)?;
    decrypt_vault_with_key(envelope, &key)
}

pub fn decrypt_vault_with_key(
    envelope: &EncryptedVault,
    key: &[u8; SYNC_KEY_LENGTH],
) -> Result<Vec<u8>, SyncCryptoError> {
    envelope.validate()?;
    let (_, nonce) = envelope.encryption.validate()?;
    let aad = associated_data(
        &envelope.vault_id,
        envelope.revision,
        &envelope.updated_at,
        &envelope.updated_by_device_id,
        &envelope.encryption,
    )?;
    let ciphertext = STANDARD
        .decode(&envelope.ciphertext)
        .map_err(|_| SyncCryptoError::Decryption)?;
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| SyncCryptoError::Decryption)?;
    let nonce = Nonce::try_from(nonce.as_slice()).map_err(|_| SyncCryptoError::Decryption)?;
    let plaintext = cipher.decrypt(
        &nonce,
        Payload {
            msg: &ciphertext,
            aad: &aad,
        },
    );
    plaintext.map_err(|_| SyncCryptoError::Decryption)
}

fn validate_metadata(vault_id: &str, updated_by_device_id: &str) -> Result<(), SyncCryptoError> {
    if uuid::Uuid::parse_str(vault_id).is_err() {
        return Err(SyncCryptoError::InvalidInput(
            "Vault ID must be a UUID".into(),
        ));
    }
    if uuid::Uuid::parse_str(updated_by_device_id).is_err() {
        return Err(SyncCryptoError::InvalidInput(
            "Device ID must be a UUID".into(),
        ));
    }
    Ok(())
}

pub fn encrypt_vault_with_key(
    plaintext: &[u8],
    key: &[u8; SYNC_KEY_LENGTH],
    vault_id: String,
    revision: u64,
    updated_at: String,
    updated_by_device_id: String,
    salt: [u8; SALT_LENGTH],
) -> Result<EncryptedVault, SyncCryptoError> {
    validate_metadata(&vault_id, &updated_by_device_id)?;
    if plaintext.is_empty() {
        return Err(SyncCryptoError::InvalidInput("Vault JSON is empty".into()));
    }
    let mut nonce = [0u8; NONCE_LENGTH];
    OsRng.fill_bytes(&mut nonce);
    let encryption = EncryptionMetadata::new(salt, nonce);
    let aad = associated_data(
        &vault_id,
        revision,
        &updated_at,
        &updated_by_device_id,
        &encryption,
    )?;
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| SyncCryptoError::Encryption)?;
    let nonce = Nonce::try_from(nonce.as_slice()).map_err(|_| SyncCryptoError::Encryption)?;
    let ciphertext = cipher
        .encrypt(
            &nonce,
            Payload {
                msg: plaintext,
                aad: &aad,
            },
        )
        .map_err(|_| SyncCryptoError::Encryption)?;
    let envelope = EncryptedVault {
        format_version: super::models::REMOTE_FORMAT_VERSION,
        vault_id,
        revision,
        updated_at,
        updated_by_device_id,
        encryption,
        ciphertext: STANDARD.encode(ciphertext),
    };
    envelope.validate()?;
    Ok(envelope)
}

fn derive_key(
    mut password: String,
    salt: &[u8; SALT_LENGTH],
) -> Result<[u8; SYNC_KEY_LENGTH], SyncCryptoError> {
    if password.is_empty() {
        return Err(SyncCryptoError::InvalidInput(
            "Sync password must not be empty".into(),
        ));
    }
    let params = Params::new(
        ARGON2ID_MEMORY_KIB,
        ARGON2ID_ITERATIONS,
        ARGON2ID_PARALLELISM,
        Some(SYNC_KEY_LENGTH),
    )
    .map_err(|_| SyncCryptoError::KeyDerivation)?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = [0u8; SYNC_KEY_LENGTH];
    let result = argon2
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .map_err(|_| SyncCryptoError::KeyDerivation);
    password.zeroize();
    result?;
    Ok(key)
}

fn associated_data(
    vault_id: &str,
    revision: u64,
    updated_at: &str,
    updated_by_device_id: &str,
    encryption: &EncryptionMetadata,
) -> Result<Vec<u8>, SyncCryptoError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Aad<'a> {
        format_version: u32,
        vault_id: &'a str,
        revision: u64,
        updated_at: &'a str,
        updated_by_device_id: &'a str,
        encryption: super::models::KdfParameters,
    }

    serde_json::to_vec(&Aad {
        format_version: super::models::REMOTE_FORMAT_VERSION,
        vault_id,
        revision,
        updated_at,
        updated_by_device_id,
        encryption: encryption.aad_parameters(),
    })
    .map_err(|_| SyncCryptoError::InvalidInput("could not encode AAD".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encrypt_fixture() -> EncryptedVault {
        encrypt_vault(
            br#"{"formatVersion":1,"profiles":[{"host":"example.test"}]}"#,
            "correct sync password".into(),
            "b9b92c0e-0f4d-4b64-8f1a-53f7d4f56b9e".into(),
            18,
            "2026-07-20T12:00:00Z".into(),
            "ee1cffb9-2f55-479d-8f84-a6f4a33f7c33".into(),
        )
        .unwrap()
    }

    #[test]
    fn round_trips_complete_vault_payload() {
        let envelope = encrypt_fixture();
        let plaintext = decrypt_vault(&envelope, "correct sync password".into()).unwrap();
        assert_eq!(
            plaintext,
            br#"{"formatVersion":1,"profiles":[{"host":"example.test"}]}"#
        );
        assert!(!envelope.ciphertext.contains("example.test"));
    }

    #[test]
    fn rejects_wrong_password_and_tampering() {
        let envelope = encrypt_fixture();
        assert!(decrypt_vault(&envelope, "wrong sync password".into()).is_err());

        let mut changed_revision = envelope.clone();
        changed_revision.revision += 1;
        assert!(decrypt_vault(&changed_revision, "correct sync password".into()).is_err());

        let mut changed_ciphertext = envelope;
        changed_ciphertext.ciphertext.replace_range(0..1, "A");
        assert!(decrypt_vault(&changed_ciphertext, "correct sync password".into()).is_err());
    }

    #[test]
    fn rejects_unsupported_kdf_parameters_and_header_tampering() {
        let mut envelope = encrypt_fixture();
        envelope.encryption.memory_kib = 1;
        assert!(decrypt_vault(&envelope, "correct sync password".into()).is_err());

        let mut changed_device = encrypt_fixture();
        changed_device.updated_by_device_id = uuid::Uuid::new_v4().to_string();
        assert!(decrypt_vault(&changed_device, "correct sync password".into()).is_err());

        let mut changed_nonce = encrypt_fixture();
        changed_nonce.encryption.nonce = STANDARD.encode([0u8; NONCE_LENGTH]);
        assert!(decrypt_vault(&changed_nonce, "correct sync password".into()).is_err());
    }

    #[test]
    fn uses_fresh_salt_and_nonce_for_each_encryption() {
        let first = encrypt_fixture();
        let second = encrypt_fixture();
        assert_ne!(first.encryption.salt, second.encryption.salt);
        assert_ne!(first.encryption.nonce, second.encryption.nonce);
    }

    #[test]
    fn saved_derived_key_reuses_salt_and_refreshes_nonce() {
        let first = encrypt_fixture();
        let salt = first.encryption.validate().unwrap().0;
        let key = derive_sync_key("correct sync password".into(), &salt).unwrap();
        let second = encrypt_vault_with_key(
            br#"{"formatVersion":1,"profiles":[{"host":"new.example.test"}]}"#,
            &key,
            first.vault_id.clone(),
            first.revision + 1,
            "2026-07-21T12:00:00Z".into(),
            first.updated_by_device_id.clone(),
            salt,
        )
        .unwrap();

        assert_eq!(first.encryption.salt, second.encryption.salt);
        assert_ne!(first.encryption.nonce, second.encryption.nonce);
        assert_eq!(
            decrypt_vault_with_key(&second, &key).unwrap(),
            br#"{"formatVersion":1,"profiles":[{"host":"new.example.test"}]}"#
        );
    }

    #[test]
    fn enforces_an_eight_character_sync_password_minimum() {
        let error = encrypt_vault(
            br#"{}"#,
            "seven7!".into(),
            uuid::Uuid::new_v4().to_string(),
            1,
            "2026-07-20T12:00:00Z".into(),
            uuid::Uuid::new_v4().to_string(),
        )
        .unwrap_err();
        assert!(matches!(error, SyncCryptoError::InvalidInput(_)));

        encrypt_vault(
            br#"{}"#,
            "eight88!".into(),
            uuid::Uuid::new_v4().to_string(),
            1,
            "2026-07-20T12:00:00Z".into(),
            uuid::Uuid::new_v4().to_string(),
        )
        .unwrap();
    }
}
